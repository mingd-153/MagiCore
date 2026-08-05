# Agent Skills

This project currently expects two external Codex skills to be available and
used as working defaults when appropriate:

## Installed

1. `ponytail`
   - Source: `DietrichGebert/ponytail`
   - Installed to: `~/.codex/skills/ponytail`
   - Purpose: bias implementation toward simpler, smaller, lower-boilerplate
     solutions and push back on unnecessary complexity.

2. `caveman`
   - Source: `JuliusBrussee/caveman`
   - Installed to: `~/.codex/skills/caveman`
   - Purpose: keep communication terse and efficient when a shorter response
     style helps.

## Project Expectation

- For MegaGate work, prefer the `ponytail` mindset when choosing structure,
  abstractions, and dependency additions.
- Use `caveman` only when brevity is genuinely useful; do not sacrifice
  accuracy or safety-critical clarity.
- These skills are installed in the Codex environment, not vendored into the
  repository source tree.

## Next-Step Note

- If we want repo-local reproducibility later, we can add a bootstrap helper
  that verifies these skills exist before starting a MegaGate agent session.
