# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.x     | ✅ |

## Reporting a Vulnerability

We take the security of mgpm seriously. If you believe you've found a security vulnerability, please report it to us as described below.

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, please report them via email to **security@megagate.dev** (create this email or use a real one).

You should receive a response within 48 hours. If for some reason you do not, please follow up via email to ensure we received your original message.

Please include the following information:
- Type of issue (e.g., buffer overflow, SQL injection, command injection, etc.)
- Full paths of source file(s) related to the manifestation of the issue
- The location of the affected source code (tag/branch/commit or direct URL)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the issue, including how an attacker might exploit it

## Preferred Languages

We prefer all communications to be in English.

## Policy

We follow coordinated disclosure:
1. Reporter reports vulnerability privately
2. We acknowledge receipt within 48 hours
3. We investigate and fix within 90 days
4. We release a security advisory and credit the reporter

## Scope

Only the latest stable release of mgpm is covered by this policy.

## Out of Scope

- Dependency confusion in packages not published by mgpm
- Vulnerabilities in packages installed by mgpm (report to the package maintainer)
- Theoretical attacks without proof of concept
- Attacks requiring physical access to the victim's machine
