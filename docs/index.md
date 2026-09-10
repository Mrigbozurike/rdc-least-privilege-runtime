# rdc

rdc lets a coding agent on one machine see and operate the desktop of another over your
Tailscale network: screenshots, mouse, keyboard, window focus and clipboard. It works with
macOS, Linux and Windows targets and plugs into Claude Code, or any MCP client, as a set of
tools.

A typical use: an agent is fixing something on a headless Mac mini and reaches a dialog that
needs a click. It takes a screenshot, clicks the button and continues.

- No passwords, tokens or certificates. `rdc serve` listens only on the machine's Tailscale
  address and asks the local `tailscaled` who each caller is. You allow tailnet logins, device
  names or tags, and can limit each to `view`, `input` or `clipboard`.
- No shell. rdc is a screen-and-input surface. Use SSH for commands.
- One binary. The same executable is the daemon, the CLI and the MCP server.
- Every request and rejection is written to an audit log.

## Where to start

| If you want to… | Read |
|---|---|
| understand what happens when an agent clicks | [How rdc works](how-it-works.md) |
| install it | [Install](install.md), then the guide for your platform |
| decide who may connect and what they may do | [Grants and Tailscale policy](grants-and-acls.md) |
| wire it into Claude Code | [MCP tools](mcp.md) |
| fix something | [Troubleshooting](troubleshooting.md) |

## Status

| Target platform | State |
|---|---|
| Linux, Wayland (Hyprland / wlroots) | verified |
| Linux, Wayland (GNOME, KDE) | screenshots only; input not wired up yet |
| Linux, X11 | compiles, untested |
| macOS 15+, Apple silicon | verified |
| Windows 11 | verified on one display; multi-monitor implemented, untested |

Releases: [github.com/bscott/rdc/releases](https://github.com/bscott/rdc/releases). Binaries are
not code-signed; see [Install](install.md#unsigned-binaries).

rdc is free software under the GNU GPL-3.0-or-later.
