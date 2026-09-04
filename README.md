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

Phase 1 (core + Linux/Hyprland + HTTP daemon + CLI) works. MCP, macOS packaging and Windows
follow. See the plan in the repo history for the roadmap.

## Quick start

On the machine to control:

```sh
rdc doctor                                   # permissions, tailscaled, displays
rdc serve --allow you@example.com            # or put it in config, see below
```

On your machine:

```sh
rdc -t brians-m4-mac-mini whoami             # host[:port], URL, or a name from config
rdc -t brians-m4-mac-mini shot --max 1568 -o shot.png
rdc -t brians-m4-mac-mini click 512 300
rdc -t brians-m4-mac-mini key cmd+q
```

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

[targets.macmini]
url = "http://brians-m4-mac-mini.your-tailnet.ts.net:7770"
```

## Platform notes

- **Linux / Wayland**: capture via xdg-desktop-portal Screenshot or wlr-screencopy; input via
  the wlr virtual pointer and keyboard protocols (Hyprland, sway, river...). On Hyprland,
  window listing and focus use `hyprctl`. X11 sessions use xcap/x11rb.
- **macOS**: needs Screen Recording and Accessibility granted once. Ship as a signed `.app`
  so the grants survive rebuilds (see `scripts/macos`). Runs as a per-user LaunchAgent.
- **Windows**: xcap + SendInput. Tailscale LocalAPI over the named pipe.

## Development

```sh
cargo build --release
cargo test
./target/release/rdc serve --dev-loopback     # unauthenticated 127.0.0.1, for local testing only
./target/release/rdc -t http://127.0.0.1:7770 shot
```
