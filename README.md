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

### Packages

Each release has these files besides the binaries (names carry no version):

| File | What it is | Updates |
|---|---|---|
| `vibefork-windows-<arch>.msi` | Per-user installer: no administrator rights, `%LOCALAPPDATA%\Programs\vibefork` (the wizard offers another folder), Start menu shortcut, optional desktop shortcut and start at sign-in, the `vibefork://` handler and `PATH`; uninstalled in Settings → Apps, data and projects stay; the wizard speaks the Windows display language when it is one of the agent's languages, English otherwise | the agent runs the newer installer (`msiexec /passive`) |
| `vibefork-windows-<arch>-portable.zip` | Folder `Vibefork`: settings and data in `data\`, projects in `projects\` next to the programs (marker `vibefork.install` = `portable`); nothing is registered | the programs in the folder are replaced |
| `vibefork-macos-universal.dmg` | `Vibefork.app` for Apple silicon and Intel; drag it to Applications | the programs in the app are replaced |
| `vibefork-linux-<arch>.AppImage` (+ `.zsync`) | One file; without arguments it starts the tray, with arguments it is the `vibefork` command; the `.zsync` serves AppImageUpdate | the file is replaced |
| `vibefork-linux-<arch>.tar.gz` | Folder `vibefork` with both programs; `vibefork install` puts them in `~/.local/bin` | the programs are replaced |

`vibefork doctor` shows how the agent is installed (`msi`, `portable`, `appimage`, `files`).

The packages are not signed by Microsoft or Apple. Windows SmartScreen asks before the first start of
the installer ("More info" → "Run anyway"); macOS asks before the first start of the app (System
Settings → Privacy & Security → "Open Anyway"; the disk image has the steps in
`If macOS blocks Vibefork.txt`). Every file is listed with its SHA-256 in `SHA256SUMS.txt` and in
the signed update manifest.

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
`vibefork-tray-<os>-<arch>[.exe]`), the packages (`scripts/package.py`: WiX 5 installer and zip on
Windows, `lipo` + `hdiutil` on macOS, appimagetool and the AppImage runtime pinned by SHA-256 on
Linux; each package is installed or started in the run), `install.ps1` and `install.sh` pinned to their hashes,
`SHA256SUMS.txt` and the release subkey certificate (`subkey.cert.json`, `subkey.cert.sig`).
`scripts/sign_release.py` adds the update manifest (`manifest.json`, `manifest.sig`) signed with
the release subkey; `vibefork-sign verify <dir>` checks a release directory the way the agent
checks its update channel. A draft is published by hand.

## License

[Functional Source License 1.1, MIT Future License](LICENSE.md) (FSL-1.1-MIT): you can use, read,
modify and share the code for any purpose except a competing product; **each version becomes MIT
two years after its release**.
