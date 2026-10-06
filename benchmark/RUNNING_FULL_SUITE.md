# Running Full Benchmark Suite - INVALIDATED

> **⚠️ NOTICE (2026-09-05)**: This guide is **INVALIDATED**. Suite scripts referenced here produced data with validation gaps. Preserved for audit trail. **DO NOT RUN** for new benchmarks.

## Invalidation Reason

See [`BENCHMARK_STATUS.md`](BENCHMARK_STATUS.md) for issues and replacement procedure.

---

# Original Guide (ARCHIVED)


**Status:** ✅ RUNNING (Started 2026-08-27 14:09:42 +07:00)
**Progress:** Check real-time with `./scripts/monitor_suite.sh`
**Estimated Duration:** 4-6 hours (overnight run)

---

## What's Running

**Full benchmark suite:** 20 runs total
- 5 runs × npm (JavaScript baseline)
- 5 runs × pnpm (fast disk-efficient PM)
- 5 runs × bun (fastest PM baseline)
- 5 runs × mgc (MagiCore — testing)

**Each run measures:**
- Cold install time (cache cleared)
- Warm install time (cache hot)
- Disk usage (node_modules size)
- Machine spec (CPU, RAM, OS)

---

## Monitoring Progress

### Quick Check
```bash
cd benchmark
./scripts/monitor_suite.sh
```

**Output example:**
```
Progress: 8/20 runs (40%)

Breakdown by PM:
  npm   : 5/5 runs ✓
  pnpm  : 3/5 runs
  bun   : 0/5 runs
  mgc   : 0/5 runs

Latest result:
  pnpm_run3_20260827_152342.json
  pnpm: 12.45s cold, 125MB disk

Estimated remaining: 3h 15m
Estimated completion: 2026-08-27 18:30:00
```

### Watch Live Log
```bash
tail -f results/suite_20260827_140942.log
```

### List Completed Runs
```bash
ls -lh results/*.json
```

---

## What Happens After Completion

### Automatic Analysis
The suite script will automatically run `analyze_results.py`:

```bash
# Generates:
# - Mean, median, stddev for each PM
# - Markdown comparison tables
# - Relative speed analysis
# - Disk efficiency comparison
```

**Output:** Markdown tables ready to paste into `docs/BENCHMARK.md`

### Manual Analysis (If Needed)
```bash
python3 scripts/analyze_results.py results/
```

---

## Next Steps (After Completion)

### 1. Review Results
```bash
# Check all JSON files
ls results/*.json

# Verify 5 runs per PM
ls results/npm_*.json | wc -l   # Should be 5
ls results/pnpm_*.json | wc -l  # Should be 5
ls results/bun_*.json | wc -l   # Should be 5
ls results/mgc_*.json | wc -l   # Should be 5
```

### 2. Analyze Data
```bash
# Run analyzer (if not auto-run)
python3 scripts/analyze_results.py results/

# Copy output to clipboard (macOS)
python3 scripts/analyze_results.py results/ | pbcopy
```

### 3. Update BENCHMARK.md

**Replace preliminary results with final data:**

```bash
# Open docs/BENCHMARK.md
# Find section "## Benchmark Results"
# Paste tables from analyzer output
# Remove "🚧 PRELIMINARY" badge
# Update "Last Updated" date
```

**Before:**
```markdown
> 🚧 **PRELIMINARY RESULTS** — Based on 1 run per PM.
> Full 5-run suite pending execution (4-6h).
```

**After:**
```markdown
**Final Results** — Based on 5 runs per PM (Aug 27, 2026).
Statistical rigor: mean, median, stddev reported.
```

### 4. Verify Reproducibility

**Check variance:**
- Stddev should be <10% of mean (good reproducibility)
- If stddev >15%, investigate outliers (thermal throttling? network?)

**Example good result:**
```
npm: 77.1s ± 2.3s (stddev 3%)
```

**Example needs investigation:**
```
npm: 77.1s ± 15.8s (stddev 20%)  ← High variance!
```

### 5. Commit + Push

**After updating BENCHMARK.md:**

```bash
cd /Users/doanmihh/Documents/Workspace/MagiCore

# Review changes
git diff docs/BENCHMARK.md

# Commit benchmark scripts + results
git add benchmark/scripts/*.sh benchmark/scripts/*.py
git add docs/BENCHMARK.md

git commit -m "feat(benchmark): complete full 20-run suite with statistical analysis

- 5 runs per PM (npm, pnpm, bun, mgc)
- Native macOS execution (Apple M2, 8 cores, 16GB)
- Statistical analysis: mean, median, stddev
- Remove preliminary status, final launch data

Results: [paste key findings here]
- npm: X.Xs ± Y.Ys
- pnpm: X.Xs ± Y.Ys
- bun: X.Xs ± Y.Ys
- mgc: X.Xs ± Y.Ys

Closes Gate 1 Week 2 (full benchmark requirement)"
```

---

## Troubleshooting

### Suite Stuck / No Progress

**Check if process running:**
```bash
ps aux | grep run_full_suite_native
```

**Check log for errors:**
```bash
tail -100 results/suite_20260827_140942.log
```

**Common issues:**
- Network timeout (npm registry slow) → Wait, suite will continue
- Thermal throttling (laptop hot) → Normal, cooldown included
- Disk full → Check `df -h`

### Resume After Interruption

**The suite is resilient:**
- Each run creates independent JSON file
- Partial results are valid (e.g., 12/20 runs)
- Analyzer works with any number of runs

**To get partial results:**
```bash
# Analyze what's complete
python3 scripts/analyze_results.py results/

# Report: "npm: 5/5, pnpm: 3/5, bun: 2/5, mgc: 2/5"
```

### Re-run Specific PM

**If one PM failed all runs:**
```bash
# Re-run just mgc (5 runs)
for i in {1..5}; do
  ./scripts/run_benchmark_native.sh mgc $i
  sleep 30
done
```

---

## Expected Timeline

Based on preliminary runs:

| PM | Avg Time/Run | 5 Runs Total |
|---|---|---|
| npm | ~80s | ~7 min |
| pnpm | ~15s | ~2 min |
| bun | ~100s | ~9 min |
| mgc | ~20s (estimate) | ~2 min |

**Total runtime:** ~20 min active + 30s × 19 cooldowns = **~30 minutes**

**But:** Network variance, cache behavior, thermal effects → **budget 4-6 hours safe**.

---

## Success Criteria

✅ **Suite successful if:**
1. 15+ runs complete (75% success rate OK)
2. At least 3 runs per PM (minimum for stddev)
3. Results variance <15% (reproducible)
4. No systematic failures (all PMs work)

🟡 **Acceptable partial success:**
- 12/20 runs (60%) — Still enough for analysis
- Report missing data honestly in BENCHMARK.md

🔴 **Failure (re-run needed):**
- <10 runs total
- Any PM has 0 successful runs
- Variance >30% (test environment unstable)

---

## Background Process Info

**Terminal ID:** `term_1787814582406_a021hypmrlo`

**Check if still running:**
```bash
# From Kiro IDE, run:
# (This is internal, user doesn't need this)
```

**Kill if needed:**
```bash
pkill -f run_full_suite_native
```

---

## Questions?

**"How long left?"**
→ Run `./scripts/monitor_suite.sh` for ETA

**"Can I use my computer while this runs?"**
→ Yes, but avoid:
  - Heavy CPU tasks (affects benchmark timing)
  - Network-heavy tasks (affects install speed)
  - Closing terminal/sleeping computer (kills process)

**"What if I need to stop?"**
→ Press Ctrl+C in terminal, or `pkill -f run_full_suite_native`
→ Partial results still valid

**"Results look wrong?"**
→ Check `results/suite_*.log` for errors
→ Verify machine wasn't under load during runs
→ Re-run specific PM if needed

---

## Final Checklist

After suite completes:

- [ ] Run `./scripts/monitor_suite.sh` — Verify 100% or acceptable %
- [ ] Run `python3 scripts/analyze_results.py results/` — Get tables
- [ ] Update `docs/BENCHMARK.md` — Paste tables, remove PRELIMINARY
- [ ] Verify variance <15% — Check reproducibility
- [ ] Review `results/SUITE_SUMMARY_*.txt` — Auto-generated summary
- [ ] Commit changes — Scripts + docs
- [ ] Celebrate 🎉 — V1.0 launch data complete!

---

*Generated: 2026-08-27 14:10 +07:00*
*Status: RUNNING (background process)*
*ETA: 4-6 hours (check with monitor script)*
