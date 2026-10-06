# Slice checklist: strict versioned lock parsing

- [x] Add unknown-field regression tests and observe expected RED.
- [x] Reject unknown root and nested v4 fields without changing canonical digest output.
- [x] Run focused and crate-wide verification.
- [x] Review the final diff and record evidence in the local changelog.

## Final evidence

- `cargo test -p mgc-lockfile --locked --quiet`: exit 0; all crate test binaries passed, including `v4_canonical` 16/16.
- `cargo test --workspace --locked --no-fail-fast --quiet`: first run had one live npm freshness request failure; the exact test passed alone with network permission, and the second full-workspace run exited 0.
- `cargo fmt --all --check`, `cargo clippy -p mgc-lockfile --all-targets --locked -- -D warnings`, and `git diff --check`: exit 0.
- The malformed-v4 corpus now also exercises the production `parse_document()` version-dispatch path, not only the direct helper: 16/16 canonical tests pass.
- `cargo clean` removed only this task's generated `target/` artifacts (5.8 GiB); 16 GiB free afterward. No unrelated caches were deleted.

## Global gates executed after the parser slice

- `python3 scripts/audit_mutation_calls.py`: PASS, 80 mutation call sites allowlisted; 272 filesystem-write substrate sites inventoried.
- `python3 scripts/audit_dependency_delegation.py`: EXIT 1; 350 Rust findings in total (305 allowed, 27 violations, 18 review-required), plus 32 Python process APIs and 361 shell process surfaces. The 27 are not all package-manager calls: the report includes build, app-dev, deploy, arbitrary user-command, lifecycle-script, process-wrapper, and distribution-tool routes. Static coverage gaps remain declared by the script.
- `python3 scripts/audit_capability_matrix.py`: PASS, generated 11 audit/scanner lanes; expected release audit lanes 7/7. This matrix is audit coverage, not native lifecycle ownership evidence.
- `python3 scripts/verify_release_manifests.py`: EXIT 1; 10 RC9 checksum placeholders, publish blocked until real release archives exist.
- Disk after the evidence runs: `cargo clean` removed generated `target/` (4.6 GiB); 16 GiB free. No unrelated caches deleted.

## Slice: authenticate v4 workspace and optimizer state

- [x] Add regressions proving workspace topology and optimizer profile affect the v4 digest/signature.
- [x] Canonically serialize both fields and test order stability plus parse/round-trip digest equality.
- [x] Verify signed-file tampering is rejected when either field changes.
- [x] Run focused lockfile tests, clippy, fmt, diff check, then clean generated `target/` after the interrupted workspace run.

### Evidence / limitation

- Focused `v4_canonical` and `v4_policy`: 40/40 passed; crate-wide `mgc-lockfile` tests and clippy passed.
- `cargo fmt --all --check` and `git diff --check` passed.
- The latest workspace run did not finish: `test_embedded_template_dep_majors_track_latest` failed in the `mgc --lib` and `mgc` targets because the live npm metadata read for `typescript` failed; later long bundler E2Es passed before user interruption. Do not count this run as workspace-green.
- Removed only generated `target/` build output (4.3 GiB). Disk remains below the requested 30 GiB free threshold at about 12 GiB; no unrelated data was deleted.
