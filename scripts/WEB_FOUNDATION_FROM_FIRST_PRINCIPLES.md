# MegaGate Web Foundation From First Principles

Ngay cap nhat: 2026-07-12

Tai lieu nay khong noi ve syntax template hay command surface truoc.
No tra loi 5 cau hoi nen tang neu `mg/web` duoc xay tu dau.

---

## 1. Package la gi?

Trong MegaGate, package khong nen duoc xem don gian la:

- mot thu can `download`
- mot thu nam trong `node_modules`
- mot entry trong `package.json`

Package nen duoc xem la:

- mot don vi noi dung co dinh danh
- mot node trong dependency graph
- mot object co metadata, integrity, version, va policy

He qua:

- storage phai doc lap voi project
- project chi nen materialize view can thiet
- `node_modules` khong nen la source of truth

---

## 2. Project la gi?

Project khong chi la mot folder co `package.json`.

Trong MegaGate, project nen duoc xem la:

- mot runtime target
- mot dependency view
- mot scaffold contract
- mot tap boundary giua code, config, policy, va materialized packages

He qua:

- project co the nho hoac lon
- nhung structure phai heavy-ready mac dinh
- project co the duoc tao cho ecosystem JS/TS
- nhung logic cot song van nen do Rust giu

---

## 3. Install la gi?

Install khong chi la "cai package".

Install nen duoc tach thanh:

1. resolve graph
2. fetch / verify package
3. store package
4. apply policy / integrity
5. materialize compatibility view cho project

He qua:

- install model co the toi uu doc lap voi output app architecture
- package management va web scaffold khong nen bi nham la mot

---

## 4. `node_modules` la gi?

Trong MegaGate, `node_modules` nen duoc xem la:

- compatibility projection
- ecosystem-facing filesystem view
- khong phai noi luu tru that su

He qua:

- core model phai song duoc ma khong phu thuoc vao `node_modules` nhu source of truth
- compatibility voi Node ecosystem van co the giu
- nhung MegaGate co quyen cai tien store/materialization model phia sau

---

## 5. Workspace la gi?

Workspace nen la first-class object.

No khong chi la:

- mot list apps
- mot list packages

Ma la:

- mot don vi cai dat
- mot don vi policy
- mot don vi chia se config/contracts
- mot runtime graph lon hon project don

He qua:

- monorepo khong nen duoc coi la addon
- structure phai ro ngay tu dau
- root va app/package con phai co boundary ro

---

## 6. Web scaffold la gi?

Web scaffold khong chi la bo template "tao app".

No nen la:

- mot contract cho growth
- mot heavy-ready structure
- mot ecosystem adapter
- mot cho dat chan cho Rust-first architecture

He qua:

- scaffold khong duoc toi uu cho demo nho ma lam hong duong scale sau nay
- UI, config, content, hooks, router, assets, styles phai tach ro
- Rust lane phai co cho de mo rong dan

---

## 7. Chot nen tang

Neu xay tu dau, `mg/web` nen follow:

- store la that
- `node_modules` la projection
- workspace la first-class
- core la Rust
- JS/TS la compatibility layer khi can
- scaffold mac dinh la heavy-ready

Day la nen tang de sau nay:

- hoc tu pnpm ma khong copy pnpm
- hoc tu yarn ma khong copy yarn
- hoc tu bun ma khong copy bun
- hoc tu vite ma khong copy vite

