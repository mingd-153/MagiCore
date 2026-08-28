#!/bin/bash
# Script to migrate inline #[cfg(test)] mod tests to separate test files
# Per RULE.md §5: CẤM inline tests trong src/*.rs

set -euo pipefail

echo "=== MagiCore Inline Test Migration ==="
echo "Moving #[cfg(test)] mod tests { } from src/ to test/"
echo ""

# Find all files with inline test modules
files_with_inline_tests=$(grep -rl "#\[cfg(test)\]" adapters/ core/ cli/src/ 2>/dev/null | grep -E "\.rs$" | grep -v "/test/" || true)

if [ -z "$files_with_inline_tests" ]; then
    echo "✅ No inline tests found! All tests already in test/ directories."
    exit 0
fi

count=0
for file in $files_with_inline_tests; do
    # Check if it actually has "mod tests"
    if ! grep -q "mod tests" "$file"; then
        continue
    fi
    
    ((count++))
    echo "[$count] Found inline tests: $file"
    
    # Get directory and filename
    dir=$(dirname "$file")
    base=$(basename "$file" .rs)
    
    # Create test directory if not exists
    test_dir="$dir/test"
    mkdir -p "$test_dir"
    
    # Extract test module content (between mod tests { and final })
    # This is simplified — actual extraction needs careful parsing
    echo "    → Would extract tests to: $test_dir/${base}_test.rs"
    echo "    → Would add #[cfg(test)] #[path = \"test/${base}_test.rs\"] mod ${base}_test; to source"
    echo ""
done

echo ""
echo "=== Summary ==="
echo "Found $count files with inline tests"
echo ""
echo "⚠️  This is a DRY-RUN script (shows what would be done)"
echo "⚠️  Actual migration requires careful AST parsing to avoid breaking code"
echo ""
echo "Manual migration steps per file:"
echo "1. Extract mod tests { ... } content"
echo "2. Save to <folder>/test/<name>_test.rs (without mod tests wrapper)"
echo "3. Replace inline block with: #[cfg(test)] #[path = \"test/<name>_test.rs\"] mod <name>_test;"
echo "4. Run cargo test to verify"
echo ""
echo "Estimated effort: $count files × 10 min = $((count * 10 / 60)) hours"
