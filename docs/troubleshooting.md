# Troubleshooting

Start with `rdc doctor` on the machine being controlled. It checks each layer in order and says
which one failed.

## Daemon won't start

| Message | Meaning | Fix |
|---|---|---|
| `allowlist is empty` | no `--allow` and no `[serve].allow` | add at least one entry |
| `X is not a Tailscale address; refusing` | `--bind` or config points at a LAN/public IP | bind the Tailscale IP or leave `bind` unset |
| `tailscaled unreachable` / `no Tailscale IP` | Tailscale not running or not logged in | `tailscale status`; on macOS make sure the Tailscale app is running in the same user session |
| `bind … Address already in use` | another rdc or something else on 7770 | `--port` |

## Client can't connect

| Symptom | Cause | Fix |
|---|---|---|
| connection refused / timeout | daemon not running, wrong host, or you're not on the tailnet | `tailscale ping HOST`; check `rdc service status` on the target |
| `403 … is not in the allowlist` | whois succeeded but you're not allowed | add your login/node/tag; `rdc -t HOST whoami` shows what the daemon sees once allowed, the 403 message shows it when not |
| `403 … is not a Tailscale address` | request arrived from a non-tailnet IP | use the Tailscale hostname or 100.x address |
| `403 forbidden: … may not use \`input\`` | your grant is limited to some capabilities | widen `can` in the grant on the daemon side; `rdc -t HOST whoami` shows your `caps` |
| `403 loopback connections are not accepted` | you're on the same machine | use `--target local`, or start the daemon with `--dev-loopback` for testing |
| `421 request addressed to unexpected host` | the URL uses a name the daemon doesn't recognise as itself | use the Tailscale MagicDNS name or IP, or add the name to `[serve].hosts` |
| `400 point (…) is outside the desktop` | coordinates beyond the displays | check `rdc displays`; with MCP, take a fresh screenshot |
| `400 scroll steps must be within ±100` | scroll amount too large | scroll in smaller steps |

## Screenshots

| Symptom | Cause | Fix |
|---|---|---|
| macOS: image is only the wallpaper, `windows` lists 1 entry | Screen Recording not granted to this exact build | [macOS setup](setup-macos.md#things-to-know): re-grant, `tccutil reset`, avoid ad-hoc signing |
| macOS: screenshots take many seconds | `screencapture` failed and rdc fell back to CoreGraphics | check the daemon log for `screencapture failed`; usually a permission problem |
| Wayland: `xcap: …` error | no portal backend or screencopy support | install `xdg-desktop-portal-<compositor>`; on GNOME the portal Screenshot dialog may need approving once |
| black image on X11 | compositor/driver quirk | try `--display primary` |

## Input

| Symptom | Cause | Fix |
|---|---|---|
| macOS: `permission denied: Accessibility` or clicks do nothing | Accessibility not granted | grant to `rdc.app`, restart daemon |
| Wayland: `no way to move the mouse` | compositor lacks wlr virtual pointer (GNOME, KDE) | untested paths; please report |
| clicks land in the wrong place | coordinates from a stale or differently sized screenshot; or CLI given image pixels instead of desktop points | take a new screenshot; use `rdc mcp`, or convert as in [CLI reference](cli.md#mapping-a-screenshot-pixel-to-a-click) |
| `key`: `"foo" is not a modifier` | typo in the chord | see the chord grammar in the CLI reference |
| Windows: input ignored or focus refused for an elevated app | UIPI; the daemon runs at lower integrity | use `rdc service install` (task runs elevated) rather than starting `rdc serve` from a normal shell |
| Windows: `doctor` shows a 1024×768 "WinDisc" display and screenshots fail | you're in an SSH or service session (session 0) | run the daemon via `rdc service install`; test from the client machine |

## MCP

| Symptom | Fix |
|---|---|
| Claude Code shows the server as failed | run `rdc mcp --target NAME` in a terminal; it should sit waiting on stdin with no errors. Common causes: `rdc` not on PATH, unknown target name, config parse error |
| tools work but images are huge/slow | `rdc mcp --max 1200` |
| the agent clicks the wrong thing after a `screenshot` with a custom `max` | the skill tells agents to click from the latest image; remind it |

## Logs

- Daemon: `RDC_LOG=debug rdc serve …` (foreground); service logs are in `journalctl --user -u
  dev.rdc.daemon` (Linux) or `~/Library/Application Support/rdc/serve.log` (macOS).
- Every request is logged with the resolved identity; rejections are logged at `warn`.
- The audit log has one line per request or rejection: `rdc audit -n 100` on the daemon machine.
- Client and MCP: `--log debug` or `RDC_LOG=debug`; goes to stderr, so it won't corrupt MCP stdio.
