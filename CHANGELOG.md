# Changelog

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
