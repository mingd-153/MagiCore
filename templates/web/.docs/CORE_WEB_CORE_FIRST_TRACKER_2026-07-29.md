# Core Web Core-First Tracker (2026-07-29)

## Muc tieu cua file nay

File nay dung de theo doi dung nhac:

1. fix `core-web` truoc
2. sau khi `core-web` on dinh moi mo rong sang framework
3. moi viec phai co checklist, co ket qua test that, co ghi chu ro de tranh miss

## Nguyen tac lam viec

- Khong nhay vao polish framework khi core con sai.
- Khong tinh la xong neu chi sua CLI surface ma runtime that van fail.
- Moi flow phai di theo thu tu:
  - `mg create-web`
  - `mg install-web`
  - `mg dev`
  - `mg build`
  - neu co native lane thi test them `mg build --target native`
- Moi framework pass phai co output that, khong danh dau theo cam giac.

## Dinh nghia "core on"

`core-web` chi duoc xem la on khi cac dieu kien sau deu dat:

- [ ] install path khong bi vo resolver/runtime voi package manager layout
- [ ] `mg dev` khong can wrapper PM ben ngoai
- [ ] `mg build` biet chon dung build lane theo framework/runtime
- [ ] `mg start` phuc vu dung artifact vua build ra
- [ ] layout install khong lam gãy peer deps / nested deps / bin scripts
- [ ] lockfile + cache + materialization khong tu sinh output sai theo lan chay
- [ ] test unit + test runtime toi thieu deu xanh

## Phase A - Khoa core truoc

### A1. CLI / execution surface

- [x] Chuan hoa host frontend `localhost:4315`
- [x] Chuan hoa host backend `localhost:3415`
- [x] `mg dev` doc execution profile
- [x] `mg build` doc execution profile
- [x] `mg start` doc execution profile
- [ ] Loai bo cac default / help / docs con lech so voi flow core-moi

### A2. Install / materialization / cache

- [x] Da xac nhan co 2 layout:
  - `strict`
  - `legacy_flat`
- [x] Da doi web default tam thoi sang `legacy_flat`
- [ ] Audit lai toan bo strict materialization cho:
  - peer deps
  - nested deps
  - framework runtime resolution
  - `.bin` scripts
- [ ] Tim va fix tan goc ly do `strict` lam Vue / Next fail runtime resolution
- [ ] Xac nhan cache reinstall khong de lai state sai sau nhieu lan install
- [x] Core co co che tu-heal extracted cache marker thieu content signature
- [ ] Xac nhan `mg.lock` va node_modules sinh ra on dinh qua 3 lan install lien tiep

### A3. Build / dev runtime core

- [x] `mg build` da co framework-aware lane cho mot so FE
- [ ] Ra soat lai mapping build lane cho:
  - Vite family
  - Next
  - Nuxt
  - Astro
  - Angular
- [x] Xac nhan `mg dev` va `mg build` dung chung mot logic runtime-hop-le cho React / Vue / Next dai dien
- [ ] Xac nhan framework binary local khong bi chay lech root vi symlink/cache path
- [ ] Them test regression cho launcher / symlink-preserve / local binary resolution

## Phase B - Framework matrix sau khi core dat nguong

Chi bat dau phase nay khi Phase A da du nguong toi thieu.

### B1. Frontend dai dien

- [x] Vanilla
  - create/install/dev/build/native build da verify
- [x] React Vite
  - create/install/build da verify
- [x] Next.js
  - create/install/build da verify
- [ ] Vue Vite
  - create/install/build da verify lai sau khi fix extracted cache self-heal
- [ ] SvelteKit
- [ ] Nuxt
- [ ] Astro
- [ ] Angular
- [ ] Qwik
- [ ] Solid
- [ ] Remix

### B2. Tieu chi pass cho moi frontend

- [ ] `mg create-web <fw> app --ts` pass
- [ ] `mg install-web` pass
- [x] `mg dev --host localhost --port 4315` len duoc HTTP 200 cho dai dien React
- [x] `mg dev --host localhost --port 4316` len duoc HTTP 200 cho dai dien Vue
- [x] `mg dev --host localhost --port 4317` len duoc HTTP 200 cho dai dien Next
- [ ] `mg build --quiet` pass
- [ ] `mg start` len duoc HTTP 200 neu artifact co the serve local
- [ ] folder scaffold sinh ra ro rang:
  - `src`
  - `assets`
  - `router`
  - `hooks`
  - `config`
  - `content`
  - `pages` neu phu hop
- [ ] UI scaffold co noi dung toi thieu dung va khong de vo layout

## Phase C - Sau frontend moi toi backend / fullstack / monorepo

### C1. Backend

- [ ] Fastify
- [ ] Express
- [ ] Hono
- [ ] NestJS
- [ ] Flask
- [ ] FastAPI
- [ ] Django
- [ ] Gin
- [ ] Fiber
- [ ] Echo
- [ ] Actix Web
- [ ] Axum
- [ ] Spring Boot
- [ ] Quarkus
- [ ] Laravel
- [ ] Symfony

### C2. Fullstack / Monorepo

- [ ] Split fullstack
- [ ] Monorepo FE + BE
- [ ] Shared package / schema layer
- [ ] Root manifest / workspace behavior
- [ ] Local dev orchestration

## No ky thuat dang mo ngay luc nay

### 1. Core materialization van con viec, nhung da sua dung 1 loi goc

Trang thai that:

- React Vite build pass
- Next build pass sau khi fallback ve layout phu hop hon
- Vue Vite da build pass tren project moi sau khi sua extracted cache reuse

Dieu nay cho thay:

- Loi vua sua nam o `core-web` extracted package cache validation
- Tarball that cua `entities@7.0.1` dung, nhung extracted cache cu co marker schema v2 thieu content signature nen bi reuse sai
- Co che moi se buoc rebuild khi marker nhanh khop nhung thieu content signature

### 2. Strict layout peer dep — DA XU LY

Root cause: `get_dependencies` khong include `peerDependencies` tu npm registry
→ solver khong resolve peer deps → khong co trong graph → khong co trong vstore
→ strict layout khong the link peer dep vao `node_modules/.megagate/` → runtime fail.

Fix: `collect_resolved_deps` nhan `peer: bool`, `get_dependencies` / `prefetch_dependencies`
collect `peer_dependencies` tu `VersionInfo` va tra ve `ResolvedDep { peer: true }`.
Solver enqueue peer deps nhu regular deps → vstore co peer dep → strict layout link duoc.

Trang thai that:

- strict layout co y tuong tot
- da co peer dep trong solver output + graph + vstore
- can test runtime that voi `MEGAGATE_WEB_STRICT_LAYOUT=1`
- van chua du dieu kien de lam web default product (can them runtime regression test)

### 3. Build lane da tot hon, nhung chua xong

Trang thai that:

- `mg build` da khong con ep tat ca frontend vao esbuild lane chung
- nhung mapping va launcher van con can regression test tiep

## Thu tu uu tien tiep theo

1. verify `mg dev` thuc te cho React / Next / Vue
2. audit tiep strict layout thay vi chi fallback `legacy_flat`
3. verify cache reinstall / repeated install qua 3 lan lien tiep
4. khi 3 FE dai dien on roi moi mo rong sang FE khac
5. sau do moi qua BE / fullstack / monorepo

## Log ket qua gan nhat

### Da xong

- [x] Tao tracker rieng cho huong `core truoc, framework sau`
- [x] Chot lai quy tac lam viec tranh miss cong viec
- [x] Xac nhan Vue fail hien tai la loi core materialization/runtime resolution, khong phai chi do thieu UI
- [x] Xac nhan tarball `entities@7.0.1` that co day du `dist/commonjs/*`
- [x] Xac nhan extracted shared cache cu bi reuse voi marker schema v2 nhung `file_count=0`, `unpacked_size=0`, `file_tree_sha256=\"\"`
- [x] Sua core de marker thieu content signature khong duoc tai su dung nua
- [x] Doi marker write path sang luon ghi day du content signature
- [x] Them regression test cho extracted cache schema v2 thieu signature
- [x] Verify runtime that:
  - `mg create-web vue-vite /private/tmp/mg-vue-check-3 --ts` -> ok
  - `mg install-web` -> ok
  - `mg build --quiet` -> ok
  - `mg build --quiet` tren `/private/tmp/mg-react-check` -> ok
  - `mg build --quiet` tren `/private/tmp/mg-next-check-2` -> ok
- [x] Verify `mg dev` runtime that:
  - React native MgDevServer tren `127.0.0.1:4315` -> `HTTP 200`
  - Vue native MgDevServer tren `127.0.0.1:4316` -> `HTTP 200`
  - Next dev server tren `127.0.0.1:4317` -> `HTTP 200`
- [x] Xac nhan them mot state-bug cua Next cung thuoc nhom extracted cache marker cu:
  - `nanoid@3.3.16` trong chain `next -> postcss` co marker schema v2 nhung `file_count=0`, `unpacked_size=0`, `file_tree_sha256=""`.
  - Reinstall bang core moi da tu-heal va xoa loi `500` khi `GET /`.

### Can lam tiep ngay

- [ ] Lap bai test reinstall 3 lan lien tiep va so sanh `mg.lock` + node_modules
- [ ] Test runtime that strict layout: `MEGAGATE_WEB_STRICT_LAYOUT=1 mg install-web && mg dev`
- [ ] Test strict layout cho React/Vue/Next de xac nhan peer dep fix hoat dong
