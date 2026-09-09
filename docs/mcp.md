# MCP tools

`rdc mcp --target NAME` speaks the Model Context Protocol over stdio, so any MCP client can use
it. It exposes one machine per server process; run several for several machines.

## Claude Code setup

Project-local, in `.mcp.json` at the repo root (an example ships as `.mcp.json.example`):

```json
{
  "mcpServers": {
    "studio-mac": { "command": "rdc", "args": ["mcp", "--target", "studio-mac"] }
  }
}
```

Or globally in `~/.claude.json` under the same `mcpServers` key. `rdc` must be on Claude Code's
`PATH`; otherwise give the absolute path in `command`. `--target` takes the same values as the
CLI: a name from `[targets]`, a host, a URL, or `local`.

Also drop the repo's `skills/rdc/` folder into `~/.claude/skills/` (or your agent's skill
directory). It teaches the agent when and how to use these tools, including the coordinate rules
below.

## The coordinate model

The agent never deals with display scaling or multi-monitor offsets:

1. `screenshot` returns an image (downscaled so its longer edge is at most 1568 px by default)
   plus a text line stating the desktop region it covers.
2. Every x/y the agent passes to `click`, `mouse_move`, `drag` or `scroll` is a **pixel position
   in the most recent screenshot**.
3. rdc maps that pixel to a logical desktop point and sends it to the daemon.
4. Most actions return a fresh screenshot, so the mapping stays current.

If the agent calls an action before any screenshot, rdc takes one silently to establish the
mapping. Coordinates read from an old screenshot taken with a different `max` are wrong; the
skill tells agents to always click from the latest image.

## Tools

Parameters marked † default to `true` and mean "return a screenshot after the action";
pass `false` to skip it when chaining several actions.

| Tool | Parameters | Returns |
|---|---|---|
| `screenshot` | `display` (`all` default, `primary`, or id), `max` (px, default 1568) | image + region text |
| `displays` | | JSON list of displays |
| `windows` | | JSON list; each window has `desktop` rect and, once a screenshot exists, an `image` rect in current screenshot pixels |
| `focus` | one of `id`, `app` (substring), `title` (substring); `then_screenshot`† | text (+ image) |
| `mouse_move` | `x`, `y`; `then_screenshot`† | text (+ image) |
| `click` | `x`, `y`, `button` (`left` default, `right`, `middle`), `count` (1–3); `then_screenshot`† | text (+ image) |
| `drag` | `x1`, `y1`, `x2`, `y2`, `button`; `then_screenshot`† | text (+ image) |
| `scroll` | optional `x`, `y`; `dx`, `dy` in wheel steps (positive = right / down); `then_screenshot`† | text (+ image) |
| `type` | `text`; `then_screenshot`† | text (+ image) |
| `key` | `chord`, e.g. `enter`, `cmd+q`, `ctrl+shift+t`; `then_screenshot`† | text (+ image) |
| `clipboard_get` | | clipboard text |
| `clipboard_set` | `text` | confirmation |

`screenshot`, `displays`, `windows` and `clipboard_get` are annotated read-only.

Errors come back as MCP errors with the daemon's message, for example
`"bogus" is not a modifier in "bogus+q"` or `no window matches App("Foo")`.

## A typical exchange

```
agent → screenshot
      ← [image 1568×882] "studio-mac: 1568x882 image of desktop region x=0 y=0 w=2560 h=1440 …"
agent → windows
      ← [{ "app": "Installer", "title": "Install Foo", "image": {"x": 612, "y": 300, "w": 340, "h": 220}, … }]
agent → click { "x": 780, "y": 480 }
      ← "Left click x1 at image (780, 480) = desktop (1273, 784)" + [fresh image]
agent → key { "chord": "enter", "then_screenshot": false }
      ← "pressed enter"
```

## Tuning

- `rdc mcp --max 1200` sends smaller images (cheaper, less detail); `--max 2000` the reverse.
- Tool calls are serialized inside one server process, so a burst of calls keeps its order.
- Actions wait about 350 ms before the follow-up screenshot so the UI can settle.
