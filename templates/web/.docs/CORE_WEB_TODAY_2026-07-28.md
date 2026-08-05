# Core Web Today - Tuesday, July 28, 2026

## Muc tieu trong ngay

- siet lai product surface de ban release hien tai trung thuc la `core-web`
- tiep tuc dao vao `heavy-empty-cache-install-direct`
- chi giu cac toi uu co:
  - correctness xanh
  - benchmark thuc te di len
  - khong fake claim

## Nhung thay doi da GIU

### 1. Product surface chi cong bo `web`

- `cli/src/factory.rs`
  - `available_cores()` chi con cong bo `web`
- `cli/src/main.rs`
  - an help surface cua:
    - `create-game`
    - `create-ai`
    - `create-clo`
    - `create-cicd`
    - `create-iot`
    - `create-app`
    - `create-lib`
    - va cac lenh `install-*`, `add-*`, `remove-*`, `list-*`, `update-*` cua non-web cores
- y nghia:
  - product hien tai khong con tu vo hua hoang ve 7 core chua implement

### 2. Fix regression strict layout voi package scoped

- root cause:
  - lay `parent()` tu `strict_vstore_package_dir(...)`
  - sai voi package scoped nhu `@nuxt/kit`
- da sua:
  - tach `strict_vstore_node_modules_dir(...)`
  - dependency-link phase quay lai dung muc `.../.megagate/<pkg>@<ver>/node_modules`
- y nghia:
  - nested dependency linking dung tro lai

### 3. Cat staging churn thua trong strict path

- truoc day:
  - strict install van tao `staging_root/node_modules`
  - nhung khong he dung toi
- da sua:
  - chi tao `staging_root` khi `legacy_flat == true`
- ket qua:
  - giam fs churn o first-run cold lane

### 4. Cat DB I/O thua trong strict path

- truoc day:
  - strict install van `Database::open(...)`
  - van `insert_package(...)`
- nhung:
  - strict materialization hien tai khong dua vao DB nay
- da sua:
  - chi mo/ghi DB trong nhanh `legacy_flat`
- ket qua:
  - them mot buoc giam I/O nen o first-run

## Nhung thay doi da BO / ROLLBACK

### 1. Siat task concurrency mac dinh cua pipeline

- da thu:
  - doi default `pipeline_task_concurrency_limit()` thanh `max(download, extract)`
- test:
  - pass
- benchmark:
  - xau hon baseline dang giu
- ket luan:
  - rollback
  - khong lap lai huong nay y chang

### 2. Cat cache-read khi ghi `installing` lockfile state

- da thu:
  - chi bo sung integrity tu cache khi ghi state `locked`
  - bo qua bu integrity tu cache o state `installing`
- test:
  - pass
- benchmark:
  - `benchmark_brutal_results_20260728_173218.md`
  - `20.370s +- 1.692s`
- ket luan:
  - xau hon baseline dang giu
  - rollback
  - khong giu huong nay

### 3. Chi mo lockfile cache khi graph co package thieu integrity

- da thu:
  - chi `PackageCache::new(...)` neu graph co package `integrity.is_empty()`
- test:
  - pass
- benchmark:
  - `benchmark_brutal_results_20260728_173555.md`
  - `22.244s +- 2.404s`
- ket luan:
  - xau hon baseline dang giu
  - rollback
  - khong giu huong nay

## Benchmark quan trong trong ngay

### Moc benchmark dang GIU hien tai

- file:
  - `benchmark_brutal_results_20260728_172050.md`
- lane:
  - `heavy-empty-cache-install-direct`
- ket qua:
  - `18.711s +- 1.797s`
  - range: `17.287s .. 20.730s`
  - `3 runs`

### Cac moc de doc dung trong ngay

- regression-sua-xong nhung chua nhanh hon:
  - `benchmark_brutal_results_20260728_162642.md`
  - `21.995s +- 1.390s`
- sau khi cat staging churn:
  - `benchmark_brutal_results_20260728_165509.md`
  - `19.759s +- 1.201s`
- sau khi cat DB I/O strict path:
  - `benchmark_brutal_results_20260728_172050.md`
  - `18.711s +- 1.797s`
- thu cat cache-read o `installing` lockfile state roi rollback:
  - `benchmark_brutal_results_20260728_172050.md`
  - `benchmark_brutal_results_20260728_172556.md`
  - `19.473s +- 1.742s`
- thu cat cache-read o `installing` lockfile state roi rollback:
  - `benchmark_brutal_results_20260728_173218.md`
  - `20.370s +- 1.692s`
- thu chi mo lockfile cache khi graph thieu integrity roi rollback:
  - `benchmark_brutal_results_20260728_173555.md`
  - `22.244s +- 2.404s`

## Test / verification da qua

- `cargo test -p mg-web-adapter --lib`
  - `53 passed`
- `cargo test -p mg-resolver`
  - `16 passed`
- `cargo test -p mg test_available_cores_matches_build_shape -- --nocapture`
  - pass
- `cargo test -p mg test_help_surface_matches_build_shape -- --nocapture`
  - pass

## Cach doc trung thuc den cuoi ngay

- core-web hom nay:
  - sach hon
  - dung hon
  - product surface trung thuc hon
  - cold heavy lane da giam duoc that
- nhung:
  - chua du de claim `faster than Bun`
  - chua du de claim `better than pnpm` tren toan bo lifecycle
  - chua du de goi la product final
- posture dung hien tai:
  - `core-web beta`
  - native Rust-first
  - warm/steady path rat on
  - cold path dang giam nhung van la blocker lon nhat

## No ky thuat con lai lon nhat

### 1. Metadata first-run

- van can dao tiep:
  - metadata fetch cost
  - serialization/caching pressure
  - truong hop alias-heavy / large graph

#### Profile truc tiep tren fixture `heavy-web` - Tuesday, July 28, 2026 17:35

- chay truc tiep:
  - `MEGAGATE_WEB_PROFILE_INSTALL=1 mg install --core web --ignore-scripts`
  - tren fixture `tools/core-web-lab/fixtures/heavy-web`
  - empty shared cache
- ket qua command-level:
  - `resolve_graph=9163ms`
  - `adapter_install=12835ms`
  - `install_with_adapter_total=22004ms`
- ket qua install-level:
  - `prepare_extracted_roots=12491ms`
  - `materialize_dependency_graph=12771ms`
  - `write_lockfile=12825ms`
- ket qua pipeline:
  - `packages=643`
  - `bytes=104449946`
  - `download_ms_total=173121`
  - `download_ms_max=10047`
  - `extract_ms_total=6707`
  - `extract_ms_max=801`
- cac download cham nhat:
  - `@next/swc-darwin-arm64@14.2.33` -> `10047ms`
  - `next@14.2.35` -> `7639ms`
  - `webpack@5.109.1` -> `5286ms`
- cach doc:
  - `resolve_graph` dang an khoang `9.1s`
  - install phase dang an khoang `12.8s`
  - trong install phase, download/network van nang hon extract rat nhieu
  - vay nen 2 diem dung de dao tiep la:
    - metadata/resolve first-run
    - download scheduling / large tarball path

### 2. Download scheduling thuc su

- can tiep tuc tim:
  - overlap hop ly ma khong tranh bandwidth voi tuyen prefetch khac
  - queue-wait o graph nang
  - first-run download ordering

### 3. Lockfile / install-state writes

- van can xem lai:
  - co can `installing` state tren strict path trong moi tinh huong hay khong
  - co the cat them write amplification nao nua khong

## Nguyen tac tiep tuc follow

- khong fake benchmark
- khong fake product-readiness
- khong giu optimization neu benchmark xau di
- uu tien thay doi:
  - nho
  - do duoc
  - rollback de
  - an toan correctness
