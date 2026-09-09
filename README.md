# rdc — Remote Desktop Control for AI agents

`rdc` lets a coding agent on your machine see and drive the desktop of another machine
(macOS, Linux, Windows) over your Tailscale tailnet: screenshots, mouse, keyboard, window
focus and clipboard. One binary, two roles:

- `rdc serve` runs on the machine to be controlled. It listens only on the Tailscale IP and
  identifies every caller with tailscaled's `whois`, checked against an allowlist. No shared
  secrets, no TLS to manage: the tailnet is the security boundary.
- `rdc mcp --target NAME` runs locally as an MCP stdio server so Claude Code (or any MCP
  client) gets `screenshot`, `click`, `type`, `key`, ... tools. The same commands exist as a
  CLI (`rdc -t NAME shot`, `rdc -t NAME click 640 400`).

No shell execution is exposed; use SSH for that.

## Status

| Platform | Daemon (`rdc serve`) | Notes |
|---|---|---|
| Linux, Wayland (Hyprland) | verified | capture via portal/wlr-screencopy, input via wlr virtual pointer/keyboard, windows via `hyprctl` |
| Linux, Wayland (GNOME/KDE) | compiles, untested | capture via portal; input via `xdg_desktop`/libei paths in enigo |
| Linux, X11 | compiles, untested | xcap + x11rb |
| macOS 15+ (Apple silicon) | verified | needs Screen Recording + Accessibility; ship as a signed `.app`, see below |
| Windows 10/11 | compiles in CI, untested | window focus and service install not implemented yet |

CI builds and tests all three on every push. Release binaries are attached to tags.

**Release binaries are not code-signed.** On macOS, Gatekeeper blocks the raw download and, more
importantly, TCC permissions granted to an unsigned binary are tied to its exact hash and vanish on
every upgrade. Build on the Mac and sign with your own identity using `scripts/macos` (a
self-signed certificate is enough). On Windows, SmartScreen warns on first run. Verify downloads
against `SHA256SUMS`.

## Quick start

On the machine to control:

```sh
rdc doctor                                   # permissions, tailscaled, displays
rdc serve --allow you@example.com            # or put it in config, see below
```

On your machine:

```sh
rdc -t studio-mac whoami             # host[:port], URL, or a name from config
rdc -t studio-mac shot --max 1568 -o shot.png
rdc -t studio-mac click 512 300
rdc -t studio-mac key cmd+q
```

### Claude Code

Add to `.mcp.json` in a project (or `~/.claude.json` for everywhere); see `.mcp.json.example`:

```json
{
  "mcpServers": {
    "studio-mac": { "command": "rdc", "args": ["mcp", "--target", "studio-mac"] }
  }
}
```

The agent gets `screenshot`, `displays`, `windows`, `focus`, `mouse_move`, `click`, `drag`,
`scroll`, `type`, `key`, `clipboard_get`, `clipboard_set`. Coordinates the agent passes are
pixels in the most recent screenshot; rdc converts them to desktop points. Actions return a
fresh screenshot by default (`then_screenshot: false` to skip).

### CLI coordinates

Coordinates are logical desktop points. A screenshot reports the desktop rectangle it covers,
so pixel `(px, py)` in an image of size `W x H` maps to
`(rect.x + px * rect.w / W, rect.y + py * rect.h / H)`. The MCP layer does this for you.

## Config

`~/.config/rdc/config.toml` (Linux/macOS) or `%APPDATA%\rdc\config.toml`:

```toml
[serve]
port = 7770
# tailnet logins, node names, tags, or "*"
allow = ["you@example.com", "tag:family"]

[targets.studio-mac]
url = "http://studio-mac.example-tailnet.ts.net:7770"
```

## Platform notes

- **Linux / Wayland**: capture via xdg-desktop-portal Screenshot or wlr-screencopy; input via
  the wlr virtual pointer and keyboard protocols (Hyprland, sway, river...). On Hyprland,
  window listing and focus use `hyprctl`. X11 sessions use xcap/x11rb.
- **macOS**: needs Screen Recording and Accessibility granted once. Ship as a signed `.app`
  so the grants survive rebuilds (see `scripts/macos`). Runs as a per-user LaunchAgent. The
  bundle identifier is `dev.bscott.rdc`; change it in `scripts/macos/bundle-and-sign.sh` and
  `src/service/mod.rs` if you fork.
- **Windows**: xcap + SendInput. Tailscale LocalAPI over the named pipe.

## For agents

`skills/rdc/SKILL.md` is an Agent Skill describing how to use rdc's MCP tools and CLI and how to
set up a target. Point your agent's skill loader at that folder, or copy it into
`~/.claude/skills/rdc/`.

## Development

```sh
cargo build --release
cargo test
./target/release/rdc serve --dev-loopback     # unauthenticated 127.0.0.1, for local testing only
./target/release/rdc -t http://127.0.0.1:7770 shot
```

## Security

See [SECURITY.md](SECURITY.md) for the threat model and how to report a vulnerability. In one
line: anyone on the allowlist has full control of the desktop, so the allowlist and your tailnet
are the whole security boundary.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). CI runs rustfmt, clippy and tests on Linux, macOS and
Windows.

## License

rdc is free software under the [GNU Affero General Public License v3.0 or later](LICENSE).
If you modify rdc and let others interact with it over a network (including running a modified
`rdc serve` that other people's agents connect to), the AGPL requires you to offer them the
corresponding source. Copyright (C) 2026 Brian Scott.
