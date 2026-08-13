# shellphone

Pipe any CLI command to a secure, mobile-friendly web terminal. Run a command, scan the QR code, interact from your phone.

```
shellphone run htop
shellphone run --tunnel cloudflared bash
shellphone run --tunnel none --tls docker logs -f my-app
```

## How it works

shellphone spawns your command in a pseudo-terminal, serves an xterm.js web UI over WebSocket, and optionally tunnels it to the internet. Authentication uses a one-time token embedded in the QR code — consumed on first connection, replaced with a refresh token for reconnection.

## Features

- **Single binary** — frontend vendored and embedded at compile time
- **Tunnel auto-detection** — cloudflared, ngrok, bore, tailscale, or custom commands
- **One-time token auth** with refresh token for reconnection
- **Built-in TLS** — self-signed certs with `--tls`, fingerprint printed for TOFU
- **Terminal resize** — negotiated between browser and PTY
- **Graceful shutdown** — exits when the command finishes

## Install

```
cargo install --path .
```

## Usage

```
shellphone run [OPTIONS] <CMD>...

Options:
    --tunnel <PROVIDER>    auto, cloudflared, ngrok, bore, tailscale, custom, none [default: auto]
    --tunnel-cmd <CMD>     Custom tunnel command ({port} placeholder)
    --tls                  Enable built-in TLS with self-signed certificate
    --bind <ADDR>          Bind address [default: 127.0.0.1:3845]
```

## Security

1. Server binds to localhost only (unless `--tls` with `--bind 0.0.0.0`)
2. 256-bit one-time token, consumed on first connection
3. Constant-time token comparison (`subtle`)
4. Refresh token for reconnection, dies with the process
5. TLS via tunnel or built-in self-signed certs
6. QR code cleared from terminal after client connects
