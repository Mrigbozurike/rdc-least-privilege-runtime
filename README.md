# rdc — Remote Desktop Control for AI agents

Let a coding agent on your laptop **see and operate another computer's desktop** over your
Tailscale network: take screenshots, move and click the mouse, type, press shortcuts, focus
windows, read and write the clipboard. Works with macOS, Linux and Windows targets and plugs into
Claude Code (or any MCP client) as a set of tools.

Typical use: your agent is fixing something on a headless Mac mini and hits a dialog that needs
a click. Instead of walking over or opening a KVM, the agent takes a screenshot, clicks the
button, and carries on.

```
   your machine                                          machine being controlled
  ┌─────────────────────────────────┐                   ┌──────────────────────────┐
  │ Claude Code ── MCP ──► rdc mcp  │   Tailscale       │  rdc serve               │
  │ or:                    rdc -t … ├──── HTTP ────────►│  identifies you via      │
  │                                 │   (WireGuard)     │  tailscaled whois, then  │
  └─────────────────────────────────┘                   │  screenshots · input     │
                                                        │  windows · clipboard     │
                                                        └──────────────────────────┘
```

- **No passwords, tokens or certificates.** `rdc serve` listens only on the machine's Tailscale
  address and asks the local `tailscaled` who each caller is. You allow tailnet logins, device
  names or tags. Your tailnet is the security boundary.
- **No shell.** rdc is a screen-and-input surface only. Use SSH for commands.
- **One static binary**, written in Rust. Same binary is the daemon, the CLI and the MCP server.

## Status

| Target platform | State | Notes |
|---|---|---|
| Linux, Wayland (Hyprland / wlroots) | verified | portal or wlr-screencopy capture, wlr virtual input, `hyprctl` window control |
| Linux, Wayland (GNOME, KDE) | compiles, untested | portal capture; input via enigo's portal/libei paths |
| Linux, X11 | compiles, untested | |
| macOS 15+, Apple silicon | verified | needs Screen Recording + Accessibility, see [macOS setup](docs/setup-macos.md) |
| Windows 10/11 | compiles in CI, untested | window focus and service install not implemented yet |

CI builds and tests all three on every push. Tagged releases attach binaries, which are **not
code-signed** (see [Install](docs/install.md#unsigned-binaries)).

## Five-minute start

**1. Install rdc on both machines.** Download a release archive or `cargo install --git
https://github.com/bscott/rdc`. Details and build dependencies: [docs/install.md](docs/install.md).

**2. On the machine you want to control**, check readiness and start the daemon, allowing your own
tailnet login:

```sh
rdc doctor
rdc serve --allow you@example.com
```

`rdc doctor` tells you if a permission or `tailscaled` is missing. When it works, install it as a
background service so it survives reboots: `rdc service install` (macOS LaunchAgent or Linux
systemd user service). macOS needs two one-time permission grants; follow
[docs/setup-macos.md](docs/setup-macos.md).

**3. On your laptop**, talk to it by Tailscale hostname:

```sh
rdc -t studio-mac whoami                        # the daemon confirms who you are
rdc -t studio-mac shot --max 1568 -o shot.png   # screenshot, downscaled
rdc -t studio-mac click 640 400                 # desktop coordinates
rdc -t studio-mac key cmd+q
```

**4. Give it to your agent.** Add a target to `~/.config/rdc/config.toml` and register the MCP
server in Claude Code:

```toml
[targets.studio-mac]
url = "http://studio-mac.example-tailnet.ts.net:7770"
```

```json
{ "mcpServers": { "studio-mac": { "command": "rdc", "args": ["mcp", "--target", "studio-mac"] } } }
```

The agent now has `screenshot`, `click`, `type`, `key`, `focus` and friends. It passes pixel
coordinates from the screenshot it just saw; rdc converts them to desktop points. Every action
returns a fresh screenshot so the agent can check its work. Full tool reference:
[docs/mcp.md](docs/mcp.md). An [Agent Skill](skills/rdc/SKILL.md) is included that teaches
agents how to use rdc well.

## Documentation

| | |
|---|---|
| [Install](docs/install.md) | release binaries, `cargo install`, build dependencies per OS |
| [Configuration](docs/configuration.md) | `config.toml`, allowlist rules, targets, environment variables |
| [CLI reference](docs/cli.md) | every subcommand and flag |
| [MCP tools](docs/mcp.md) | tool reference, coordinate model, Claude Code setup |
| [macOS setup](docs/setup-macos.md) | signing, permissions, LaunchAgent, upgrades |
| [Linux setup](docs/setup-linux.md) | Wayland compositors, X11, systemd service |
| [Windows](docs/setup-windows.md) | what works today and what doesn't |
| [Architecture](docs/architecture.md) | how it fits together, wire API, auth flow |
| [Troubleshooting](docs/troubleshooting.md) | symptoms and fixes |
| [Security](SECURITY.md) | threat model and vulnerability reporting |
| [Contributing](CONTRIBUTING.md) | development, testing, pull requests |

## How it stays safe

`rdc serve` refuses to bind anything but a Tailscale address (or `127.0.0.1` with the explicit
`--dev-loopback` testing flag, which disables auth). For each request it resolves the peer IP
through `tailscaled`'s `whois` and matches the login, node name or tags against your allowlist.
An empty allowlist refuses to start. Everyone on the allowlist has full control of the desktop;
there are no finer permissions yet. Read [SECURITY.md](SECURITY.md) before exposing a machine
you care about.

## License

rdc is free software under the [GNU Affero General Public License v3.0 or later](LICENSE).
If you modify rdc and let others interact with it over a network, including running a modified
`rdc serve` that other people's agents connect to, the AGPL requires you to offer them the
corresponding source. Copyright (C) 2026 Brian Scott.
