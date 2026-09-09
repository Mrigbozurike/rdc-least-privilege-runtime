# Windows

**Status: compiles in CI, not yet tested on a real machine.** Treat everything here as a plan
until someone confirms it.

## What is implemented

- Capture via xcap (GDI / Windows Graphics Capture).
- Mouse and keyboard via `SendInput` (enigo), including per-monitor DPI handling.
- Clipboard via arboard.
- Tailscale identity through the LocalAPI named pipe
  `\\.\pipe\ProtectedPrefix\Administrators\Tailscale\tailscaled`, with the `tailscale.exe` CLI
  as fallback.
- Config at `%APPDATA%\rdc\config.toml`.

## Not implemented yet

- **Multiple monitors.** Absolute mouse positioning currently normalises against the primary
  monitor, so only the primary display can be targeted reliably. Secondary and negative-origin
  displays need a `SendInput` path using the virtual-desktop extents.
- `focus` (bringing a window forward) returns "unsupported". `SetForegroundWindow` has
  focus-stealing rules that need care.
- `rdc service install`. Run `rdc serve` from a Task Scheduler task at logon, or a shortcut in
  `shell:startup`, for now.

## Running

```powershell
rdc doctor
rdc serve --allow you@example.com
```

Windows Firewall will ask whether to allow `rdc.exe`; allow it on private networks. The daemon
binds only the Tailscale interface's address regardless.

## Known caveats

- Input to windows running at a higher integrity level than `rdc serve` (UAC prompts, apps run
  as administrator) is blocked by Windows. Run rdc elevated if you need that.
- The release binary is unsigned; SmartScreen will warn on first run.

If you test on Windows, please open an issue with your results, even if everything works.
