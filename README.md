# Vibefork agent

The local agent of [vibefork.me](https://vibefork.me): make your own change in any open-source
project with the AI you already have — on your machine.

> **Status: pre-beta.** Projects, journal, providers (Claude Code, Codex, OpenAI-compatible), Auto / Plan,
> toolchains, builds, per-user install and signed updates work; release builds are not published yet.

## Use

```sh
vibefork install          # per-user, no administrator rights: binaries, vibefork:// handler, PATH
                          # the agent starts on demand from a vibefork://start link; `vibefork autostart on` starts it at login
vibefork start            # the agent in the background (vibefork serve — in this terminal)
vibefork open             # the interface; pair the browser in the dialog that appears
vibefork import owner/name   # take a project from GitHub (no git needed)
vibefork save <project>   # save your changes to your copy on GitHub (walks you through the first time)
vibefork doctor           # what works on this machine
vibefork keys set openrouter   # optional: free OpenRouter models without any CLI installed
vibefork update           # signed self-update; --check reports only; --auto off keeps the check and waits for this command
vibefork uninstall        # removes the binaries and registrations, keeps projects and data
                          # --full: also all agent data and keys; --full --with-projects: projects too (asks first)
```

`vibefork` without arguments in a terminal shows the status and a menu.

- **Architecture, layers, invariants:** [ARCHITECTURE.md](ARCHITECTURE.md)
- **Rules for contributors and AI models writing code:** [AGENTS.md](AGENTS.md)
- **Security measures, known limits, reporting:** [SECURITY.md](SECURITY.md)

## Build

```sh
cargo build --release          # binary: target/release/vibefork
cargo test --workspace
python3 scripts/check_layers.py
VF_UPDATE_SCHEMA=1 cargo test -p vf-server --test schema   # regenerate schema/ after protocol changes
```

The API description is generated from the code into [`schema/`](schema): `openapi.json`, `protocol.schema.json`, `protocol.ts`.

The toolchain is pinned in `rust-toolchain.toml`.

## Releases

A push to the `release` branch (or a manual run of `release.yml` with a candidate version) builds
`vibefork` and `vibefork-tray` for Windows, macOS and Linux (x86_64 and aarch64) and creates a
**draft** GitHub Release `v<version>` with the binaries (`vibefork-<os>-<arch>[.exe]`,
`vibefork-tray-<os>-<arch>[.exe]`), `install.ps1` and `install.sh` pinned to their hashes,
`SHA256SUMS.txt` and the release subkey certificate (`subkey.cert.json`, `subkey.cert.sig`).
`scripts/sign_release.py` adds the update manifest (`manifest.json`, `manifest.sig`) signed with
the release subkey; `vibefork-sign verify <dir>` checks a release directory the way the agent
checks its update channel. A draft is published by hand.

## License

[Functional Source License 1.1, MIT Future License](LICENSE.md) (FSL-1.1-MIT): you can use, read,
modify and share the code for any purpose except a competing product; **each version becomes MIT
two years after its release**.
