# Web Product Push - 2026-07-27

## Muc tieu

Day `core-web` len muc beta nghiem tuc hon bang cach:

- giam hanh vi "fake-ready"
- xoa wrapper/placeholder con sot trong runtime path
- siet local-dev safety
- ghi lai trung thuc nhung gi da dat va nhung gi chua dat

## Da xac nhan truoc khi sua tiep

- `mg/web` install/add/remove/build/list/why dang chay native, khong goi `npm/pnpm/bun/yarn`
- `mg dev` va `mg run` da chan script wrapper goi PM ben ngoai
- lifecycle scripts cua web adapter da chan wrapper PM ben ngoai
- `--audit-strict` da duoc noi vao web install path, khong con bi reject som o dispatch layer
- `cargo test -p mg-web-adapter` pass
- `cargo test -p mg audit_strict -- --nocapture` pass

## Dot fix nay

1. dọn template/runtime con bind `0.0.0.0` trong local dev path
2. thay placeholder UI/router con sot bang output co nghia hon
3. cap nhat report/doc de phan biet ro:
   - cai gi da native that
   - cai gi moi la scaffold/orchestration
   - cai gi chua du dieu kien de claim product-ready

## Da lam xong trong dot nay

- local source runtime cho:
  - fastify split/fullstack templates
  - node fastify backend templates
  - rust actix-web / axum backend templates
  - python fastapi / flask backend templates
  da bind ve `127.0.0.1` cho local dev
- Docker/container runtime van giu `0.0.0.0`
- React / Solid / Vanilla starter router khong con la placeholder; da render welcome screen co nghia, dung chung `config/framework.*`
- benchmark subset moi da chay xong:
  - file: `benchmark_brutal_results_20260727_205854.md`
  - lane pass: `7/7`
- bo ep `http1_only()` trong npm registry client de khong tu chan HTTP/2 neu registry/runtime ho tro
- benchmark cold/warm recheck sau thay doi network path:
  - file: `benchmark_brutal_results_20260727_210702.md`
  - lane pass: `2/2`
- da thu nghiem giu speculative prefetch chay detached trong strict layout
  - file benchmark: `benchmark_brutal_results_20260727_211124.md`
  - ket qua: cold lane khong dep hon, lane doi sanh bi fail warmup
  - xu ly: revert thu nghiem nay, khong dua vao beta state
- toi uu resolver de tai su dung ket qua `prefetch_versions(...)` thay vi goi `get_versions(...)` lap lai trong cung batch
- heavy cold benchmark sau toi uu resolver:
  - file: `benchmark_brutal_results_20260727_213749.md`
  - mg: `~30.433s`
  - bun: `~21.735s`
  - pnpm: `~17.471s`
  - doc dung: heavy first-run van thua kha ro
- them guard cho monorepo cold orchestration:
  - file: `cli/src/commands/core/web.rs`
  - neu la monorepo package-target cold install nhieu workspace cung luc, se ha concurrency xuong `1`
  - co the override bang `MEGAGATE_WEB_MONOREPO_INSTALL_CONCURRENCY`
- benchmark doi chieu sau guard:
  - file: `benchmark_brutal_results_20260727_215214.md`
  - `monorepo-install`
    - mg: `~33.2ms`
    - bun: `~266.0ms`
    - pnpm: `~267.2ms`
  - `heavy-empty-cache-install`
    - mg: `~24.256s`
    - bun: `~20.308s`
    - pnpm: `~17.742s`
  - doc dung:
    - monorepo orchestration tot hon ro
    - nhung heavy empty-cache global lane van chua dat muc canh tranh hon Bun/pnpm
- bo sung instrumentation cho heavy cold lane:
  - `adapters/web/src/lib.rs`
  - `adapters/web/src/native/npm_registry.rs`
  - `tools/core-web-lab/fixtures/heavy-web`
  - `benchmark.sh`
- giu lai:
  - `pipeline-profile` de tach ro `download`, `extract`, `queue_wait`, `io`
  - `network-profile` de xem retry path neu can
  - heavy fixture that trong repo de benchmark khong bi drift theo shell-script tam
- da thu nhung da rollback:
  - speculative strict-layout prefetch cho graph lon
  - scheduler uu tien direct/dependent package trong download pipeline
  - ly do: benchmark cold lane xau di, khong dua vao beta state
- benchmark/tuning them:
  - `benchmark_brutal_results_20260727_220544.md`
    - mg `~21.763s`
    - bun `~18.436s`
    - pnpm `~17.234s`
  - download concurrency tuning:
    - `24 -> ~20.504s`
    - `32 -> ~28.724s`
    - `48 -> ~22.408s`
  - metadata concurrency tuning:
    - `16 -> ~23.112s`
    - `24 -> ~22.960s`
    - `32 -> ~26.088s`
  - doc dung:
    - default `24` cho download va metadata dang tot nhat trong cac muc da do
    - lane nang dang nghen o download scheduler / queue wait, khong phai extract
- benchmark runner failure duoc tach ro:
  - `benchmark_brutal_results_20260727_223632.md`
   - lane `heavy-empty-cache-install` fail vi benchmark chay trong moi truong khong co registry access
   - reproduce truc tiep voi network that:
     - `mg install --core web --ignore-scripts` tren fixture `heavy-web`
     - ket qua: `86 packages`, `104456575 from cache`, `13517 ms total`
   - benchmark rerun co network:
     - `benchmark_brutal_results_20260727_223755.md`
     - mg `~21.583s`
   - doc dung:
     - can tach `engine time` voi `benchmark harness time`
     - fail local sandbox khong duoc dung de ket luan core-web bi crash
- dot sua sau do tap trung vao strict cold path:
  - `adapters/web/src/lib.rs`
  - thay doi:
    - tat `resolve` speculative tarball prefetch theo mac dinh; chi bat khi dat `MEGAGATE_WEB_RESOLVE_PREFETCH=1`
    - doi `pipeline_download_and_extract(...)` tu `spawn all + JoinSet` sang `buffer_unordered(...)` co backpressure
    - muc tieu: giam task storm, giam queue pressure, tranh strict-layout cold path vua spawn speculative prefetch vua tu download lai
  - verify:
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
    - `bash -n benchmark.sh`
  - benchmark moi:
    - `benchmark_brutal_results_20260727_224206.md`
    - `heavy-empty-cache-install`
      - mg: `~27.379s`
    - `heavy-empty-cache-install-direct`
      - mg: `~20.968s`
  - doc dung:
    - lane `direct` tach bot overhead copy/setup cua harness, gan voi engine hon
    - cold heavy path van chua dat muc canh tranh voi Bun/pnpm
    - viec them lane `direct` giup benchmark trung thuc hon, khong con tron "core cham" voi "harness cham"
- da thu them nhung rollback ngay:
   - gioi han `pipeline task concurrency` ve muc mac dinh bang `download_concurrency_limit()`
   - benchmark:
     - `benchmark_brutal_results_20260727_224557.md`
     - `heavy-empty-cache-install-direct`
       - mg: `~21.772s`
   - so voi moc truoc do:
     - `benchmark_brutal_results_20260727_224206.md`
     - `heavy-empty-cache-install-direct`
       - mg: `~20.968s`
   - ket luan:
     - task-cap experiment lam xau di direct cold lane
     - rollback, khong dua vao beta state
- resolver cold-path cleanup nho nhung co ich:
  - `core/crates/mg-resolver/src/solver/mod.rs`
  - bo prefetch batch trung lap sau `initial_prefetch_versions`
  - verify:
    - `cargo test -p mg-resolver` -> `16/16`
    - `cargo test -p mg-web-adapter --lib` -> `50/50`
  - benchmark:
    - baseline:
      - `benchmark_brutal_results_20260727_224206.md`
      - `heavy-empty-cache-install-direct` -> `~20.968s`
    - sau fix:
      - `benchmark_brutal_results_20260727_224906.md`
      - `heavy-empty-cache-install-direct` -> `~20.640s`
  - ket luan:
    - cai thien nho nhung that
    - giu lai
- da thu them o `prefetch_dependencies(...)` nhung rollback:
  - muc tieu:
    - gom metadata fetch theo `source package` thay vi theo tung `PackageId`
  - benchmark:
    - `benchmark_brutal_results_20260727_225219.md`
    - `heavy-empty-cache-install-direct` -> `~21.119s`
  - so voi baseline dang giu:
    - `benchmark_brutal_results_20260727_224906.md`
    - `heavy-empty-cache-install-direct` -> `~20.640s`
  - ket luan:
    - khong promotable
    - da rollback khoi source
- da thu them o strict `first materialization` nhung rollback:
  - muc tieu:
    - bo staging root thua o strict layout
    - bo prune root khi `node_modules` ban dau trong
  - benchmark:
    - `benchmark_brutal_results_20260727_225807.md`
    - `heavy-empty-cache-install-direct` -> `~25.543s`
  - so voi baseline dang giu:
    - `benchmark_brutal_results_20260727_224906.md`
    - `heavy-empty-cache-install-direct` -> `~20.640s`
  - ket luan:
    - xau hon ro ret
    - da rollback khoi source
    - khong promotable
- da thu them o strict dependency-linking cho leaf packages nhung rollback:
  - muc tieu:
    - tri hoan `mkdir` cho `pkg_local_node_modules` o cac package khong co deps
  - benchmark:
    - `benchmark_brutal_results_20260727_230223.md`
    - `heavy-empty-cache-install-direct` -> `~28.418s`
  - so voi baseline dang giu:
    - `benchmark_brutal_results_20260727_224906.md`
    - `heavy-empty-cache-install-direct` -> `~20.640s`
  - ket luan:
    - xau hon rat nhieu
    - da rollback khoi source
    - khong promotable
- da thu them o fresh virtual-store fast path nhung rollback:
  - muc tieu:
    - bo `installed_package_matches(...)` khi virtual store dang trong o first install
  - benchmark:
    - `benchmark_brutal_results_20260727_230526.md`
    - `heavy-empty-cache-install-direct` -> `~25.514s`
  - so voi baseline dang giu:
    - `benchmark_brutal_results_20260727_224906.md`
    - `heavy-empty-cache-install-direct` -> `~20.640s`
  - ket luan:
    - xau hon ro ret
    - da rollback khoi source
    - khong promotable

## Doc benchmark moi nhat

- `empty-cache-install`
  - mg: `~8.772s`
  - bun: `~8.389s`
  - pnpm: `~6.146s`
  - doc dung: MG van thua o cold empty-cache path
- `warm-install`
  - mg: `~225ms`
  - bun: `~1.692s`
  - pnpm: `~3.800s`
  - doc dung: MG thang rat ro o warm path
- `add-single-steady`
  - mg: `~43.6ms`
  - bun: `~54.1ms`
  - pnpm: `~770ms`
- `remove-single-steady`
  - mg: `~33.3ms`
  - bun: `~75.5ms`
  - pnpm: `~676.6ms`
- `list`
  - mg: `~105.3ms`
  - bun: `~888.1ms`
  - pnpm: `~2.215s`
- `build`
  - mg: `~147.9ms`
  - bun: `~1.386s`
  - pnpm: `~2.666s`
- `mg-create-web`
  - mg: `~1.868s`

## Ket luan tam thoi

- chua du trung thuc de goi la "product ngay bay gio" neu target la:
  - hon Bun o moi lane
  - hon pnpm ve toan bo lifecycle + security
  - da xong multi-language PM core thuc su
- du de goi la:
  - `core-web beta` nghiem tuc hon truoc
  - native web PM core dang tien nhanh
  - scaffold + steady path + command honesty da on hon ro

## Audit loop tiep theo nen tap trung

- cold empty-cache install van la nut that lon nhat
- can audit sau hon vao:
  - metadata fetch cost truoc resolver/install
  - tarball download scheduler va queue wait cua graph nang
  - su khac biet giua `heavy-empty-cache-install` va `heavy-empty-cache-install-direct`
  - monorepo aggregate resolve/materialize thay vi lap lai theo workspace target
  - lockfile/readiness path cho install lan dau
  - integrity/cache verification tren duong cold, khong chi warm path
  - heavy dependency graph path, khong chi subset nhe
  - can co them mot bang "giu / rollback / tiep tuc dao" de tranh lap lai thu nghiem da that bai
  - tuyen `first materialization` van can dao tiep, nhung khong theo huong staging/prune cleanup vua fail
  - tuyen `strict dependency-linking` can dao tiep, nhung khong theo huong leaf mkdir deferral vua fail
  - tuyen `fresh virtual-store fast path` khong nen dao lai theo cach vua thu

## Nguyen tac

- khong fake benchmark
- khong fake security
- khong claim "hon Bun/pnpm" neu so lieu hien tai chua chung minh
- sua o source sinh template truoc, sau do sua output template da ton tai
