# Phase 1: CLI Commands — Task List

## Goal
Complete CLI with all package management commands.

## Files to Create
- [ ] `src/cli/commands/install.ts`
- [ ] `src/cli/commands/add.ts`
- [ ] `src/cli/commands/update.ts`
- [ ] `src/cli/commands/remove.ts`
- [ ] `src/cli/commands/list.ts`
- [ ] `src/cli/commands/verify.ts`
- [ ] `src/cli/commands/store.ts`
- [ ] `src/cli/commands/security.ts`
- [ ] `src/cli/commands/workspace.ts`
- [ ] `src/cli/index.ts` — Main entry (replace current cli.ts)

## Commands Spec

### Install
```
megagate install [--frozen-lockfile] [--production] [--offline] [--prefer-offline] [--ignore-minimum-age]
```

### Add
```
megagate add <pkg@version> [-D|--dev] [-O|--optional]
```

### Update
```
megagate update [pkg@version] [--latest]
```

### Remove
```
megagate remove <name>
```

### List
```
megagate list [-d|--depth <n>] [--json]
```

### Verify
```
megagate verify
```

### Store
```
megagate store path
megagate store prune
megagate store verify
```

### Security
```
megagate security approve-builds <pkg> [--script <name>]
megagate security audit [--format json|text]
```

### Workspace
```
megagate workspace run <script> [-r|--recursive] [--filter <selector>]
```

## Acceptance Criteria
- [ ] All commands work with correct flags
- [ ] Help text shows for each command
- [ ] Exit codes match SPEC
- [ ] JSON output for `--json` flags
- [ ] Integration test: CLI E2E

## Commands to Test
```bash
pnpm test -- tests/integration/cli.test.ts
```

## Dependencies
- All Phase 1 modules
