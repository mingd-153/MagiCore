# Core Web Execution Plan (2026-07-29)

## Muc tieu

- On dinh lai `core-web` truoc khi benchmark tiep.
- Chot lai policy host/port de UX ro rang hon:
  - Frontend: `http://localhost:4315`
  - Backend: `http://localhost:3415`
- Lam sach correctness cho cac flow chinh:
  - `mg create-web`
  - `mg install-web`
  - `mg dev`
  - `mg start`
- Chay mot vong test tong va ghi lai ket qua that.

## Checklist song

### 1. Ke hoach va audit

- [x] Tao file checklist song cho vong sua ngay 2026-07-29
- [x] Ra soat lai surface `create/install/dev/start`
- [x] Doi chieu idea "native compiler" voi codebase hien tai

### 2. Correctness truoc benchmark

- [x] Bat loi build/test hien tai (`peer_deps` trong test CLI)
- [x] Fix het test do muc CLI/core-web dang gap
- [ ] Giam warning ro rang o nhung file vua sua

### 3. Host / port policy

- [x] Chuan hoa default frontend ve `localhost:4315` o user-facing flow
- [x] Chuan hoa default backend ve `localhost:3415` o user-facing flow
- [x] Giu local bind strategy an toan, khong mo rong vo tinh ra `0.0.0.0`

### 4. Flow chinh can verify

- [x] `mg create-web --help`
- [x] `mg dev --help`
- [x] `mg start`
- [x] `mg build`
- [x] `mg create-web` scaffold mot project frontend dai dien
- [x] `mg install-web` tren project dai dien
- [x] `mg dev` tren project dai dien

### 5. Test tong

- [x] `cargo test -p mg test_help_surface_matches_build_shape -- --nocapture`
- [x] `cargo test -p mg test_available_cores_matches_build_shape -- --nocapture`
- [x] Chay them mot vong test tong co lien quan core-web neu can

## Ghi chu trong qua trinh

- Vong nay uu tien correctness + UX surface truoc.
- Chua coi benchmark moi la hop le neu test/build shape con do.
- Neu co thay doi huong benchmark, phai cap nhat lai sau khi flow chinh xanh.
- Da fix pha 1:
  - bo sung `peer_deps` cho fixture/test cua CLI va web adapter
  - cap nhat user-facing localhost policy cho FE/BE
  - dong bo expectation test theo port backend `3415`
- Ket qua test ngay 2026-07-29:
  - `cargo test -p mg -- --nocapture` -> xanh toan bo
  - `cargo test -p mg-web-adapter --lib` -> `56 passed`
  - `cargo test -p mg test_help_surface_matches_build_shape -- --nocapture` -> xanh
  - `cargo test -p mg test_available_cores_matches_build_shape -- --nocapture` -> xanh
  - `cargo run -p mg -- --help` -> ok
  - `cargo run -p mg -- create-web --help` -> ok
  - `cargo run -p mg -- dev --help` -> ok
- Ket qua runtime verify ngay 2026-07-29 tren project mau `/private/tmp/mg-runtime-react`:
  - `mg create-web react-vite /private/tmp/mg-runtime-react --ts` -> ok
  - `mg install-web --ignore-scripts` -> ok, `7 packages installed`, `600 ms total`
  - `mg build` -> ok, `Bundle created: 1024.59 KB in 49.353416ms`
  - `mg dev --host localhost --port 4315` -> ok sau khi fix route Axum, `curl -I http://localhost:4315` -> `HTTP/1.1 200 OK`
  - `mg start` -> ok, serve `dist/` tren `http://localhost:4315`, `curl -I http://localhost:4315` -> `HTTP/1.1 200 OK`
- Ket qua benchmark subset sau vong fix/runtime verify:
  - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_210026.md`
  - lanes da chay:
    - `mg-create-web` -> `2.749 s`
    - `build` -> `207.1 ms`
    - `dev-startup` -> `241.1 ms`
    - `start-startup` -> `201.9 ms`
    - `heavy-empty-cache-install-direct` -> `69.325 s`
    - `heavy-build` -> `433.2 ms`
    - `heavy-dev-startup` -> `476.5 ms`
  - tat ca lane trong subset nay deu `PASS`
- Toi uu cold path tiep theo ngay 2026-07-29:
  - sua `core/crates/mg-store/src/cas/store.rs`
    - bo verify lai source CAS tren moi `export_to(...)`
    - bo verify destination sau `hard_link(...)`
    - chi con verify destination o nhanh fallback `copy`
  - sua `core/crates/mg-store/src/cas/write.rs`
    - bo `sync_all()` tren moi file CAS
    - bo read-back verify tren moi file CAS
    - giu content hash tu bytes dau vao lam nguon truth cho cache artifact
  - profile heavy cold lane sau patch:
    - truoc patch cuoi:
      - install profile tong: `~65020 ms`
      - `prepare_extracted_roots`: `~65005 ms`
    - sau patch cuoi:
      - install profile tong: `~3647 ms`
      - `prepare_extracted_roots`: `~3632 ms`
  - benchmark harness moi:
    - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_210941.md`
    - `heavy-empty-cache-install-direct` -> `4.616 s`
    - `heavy-build` -> `483.0 ms`
    - `heavy-dev-startup` -> `476.2 ms`
- Benchmark doi dau sau toi uu (MG vs Bun vs pnpm):
  - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_211052.md`
  - `build`
    - MG: `212.6 ms`
    - Bun: `1.803 s`
    - pnpm: `2.434 s`
  - `dev-startup`
    - MG: `212.2 ms`
    - Bun: `949.6 ms`
    - pnpm: `2.530 s`
  - `start-startup`
    - MG: `209.9 ms`
    - Bun: `1.306 s`
    - pnpm: `2.733 s`
  - `heavy-empty-cache-install-direct`
    - MG: `4.398 s`
    - lane nay trong file benchmark hien tai chi co MG, chua co Bun/pnpm doi chieu truc tiep
  - `heavy-build`
    - MG: `441.6 ms`
    - Bun: `3.126 s`
    - pnpm: `4.579 s`
  - `heavy-dev-startup`
    - MG: `437.4 ms`
    - Bun: `1.604 s`
    - pnpm: `3.931 s`
- Benchmark mutate/install compare sau toi uu store/CAS:
  - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_211230.md`
  - `cold-install`
    - MG: `154.6 ms`
    - Bun: `279.0 ms`
    - pnpm: `1.505 s`
    - ket qua: MG nhanh hon Bun `1.80x`, nhanh hon pnpm `9.73x`
  - `warm-install`
    - MG: `231.7 ms`
    - Bun: `1.023 s`
    - pnpm: `3.393 s`
    - ket qua: MG nhanh hon Bun `4.42x`, nhanh hon pnpm `14.64x`
  - `add-single`
    - MG: `381.5 ms`
    - Bun: `505.9 ms`
    - pnpm: `2.281 s`
    - ket qua: MG nhanh hon Bun `1.33x`, nhanh hon pnpm `5.98x`
  - `add-multiple`
    - MG: `860.6 ms`
    - Bun: `264.4 ms`
    - pnpm: `2.112 s`
    - ket qua: MG van thua Bun `3.25x`, nhung nhanh hon pnpm ro ret
  - `remove-single`
    - MG: `128.1 ms`
    - Bun: `255.7 ms`
    - pnpm: `2.196 s`
    - ket qua: MG nhanh hon Bun `2.00x`, nhanh hon pnpm `17.15x`
  - `heavy-empty-cache-install-direct`
    - MG: `5.115 s`
    - lane nay hien tai van la MG-only lane trong harness compare nay
  - tong ket ngan:
    - MG dang dan dau o da so lane da so sanh that: `cold-install`, `warm-install`, `add-single`, `remove-single`
    - MG van con diem nghen ro o `add-multiple`
    - heavy cold install da roi tu hang `~69 s` xuong `~4.6-5.1 s`, la buoc nhay lon nhat cua vong nay
- Toi uu them cho mutate lane ngay 2026-07-29:
  - sua `adapters/web/src/lib.rs`
    - `latest_version_string(...)` khong doc metadata truc tiep qua `load_metadata_with_fallback(...)` nua
    - thay vao do dung `self.provider.metadata(...)` de tai su dung RAM metadata cache trong process
  - ly do:
    - `add-single` van on truoc do, nhung `add-multiple` bi phat chi phi lap metadata lookup/fallback
    - cache RAM cua `NpmDependencyProvider` giup multi-add khong lap doc metadata khong can thiet
  - verify sau sua:
    - `cargo test -p mg-web-adapter --lib` -> `56 passed`
    - `cargo test -p mg -- --nocapture` -> xanh toan bo, framework matrix scaffold van xanh
  - benchmark mutate lane moi:
    - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_211637.md`
    - luu y:
      - co mot lan benchmark sandbox bi fail do package moi chua co trong cache local
      - da rerun bang network that de lay so lieu dung
    - `add-single`
      - MG: `154.7 ms`
      - Bun: `827.3 ms`
      - pnpm: `2.917 s`
      - ket qua: MG nhanh hon Bun `5.35x`, nhanh hon pnpm `18.85x`
    - `add-multiple`
      - MG: `161.9 ms`
      - Bun: `896.4 ms`
      - pnpm: `3.031 s`
      - ket qua: MG nhanh hon Bun `5.54x`, nhanh hon pnpm `18.72x`
  - y nghia:
    - lane tung la diem nghen lon nhat (`add-multiple`) da duoc lat nguoc ket qua
    - mutate path hien tai dang o trang thai rat canh tranh trong matrix vua do
- Don no ky thuat + verify lai build/test ngay 2026-07-29:
  - da bo import thua trong `adapters/web/src/lib.rs`
  - da bo bien thua trong `cli/src/bundler/deps_bundler.rs`
  - da bo `BuildCache` va field `cache` khong con duoc dung trong `cli/src/bundler/dev_server.rs`
  - da sua warning chi so mang trong `adapters/web/src/native/npm_registry.rs`
  - da bo helper chet `batch_prefetch_concurrency_limit()` trong `adapters/web/src/lib.rs`
  - verify:
    - `cargo test -p mg-web-adapter --lib` -> xanh
    - `cargo test -p mg -- --nocapture` -> xanh toan bo, framework matrix van xanh
    - `cargo build --release -p mg` -> ok
- Benchmark cold/heavy sau khi don warning va giu binary moi:
  - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_212241.md`
  - `cold-install`
    - MG: `138.3 ms`
    - Bun: `803.1 ms`
    - pnpm: `2.180 s`
    - ket qua: MG nhanh hon Bun `5.81x`, nhanh hon pnpm `15.76x`
  - `warm-install`
    - MG: `251.9 ms`
    - Bun: `1.549 s`
    - pnpm: `3.904 s`
    - ket qua: MG nhanh hon Bun `6.15x`, nhanh hon pnpm `15.49x`
  - `heavy-empty-cache-install-direct`
    - MG: `7.514 s`
    - lane nay van la MG-only lane
  - nhan xet:
    - cold/warm install dang o phong do rat tot tren matrix vua do
    - heavy cold lane van can benchmark doi chieu cong bang hon va toi uu tiep neu muc tieu la ep sat tran
- Sua harness + chot heavy cross-PM lane ngay 2026-07-29:
  - sua `benchmark.sh`
    - bo sung ghi chu ro:
      - `heavy-empty-cache-install` la lane heavy cold-cache doi chieu cong bang giua cac PM
      - `heavy-empty-cache-install-direct` va `alias-heavy-empty-cache-install-direct` la MG-only diagnostic lanes
    - da bat va sua mot loi harness:
      - chen backticks vao report header trong heredoc shell lam shell co gang thuc thi ten lane nhu command
      - da doi sang text thuong de tranh command substitution ngo ngang
  - benchmark heavy compare moi:
    - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_212656.md`
    - `heavy-empty-cache-install`
      - MG: `12.074 s`
      - Bun: `30.493 s`
      - pnpm: `16.881 s`
      - ket qua: MG nhanh hon pnpm `1.40x`, nhanh hon Bun `2.53x`
    - nhan xet:
      - day la baseline nhe hon muc tieu "hon Bun rat nhieu", nhung da la mot ket qua that va cong bang hon so voi MG-only heavy direct lane
      - do lech chuan cua Bun/pnpm o lane nay van kha lon, nen can chay lai tren may it nhieu hon de co median on dinh hon neu muon dua vao public report
- Chot benchmark policy ro hon ngay 2026-07-29:
  - verify bo sung cho lane `dev/build` tren scaffold toi gian `/private/tmp/mg-dev-build-check`:
    - `mg create-web vanilla /private/tmp/mg-dev-build-check --js` -> ok
    - `mg build --quiet` -> ok, `~22.98 ms`
    - `mg build --target native --quiet` -> ok, `~45.15 ms`
    - native binary chay duoc:
      - `/private/tmp/mg-dev-build-check/crates/engine/target/release/mg-web-engine`
      - startup `~4.43 ms`
    - `mg dev --host localhost --port 4315` truoc install -> fail ro rang:
      - `Missing local executable 'vite'. Run 'mg install-web'`
    - `mg install-web` -> ok
      - `1 packages installed`
      - `260 ms total`
    - `mg dev --host localhost --port 4315` sau install -> server len duoc
      - `curl -I http://localhost:4315` -> `HTTP/1.1 200 OK`
  - ket luan tam thoi cho flow toi gian:
    - `create -> install -> dev -> build -> build native -> run native binary` da di het vong
    - no van la `compatibility-shell` cho FE, chua phai native frontend runtime that
    - nhung correctness cua flow chinh hien tai da ro rang va de benchmark hon truoc
  - verify bo sung framework FE dai dien:
    - `/private/tmp/mg-react-check`
      - `mg create-web react-vite --ts` -> ok
      - `mg install-web` -> ok
      - `mg build --quiet` -> ok
    - `/private/tmp/mg-next-check-2`
      - `mg create-web nextjs --ts` -> ok
      - `mg install-web` -> ok
      - sau khi doi mac dinh web install sang `legacy_flat`, `mg build --quiet` -> ok
    - `/private/tmp/mg-vue-check`
      - `mg create-web vue-vite --ts` -> ok
      - `mg install-web` -> ok
      - `mg build --quiet` -> van fail
      - trang thai that:
        - launcher build framework-aware da vao dung `vite build`
        - loi con lai nam o dependency materialization / nested resolution cho Vue compiler chain
        - vi du loi cuoi: `MODULE_NOT_FOUND` trong nhanh `@vue/compiler-* -> @vue/compiler-core -> entities`
  - thay doi ky thuat cua vong nay:
    - `cli/src/commands/build.rs`
      - `mg build` da co nhanh framework-aware thay vi ep toan bo frontend vao bundler esbuild chung
      - da chan script build delegate sang PM ngoai
      - da them test cho framework build mapping
    - `cli/src/commands/core/shared.rs`
      - web install mac dinh tam thoi dung `legacy_flat`
      - `MEGAGATE_WEB_STRICT_LAYOUT=1` van co the bat lai strict layout de debug / benchmark
  - ket luan tam thoi:
    - `react-vite`: build on
    - `nextjs`: build on
    - `vue-vite`: chua on, no ky thuat hien tai la materializer/layout chua giai quyet tron ven runtime resolution
  - sua `benchmark.sh`
    - them `BENCH_ALLOW_NETWORK` (mac dinh `1`)
    - them `ONLINE_REQUIRED_LANES` de danh dau nhung lane can live network hoac cache da duoc seed
    - report gio tu ghi ro:
      - lane nao la `MG-only diagnostic`
      - lane nao `requires live network or pre-seeded cache`
    - neu `BENCH_ALLOW_NETWORK=0`, benchmark se `SKIP` dung lane can network thay vi fail do sandbox/cache miss
  - verify:
    - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_215536.md`
    - lane: `add-single`
    - config: `BENCH_ALLOW_NETWORK=0`
    - ket qua: `SKIPPED (network-disabled)` dung nhu mong doi
  - y nghia:
    - report benchmark gio trung thuc hon ve dieu kien chay
    - giam fail gia do network/cache va tach bach duoc "loi core" voi "dieu kien benchmark"
- Them `seed-cache mode` cho benchmark ngay 2026-07-29:
  - sua `benchmark.sh`
    - them `BENCH_SEED_CACHE=1|0`
    - them `SEEDABLE_CACHE_LANES`
    - voi cac lane nhu `add-single`, `add-multiple`, `remove-single`, harness co the chay `--prepare` de seed cache truoc moi run
    - muc tieu: tach chi phi fetch package moi khoi mutate/install cost khi can benchmark "steady-state but still real"
  - verify:
    - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_215810.md`
    - lane: `add-single`
    - config:
      - `BENCH_SEED_CACHE=1`
      - `BENCH_ALLOW_NETWORK=1`
    - ket qua:
      - report hien ro `Seed-cache mode: 1`
      - lane `add-single` -> `PASS`
      - MG: `140.1 ms`
  - y nghia:
    - tu nay co the benchmark tach ro hon giua:
      - online fetch cost
      - seeded mutate cost
      - warm cache reuse cost
    - bang benchmark se it bi bat be hon khi dua ra public
- Mo rong benchmark surface tiep ngay 2026-07-29:
  - sua `benchmark.sh`
    - them lane `update-web` (MG-only diagnostic)
    - them lane `audit-web` (MG-only diagnostic)
    - them fixture `ultra-heavy-web`
    - them lane `ultra-heavy-empty-cache-install` de ep graph nang hon `heavy-web`
  - verify lane moi:
    - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_220238.md`
      - `update-web` -> `PASS`, MG: `2.019 s`
      - `audit-web` lan dau bi `FAIL`, nhung nguyen nhan la loi runner Bash (`local audit_status` o top-level case), khong phai loi core
    - da fix harness do
    - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_220323.md`
      - `audit-web` -> `PASS`, MG: `529.7 ms`
  - ket qua `ultra-heavy`:
    - file: `/Users/doanmihh/Documents/Workspace/MegaGate/benchmark_brutal_results_20260729_220240.md`
    - `ultra-heavy-empty-cache-install`
      - MG: `22.299 s`
      - Bun: `22.101 s`
      - pnpm: `14.867 s`
      - ket qua: `pnpm` nhanh hon `bun` `1.49x` va nhanh hon `mg` `1.50x`
  - y nghia:
    - day la mot phat hien rat quan trong:
      - tren `heavy-web`, MG dang thang
      - nhung tren `ultra-heavy-web`, MG khong con dan dau
    - nghia la cold/heavy path da tien bo rat manh, nhung chua scale tot o muc graph nang hon nua
    - day chinh la no ky thuat "that", khong phai noise benchmark
- Profile truc tiep `ultra-heavy` ngay 2026-07-29:
  - da dung lai fixture `ultra-heavy-web` va chay:
    - `MEGAGATE_WEB_PROFILE_INSTALL=1`
    - `MEGAGATE_SHARED_CACHE_DIR=<empty>`
    - `mg install --core web --ignore-scripts`
  - ket qua command profile:
    - `resolve_graph=5437ms`
    - `prepare_install_execution_total=5444ms`
    - `adapter_install=8926ms`
    - `install_with_adapter_total=14371ms`
  - ket qua install profile:
    - `prefetch_tarballs=15ms`
    - `prepare_extracted_roots=8754ms`
    - `materialize_dependency_graph=8876ms`
    - `write_lockfile=8909ms`
    - tong install profile: `8909ms`
  - ket qua pipeline profile:
    - `packages=715`
    - `bytes=120198819`
    - `download_ms_total=93648`
    - `download_ms_max=1528`
    - `extract_ms_total=59147`
    - `extract_ms_max=1337`
    - slow download lon nhat:
      - `@next/swc-darwin-arm64@14.2.33` ~ `1528ms`
      - `next@14.2.35` ~ `1127ms`
    - slow extract lon nhat:
      - `clsx@2.1.1` ~ `1337ms`
      - `framer-motion@12.43.0` ~ `1324ms`
      - `webpack@5.109.2` ~ `1315ms`
      - `next@14.2.35` ~ `1315ms`
  - ket luan ky thuat:
    - o `ultra-heavy`, diem nghen khong con chi nam o materialization
    - `resolve_graph` da tro thanh mot khoi lon (~`5.4s`)
    - sau do `prepare_extracted_roots` van la pha install dat nhat (~`8.8s`)
    - nghia la de keo lai `ultra-heavy`, can toi uu dong thoi:
      - metadata/resolve path
      - extract/materialize path
- Loi runtime that da bat va fix trong vong nay:
  - `cli/src/bundler/dev_server.rs`
  - route Axum cu sai cu phap:
    - `"/@megagate/deps/{*pkg}"` -> panic khi boot
    - `"/{*path}"` -> route khong dung kieu Axum dang dung
  - da sua thanh:
    - `"/@megagate/deps/*pkg"`
    - `"/*path"`
- No ky thuat con lai sau vong nay:
  - heavy cold lane da giam rat manh va da co doi chieu cong bang hon, nhung van can matrix nang hon nua neu muon cong bo public benchmark
  - can mo rong `seed-cache` mode cho them lane update/audit neu benchmark surface tiep tuc lon hon
  - can audit tiep nhom cold/heavy lane thay vi mutate lane, vi mutate path vua duoc cai thien rat manh
  - uu tien cao nhat hien tai:
    - tim vi sao `ultra-heavy-empty-cache-install` lam MG roi xuong sau pnpm
    - profile lai chi tiet cac pha download / extract / materialize / lockfile / manifest / verify o `ultra-heavy-web`
    - toi uu giam `resolve_graph` cho graph alias/package rat lon
  - can benchmark tiep voi matrix nang hon nua sau khi xu ly `add-multiple`
  - neu muon len beta chac hon, can them lane so sanh cho heavy graph voi doi chieu Bun/pnpm cung do nang, khong chi MG-only
  - can chot lai benchmark policy de bao cao cong khai:
    - uu tien median/mean + do lech chuan
    - tach ro lane "MG-only diagnostic" va lane "cross-PM compare"
## Update - 2026-07-29 (late)

### Code changes completed in this pass

- Removed duplicate strict-install background tarball prefetch from `/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lib.rs`.
  - Before: strict install could both `spawn_tarball_download(...)` and run `pipeline_download_and_extract(...)` over the same `fetch_graph`.
  - Risk: self-contention on bandwidth and shared-cache IO during cold installs.
  - After: strict path only uses the bounded pipeline.
- Switched `get_tarball_bytes(...)` to use `batch_download_tarball(...)` instead of the metadata-oriented tarball client.
  - Intent: use the large-body HTTP lane for install-time tarball fetches too.
- Removed now-unused batch-prefetch env helper/tests and kept resolve-prefetch tests only.

### Validation

- `cargo test -p mg-web-adapter --lib` -> `56 passed`
- `cargo build --release -p mg` -> success

### Ultra-heavy cold profile snapshots

#### Baseline before this pass

- `resolve_graph ~= 5437ms`
- `solver_solve ~= 5345ms`
- `prepare_extracted_roots ~= 8754ms`
- `adapter_install ~= 8926ms`
- `install_with_adapter_total ~= 14371ms`

#### After removing duplicate strict-install background prefetch

- `resolve_graph = 6045ms`
- `solver_solve = 6034ms`
- `prepare_extracted_roots = 9729ms`
- `adapter_install = 9898ms`
- `install_with_adapter_total = 15948ms`

Interpretation:

- This confirmed the duplicate-prefetch path was not the main bottleneck.
- Cold lane remains dominated by:
  1. `solver_solve`
  2. `prepare_extracted_roots`

#### After switching install tarball fetches to batch client

- `resolve_graph = 4975ms`
- `solver_solve = 4962ms`
- `prepare_extracted_roots = 10676ms`
- `adapter_install = 10854ms`
- `install_with_adapter_total = 15838ms`

Interpretation:

- Resolver improved materially in this sample (~5.0s vs ~6.0s previous run), but install phase regressed again.
- Net result: cold ultra-heavy lane is still not stable enough to claim a real product win.
- The main unresolved debt is still:
  - first-run resolver request/selection cost
  - install extract/materialization cost for large graphs

### What this means right now

- We did make real code changes, not just documentation updates.
- Those changes were safe and test-clean.
- They did **not** yet produce a consistently better cold ultra-heavy install.
- Next optimization work should target:
  1. deeper instrumentation inside `solver_solve`
  2. extraction/materialization path, not just tarball download plumbing

## Update - 2026-07-29 (streaming tarball lane)

### Code changes completed

- Added `DownloadedTarball` in `/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/native/npm_registry.rs`
  - `Bytes(Vec<u8>)`
  - `Streamed { computed_integrity, bytes_len }`
- Added `download_tarball_auto(...)`
  - small tarballs stay buffered
  - large tarballs stream directly to disk with inline SHA-512 integrity computation
- Upgraded install pipeline in `/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lib.rs`
  - `TarballFetchResult` now supports:
    - in-memory payloads
    - cached-path payloads
  - strict install can now extract from cached tarball path directly for large packages
  - large cold-path packages no longer have to go through the "read whole archive into RAM first" lane

### Validation

- `cargo test -p mg-web-adapter --lib` -> `56 passed`
- `cargo build --release -p mg` -> success

### Ultra-heavy cold profile after streaming lane

- `resolve_graph = 4308ms`
- `solver_solve = 4296ms`
- `prepare_extracted_roots = 9084ms`
- `adapter_install = 9265ms`
- `install_with_adapter_total = 13581ms`

### Delta vs previous profiled run

Previous:

- `resolve_graph = 4975ms`
- `prepare_extracted_roots = 10676ms`
- `adapter_install = 10854ms`
- `install_with_adapter_total = 15838ms`

Current:

- `resolve_graph = 4308ms`
- `prepare_extracted_roots = 9084ms`
- `adapter_install = 9265ms`
- `install_with_adapter_total = 13581ms`

Net read:

- resolver improved again
- install phase improved materially
- total cold ultra-heavy lane improved by roughly `2.2s` in this sample

This is the first pass in this sequence that looks like a real cold-path win instead of noise.

## Update - 2026-07-29 (execution-aware scaffold and command path)

### Code changes completed

- Added execution metadata to project config in `/Users/doanmihh/Documents/Workspace/MegaGate/core/crates/mg-config/src/project.rs`
  - `execution.architecture`
  - `execution.lane`
  - `execution.compatibility_layer`
  - `execution.native_targets`
- Updated scaffold template context in `/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/scaffold/processor.rs`
  - scaffolded `mg.toml` now carries execution strategy metadata for web projects
- Updated base web template:
  - `/Users/doanmihh/Documents/Workspace/MegaGate/templates/web/shared/partials/base/sources/web.toml`
  - `/Users/doanmihh/Documents/Workspace/MegaGate/templates/web/shared/partials/base/template.toml`
- Added execution-aware context helpers in `/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/context.rs`
  - commands can now read and summarize execution profile centrally
- Made runtime commands execution-aware:
  - `/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/commands/dev.rs`
  - `/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/commands/start.rs`
  - current behavior is informational, but the command path now consumes the shared execution contract

### Validation

- `cargo test -p mg-config` -> pass
- `cargo test -p mg test_create_web_writes_project_toml_for_monorepo -- --nocapture` -> pass
- `cargo test -p mg test_help_surface_matches_build_shape -- --nocapture` -> pass
- `cargo test -p mg test_available_cores_matches_build_shape -- --nocapture` -> pass

### Why this matters

- MegaGate web no longer treats native execution as a loose future note hidden in template partials.
- The project contract now has a shared execution story that scaffold, config loading, and command runtime can all see.
- This is the minimum viable foundation for future work like:
  - execution-aware `mg build`
  - `mg compile-web`
  - native bridge activation
  - multi-language execution lanes beyond JS/TS

## Update - 2026-07-29 (execution-aware build path)

### Code changes completed

- Updated `/Users/doanmihh/Documents/Workspace/MegaGate/cli/src/commands/build.rs`
  - `mg build` now reads execution metadata from project config
  - build path resolves a web build lane from:
    - explicit `--target`
    - otherwise `mg.toml -> execution.lane`
  - current supported internal lanes:
    - `compatibility-shell`
    - `native-ready`
    - `compiled-executable`
  - today, only compatibility-shell artifact build is fully wired
  - `native-ready` and `compiled-executable` are now explicit decision points instead of hidden future assumptions
- Added build unit tests for lane resolution

### Validation

- `cargo test -p mg build::tests:: -- --nocapture` -> pass
- `cargo test -p mg test_help_surface_matches_build_shape -- --nocapture` -> pass
- `cargo run -p mg -- build --help` -> pass

### What changed behavior-wise

- `mg build` now prints execution profile before building
- `mg build --target native`
  - resolves to compiled-executable lane
  - still falls back to compatibility-shell artifact generation in this phase
- `mg build` without `--target`
  - now follows project execution metadata rather than assuming one implicit lane forever

## Borrowed ideas worth adopting next

### Bun ideas worth borrowing

- isolated installs / strict linker shape
  - central package store + symlinked root exposure
  - peer-set-aware dedupe
- global virtual store
  - materialize once in shared cache, then relink fast across projects
- content-addressed transpiler cache
  - especially valuable for `mg dev`, `mg start`, and framework CLIs
- secure lifecycle default
  - dependency scripts blocked by default, explicit trust allowlist

### Vercel ideas worth borrowing

- artifact-oriented remote cache model
  - cache outputs by task/input hash, not just dependency blobs
- explicit cache invalidation policy
  - local/warm/shared/remote lanes should be separable and measurable
- monorepo-first cache semantics
  - only rebuild or rematerialize affected packages/apps

### MegaGate interpretation

- For MegaGate web, the strongest hybrid direction is:
  1. Bun-style strict isolated install/store semantics
  2. Bun-style shared/global transpiler cache for local DX
  3. Vercel-style task/artifact cache model for monorepo and CI lanes
  4. MegaGate-specific policy layer for security, multi-language runtimes, and memory control

## Update - 2026-07-29 (extracted cache self-heal)

### Root cause confirmed

- `entities@7.0.1.tgz` in shared tarball cache is correct and contains `dist/commonjs/*`.
- The failure lived in extracted package cache reuse:
  - cached extracted root had schema v2 marker
  - but marker content fields were empty:
    - `file_count = 0`
    - `unpacked_size = 0`
    - `file_tree_sha256 = ""`
- Fast-path validation reused that incomplete root, so Vue failed at runtime when Node resolved `dist/commonjs/decode.js`.

### Code changes completed

- Updated [/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lib.rs](/Users/doanmihh/Documents/Workspace/MegaGate/adapters/web/src/lib.rs)
  - added `extracted_marker_has_content_signature(...)`
  - extracted cache roots with schema v2 but missing content signature are no longer reusable
  - marker write path now always computes and stores full content signature
- Added regression test:
  - `test_install_rebuilds_schema_v2_root_when_marker_signature_is_missing`

### Validation

- `cargo test -p mg-web-adapter test_install_rebuilds_schema_v2_root_when_marker_signature_is_missing -- --nocapture` -> pass
- `cargo test -p mg-web-adapter test_install_rebuilds_cached_root_when_file_tree_is_incomplete -- --nocapture` -> pass
- runtime verify:
  - `target/debug/mg create-web vue-vite /private/tmp/mg-vue-check-3 --ts` -> pass
  - `target/debug/mg install-web` in `/private/tmp/mg-vue-check-3` -> pass
  - `target/debug/mg build --quiet` in `/private/tmp/mg-vue-check-3` -> pass
  - `target/debug/mg build --quiet` in `/private/tmp/mg-react-check` -> pass
  - `target/debug/mg build --quiet` in `/private/tmp/mg-next-check-2` -> pass

### Remaining technical debt after this fix

- `mg install-web` still does not accept a project path flag like `--project`; it must run from project root.
- Web default still relies on `legacy_flat` fallback.
- `strict` layout still needs deeper runtime audit before it can become the real product default.
