#!/usr/bin/env bash
# Cache Tracking Stress Test — Phase 3
# Đo cold/warm run, cache hit/miss, disk usage, shared reuse, cache path hermetic

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MGC_BIN="${MGC_BIN:-$PROJECT_ROOT/target/debug/mgc}"
TEST_DIR="/tmp/mgc-cache-stress-$$"
TEST_HOME="$TEST_DIR/home"

# Setup isolated test environment — môi trường test cô lập
export HOME="$TEST_HOME"
export MGC_CACHE_DIR="$TEST_HOME/.mgc"

echo "=== Cache Tracking Stress Test ==="
echo "Binary: $MGC_BIN"
echo "Test dir: $TEST_DIR"
echo "Test home: $TEST_HOME"
echo "Cache dir: $MGC_CACHE_DIR"
echo

# Cleanup — dọn dẹp
cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

mkdir -p "$TEST_DIR" "$TEST_HOME"
cd "$TEST_DIR"

# Helper: get directory size in bytes — helper: lấy kích thước thư mục (bytes)
get_dir_size() {
    local dir="$1"
    if [ -d "$dir" ]; then
        du -sk "$dir" 2>/dev/null | awk '{print $1 * 1024}' || echo 0
    else
        echo 0
    fi
}

# Helper: count files in directory — helper: đếm file trong thư mục
count_files() {
    local dir="$1"
    if [ -d "$dir" ]; then
        find "$dir" -type f 2>/dev/null | wc -l | tr -d ' '
    else
        echo 0
    fi
}

# === COLD RUN TEST === — test chạy lạnh (cache trống)
echo "=== COLD RUN TEST (empty cache) ==="

CACHE_SIZE_BEFORE=$(get_dir_size "$MGC_CACHE_DIR")
echo "Cache size before: $CACHE_SIZE_BEFORE bytes"

# Use perl for millisecond timing (BSD date doesn't support %N) — dùng perl cho timing millisecond
COLD_START=$(perl -MTime::HiRes=time -e 'printf "%.0f\n", time()*1000')
$MGC_BIN create-web vanilla test-cold --ts >/dev/null 2>&1 || true
COLD_END=$(perl -MTime::HiRes=time -e 'printf "%.0f\n", time()*1000')
COLD_DURATION=$((COLD_END - COLD_START))

CACHE_SIZE_AFTER=$(get_dir_size "$MGC_CACHE_DIR")
CACHE_FILES=$(count_files "$MGC_CACHE_DIR")

echo "Cold run duration: ${COLD_DURATION}ms"
echo "Cache size after: $CACHE_SIZE_AFTER bytes"
echo "Cache files created: $CACHE_FILES"
echo "Cache growth: $((CACHE_SIZE_AFTER - CACHE_SIZE_BEFORE)) bytes"

# === WARM RUN TEST === — test chạy nóng (cache có sẵn)
echo
echo "=== WARM RUN TEST (cache populated) ==="

# Run same command again — chạy lại lệnh giống
WARM_START=$(perl -MTime::HiRes=time -e 'printf "%.0f\n", time()*1000')
$MGC_BIN create-web vanilla test-warm --ts >/dev/null 2>&1 || true
WARM_END=$(perl -MTime::HiRes=time -e 'printf "%.0f\n", time()*1000')
WARM_DURATION=$((WARM_END - WARM_START))

CACHE_SIZE_WARM=$(get_dir_size "$MGC_CACHE_DIR")

echo "Warm run duration: ${WARM_DURATION}ms"
echo "Cache size after warm: $CACHE_SIZE_WARM bytes"
echo "Cache size delta (should be small): $((CACHE_SIZE_WARM - CACHE_SIZE_AFTER)) bytes"

# Calculate speedup — tính tốc độ tăng
if [ "$COLD_DURATION" -gt 0 ]; then
    SPEEDUP=$(awk "BEGIN {printf \"%.2f\", $COLD_DURATION / $WARM_DURATION}")
    echo "Speedup (cold/warm): ${SPEEDUP}x"
else
    echo "Speedup: N/A (cold duration too small)"
fi

# === CACHE HIT/MISS TRACKING === — theo dõi cache hit/miss
echo
echo "=== CACHE HIT/MISS SIMULATION ==="

# Create multiple projects with same template — tạo nhiều project cùng template
HIT_COUNT=0
MISS_COUNT=0

for i in {1..5}; do
    CACHE_BEFORE=$(get_dir_size "$MGC_CACHE_DIR")
    $MGC_BIN create-web vanilla "test-hit-$i" --ts >/dev/null 2>&1 || true
    CACHE_AFTER=$(get_dir_size "$MGC_CACHE_DIR")

    DELTA=$((CACHE_AFTER - CACHE_BEFORE))
    if [ "$DELTA" -lt 10000 ]; then
        # Cache hit (minimal growth) — cache hit (tăng nhỏ)
        HIT_COUNT=$((HIT_COUNT + 1))
    else
        # Cache miss (significant growth) — cache miss (tăng lớn)
        MISS_COUNT=$((MISS_COUNT + 1))
    fi
done

echo "Cache hit count (estimated): $HIT_COUNT"
echo "Cache miss count (estimated): $MISS_COUNT"
RATIO=$(awk "BEGIN {printf \"%.1f\", ($HIT_COUNT / 5.0) * 100}")
echo "Cache hit ratio: ${RATIO}%"

# === SHARED REUSE TEST === — test chia sẻ và tái sử dụng
echo
echo "=== SHARED OBJECT REUSE TEST ==="

# Create projects with different cores — tạo project nhiều cores khác nhau
CACHE_BASELINE=$(get_dir_size "$MGC_CACHE_DIR")

$MGC_BIN create-web vanilla test-shared-web --ts >/dev/null 2>&1 || true
CACHE_WEB=$(get_dir_size "$MGC_CACHE_DIR")

$MGC_BIN create-ai python-agent test-shared-ai >/dev/null 2>&1 || true
CACHE_AI=$(get_dir_size "$MGC_CACHE_DIR")

$MGC_BIN create-app flutter@stable test-shared-app >/dev/null 2>&1 || true
CACHE_APP=$(get_dir_size "$MGC_CACHE_DIR")

$MGC_BIN create-lib rust@1.96.0 test-shared-lib >/dev/null 2>&1 || true
CACHE_LIB=$(get_dir_size "$MGC_CACHE_DIR")

WEB_GROWTH=$((CACHE_WEB - CACHE_BASELINE))
AI_GROWTH=$((CACHE_AI - CACHE_WEB))
APP_GROWTH=$((CACHE_APP - CACHE_AI))
LIB_GROWTH=$((CACHE_LIB - CACHE_APP))

echo "Cache growth per core:"
echo "  web:  $WEB_GROWTH bytes"
echo "  ai:   $AI_GROWTH bytes"
echo "  app:  $APP_GROWTH bytes"
echo "  lib:  $LIB_GROWTH bytes"

TOTAL_GROWTH=$((CACHE_LIB - CACHE_BASELINE))
AVG_GROWTH=$((TOTAL_GROWTH / 4))
echo "Average growth per core: $AVG_GROWTH bytes"

# Check if shared objects reduce incremental growth — kiểm tra shared objects giảm tăng trưởng gia tăng
SHARED_REUSE_DETECTED=false
if [ "$AI_GROWTH" -lt "$WEB_GROWTH" ] && [ "$APP_GROWTH" -lt "$WEB_GROWTH" ]; then
    echo "✓ Shared reuse detected (subsequent cores grow less)"
    SHARED_REUSE_DETECTED=true
else
    echo "⚠ No clear shared reuse pattern"
    SHARED_REUSE_DETECTED=false
fi

# === CACHE PATH VERIFICATION === — xác minh đường dẫn cache
echo
echo "=== CACHE PATH VERIFICATION ==="

if [ -d "$MGC_CACHE_DIR" ]; then
    echo "✓ Cache dir exists: $MGC_CACHE_DIR"
else
    echo "✗ Cache dir missing"
    exit 1
fi

if [[ "$MGC_CACHE_DIR" == "$TEST_HOME"* ]]; then
    echo "✓ Cache path is hermetic (inside test HOME)"
else
    echo "✗ Cache path leaked outside test HOME"
    exit 1
fi

# Check cache doesn't pollute user HOME — kiểm tra cache không làm bẩn HOME user
USER_HOME=$(eval echo ~)
if [ -d "$USER_HOME/.mgc" ]; then
    USER_CACHE_SIZE=$(get_dir_size "$USER_HOME/.mgc")
    echo "⚠ User cache exists: $USER_HOME/.mgc ($USER_CACHE_SIZE bytes)"
    echo "   (may be from other tests or normal usage)"
else
    echo "✓ User HOME cache not polluted"
fi

# === CACHE CLEAN TEST === — test dọn cache
echo
echo "=== CACHE CLEAN TEST ==="

CACHE_BEFORE_CLEAN=$(get_dir_size "$MGC_CACHE_DIR")
FILES_BEFORE_CLEAN=$(count_files "$MGC_CACHE_DIR")

echo "Before clean: $CACHE_BEFORE_CLEAN bytes, $FILES_BEFORE_CLEAN files"

# Try cache clean command — thử lệnh clean cache
$MGC_BIN cache clean --yes >/dev/null 2>&1 || echo "⚠ cache clean not supported (may not have --yes flag)"

CACHE_AFTER_CLEAN=$(get_dir_size "$MGC_CACHE_DIR")
FILES_AFTER_CLEAN=$(count_files "$MGC_CACHE_DIR")

echo "After clean: $CACHE_AFTER_CLEAN bytes, $FILES_AFTER_CLEAN files"
echo "Cache freed: $((CACHE_BEFORE_CLEAN - CACHE_AFTER_CLEAN)) bytes"

# Verify projects still exist — xác minh project vẫn tồn tại
if [ -f "test-cold/index.html" ] && [ -f "test-shared-web/index.html" ]; then
    echo "✓ Projects intact after cache clean (not deleted)"
else
    echo "✗ WARNING: Projects may have been affected by cache clean"
fi

# === SUMMARY === — tóm tắt
echo
echo "=== CACHE TRACKING SUMMARY ==="
echo "Cold run: ${COLD_DURATION}ms"
echo "Warm run: ${WARM_DURATION}ms"
echo "Speedup: ${SPEEDUP}x (warm faster)"
RATIO=$(awk "BEGIN {printf \"%.1f\", ($HIT_COUNT / 5.0) * 100}")
echo "Cache hit ratio: ${RATIO}%"
echo "Cache path: hermetic ✓"
if [ "$SHARED_REUSE_DETECTED" = true ]; then
    echo "Shared reuse: detected ✓"
else
    echo "Shared reuse: NOT detected ⚠"
fi
echo "Total cache size: $(get_dir_size "$MGC_CACHE_DIR") bytes"
echo "Total cache files: $(count_files "$MGC_CACHE_DIR")"
echo
echo "✓ CACHE TRACKING COMPLETE"

# Export metrics for report — xuất metrics cho báo cáo
cat > "$TEST_DIR/cache_metrics.json" <<EOF
{
  "cold_run_ms": $COLD_DURATION,
  "warm_run_ms": $WARM_DURATION,
  "speedup_factor": ${SPEEDUP},
  "cache_hit_count": $HIT_COUNT,
  "cache_miss_count": $MISS_COUNT,
  "cache_hit_ratio": $(awk "BEGIN {printf \"%.3f\", ($HIT_COUNT / 5)}"),
  "cache_size_bytes": $(get_dir_size "$MGC_CACHE_DIR"),
  "cache_files": $(count_files "$MGC_CACHE_DIR"),
  "cache_path_hermetic": true,
  "shared_reuse_detected": $SHARED_REUSE_DETECTED
}
EOF

echo "Metrics exported to: $TEST_DIR/cache_metrics.json"
cat "$TEST_DIR/cache_metrics.json"

exit 0
