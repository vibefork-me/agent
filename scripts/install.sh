#!/bin/sh
# Installs the Vibefork agent for the current user (no administrator rights): downloads the binaries of
# the release channel, checks their SHA-256 against the channel's manifest, runs `vibefork install`,
# starts the agent and opens the interface.
#
#   curl -fsSL https://vibefork.me/install.sh | sh
#
# VF_UPDATE_URL — release channel (default: https://code.vibefork.me/releases/stable); VF_NO_OPEN=1 — do not open the browser.
# Later updates are verified by the agent's built-in release key.
set -eu

CHANNEL="${VF_UPDATE_URL:-https://code.vibefork.me/releases/stable}"

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) echo "vibefork: unsupported system $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) echo "vibefork: unsupported processor $(uname -m)" >&2; exit 1 ;;
esac

if command -v sha256sum >/dev/null 2>&1; then sha() { sha256sum "$1" | cut -d' ' -f1; }
else sha() { shasum -a 256 "$1" | cut -d' ' -f1; }; fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$CHANNEL/manifest.json" -o "$tmp/manifest.json"

# Downloads one binary of the channel and checks it; `optional` binaries may be absent.
fetch() {
  name="$1-$os-$arch"
  expected="$(sed -n "s/.*\"$name\": *\"\([0-9a-f]\{64\}\)\".*/\1/p" "$tmp/manifest.json" | head -n 1)"
  if [ -z "$expected" ]; then
    [ "${2:-}" = optional ] && return 0
    echo "vibefork: the release has no build for $os-$arch" >&2; exit 1
  fi
  curl -fsSL "$CHANNEL/$name" -o "$tmp/$1"
  if [ "$(sha "$tmp/$1")" != "$expected" ]; then
    echo "vibefork: $name does not match the release manifest" >&2; exit 1
  fi
  chmod +x "$tmp/$1"
}

fetch vibefork
fetch vibefork-tray optional
"$tmp/vibefork" install
if [ "$os" = macos ]; then dir="${VF_INSTALL_DIR:-$HOME/Applications/Vibefork.app/Contents/MacOS}"
else dir="${VF_INSTALL_DIR:-$HOME/.local/bin}"; fi
"$dir/vibefork" start
[ -n "${VF_NO_OPEN:-}" ] || "$dir/vibefork" open || true
echo "Vibefork is installed: $dir/vibefork (try \`vibefork doctor\`)"
