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
| A page makes a native dialog say something else | Text from a page (browser name, project) is shown as one plain line: no line breaks, control or bidi characters, no markup; one site keeps at most three pairing dialogs waiting | `vf-core` tests, `attacks.rs` |
| A release build accepts other websites | Extra allowed origins work only in development builds; CI fails if a release build enables a development-only feature | `ci.yml` |
| A tampered interface | The fallback interface is served only from a bundle signed with a key built into the agent; redirects are not followed; CSP, `frame-ancestors 'none'` | `vf-server/tests/ui.rs` |
| A tampered update | Releases are signed with a separate key built into the agent; every binary is checked against the signed manifest; the new binary must start before it replaces the old one; `vibefork update --rollback` | `vf-updater/tests/release.rs` |
| A tampered toolchain download | `mise` is pinned to a version and SHA-256 per platform | `vf-toolchains` |
| Your API keys leak | Keys live in the OS secret store; they are not passed to other programs; the models, their commands and builds get only an allow-listed part of your environment (system, user folders, locale, proxy, certificates, toolchain homes), no tokens or keys; secrets are redacted in logs | `attacks.rs` (`secrets_never_reach_the_logs`), `vf-providers` (`env`, `tools`) |
| Your GitHub account is misused | The agent signs in to GitHub itself (device flow): the token never passes through a server or a web page; it is kept in the OS secret store. The Vibefork.me app can only write content to the copies you allow it on — no creating, deleting or administering repositories. The first save of a project is confirmed in the native dialog | `vf-github`, manual run |
| Your personal AI setup is used | Claude Code and Codex run without your user settings, MCP servers, plugins and rules | `vf-providers/tests/normalize.rs` |
| A project's files act on the agent | Git hooks, `core.fsmonitor`, external diff and signing are off for every git call; the agent's own model loop cannot leave the project or touch `.git` | `vf-git/tests/checkpoint.rs`, `vf-providers` (`tools`) |
| A page reads secrets through the code viewer | The viewer reads only inside a project the browser was granted; `..`, absolute paths and links out of the project are refused; git data and usual secret files (`.env*`, keys, certificates, `.npmrc`, `.netrc`) are never shown; Windows path aliases (`name::$DATA`, trailing dot or space) are refused; files over 2 MiB are described, not sent | `vf-engine/tests/engine.rs`, `attacks.rs` |
| A model or a project reads your other files | Landlock on Linux, Seatbelt on macOS for the CLI, its commands and builds | `vf-sandbox/tests/linux.rs` (canary), `vf-sandbox/tests/macos.rs` |
| Malformed input crashes or misleads the agent | Parsers of untrusted input are fuzzed | `vf-engine/tests/robustness.rs`, `fuzz/` |
| Another user of the machine | Nothing works without pairing; the agent's folders (data, projects, tools, caches, artifacts — on any disk) are owner-only: Unix `0700`, on Windows a protected DACL with only you and SYSTEM; `vibefork doctor` checks it | `vf-platform` (`access`) |

## Known limits

- **Isolation on Linux and macOS.** On Linux (Landlock, no root) and macOS (Seatbelt, no root) the
  coding CLI, everything it runs, the agent's own model loop and builds see only the project, the
  agent's tools and caches, system folders and the CLIs' own data; the rest of your home is closed.
  Network is open. On Windows these processes still have your user's rights. The level is reported
  with every task.
- Package installation scripts (`npm`, `pip`) and build commands run with your user's rights on Windows without isolation; on Linux and macOS builds run inside the sandbox.
- On Windows an AppContainer sandbox exists but is off by default (`VF_SANDBOX_WINDOWS=appcontainer`
  turns it on): Git Bash, PowerShell 7 and Codex CLI do not start in it, and Node.js fails on
  projects outside the system drive.
- Inside the sandbox the coding CLI's own sign-in data is readable by the commands it runs.
- Windows Authenticode and Apple notarization signatures are not set up yet; releases are verified
  by the agent's own signature.
