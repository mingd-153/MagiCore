# codebaseRoadmap.md — Bản đồ cấu trúc MagiCore / Repository Structure Map

> Cập nhật theo checkout ngày 2026-10-06.
> Đọc RULE.md và AGENTS.md trước khi làm việc; dùng map này để định vị, không dùng nó thay source/test.
> Đây là **bản đồ thư mục**, không phải ma trận hỗ trợ chức năng hay tuyên bố test xanh.

## 0. Quy tắc đọc map

1. Xác nhận đường dẫn bằng find, rg --files, Cargo.toml, manifest của module và workflow hiện hành.
2. Tên/thư mục có mặt không chứng minh feature đã triển khai hoặc lifecycle hoạt động.
3. Không ghi branch, commit SHA hoặc CI status động vào map. Xem chúng tại thời điểm review.
4. Root docs/ là tài liệu nội bộ local-only theo .gitignore; file review/map ở root được theo dõi riêng sau yêu cầu của user.

## 1. Cây thư mục hiện tại

~~~text
MagiCore/
├── AGENTS.md, RULE.md                 # hướng dẫn agent và quy tắc repo
├── CODEBASE_REVIEW.md                 # quy trình rà soát codebase
├── codebaseRoadmap.md                 # map đường dẫn này
├── Cargo.toml, Cargo.lock              # root Cargo workspace và dependency lock
├── core/
│   ├── Cargo.toml                      # nested workspace metadata cho crates/*
│   └── crates/
│       ├── mgc-adapter-base/
│       ├── mgc-audit/
│       ├── mgc-cache/
│       ├── mgc-config/
│       ├── mgc-crypto/
│       ├── mgc-exec/
│       ├── mgc-fetcher/
│       ├── mgc-http/
│       ├── mgc-lockfile/
│       ├── mgc-oci/
│       ├── mgc-oidc/
│       ├── mgc-pack/
│       ├── mgc-platform/
│       ├── mgc-plugin/
│       ├── mgc-publish/
│       ├── mgc-registry-server/
│       ├── mgc-resolver/
│       ├── mgc-sbom/
│       ├── mgc-search/
│       ├── mgc-store/
│       ├── mgc-types/
│       ├── mgc-ui/
│       └── mgc-workspace/
├── adapters/
│   ├── ai/
│   ├── app/
│   ├── cicd/
│   ├── cloud/
│   ├── game/
│   ├── hardware/
│   ├── iot/
│   ├── lib/
│   └── web/
├── cli/
│   ├── src/                            # CLI implementation
│   ├── tests/                          # CLI integration tests and fixtures
│   ├── embedded/                       # embedded resources
│   └── docs/                            # module documentation; inspect ignore rules
├── tools/
│   ├── mgc-dist/                       # distribution tool
│   └── mgc-mcp/                        # present on disk; excluded by root Cargo.toml
├── templates/
│   └── web/                            # only source-template core present at this snapshot
├── tests/
│   ├── e2e/
│   │   ├── src/
│   │   └── tests/
│   └── fixtures/
├── scripts/                            # repository automation and checks
├── ci/
│   └── evidence/
├── .github/
│   └── workflows/                      # CI, lifecycle, security, release workflows
├── benchmark/
│   ├── env/
│   ├── results/
│   └── scripts/
├── assets/
│   └── boards/
├── data/
│   └── registry/                       # runtime registry state; gitignored
├── deploy/
│   ├── docker/
│   └── nginx/
├── packaging/
│   ├── artifacts/
│   ├── homebrew/
│   ├── packages/
│   └── scoop/
├── tasks/                              # README, plan.md, todo.md at this snapshot
├── docs/                               # internal docs; local-only by default
└── dist/                               # present in checkout; verify contents/tracking before use
~~~

At this snapshot, <code>core/crates/</code> contains 23 crate directories and <code>adapters/</code> contains 9 adapter directories. These counts describe folder presence only.

## 2. Cargo workspace boundaries

- Root Cargo.toml declares <code>core/crates/*</code>, <code>adapters/*</code>, <code>cli</code>, <code>tools/*</code> and <code>tests/e2e</code> as workspace member patterns.
- Root manifest excludes <code>tools/mgc-mcp</code> and <code>tools/core-web-lab</code>; the latter was not present in the inspected folder tree.
- <code>core/Cargo.toml</code> declares its own <code>crates/*</code> workspace metadata. Check the root manifest and package manifests when deciding which workspace command applies.
- <code>tools/mgc-dist</code> is present. Inspect workspace metadata before asserting each tool's membership.
- Workspace version, Rust edition/MSRV, feature flags and actual package names are dynamic facts: read manifests rather than copying old values from this map.

## 3. Source, tests and templates

| Need to inspect | Start at |
|---|---|
| Shared Rust behavior | <code>core/crates/&lt;crate&gt;/src/</code> and that crate's manifest/tests |
| Ecosystem adapter behavior | <code>adapters/&lt;core&gt;/src/</code>, <code>tests/</code>, <code>benches/</code> and manifest |
| CLI routing/commands | <code>cli/src/dispatch/</code>, <code>cli/src/commands/</code>, <code>cli/tests/</code> |
| Workspace E2E | <code>tests/e2e/src/</code>, <code>tests/e2e/tests/</code>, <code>tests/fixtures/</code> |
| Source templates | <code>templates/</code>; only <code>templates/web/</code> was present at this snapshot |
| Embedded template resources | <code>cli/embedded/</code>; inspect generation/embedding scripts before changing |
| CI and gates | <code>.github/workflows/</code>, <code>ci/</code>, <code>scripts/</code> |
| Distribution and packaging | <code>packaging/</code>, <code>tools/mgc-dist/</code>, <code>dist/</code> |

Adapters commonly have <code>src/</code>, <code>tests/</code>, <code>benches/</code>, and some have <code>docs/</code>; verify each adapter independently. Tests also live under <code>cli/tests/</code> and <code>tests/e2e/tests/</code>. Do not relocate tests based on an assumed universal layout.

## 4. Documentation and ignore boundaries

- Root <code>docs/</code> is private/local-only under <code>.gitignore</code>; do not stage its reports, specs, changelog, archive or other files unless the user names the exact file.
- <code>docs/specs/magiCoreChangeLog.md</code> is the required local change record under current agent instructions; it is not included in the public documentation commit by default.
- Root <code>CODEBASE_REVIEW.md</code> and <code>codebaseRoadmap.md</code> are the two shared agent documents explicitly approved for Git. Their root exceptions are in <code>.gitignore</code>; a local <code>.git/info/exclude</code> entry does not apply to other clones.
- Module <code>docs/</code> folders have their own ignore rules. Check <code>.gitignore</code> before adding files there.
- <code>tasks/</code> contains <code>README.md</code>, <code>READMEVN.md</code>, <code>plan.md</code> and <code>todo.md</code> in this checkout. Read task content only when the task at hand points to it.

## 5. Test/CI status discipline

- User correction on 2026-10-06: the test run referred to in the active discussion had **failed**. This map does not contain its run ID, commit SHA, job, log or root cause; verify those from the actual CI run before attributing the failure.
- Preserve failure as failure. Skips, ignored tests, missing tools and unexecuted lanes are not passes.
- To assess current CI, list runs and open logs for the exact branch/HEAD under review; do not reuse an older successful run or a matrix result from another SHA.
- This map has no claim that any core, framework, OS or workflow is currently green.

## 6. Review entry points

1. Read <code>RULE.md</code> and <code>AGENTS.md</code>.
2. Read <code>CODEBASE_REVIEW.md</code> for the review flow, then use this file to find relevant folders.
3. Check branch/HEAD/status/diff and inspect current manifests, source, tests and workflows for the request.
4. Report only evidence actually observed. Identify the SHA/run for CI claims and state any manual/tooling exclusions.

## 7. Updating this map

- Refresh only the paths/sections that changed; confirm entries against the current filesystem and manifests.
- Update the date in the header when the structure map changes.
- Append a bilingual entry to <code>docs/specs/magiCoreChangeLog.md</code> when required by <code>AGENTS.md</code>; keep that internal file local-only unless separately authorized.
- Do not restore stale feature claims, old version/branch/CI status, deleted folder paths, or guessed lifecycle capabilities.
