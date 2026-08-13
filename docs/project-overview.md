---
id: project-overview
title: Project overview
altitude: 0
topics:
- cli
summary: 'What shellphone is: a Rust CLI that bridges a shell command to a secure mobile web terminal via tunnel.'
---

# Project overview

shellphone is a single-binary Rust CLI that runs a shell command in a pseudo-terminal and exposes it through a mobile-friendly web interface over WebSocket. The user scans a QR code from their phone to connect and interact with the running command.

## Usage

```
shellphone run <command...>
shellphone run --tunnel cloudflared bash
shellphone run --tunnel none --tls htop
```

The binary name is `shellphone`, defined in `Cargo.toml` under `[[bin]]`. The `run` subcommand takes a trailing variadic argument, joins it into a single string, and passes it to `sh -c`.

## Single-binary design

The frontend (HTML/JS/CSS in `frontend/`, including vendored xterm.js) is embedded into the Rust binary at compile time via `rust-embed`. No runtime file dependencies, no CDN — fully offline capable.

## Key features

- **Tunnel auto-detection:** tries cloudflared, ngrok, bore, tailscale in order; supports custom commands via `--tunnel-cmd`
- **One-time token auth** with refresh token for reconnection stored in localStorage
- **Built-in TLS** with self-signed certs (`--tls`) and SHA-256 fingerprint for TOFU
- **Compact QR code** using Unicode half-block rendering, cleared from terminal once a client connects
- **Graceful shutdown** when the child process exits
- **Terminal resize** negotiation between browser and PTY via [[websocket-protocol]]
