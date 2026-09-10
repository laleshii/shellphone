# Changelog

## 0.4.1

### Changed

- `--tunnel tailscale` now serves directly over the tailnet: shellphone binds to the node's Tailscale IPv4 and prints the MagicDNS URL. No relay, no Funnel to enable, lowest latency for the herdr view
- The previous Funnel behaviour moved to `--tunnel tailscale-funnel` (still what `auto` falls back to)
- Switching herdr panes no longer focuses the terminal; tap it when you want the keyboard

## 0.4.0

### Added

- `shellphone herdr [PANE]` — drive a running [herdr](https://herdr.dev) session from your phone: browse workspaces, tabs and panes with live agent status, attach to one pane at a time, switch with ‹ › or the pane list, and focus a pane on the desktop
- Pane list groups by priority (needs you, done, working, idle, shells) by default, or by workspace and tab; the choice is remembered
- Panes are rendered at phone size through `herdr terminal session control --takeover`; herdr restores the desktop size when you detach
- `--session <name>`, `--socket <path>` and `--herdr-bin <path>` to pick the herdr server; `$HERDR_SOCKET_PATH` is honoured when running from inside a herdr pane
- Reconnecting phones get the current screen replayed instead of a blank terminal
- Swipe scrolling of herdr's scrollback with fling; one finger row scrolls one row, full-screen apps receive wheel ticks

### Changed

- `server.rs` now dispatches on a backend (`Pty` or `Herdr`); `main::serve` owns the shared tunnel, QR and auth flow

## 0.3.1

### Added

- `shellphone attach <url>` — connect to a running session from another terminal, no browser needed
- Auto-reconnect on connection drop using refresh token (same mechanism as the web client)
- `-k` / `--insecure` flag to accept self-signed TLS certificates
- opencode agent support (`shellphone agent opencode`)

## 0.3.0

### Added

- `shellphone agent` subcommand — resume AI coding agent sessions (Claude, Codex, opencode)
- Replaced xterm.js with [wterm](https://github.com/vercel-labs/wterm) — DOM-based rendering with native text selection, smooth scrolling, and smaller bundle (66KB vs 295KB)
- TUI scroll support — swipe gestures dispatch native wheel events in alternate buffer mode, scrolling works in Claude Code, vim, less, etc.

### Changed

- Frontend now uses an esbuild build step (`frontend-src/` → `frontend/`)
- Touch scroll in alternate buffer uses `passive: false` with `preventDefault()` so the browser doesn't steal the gesture mid-swipe

## 0.2.1

### Fixed

- Touch scrolling now works on real phones — handler moved to `.xterm-screen` (the visible layer) instead of `.xterm-viewport` (hidden underneath)
- TUI app scrolling (Claude Code, vim, less) — swipe gestures send mouse wheel escape sequences in alternate buffer mode
- Replaced Ctrl toggle with dedicated toolbar buttons (Esc, Tab, arrows, Enter)
- Fixed row height calculation for scroll accumulator

## 0.2.0

### Added

- Mobile toolbar with Esc, Tab, arrow keys, and Enter buttons (touch devices only)
- Virtual keyboard support — terminal resizes when the keyboard appears/disappears
- Touch scroll improvement — 3x scroll multiplier over xterm.js default
- Scrollback buffer increased to 5000 lines

## 0.1.0

Initial release.

- Single binary with embedded frontend (xterm.js + fit addon)
- Tunnel auto-detection (cloudflared, ngrok, bore, tailscale, custom)
- One-time token auth with refresh token for reconnection
- Built-in TLS with self-signed certificates
- Terminal resize negotiation between browser and PTY
- Graceful shutdown on command exit
