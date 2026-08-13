# Changelog

## 0.1.0 — 2026-08-13

Initial release.

- Run any shell command in a PTY and expose it via a mobile-friendly xterm.js web UI
- One-time token authentication with refresh token for reconnection
- Constant-time token comparison via `subtle`
- Tunnel auto-detection: cloudflared, ngrok, bore, tailscale
- Custom tunnel command support (`--tunnel-cmd`)
- Built-in TLS with self-signed certificates (`--tls`)
- Vendored xterm.js — single binary, no CDN dependency
- Terminal resize negotiation between browser and PTY
- Compact QR code rendering with Unicode half-blocks, cleared on client connect
- Graceful shutdown when the child process exits
- Port fallback if default 3845 is taken
