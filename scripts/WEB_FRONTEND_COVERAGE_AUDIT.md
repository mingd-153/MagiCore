# Web Frontend Coverage Audit

Ngay cap nhat: 2026-07-12

## Muc tieu

Tai lieu nay ghi lai muc do san sang hien tai cua frontend scaffolds trong `core-web`.

No tra loi 3 cau hoi:

1. Framework nao da co scaffold that su
2. Framework nao moi la placeholder
3. CLI hien tai se thanh cong hay fail-fast voi tung nhom

## Tieu chi danh gia

Mot frontend scaffold duoc xem la "production-shape" khi output co toi thieu:

- `package.json`
- entry file (`main.tsx`, `main.ts`, hoac `src/app/page.tsx`)
- app surface (`App.tsx`, `App.vue`, hoac `src/app/page.tsx`)
- router hoac app entry phu hop framework
- `src/config/*`
- `src/content/*`
- `src/hooks/*`
- `src/styles/*`
- `crates/engine/*`

## Ket qua single frontend

### Production-shape

- `vanilla`
- `react-vite`
- `nextjs`
- `vue-vite`

### Placeholder va da fail-fast

- `angular`
- `astro`
- `nuxt`
- `qwik`
- `solidjs`
- `sveltekit`

## Ket qua monorepo frontend

### Production-shape

- `vanilla`
- `react-vite`
- `vue-vite`

### Placeholder va da fail-fast

- `angular`
- `astro`
- `nextjs`
- `nuxt`
- `qwik`
- `solidjs`
- `sveltekit`

## Nhung gi da duoc siet trong vong nay

- `vue-vite` single frontend da du contract
- `vue-vite` monorepo frontend da du contract
- shared frontend da tach thanh:
  - `frontend-foundation` / `monorepo-frontend-foundation`
  - `frontend-common` / `monorepo-frontend-common`
- React-only shell khong con bi inject nham vao Vue scaffold
- frontend placeholder gio fail-fast thay vi tao project rong

## Vi du UX hien tai

### Dung

- `mg create-web react-vite my-app --ts`
- `mg create-web nextjs my-app --ts`
- `mg create-web vue-vite my-app --ts`
- `mg create-web react-vite my-mono --ts --monorepo --backend fastify`
- `mg create-web vue-vite my-mono --ts --monorepo --backend fastify`

### Se fail-fast

- `mg create-web angular my-app --ts`
- `mg create-web astro my-app --ts`
- `mg create-web nextjs my-mono --ts --monorepo --backend fastify`

## Goi y uu tien tiep theo

Neu tiep tuc mo rong frontend coverage, thu tu hop ly nhat la:

1. `vanilla`
2. `sveltekit`
3. `nuxt`
4. `solidjs`
5. `astro`
6. `qwik`
7. `angular`
8. `nextjs` monorepo lane neu van muon ho tro

Ly do:

- `vanilla` la nen tang nhe nhat de test contract chung
- `sveltekit` / `nuxt` / `astro` mo rong ecosystem nhanh
- `angular` nang hon va can contract rieng ro hon
- `nextjs` monorepo can quyet dinh kien truc truoc khi scaffold that
