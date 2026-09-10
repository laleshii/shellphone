---
id: architecture
title: Architecture
altitude: 0
topics:
- cli
relations:
- type: references
  target: project-overview
- type: references
  target: session-guard
- type: references
  target: herdr-bridge
summary: 'High-level architecture: how the CLI, PTY, server, auth, tunnel, TLS, herdr bridge, and frontend modules compose.'
---

# Architecture

shellphone is structured as eight Rust modules plus a bundled frontend:

```
main.rs       →  CLI parsing, orchestration, QR code rendering
auth.rs       →  one-time token + refresh token, constant-time comparison
protocol.rs   →  typed WebSocket messages (ClientMessage / ServerMessage)
pty_bridge.rs →  spawns command in PTY, returns channel handles + exit signal
herdr/        →  mirrors a running herdr session: socket API + per-pane control child
server.rs     →  axum HTTP/WS server with embedded frontend, TLS, backend dispatch
tls.rs        →  self-signed cert generation via rcgen
tunnel/       →  provider abstraction: cloudflared, ngrok, bore, tailscale, custom
frontend/     →  wterm single-page apps (index.html for PTY, herdr.html for herdr)
```

`server.rs` serves one of two backends per process: `BackendConfig::Pty` for `run`/`agent`, or `BackendConfig::Herdr` for `shellphone herdr` (see [[herdr-bridge]]). Auth, tunnel and QR handling are shared through `main::serve`.

## Data flow

1. `main` generates a 256-bit random token and calls `pty_bridge::spawn`, which returns a `CommandTx` (for stdin + resize), an `EventRx` (for stdout + exit), and an `ExitRx` (oneshot for process exit).
2. `server::start` wraps these into `AppState`, binds the server (with optional TLS via `axum-server` + `rustls`), and returns a `Notify` for client-connected events.
3. A tunnel provider is started (if not `--tunnel none`) to expose the local server.
4. The QR code is printed; when a client connects, it's cleared from the terminal via ANSI escape codes.
5. The browser authenticates via [[session-guard]], receives a refresh token, and the WebSocket bridges the PTY via the [[websocket-protocol]].
6. `main` exits when the child process exits or ctrl+c is pressed.

## Key dependencies

| Crate | Role |
|---|---|
| `axum` + `axum-server` | HTTP/WS server with TLS |
| `portable-pty` | Cross-platform pseudo-terminal |
| `rust-embed` | Compile-time frontend embedding |
| `tokio` | Async runtime |
| `clap` | CLI argument parsing |
| `qrcode` | QR code generation |
| `rcgen` + `sha2` | Self-signed cert generation |
| `subtle` | Constant-time token comparison |
| `serde` + `serde_json` | WebSocket message serialization |
