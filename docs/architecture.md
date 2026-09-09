# Architecture

One Rust crate, one binary, three roles chosen by subcommand.

```
                  ┌──────────────── rdc (binary) ────────────────┐
  agent ── MCP ──►│ mcp.rs ── view.rs ──┐                        │
  human ── CLI ──►│ main.rs ────────────┼──► dyn Desktop         │
                  │                     │      ├─ local/  (xcap, enigo, arboard, hyprctl/AppKit)
                  │                     │      └─ remote.rs (HTTP client) ──► another rdc `serve`
                  │ serve: server/ (axum) ── auth.rs (tailscale whois) ── local Desktop
                  └───────────────────────────────────────────────┘
```

## The `Desktop` trait

`src/desktop/mod.rs` defines everything rdc can do to a machine:

```rust
displays()      -> Vec<Display>          // id, name, logical rect, scale, primary
screenshot(req) -> Screenshot            // encoded image + the desktop rect it covers
windows()       -> Vec<Window>           // id, pid, app, title, rect, focused, minimized
focus(target)                            // by id / app substring / title substring
input(action)                            // move, click, button, drag, scroll, type, key
clipboard_get() / clipboard_set(text)
```

Two implementations: `desktop/local` does the work in-process; `desktop/remote` forwards over
HTTP to a daemon. The CLI and the MCP server only ever hold an `Arc<dyn Desktop>`, so
`--target local` and `--target somehost` are the same code.

## Coordinates

Server side, everything is **logical desktop points**: the virtual desktop spanning all
monitors, as the OS reports it (on a 2× display a 2880×1920 panel is 1440×960 points). A
`Screenshot` carries the rect it covers so clients can map pixels back to points regardless of
scaling or downsampling. `view.rs` does that mapping for the MCP layer.

Under Wayland, enigo's absolute pointer move takes a fraction of the first output's physical
mode. `desktop/local/input.rs` converts points → fraction of the whole layout → that extent, which
is why clicks land correctly on scaled displays.

## Input and clipboard threads

The enigo handle is owned by one dedicated OS thread and the arboard handle by another, each fed
over a channel, because neither is happily shared across threads on every platform. Keeping them
apart means a clipboard owner that never answers (Wayland transfers have no deadline) cannot
block mouse and keyboard; the caller also gives up on clipboard operations after 5 seconds.

## Wire API

Plain HTTP + JSON on the daemon, all routes behind the auth middleware. Base path `/v1`.

| Route | Purpose |
|---|---|
| `GET /state` | displays and windows |
| `GET /screenshot?display=all\|primary\|ID&format=png\|jpg&max=N` | image bytes; headers `x-rdc-rect: x,y,w,h` and `x-rdc-size: W,H` |
| `POST /act` | JSON `{"kind":"input", "type":"click", …}` / `{"kind":"focus","by":"app","value":"…"}` / `{"kind":"clipboard_set","text":"…"}` |
| `GET /clipboard` | `{"text": …}` |
| `GET /whoami` | the caller's resolved identity |
| `GET /health` | `ok` (also authenticated) |

Errors are JSON `{"code": "...", "message": "..."}` with codes `not_found`, `unsupported`,
`permission`, `unauthorized`, `bad_request`, `backend` and matching HTTP statuses. The wire types
live in `src/proto.rs` and are shared by both sides.

## Authentication flow

1. `serve` resolves the bind address: `--bind`, config, or the node's Tailscale IPv4 from the
   LocalAPI. Non-Tailscale addresses are refused (except `--dev-loopback`).
2. For every request the middleware first checks the `Host` header against the node's own IPs,
   MagicDNS name, hostname and `[serve].hosts`; anything else is `421 Misdirected Request`.
3. It then takes the peer IP from the socket. Loopback → refused, unless `--dev-loopback`.
   Non-Tailscale range → refused.
4. `tailscale.rs` calls `GET /localapi/v0/whois?addr=IP` on `tailscaled` (unix socket on Linux,
   loopback TCP + proof token for the macOS GUI variants, named pipe on Windows, or the
   `tailscale whois --json` CLI as fallback). The response gives login, node name and tags.
5. Tagged nodes have their creator's login stripped; they are identified by tags and node name
   only. The identity is matched against the grants and cached for 30 s; the union of the
   matching grants' capabilities (`view`, `input`, `clipboard`) travels with the identity.
6. Each route requires one capability: state and screenshot need `view`, `/act` needs `input`
   (or `clipboard` for clipboard writes), `/clipboard` needs `clipboard`. Missing capability →
   `403 forbidden`.
7. Every request and rejection is appended to the audit log (`src/server/audit.rs`) as a JSON
   line with identity, action summary, outcome and duration. Rejections are recorded by the
   middleware; authorized requests by the route handlers, after they know the outcome.

Input requests are validated before they touch the desktop: coordinates must lie inside the
union of the displays, scroll magnitudes are capped at 100 steps, and unsupported keys are
refused before any modifier is pressed. A drag always releases the button even if a move fails.

## Per-platform pieces

| | Capture | Input | Windows / focus | Service |
|---|---|---|---|---|
| Linux Wayland | xcap: portal Screenshot → wlr-screencopy | enigo `wayland` | `hyprctl` on Hyprland; xcap list elsewhere | systemd --user |
| Linux X11 | xcap (xcb) | enigo `x11rb` | xcap list | systemd --user |
| macOS | `screencapture` CLI (fast), xcap CoreGraphics fallback | enigo (CGEvent) | xcap list, `NSRunningApplication.activate` | LaunchAgent, signed `.app` |
| Windows | xcap (GDI/WGC) | enigo (`SendInput`) | xcap list, focus not implemented | not implemented |

## MCP layer

`mcp.rs` uses the official `rmcp` SDK. Each tool acquires one mutex so calls are serialized,
looks up the current `ViewMap` (set by the last screenshot, or taken on demand), converts image
pixels to desktop points, performs the action, waits ~350 ms and returns a new screenshot unless
asked not to. Images are returned as base64 PNG content blocks.
