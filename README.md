<div align="center">
  <img src="assets/logo-full.svg" alt="MagiCore" width="240" />
  <h1>MagiCore</h1>
  <p><strong>A Rust CLI for project and dependency workflows across multiple cores</strong></p>
  <p><a href="https://github.com/mingd-153/MagiCore/actions/workflows/ci.yml">CI</a> · <a href="https://github.com/mingd-153/MagiCore/pulls">Pull requests</a> · <a href="LICENSE">MIT license</a></p>
</div>

---

MagiCore (<code>mgc</code>) is a Rust CLI and workspace for scaffolding projects and coordinating dependency, test, build, audit, and release workflows. The CLI exposes nine core families. **Support and maturity differ by core, framework, and operation.**

## Current status

- Workspace version: **1.1.0-rc.9**; Rust edition 2024; minimum Rust version 1.86.
- This is a pre-release, not a production-stable or drop-in replacement claim.
- **Historical verification snapshot, 2026-10-06 12:24 +07:00, SHA `212a139c`:** GitHub showed PR [#3](https://github.com/mingd-153/MagiCore/pull/3) open and not merged into <code>main</code>, with 5 workflow runs and 41/41 jobs successful on the PR integration ref; GitHub reported the PR mergeable. That result applies to the listed SHA only; check the live PR for this checkout's status.
- Green CI applies to declared lanes; it does not qualify every framework or operation.

## What MagiCore provides

- Core-aware project scaffolding and capability reporting.
- Dependency workflows: install, add, remove, update, and list.
- Project test, build, run, verify, and audit commands where supported.
- Lockfile import, SBOM generation, registry publishing, workspace inspection, and cache management.
- A native Model Context Protocol server: <code>mgc mcp</code>.

Use <code>mgc capabilities</code> to inspect the capability manifest and <code>mgc &lt;command&gt; --help</code> for options in this build.

## Core support and CI evidence

The lifecycle matrix passed its declared release-blocking lanes on Ubuntu, macOS, and Windows. This is not a claim that every framework in a core family has a complete lifecycle.

| Core | Current evidence and limits |
|---|---|
| Web | JavaScript, Node, TypeScript, and vanilla lifecycle lanes passed. Other frameworks may have scaffold-only or narrower evidence. |
| AI | The Python lifecycle lane passed. Agent and MCP-server scaffolds do not inherit a full lifecycle guarantee. |
| App | Flutter lifecycle passed. Swift E2E used a fallback project on all three OS because the distributed/registry scaffold was unavailable; that does not verify the Swift scaffold. Objective-C and React Native are not release-qualified. |
| Lib | Rust, Python, TypeScript, and Go lanes passed. Java and .NET remain partial or outside release scope. |
| Cloud (<code>clo</code>) | CDK and Pulumi lanes passed. Terraform dependency operations are outside the current supported scope. |
| CI/CD (<code>cicd</code>) | GitHub Actions is a workflow scaffold; install/test/build execute on the external CI provider. |
| Game | The Rust lane is partial; package lifecycle support varies by framework. |
| IoT | The Rust lane is partial; board/toolchain combinations are not all release-qualified. |
| Hardware | Optimizer and benchmark tooling; not a general package lifecycle. |

The Native Dependency Lifecycle Evidence Matrix covers selected MagiCore-native dependency-install lanes only. Check <code>mgc capabilities</code> and workflow artifacts for lane-level detail.

## Build from source

Requirements: Rust 1.86 or newer.

~~~bash
git clone https://github.com/mingd-153/MagiCore.git
cd MagiCore
cargo build --release --bin mgc --locked
./target/release/mgc --version
./target/release/mgc capabilities
~~~

Add <code>target/release</code> to your PATH or install the binary using your platform's normal method. Tests/builds that need a language SDK still require that SDK.

## Quick start

After making <code>mgc</code> available on PATH:

~~~bash
mgc create-web react my-app --yes
cd my-app
mgc install
mgc test
mgc build
mgc audit
~~~

Other scaffold command families include <code>create-ai</code>, <code>create-app</code>, <code>create-lib</code>, <code>create-clo</code>, <code>create-cicd</code>, <code>create-game</code>, <code>create-iot</code>, and <code>create-hardware</code>. Check capabilities before assuming a framework supports each operation.

## Verification and performance

The five PR workflows cover CI, resolver archive security, lifecycle capability evidence, release archive E2E, and selected native dependency lanes. See the [CI workflow](.github/workflows/ci.yml), [lifecycle workflow](.github/workflows/lifecycle-matrix.yml), and [release E2E workflow](.github/workflows/release-binary-e2e.yml).

This README makes no speed comparison. Do not claim an advantage over npm, pnpm, Bun, or another tool without current validated benchmarks and a published method/baseline.

## Repository layout

- <code>cli/</code> — command-line application.
- <code>core/crates/</code> — shared Rust foundations.
- <code>adapters/</code> — core-specific behavior.
- <code>templates/</code> — project scaffolds and template fragments.
- <code>tests/</code> — integration and end-to-end tests.
- <code>.github/workflows/</code> — CI and release verification.

## Contributing

Open a focused pull request with relevant unit, integration, and lifecycle evidence. Follow [AGENTS.md](AGENTS.md) and [RULE.md](RULE.md); keep public support claims aligned with CI lane verdicts.

## License

MIT. See [LICENSE](LICENSE).
