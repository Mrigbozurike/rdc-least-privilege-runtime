# How rdc works

A coding agent on your machine takes a screenshot of another machine, decides where to click, and
clicks. This page follows that click through each step, shows who is allowed to send it, and
explains how a pixel in the screenshot maps to the right place on a screen with a different
resolution.

## Two roles, one binary

The same `rdc` executable runs on both ends. On the machine being controlled it is a daemon,
`rdc serve`. On your machine it is either an MCP server that Claude Code talks to, `rdc mcp`, or
a command-line client you use directly. The network between them is your Tailscale tailnet.

<figure class="rdc-fig">
<svg viewBox="0 0 900 230" role="img" aria-label="Your machine runs Claude Code, which talks MCP to rdc mcp; rdc mcp sends HTTP over the Tailscale tunnel to rdc serve on the controlled machine, which drives the desktop through the OS.">
<defs>
<marker id="arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
<path d="M0,0 L10,5 L0,10 z" fill="currentColor"/>
</marker>
</defs>
<!-- left machine -->
<rect x="20" y="30" width="340" height="170" rx="6" fill="none" stroke="currentColor" stroke-dasharray="4 4"/>
<text x="32" y="52" font-size="12" fill="currentColor" font-family="IBM Plex Mono, monospace">your machine</text>
<rect x="40" y="80" width="120" height="60" rx="4" fill="none" stroke="currentColor"/>
<text x="100" y="105" font-size="13" text-anchor="middle" fill="currentColor" font-family="IBM Plex Sans, sans-serif">Claude Code</text>
<text x="100" y="124" font-size="11" text-anchor="middle" fill="currentColor" opacity="0.7">(the agent)</text>
<rect x="220" y="80" width="120" height="60" rx="4" fill="none" stroke="currentColor"/>
<text x="280" y="105" font-size="13" text-anchor="middle" fill="currentColor" font-family="IBM Plex Mono, monospace">rdc mcp</text>
<text x="280" y="124" font-size="11" text-anchor="middle" fill="currentColor" opacity="0.7">pixels → points</text>
<line x1="160" y1="110" x2="218" y2="110" stroke="currentColor" marker-end="url(#arr)"/>
<text x="189" y="100" font-size="11" text-anchor="middle" fill="currentColor">MCP, stdio</text>
<!-- tunnel -->
<line x1="342" y1="110" x2="538" y2="110" stroke="var(--accent)" stroke-width="2" marker-end="url(#arr)" style="color: var(--accent)"/>
<text x="440" y="96" font-size="11" text-anchor="middle" fill="currentColor">HTTP · JSON · PNG</text>
<text x="440" y="132" font-size="11" text-anchor="middle" fill="currentColor" opacity="0.7">inside the Tailscale tunnel (WireGuard)</text>
<!-- right machine -->
<rect x="540" y="30" width="340" height="170" rx="6" fill="none" stroke="var(--accent)"/>
<text x="552" y="52" font-size="12" fill="var(--accent)" font-family="IBM Plex Mono, monospace">machine being controlled</text>
<rect x="560" y="80" width="120" height="60" rx="4" fill="none" stroke="currentColor"/>
<text x="620" y="105" font-size="13" text-anchor="middle" fill="currentColor" font-family="IBM Plex Mono, monospace">rdc serve</text>
<text x="620" y="124" font-size="11" text-anchor="middle" fill="currentColor" opacity="0.7">auth · audit</text>
<rect x="740" y="80" width="120" height="60" rx="4" fill="none" stroke="currentColor"/>
<text x="800" y="105" font-size="13" text-anchor="middle" fill="currentColor" font-family="IBM Plex Sans, sans-serif">the desktop</text>
<text x="800" y="124" font-size="11" text-anchor="middle" fill="currentColor" opacity="0.7">screen · mouse · keys</text>
<line x1="680" y1="110" x2="738" y2="110" stroke="currentColor" marker-end="url(#arr)"/>
<text x="709" y="100" font-size="11" text-anchor="middle" fill="currentColor">OS APIs</text>
<!-- tailscaled -->
<rect x="560" y="160" width="120" height="30" rx="4" fill="none" stroke="var(--ident)"/>
<text x="620" y="180" font-size="12" text-anchor="middle" fill="var(--ident)" font-family="IBM Plex Mono, monospace">tailscaled</text>
<line x1="620" y1="140" x2="620" y2="158" stroke="var(--ident)" marker-end="url(#arr)" style="color: var(--ident)"/>
<text x="660" y="153" font-size="11" fill="var(--ident)">whois?</text>
</svg>
<figcaption>One connection crosses between the machines, and it carries no password. The Tailscale tunnel identifies the sending device, and the daemon asks its local <code>tailscaled</code> which user or tags that device has.</figcaption>
</figure>


| | Your machine | Machine being controlled |
|---|---|---|
| Command | `rdc mcp --target studio-mac` or `rdc -t studio-mac …` | `rdc serve` |
| Job | Speaks MCP to the agent, converts screenshot pixels to desktop points | Identifies callers, captures the screen, sends input |
| Runs as | A process the agent starts | LaunchAgent (macOS), systemd user service (Linux), scheduled task (Windows) |
| Holds secrets | No | No |

## A click, end to end

<figure class="rdc-fig">
<svg viewBox="0 0 900 400" role="img" aria-label="Sequence of a click: the agent calls the screenshot tool, rdc mcp fetches a PNG over HTTP and returns it downscaled; the agent calls click with image pixel coordinates; rdc mcp converts to desktop points and posts an action; rdc serve validates, moves the pointer and clicks; rdc mcp returns a fresh screenshot.">
<defs>
<marker id="arr2" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
<path d="M0,0 L10,5 L0,10 z" fill="currentColor"/>
</marker>
</defs>
<!-- lifelines -->
<g font-family="IBM Plex Mono, monospace" font-size="13" fill="currentColor" text-anchor="middle">
<text x="120" y="28">agent</text>
<text x="420" y="28">rdc mcp</text>
<text x="720" y="28">rdc serve</text>
</g>
<g stroke="currentColor" stroke-dasharray="3 5" opacity="0.6">
<line x1="120" y1="40" x2="120" y2="380"/>
<line x1="420" y1="40" x2="420" y2="380"/>
<line x1="720" y1="40" x2="720" y2="380"/>
</g>
<g stroke="currentColor" marker-end="url(#arr2)">
<line x1="122" y1="75" x2="416" y2="75"/>
<line x1="422" y1="110" x2="716" y2="110"/>
<line x1="718" y1="140" x2="424" y2="140"/>
<line x1="418" y1="170" x2="124" y2="170"/>
<line x1="122" y1="230" x2="416" y2="230"/>
<line x1="422" y1="270" x2="716" y2="270"/>
<line x1="718" y1="305" x2="424" y2="305"/>
<line x1="422" y1="335" x2="716" y2="335"/>
<line x1="418" y1="368" x2="124" y2="368"/>
</g>
<g font-size="11.5" fill="currentColor" text-anchor="middle" font-family="IBM Plex Sans, sans-serif">
<text x="270" y="68">1 · screenshot()</text>
<text x="570" y="103">2 · GET /v1/screenshot</text>
<text x="570" y="133">3 · PNG 2560×1440 + rect header</text>
<text x="270" y="163">4 · image 1568×882 · "covers x=0 y=0 w=2560 h=1440"</text>
<text x="270" y="223">5 · click(x=780, y=480)  ← pixels in that image</text>
<text x="570" y="263">6 · POST /v1/act  click 1273,784  ← desktop points</text>
<text x="570" y="298">7 · {ok}  after validate → move → press → release</text>
<text x="570" y="328">8 · GET /v1/screenshot  (350 ms later)</text>
<text x="270" y="361">9 · fresh image so the agent can check its work</text>
</g>
<!-- highlight the conversion -->
<rect x="330" y="212" width="180" height="26" rx="13" fill="var(--accent-soft)" stroke="var(--accent)"/>
<text x="420" y="229" font-size="11" text-anchor="middle" fill="var(--accent)" font-family="IBM Plex Mono, monospace">780 × 2560/1568 = 1273</text>
</svg>
<figcaption>The agent gives coordinates as pixels of the last screenshot. <code>rdc mcp</code> stores the desktop region that screenshot covered and converts, so the model does not need to know about display scaling or multiple monitors.</figcaption>
</figure>


The daemon returns the full-resolution capture and the client downscales it, so the image size
sent to the agent can be changed without touching the remote machine. Every action returns a new
screenshot by default, which keeps the mapping current and shows the agent the result.

## Who gets in

rdc has no passwords, tokens or certificates. The Tailscale tunnel identifies the sending device,
`tailscaled` reports who that device is, and a list in the config says what they may do. Every
request goes through these checks in order.

<figure class="rdc-fig">
<svg viewBox="0 0 900 520" role="img" aria-label="Auth gate as a top-to-bottom flow: bind only a Tailscale address; Host header must name this machine or 421; peer IP must be a Tailscale address or 403; tailscaled whois resolves login, node and tags; tagged nodes drop the creator login; grants are matched and capabilities unioned or 403; the route's required capability is checked or 403; then the action runs and an audit line is written either way.">
<defs>
<marker id="arr3" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
<path d="M0,0 L10,5 L0,10 z" fill="currentColor"/>
</marker>
</defs>
<g font-family="IBM Plex Sans, sans-serif" font-size="12.5" fill="currentColor">
<!-- step boxes -->
<rect x="60" y="20" width="420" height="44" rx="4" fill="none" stroke="currentColor"/>
<text x="76" y="38" font-weight="600">0 · at startup: bind the Tailscale IP only</text>
<text x="76" y="55" font-size="11" opacity="0.75">anything else is refused, except 127.0.0.1 with --dev-loopback</text>
<rect x="60" y="90" width="420" height="44" rx="4" fill="none" stroke="currentColor"/>
<text x="76" y="108" font-weight="600">1 · Host header names this machine?</text>
<text x="76" y="125" font-size="11" opacity="0.75">its Tailscale IPs, MagicDNS name, hostname, [serve].hosts</text>
<rect x="60" y="160" width="420" height="44" rx="4" fill="none" stroke="currentColor"/>
<text x="76" y="178" font-weight="600">2 · peer IP is in 100.64/10 or fd7a:115c:a1e0::/48?</text>
<text x="76" y="195" font-size="11" opacity="0.75">taken from the TCP socket, never from a header</text>
<rect x="60" y="230" width="420" height="58" rx="4" fill="none" stroke="var(--ident)"/>
<text x="76" y="248" font-weight="600" fill="var(--ident)">3 · ask tailscaled: whois(peer IP)</text>
<text x="76" y="265" font-size="11" opacity="0.85">→ login  alice@example.com   node  studio-laptop   tags  []</text>
<text x="76" y="280" font-size="11" opacity="0.75">tagged devices: the creator's login is dropped; only tags and node name count</text>
<rect x="60" y="314" width="420" height="58" rx="4" fill="none" stroke="currentColor"/>
<text x="76" y="332" font-weight="600">4 · match grants, union their capabilities</text>
<text x="76" y="349" font-size="11" opacity="0.75">"alice@example.com" → all      { who = "monitor-bot", can = "view" }</text>
<text x="76" y="364" font-size="11" opacity="0.75">cached 30 s per IP</text>
<rect x="60" y="398" width="420" height="44" rx="4" fill="none" stroke="currentColor"/>
<text x="76" y="416" font-weight="600">5 · does this route's capability match?</text>
<text x="76" y="433" font-size="11" opacity="0.75">screenshot needs view · click needs input · clipboard needs clipboard</text>
<rect x="60" y="468" width="420" height="36" rx="4" fill="var(--accent-soft)" stroke="var(--accent)"/>
<text x="76" y="491" font-weight="600" fill="var(--accent)">6 · do it, then write the audit line</text>
</g>
<!-- down arrows -->
<g stroke="currentColor" marker-end="url(#arr3)">
<line x1="270" y1="64" x2="270" y2="88"/>
<line x1="270" y1="134" x2="270" y2="158"/>
<line x1="270" y1="204" x2="270" y2="228"/>
<line x1="270" y1="288" x2="270" y2="312"/>
<line x1="270" y1="372" x2="270" y2="396"/>
<line x1="270" y1="442" x2="270" y2="466"/>
</g>
<!-- deny branches -->
<g stroke="var(--deny)" marker-end="url(#arr3)" style="color: var(--deny)">
<line x1="480" y1="112" x2="560" y2="112"/>
<line x1="480" y1="182" x2="560" y2="182"/>
<line x1="480" y1="259" x2="560" y2="259"/>
<line x1="480" y1="343" x2="560" y2="343"/>
<line x1="480" y1="420" x2="560" y2="420"/>
</g>
<g font-family="IBM Plex Mono, monospace" font-size="12" fill="var(--deny)">
<text x="570" y="116">421 misdirected request</text>
<text x="570" y="186">403 not a Tailscale address</text>
<text x="570" y="263">403 not a tailnet peer</text>
<text x="570" y="347">403 not in the allowlist</text>
<text x="570" y="424">403 forbidden: may not use `input`</text>
</g>
<!-- audit rail -->
<line x1="840" y1="100" x2="840" y2="486" stroke="currentColor" stroke-dasharray="2 4" opacity="0.6"/>
<g stroke="currentColor" opacity="0.6" marker-end="url(#arr3)">
<line x1="760" y1="112" x2="836" y2="112"/>
<line x1="770" y1="182" x2="836" y2="182"/>
<line x1="740" y1="259" x2="836" y2="259"/>
<line x1="760" y1="343" x2="836" y2="343"/>
<line x1="800" y1="420" x2="836" y2="420"/>
<line x1="480" y1="486" x2="836" y2="486"/>
</g>
<text x="840" y="92" font-size="11" text-anchor="middle" fill="currentColor" font-family="IBM Plex Mono, monospace">audit.jsonl</text>
<text x="840" y="506" font-size="11" text-anchor="middle" fill="currentColor" opacity="0.75">every outcome, one line</text>
</svg>
<figcaption>Every rejection and every success is written to the same audit file. Tailscale reports the user who created a tagged device; rdc drops that login, so tagged devices match only by tag or node name.</figcaption>
</figure>


Grants live in the config file. A plain string grants everything; an inline table limits it.
Several matching grants add up.

```toml
[serve]
allow = [
  "alice@example.com",                                     # full control
  { who = "monitor-bot", can = "view" },                   # screenshots only
  { who = ["tag:ops", "bob@example.com"], can = ["view", "clipboard"] },
]
```

Capabilities: `view` (displays, windows, screenshots), `input` (mouse, keyboard, focus),
`clipboard` (read and write). See [Grants and Tailscale policy](grants-and-acls.md) for worked
examples and the matching Tailscale rules.

## Where the click lands

Machines report coordinates differently. A laptop with a 2880×1920 panel at 2× scale has a
1440×960 point desktop. A Mac mini at 2560×1440 is 1×. A Windows laptop at 2560×1600 and 150 %
reports physical pixels. rdc uses one rule for all of them: every coordinate on the wire is a
logical desktop point, a position in the virtual desktop spanning all monitors in the units the
OS uses to place windows. A screenshot carries the rectangle of points it covers.

<figure class="rdc-fig">
<svg viewBox="0 0 900 300" role="img" aria-label="Coordinate conversion: an image pixel 784,522 in a 1568 by 1045 screenshot maps to desktop point 720,480 on a 1440 by 960 logical desktop; the daemon then converts the point for its backend: Wayland as a fraction of the output extent giving 1440,960; X11 multiplied by scale giving 1440,960; macOS and Windows unchanged.">
<defs>
<marker id="arr4" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
<path d="M0,0 L10,5 L0,10 z" fill="currentColor"/>
</marker>
</defs>
<!-- image -->
<rect x="30" y="50" width="196" height="131" fill="var(--paper-2)" stroke="currentColor"/>
<text x="128" y="40" font-size="12" text-anchor="middle" fill="currentColor" font-family="IBM Plex Mono, monospace">screenshot 1568×1045</text>
<circle cx="128" cy="115" r="4" fill="var(--accent)"/>
<text x="128" y="205" font-size="12" text-anchor="middle" fill="currentColor" font-family="IBM Plex Mono, monospace">pixel (784, 522)</text>
<text x="128" y="222" font-size="11" text-anchor="middle" fill="currentColor" opacity="0.7">what the agent sees</text>
<line x1="232" y1="115" x2="300" y2="115" stroke="currentColor" marker-end="url(#arr4)"/>
<text x="266" y="104" font-size="11" text-anchor="middle" fill="currentColor">× 1440/1568</text>
<!-- desktop -->
<rect x="304" y="50" width="240" height="160" fill="none" stroke="var(--accent)" stroke-width="2"/>
<text x="424" y="40" font-size="12" text-anchor="middle" fill="var(--accent)" font-family="IBM Plex Mono, monospace">desktop 1440×960 points</text>
<circle cx="424" cy="130" r="4" fill="var(--accent)"/>
<text x="424" y="234" font-size="12" text-anchor="middle" fill="currentColor" font-family="IBM Plex Mono, monospace">point (720, 480)</text>
<text x="424" y="251" font-size="11" text-anchor="middle" fill="currentColor" opacity="0.7">what goes over the wire</text>
<!-- fan out -->
<g stroke="currentColor" marker-end="url(#arr4)">
<line x1="548" y1="130" x2="640" y2="70"/>
<line x1="548" y1="130" x2="640" y2="130"/>
<line x1="548" y1="130" x2="640" y2="190"/>
<line x1="548" y1="130" x2="640" y2="250"/>
</g>
<g font-family="IBM Plex Sans, sans-serif" font-size="12" fill="currentColor">
<text x="650" y="66">Wayland · fraction of first output's mode</text>
<text x="650" y="82" font-size="11" opacity="0.7" font-family="IBM Plex Mono, monospace">720/1440 × 2880 = 1440,  480/960 × 1920 = 960</text>
<text x="650" y="126">X11 · multiply by Xft.dpi scale</text>
<text x="650" y="142" font-size="11" opacity="0.7" font-family="IBM Plex Mono, monospace">720 × 2 = 1440,  480 × 2 = 960</text>
<text x="650" y="186">macOS · points already</text>
<text x="650" y="202" font-size="11" opacity="0.7" font-family="IBM Plex Mono, monospace">720, 480</text>
<text x="650" y="246">Windows · physical px already</text>
<text x="650" y="262" font-size="11" opacity="0.7" font-family="IBM Plex Mono, monospace">SendInput over the whole virtual desktop</text>
</g>
</svg>
<figcaption>One conversion on the client, then one per backend on the daemon. The numbers are the laptop's; on the Windows machine the same arithmetic maps a 1400-pixel-wide image onto its 2560×1600 screen.</figcaption>
</figure>


Before moving anything, the daemon checks that the point is inside the combined display area,
limits scrolling to 100 wheel steps, and rejects keys the platform does not have. A drag always
releases the button, even if a move in the middle fails.

## Per platform

The daemon uses different operating-system APIs on each platform. The table lists what it uses
and the platform behaviour that determined how it is installed.

| Platform | Capture | Input | Windows and focus | Runs as | What shaped it |
|---|---|---|---|---|---|
| Linux, Wayland | portal Screenshot, then wlr-screencopy | wlr virtual pointer and keyboard | `hyprctl` on Hyprland | systemd user unit | GNOME and KDE lack the wlr input protocols: capture works there, input does not yet |
| Linux, X11 | xcb | XTEST | window list only | systemd user unit | xcap reports geometry divided by DPI scale; input wants raw pixels, so rdc multiplies back |
| macOS | `screencapture` (about 0.3 s); CoreGraphics fallback is slow on recent macOS | CGEvent via enigo | xcap list, NSRunningApplication | LaunchAgent inside a signed `rdc.app` | Screen Recording and Accessibility grants are keyed to the code signature; unsigned builds lose them on every rebuild |
| Windows | GDI / Graphics Capture | `SendInput` normalised over the virtual desktop | xcap list, SetForegroundWindow | Task Scheduler logon task at standard integrity (`--elevated` opts into the highest run level) | A service or SSH session is session 0 with no display; a non-elevated daemon cannot send input to elevated windows, which is the documented trade-off |

## What gets recorded

One JSON object per request or rejection, appended to `audit.jsonl` in the platform state
directory, mode 0600, rotated by size. Typed text is recorded as a character count only. Key
chords, window selectors and error messages are recorded with control characters replaced.

```json
{"ts":"2026-09-09T16:08:55.979Z","peer":"100.64.0.7","login":"alice@example.com","node":"laptop",
 "method":"POST","path":"/v1/act","action":"input.click 100,100 Left x1",
 "outcome":"denied","status":403,"detail":"alice@example.com may not use `input` on this machine","ms":0}
```

Read it on the daemon machine with `rdc audit -n 50`, or `--json` for the raw lines.

## Security notes

- Anyone with an `input` grant controls the keyboard. A desktop session is enough to open a
  shell, so rdc does not offer one separately. Grant `view` broadly and `input` narrowly.
- The tailnet is the security boundary. A stolen device that is on the allowlist, or a Tailscale
  policy that lets the wrong nodes reach port 7770, gives access to the desktop.
- The Host check protects against browsers. A web page on an allowed machine could point its
  hostname at the daemon's address and use that machine's identity. The daemon answers 421 to any
  Host that is not one of its own names.
- The agent never handles a password or a tailnet key. It receives images and returns pixel
  coordinates.
- macOS asks again for Screen Recording about once a month. Until someone approves the prompt,
  screenshots show only the wallpaper. `rdc doctor` reports the missing permission.

Full threat model: [Security](security.md).
