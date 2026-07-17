# MegaGate Web Rust Strategy Rules

Ngay cap nhat: 2026-07-12

Tai lieu nay la rulebook chinh thuc cho huong phat trien `mg/web` theo tinh than:

1. Rust-first by default
2. JS/TS chi dung khi ecosystem boundary thuc su bat buoc
3. tat ca van nam trong `core-web` cua MegaGate

Tai lieu nay khong chi la brainstorming. Day la bo rule can duoc follow khi thiet ke, scaffold, materialize, test, va mo rong `mg/web`.

---

## 1. Muc tieu tong quat

`mg/web` phai huong toi:

- nhanh
- nhe
- an toan rat cao
- de bao tri
- de mo rong
- giam phu thuoc vao package manager / runtime ben ngoai o lop core

MegaGate khong duoc xay tu duy theo kieu:

- lay JS/TS lam loi compiler
- xem package manager / bundler ngoai la cot song bat bien
- nhet tat ca logic vao scaffold output
- xem template framework la su that kien truc

MegaGate phai co:

- core engine rieng
- output layer rieng
- compatibility layer rieng
- heavy-ready architecture mac dinh

---

## 2. Nguyen tac kien truc bat buoc

### Rule 1. Rust la ngon ngu chinh cua core web

Tat ca thanh phan sau phai uu tien viet bang Rust:

- scaffold compiler
- template materializer
- manifest transformer
- lockfile engine
- dependency resolver
- installer
- cache/store
- integrity / security checks
- package classification policy
- offline / online sync logic

Ly do:

- memory safety tot hon
- toc do native tot hon
- kiem soat he thong file tot hon
- de audit hon cho lop install / security

### Rule 2. JS/TS chi la compatibility layer khi can thiet

JS/TS duoc phep ton tai o:

- scaffold output cho user
- framework wiring
- router
- component
- hooks
- docs example
- compatibility surface voi ecosystem web

JS/TS khong duoc tro thanh:

- default engine cua MegaGate
- dependency resolution core
- installer core
- trust boundary chinh

Mac dinh:

- neu Rust lam duoc -> uu tien Rust
- neu browser / framework / ecosystem bat buoc -> moi dung JS/TS

### Rule 3. Rust-first van thuoc `core-web`, khong tach thanh core moi

Huong Rust-first cua web:

- van thuoc `megagate` -> `core-web`
- khong tao core moi ben ngoai he web
- khong bien no thanh mot san pham "khong lien quan den web core"

Dieu nay co nghia:

- lane ecosystem
- lane hybrid
- lane Rust-first

deu la cac lane ben trong `core-web`.

### Rule 4. Khong duoc gia vo Rust-first neu van la React template

Neu template la:

- `react-vite`
- `nextjs`
- `vue-vite`

thi output van phai ton trong ecosystem goc cua no.

Khong duoc:

- goi no la Rust-first
- mo ta no nhu thay the React
- trien khai lane half-Rust half-React mot cach mo ho

Neu da muon Rust-first that su thi phai tach boundary ro trong `core-web`, khong duoc mo ta mo ho nhu mot template React thong thuong.

### Rule 5. Hybrid lane la cau noi, khong phai dich cuoi

`react-vite + rust-wasm` ton tai de:

- hoc bridge giua Rust va web app
- giam logic nang trong JS
- thu nghiem cache / build / bridge / DX
- tai su dung logic Rust sang AI / game / cloud

Nhung theo huong phat trien hien tai:

- hybrid khong nen la "flag ma user phai nho"
- hybrid nen duoc xem la kien truc co the tro thanh mac dinh noi bo
- user co the chi thay mot lenh tao app don gian, con MegaGate tu chon cach to chuc phu hop

Hybrid lane khong duoc tro thanh:

- noi nhan moi loai logic khong ro boundary
- bai rac kien truc giua JS va Rust

---

## 3. Roadmap chinh thuc de uu tien

## Phase 1. Rust-first foundation + ecosystem compatibility

Uu tien:

- chot Rust-first rules cho `core-web`
- hoan thien ecosystem outputs (`react-vite`, `nextjs`, `vue-vite`)
- backend web can ban
- monorepo structure on dinh

Muc tieu:

- scaffold on dinh
- `mg create-web`
- `mg install-web`
- `mg add-web`
- `mg dev`
- structure output ro rang
- shared UI layer / shared config layer / shared content layer

Output trong phase nay duoc phep la JS/TS khi ecosystem can.

Core mac dinh van phai la Rust.

## Phase 2. Hybrid Rust-WASM / Rust-ready app structure

Them lane:

- `mg create-web react@latest <app> --rust-wasm`

Muc tieu:

- tao template hybrid chinh thuc
- co `crates/` hoac khu vuc Rust rieng cho web app
- xac dinh bridge API ro rang
- benchmark logic nang
- xac dinh cach debug, build, test, cache

Rule quan trong:

- hybrid lane van phai compiler ra output JS/TS theo ecosystem khi framework can
- user-facing app van la app React/Vite binh thuong
- gia tri cua Rust nam o:
  - memory handling tot hon
  - RAM pressure tot hon
  - logic nang tot hon
  - install / package strategy / node_modules strategy tot hon

- ve dai han, user khong nen bi buoc nho `--rust-wasm` moi lan
- dich den la architecture mac dinh co san chuan bi cho Rust lane

Phase nay phai tra loi duoc:

- Rust phu hop cho lop logic nao trong web app
- Wasm bridge cost co chap nhan duoc khong
- DX co du tot de ship product that khong

## Phase 3. Rust-First Web Lane ben trong Core-Web

Them lane:

- Rust-first lane ben trong `core-web`

Muc tieu:

- xay lane web Rust-first that su
- van thuoc `core-web`
- chi dung JS/TS neu browser hoac ecosystem buoc phai dung
- co spec rieng cho:
  - render model
  - routing
  - asset pipeline
  - style strategy
  - dev server
  - install model
  - package bridge

Phase 3 khong duoc bat dau full implementation neu phase 2 chua cho du du lieu.

---

## 4. Rule cho tung lane

## Lane A. Ecosystem outputs

Day la lane uu tien ship som.

### Bat buoc

- output chuan framework neu framework do duoc ho tro
- folder structure ro rang
- config / content / hooks / router / styles / assets tach rieng
- UI shared neu dung chung duoc
- khong hardcode content / link / brand rai rac
- JS/TS trong lane nay chi la lop ecosystem, khong duoc leo nguoc thanh core truth

### Khong duoc

- nhung logic MegaGate core vao app output
- de scaffold output phu thuoc implementation chi tiet cua engine
- dung JS/TS de xu ly viec dang ra Rust core phai lo

### Muc dich

- ship nhanh
- de adopt
- de test
- de benchmark DX

## Lane B. React + Rust-WASM / Rust-ready hybrid

Day la lane hybrid co chu dich.

### Bat buoc

- phai co boundary ro giua:
  - UI layer
  - bridge layer
  - Rust compute layer

- Rust chi nen di vao:
  - parser
  - transform
  - schema engine
  - diff engine
  - crypto
  - package analysis
  - local indexing
  - compute nang

### Khong duoc

- cho moi component goi WASM mot cach tuy tien
- de UI business logic phu thuoc manh vao glue code kho test
- lam lane nay chi de "cho co Rust"
- bien hybrid thanh thu ma user luon phai nho bang mot flag ky thuat kho hieu

### Muc dich

- tim diem can Rust that
- benchmark that
- tao cau noi sang Rust-first core

## Lane C. Rust-First Lane ben trong Core-Web

Day la lane chien luoc dai han.

### Bat buoc

- ten goi phai trung thuc trong `core-web`
- package / scaffold / docs phai tach boundary khoi lane React/Vite
- architecture proposal phai co truoc implementation lon
- Rust la ngon ngu uu tien mac dinh cho moi lop neu khong co constraint ecosystem chan lai

### Khong duoc

- nham no voi `react-vite`
- mo ta no la variant cua TS lane
- start coding full neu chua ro:
  - render model
  - hydration / no hydration
  - style pipeline
  - package bridge
  - build system

---

## 5. Rule ve folder structure

## Frontend ecosystem lane

Output toi thieu phai ro:

```text
src/
├── assets/
├── components/
├── config/
├── content/
├── hooks/
├── router/        # neu framework can
├── styles/
├── App.*
└── main.*
```

Neu la Next.js:

```text
src/
├── app/
├── assets/
├── components/
├── config/
├── content/
├── hooks/
└── styles/
```

## Monorepo lane

```text
apps/
├── frontend/
│   └── src/
│       ├── assets/
│       ├── components/
│       ├── config/
│       ├── content/
│       ├── hooks/
│       ├── router/
│       └── styles/
└── backend/
    └── src/
        ├── config/
        ├── lib/
        ├── routes/
        └── services/
```

## Hybrid lane

Khi co Rust-WASM:

```text
project/
├── src/
├── public/
├── crates/
│   └── engine/
│       ├── Cargo.toml
│       └── src/
└── ...
```

Rule:

- `crates/` khong duoc chen vo `src/`
- Rust phai nam o khu vuc rieng
- bridge phai minh bach
- neu chua build engine ngay, structure van nen heavy-ready de nang cap sau nay khong vo

---

## 6. Rule ve content / UI / branding

### Rule 6. Content khong hardcode rai rac

Tat ca gia tri sau phai duoc gom theo lop:

- brand config
- framework config
- content copy
- docs link
- github link

Vi du:

- `config/brand`
- `config/framework`
- `content/site-content`
- `hooks/useProjectLinks`

### Rule 7. UI shared phai o shared layer

Neu nhieu template frontend dung chung UI shell:

- dua vao shared partial
- leaf template chi giu framework-specific config / entry / router

Muc tieu:

- giam double code
- de custom UI ve sau
- khong de moi framework mot ban UI gan nhu giong nhau

---

## 7. Rule ve performance

### Rule 8. Rust chi can co mat o noi co benchmark justification

Neu dua Rust vao lane hybrid hoac Rust-first, phai co ly do:

- giam thoi gian xu ly
- giam memory pressure
- giam startup cost
- tang security / integrity

Khong duoc dua Rust vao chi de "cam thay xung tam".

Nhung khi co xung dot giua:

- Rust-first direction
- va ecosystem convenience

thi uu tien la:

1. giu core Rust-first
2. giam JS/TS toi muc can thiet
3. khong pha vo DX vo ich

### Rule 9. Moi lane moi phai co benchmark rieng

Can co benchmark cho:

- scaffold time
- install time
- add package time
- cold start
- hot dev loop neu co
- memory usage
- size tang them khi dua Rust-WASM vao

Khong duoc tuyen bo "nhanh hon" neu chua co so lieu.

---

## 8. Rule ve security

### Rule 10. Lop Rust phai giu trust boundary chinh

Nhung viec sau nen o phia Rust:

- integrity check
- lockfile generation
- cache reuse policy
- dependency classification
- install policy
- risky package handling

JS/TS khong nen la noi quyet dinh trust boundary sau cung.

### Rule 11. Rust-WASM khong duoc mo them attack surface vo to chuc

Neu co bridge:

- interface phai nho
- data vao / ra phai ro dang
- khong expose API vo han
- khong de user-facing app duoc truong quyen qua muc qua glue code

---

## 9. Rule ve DX va maintenance

### Rule 12. Moi lane phai co ly do ton tai

Khong mo them lane moi neu no:

- trung vai tro
- chi khac ten
- khong co boundary san pham ro

### Rule 13. Template output phai de doi UI sau nay

Phai uu tien:

- shared shell
- shared theme
- shared config
- shared content layer

De sau nay doi UI khong phai sua 20 file.

Dong thoi, template output phai:

- heavy-ready by default
- scale len project lon ma khong can dap di xay lai
- khong toi uu theo tu duy "demo nho"

### Rule 14. Doc phai noi that ve muc do on dinh

Moi lane can duoc gan ro:

- stable
- experimental
- internal
- benchmark lane

Khong duoc quang ba lane 2 hoac 3 nhu da on dinh neu chua dat.

---

## 10. Recommendation hien tai

Neu xet theo tinh hinh hien tai cua du an:

### Nen lam that:

1. hoan thien lane ecosystem (`react-vite`, `nextjs`, `vue-vite`) nhung giu rule Rust-first
2. viet spec va scaffold lane hybrid / Rust-ready cho React/Vite
3. viet architecture proposal cho Rust-first lane ben trong `core-web`

### Chua nen:

- full all-in cho Rust-first lane truoc khi co boundary va proposal ro
- nhet Rust vao lan man trong cac template ecosystem

---

## 11. Decision hien tai de follow

Cho den khi co rule moi duoc chap thuan, phai follow:

- core compiler = Rust
- core-web default direction = Rust-first
- ecosystem output = JS/TS chi khi can
- hybrid lane = duoc phep, nhung phai co boundary va benchmark
- Rust-first web = lane ben trong `core-web`, khong duoc mo ta mo ho

### Rule 15. Hybrid canonical command

Canonical command hien tai duoc chap thuan:

```text
mg create-web react@latest <app> --rust-wasm
```

Co the di kem them cac flag khac:

```text
mg create-web react@latest <app> --ts --tailwindcss --rust-wasm
```

Rule:

- output van la JS/TS ecosystem neu framework can
- Rust la lop engine / wasm layer
- khong duoc mo ta nhu mot template React da "bi thay the boi Rust"
- ve dai han, user co the khong can nho flag nay neu architecture mac dinh da gom san Rust lane

### Rule 16. Uu tien toi uu 1 frontend truoc

Truoc mat, hybrid phai duoc toi uu tren 1 frontend chuan truoc khi mo rong.

Frontend uu tien dau tien:

- React / Vite

Ly do:

- de benchmark ro
- de chot shared pattern
- de chot folder structure
- de chot bridge strategy
- de chot DX / install / cache / build assumptions

Next.js va Vue khong duoc mo rong hybrid som neu lane React/Vite chua on dinh.

Muoi mo rong sang frontend khac, can co:

- benchmark
- pattern da on
- lesson learned duoc doc hoa

---

## 12. Open rules can user chot them

Nhung diem duoi day chua nen tu doan. Can user xac nhan de chot thanh rule cung:

1. Hybrid lane co bat buoc tao `crates/engine` khong, hay cho phep ten khac?

2. Lane Rust-first co uu tien:
   - SSR
   - CSR
   - island / partial hydration
   - desktop-like local app shell
   o phase dau?

3. Muc benchmark toi thieu de chap nhan hybrid lane la gi?
   - startup
   - memory
   - install
   - compute latency

---

## 13. Chot tam thoi

Neu chua co quyet dinh moi tu user:

- Rust duoc xem la loi kien truc cua `mg/web`
- TS/JS duoc xem la lop compatibility khi can
- Hybrid lane la buoc chuyen tiep co chu dich
- Rust-first lane van thuoc `core-web`, nhung phai co boundary ro
