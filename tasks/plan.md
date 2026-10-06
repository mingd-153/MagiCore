# Plan: Fail-closed parsing for versioned lock documents

## Goal

Make the v4 lock parser reject unrecognized fields at every nested schema boundary so a field from another writer cannot be silently discarded and later mistaken for validated lock data.

## Scope and constraints

- Limit this slice to the lockfile crate's v4 serde model and regression tests.
- Preserve the existing canonical digest bytes and v4 wire shape for valid documents.
- Do not claim that v4 is wired into production install/mutation paths; those remain a separate platform-wide implementation gate.
- No tag, commit, push, or release operation.

## Tasks and acceptance

1. Add regression tests that inject an unknown root field and an unknown nested package/signature field; confirm each currently parses (RED).
2. Add strict unknown-field rejection to every v4 document/nested record type; valid canonical fixtures must still round-trip (GREEN).
3. Run the focused v4 lockfile tests, lockfile crate tests, formatting, and a final diff review; preserve unrelated working-tree changes.

## Verification

- `cargo test -p mgc-lockfile --test v4_canonical --locked`
- `cargo test -p mgc-lockfile --locked`
- `cargo fmt --all --check`
- `git diff --check`

## Out of scope

This slice does not implement v4 runtime install, v3-to-v4 migration, all-core native resolver/materializer coverage, or release provenance. Those cannot be claimed complete by parser strictness.
