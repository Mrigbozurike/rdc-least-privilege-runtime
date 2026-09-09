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

## Input thread

enigo and arboard handles are owned by one dedicated OS thread (`InputWorker`), fed over a
channel, because neither is happily shared across threads on every platform. This also
serializes input naturally.

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
2. For every connection the middleware takes the peer IP from the socket.
3. Loopback → refused, unless `--dev-loopback`. Non-Tailscale range → refused.
4. `tailscale.rs` calls `GET /localapi/v0/whois?addr=IP` on `tailscaled` (unix socket on Linux,
   loopback TCP + proof token for the macOS GUI variants, named pipe on Windows, or the
   `tailscale whois --json` CLI as fallback). The response gives login, node name and tags.
5. The identity is matched against the allowlist and cached for 30 s.

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
