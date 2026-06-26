# Phase 1: Fetcher — Streaming + Connection Pool

## Goal
Fast, memory-efficient fetcher with connection pooling and streaming extract.

## Files to Create
- [ ] `src/fetcher/pool.ts` — undici Agent wrapper
- [ ] `src/fetcher/streamExtract.ts` — Zero-buffer streaming
- [ ] `src/fetcher/registry.ts` — Registry client (replace current)
- [ ] `src/fetcher/index.ts` — High-level Fetcher class

## Key Requirements

### Connection Pool (undici)
```typescript
const pool = new Agent({
  connections: 16,
  pipelining: true,
  keepAliveTimeout: 30000,
});
```

### Streaming Extract (NO full buffer in RAM)
```
HTTP Response → TransformStream (SHA-512 hash) → tar-fs extract → store
```
- Compute integrity WHILE downloading
- Extract directly to store path
- Never hold full tarball in memory

### Offline Mode
- `--offline`: fail if not in store
- `--prefer-offline`: try store first, then network

## Acceptance Criteria
- [ ] Download 100MB package without memory spike
- [ ] Integrity verified on-the-fly
- [ ] Pool reuses connections (verify with `netstat`)
- [ ] Retry with exponential backoff
- [ ] Timeout handling
- [ ] Unit tests: stream extract, integrity, pool, offline mode

## Commands to Test
```bash
pnpm test -- tests/unit/fetcher.test.ts
```

## Dependencies
- Phase 0, Phase 1 Store
- New deps: `undici`, `tar-fs`
