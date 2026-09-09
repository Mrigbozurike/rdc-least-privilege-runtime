#!/usr/bin/env bash
# Build the GitHub release body for a version from CHANGELOG.md.
# Usage: scripts/release-notes.sh <version|tag> [changelog] > notes.md
# Fails if CHANGELOG.md has no "## <version>" section, so a tag can't ship without notes.
set -euo pipefail
VERSION="${1#v}"
CHANGELOG="${2:-CHANGELOG.md}"

section=$(awk -v v="$VERSION" '
  /^## / { if (found) exit; found = ($2 == v) ; next }
  found { print }
' "$CHANGELOG")

if [ -z "$(printf '%s' "$section" | tr -d '[:space:]')" ]; then
  echo "error: no '## $VERSION' section in $CHANGELOG" >&2
  exit 1
fi

printf '%s\n' "$section" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}'
cat <<'NOTES'

## Install

Download the archive for your platform below, verify it against `SHA256SUMS`
(`sha256sum -c SHA256SUMS`), and put `rdc` on your `PATH`. Each archive also contains the
documentation, the agent skill and the third-party license bundle. Setup guides:
[docs/install.md](https://github.com/bscott/rdc/blob/main/docs/install.md).

## Unsigned binaries

These binaries are **not code-signed or notarized**.

- **macOS**: Gatekeeper refuses the raw download, and permissions granted to an unsigned
  binary are lost on every upgrade. Build on the Mac and sign with your own identity via
  `scripts/macos/bundle-and-sign.sh`; see
  [docs/setup-macos.md](https://github.com/bscott/rdc/blob/main/docs/setup-macos.md). The
  unsigned binary is fine for the client side (`rdc -t …`, `rdc mcp`).
- **Windows**: SmartScreen warns on first run. "More info" → "Run anyway", or build from source.
- **Linux**: no signature checks apply.
NOTES
