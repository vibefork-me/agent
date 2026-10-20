# Security

The agent runs AI models and builds on your machine and is controlled from a web page, so it is
built to be safe to leave running. Invariants are in [ARCHITECTURE.md](ARCHITECTURE.md#invariants).

## Reporting

Report vulnerabilities privately through GitHub: **Security → Report a vulnerability** in this repository. Please do not open public issues for them.

## What protects you

| Threat | Measure | Where it is tested |
|---|---|---|
| A website or another program reaches the agent | Loopback only; `Host` check (DNS rebinding); `Origin` allowlist; JSON + `x-vibefork` header (forces a CORS preflight); body limit | `vf-server/tests/attacks.rs` |
| A website pretends to be your browser | Pairing only through the agent's own dialog with a code; non-extractable browser key; signed one-time challenge → 15-minute session bound to the origin; revocation | `attacks.rs` |
| A paired page (or a hacked site) does more than you want | Native dialog for building, rolling back, and for each project the browser may use | `attacks.rs` |
| A tampered interface | The fallback interface is served only from a bundle signed with a key built into the agent; redirects are not followed; CSP, `frame-ancestors 'none'` | `vf-server/tests/ui.rs` |
| A tampered update | Releases are signed with a separate key built into the agent; every binary is checked against the signed manifest; the new binary must start before it replaces the old one; `vibefork update --rollback` | `vf-updater/tests/release.rs` |
| A tampered toolchain download | `mise` is pinned to a version and SHA-256 per platform | `vf-toolchains` |
| Your API keys leak | Keys live in the OS secret store; they are not passed to other programs; secrets are redacted in logs | `attacks.rs` (`secrets_never_reach_the_logs`), `vf-providers` (`env`) |
| Your personal AI setup is used | Claude Code and Codex run without your user settings, MCP servers, plugins and rules | `vf-providers/tests/normalize.rs` |
| A project's files act on the agent | Git hooks, `core.fsmonitor`, external diff and signing are off for every git call; the agent's own model loop cannot leave the project or touch `.git` | `vf-git/tests/checkpoint.rs`, `vf-providers` (`tools`) |
| Malformed input crashes or misleads the agent | Parsers of untrusted input are fuzzed | `vf-engine/tests/robustness.rs`, `fuzz/` |
| Another user of the machine | Nothing works without pairing; the agent's folder is owner-only | — |

## Known limits

- **No isolation yet.** Builds and commands run by a model have your user's rights; the agent keeps
  them in the project folder by default, but a malicious project or a model can reach other files.
  Process isolation is planned; the `Sandbox` interface is in place.
- Windows Authenticode and Apple notarization signatures are not set up yet; releases are verified
  by the agent's own signature.
