# shellphone

Pipe any CLI command to a secure, mobile-friendly web terminal. Run a command, scan the QR code, interact from your phone.

```
# shellphone run [OPTIONS] <CMD>...
$ shellphone run --tunnel cloudflared claude --resume my-session-id

 ▄▄▄▄▄▄▄ ▄▄▄       ▄▄▄▄▄▄▄
 █ ▄▄▄ █ ▀▄ ▀█▄▀▄▄ █ ▄▄▄ █
 █ ███ █ ▀█▀  ▀█▀█ █ ███ █
 █▄▄▄▄▄█ ▄ ▄▀▄ ▄ ▄ █▄▄▄▄▄█
 ▄ ▄▄ ▄▄▄▀▀▄▄█ ▄   ▄  ▄ ▄▄
 ▄▄ ███▄ ▄ ▀ ██ ▀▀▄ ▄▄█▀
   ▄▀ ▀▄▄ ▀▄▄ ██▄▀▄▄▀▄▀█
  █ ▄ ▀▄▀██▀▀▄▄█▀▄▄▀█▀▄ █▀
 ▀ ▀▀█▀▄▀███▄▀▄ ▀█▄█▄▄▀▄ ▀
 ▄▄▄▄▄▄▄ ██▀▄█▀▄▀█ ▄ █▄▀█▄
 █ ▄▄▄ █ ▀█▄▄█▄ ▀█▄▄▄█▀ █▀
 █ ███ █ █▄▀▄██  ▀██ ██▀█▀
 █▄▄▄▄▄█ ▄ ▀ ▄ ▄▀ █▄ ▀▀█▄▄

→ https://my-cloudflare-tunnel.trycloudflare.com/t/a1b2c3d4e5f6
```

## How it works

shellphone spawns your command in a pseudo-terminal, serves a wterm web UI over WebSocket, and optionally tunnels it to the internet. Authentication uses a one-time token embedded in the QR code — consumed on first connection, replaced with a refresh token for reconnection.

## Features

- **Single binary** — frontend bundled and embedded at compile time
- **wterm terminal** — DOM-based rendering with native text selection and smooth scrolling
- **Mobile toolbar** — Esc, Tab, arrow keys, Enter (touch devices only)
- **TUI scroll support** — swipe to scroll in Claude Code, vim, less, etc.
- **Agent shortcuts** — `shellphone agent claude` to resume sessions directly
- **herdr on your phone** — `shellphone herdr` lists every workspace, tab and pane of a running [herdr](https://herdr.dev) session with live agent status; tap to attach, swipe through panes
- **Tunnel auto-detection** — cloudflared, ngrok, bore, tailscale, or custom commands
- **One-time token auth** with refresh token for reconnection
- **Built-in TLS** — self-signed certs with `--tls`, fingerprint printed for TOFU
- **Virtual keyboard support** — terminal resizes when the keyboard appears
- **Graceful shutdown** — exits when the command finishes

## Install

Needs a Rust toolchain. If you don't have one:

```sh
brew install rust                                                # macOS (Homebrew)
# or, any platform, via rustup:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # then: source "$HOME/.cargo/env"
```

Then install from crates.io:

```sh
cargo install shellphone
```

Or build from a clone of this repo:

```sh
cargo install --path .     # builds + installs onto PATH
# or just: cargo build --release   → binary at target/release/shellphone
```

## Usage

### Run any command

```
shellphone run [OPTIONS] <CMD>...
```

Examples:

```sh
shellphone run bash
shellphone run --tunnel cloudflared claude --resume my-session
shellphone run --tls --bind 0.0.0.0:3845 htop
```

### Resume an agent session

```
shellphone agent [OPTIONS] [AGENT] [SESSION]
```

Supported agents: `claude`, `codex`, `opencode`

Examples:

```sh
shellphone agent claude                    # Claude's interactive session picker
shellphone agent claude my-session-id      # Resume a specific session
shellphone agent codex                     # Codex interactive picker
shellphone agent opencode                  # Continue last opencode session
shellphone agent                           # Pick from installed agents
```

### Drive herdr from your phone

```
shellphone herdr [OPTIONS] [PANE]
```

Mirror a running [herdr](https://herdr.dev) session. The phone gets a pane picker with idle / working / blocked badges for each agent, grouped by priority (agents that need you first) or by workspace and tab, and one pane attached as a terminal sized to the phone. ‹ › cycle through panes, ☰ opens the list, ⌖ focuses the attached pane in your desktop herdr.

```sh
shellphone herdr                             # default session, starts on herdr's focused pane
shellphone herdr w2:p5                       # attach a specific pane first
shellphone herdr --session agents            # a named herdr session
shellphone herdr --tunnel cloudflared        # same tunnel options as `run`
```

Swipe to scroll: one finger row moves one row of herdr's scrollback, and a flick keeps going. In full-screen apps like Claude Code the swipe is forwarded as mouse wheel ticks.

Requires herdr 0.9 or newer on the same machine. Attaching takes over the pane's terminal size while the phone is connected (like a smaller tmux client); herdr hands the size back to the desktop when you switch away or stop shellphone. Running `shellphone herdr` from inside a herdr pane targets that session via `$HERDR_SOCKET_PATH`. A cloudflared tunnel adds 25–90 ms per round trip; for the smoothest scrolling use `--tunnel tailscale` or `--tls --bind 0.0.0.0:3845` on the same Wi-Fi.

```
    --session <NAME>       Named herdr session (default: the default session)
    --socket <PATH>        Explicit herdr socket path (overrides --session)
    --herdr-bin <PATH>     herdr binary (default: `herdr` on PATH)
```

### Attach from another terminal

```
shellphone attach [OPTIONS] <URL>
```

Connect to a running shellphone session from any terminal — no browser needed. Uses the same one-time token for initial auth, then reconnects automatically via refresh token if the connection drops.

```sh
shellphone attach "http://127.0.0.1:3845?token=abc123"
shellphone attach -k "https://10.0.10.8:3845?token=abc123"   # -k accepts self-signed certs
```

### Network options

```
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

## Building the frontend

The frontend uses [wterm](https://github.com/vercel-labs/wterm) and requires a build step:

```sh
cd frontend-src
npm install
npm run build    # outputs to ../frontend/
```

The built frontend is committed to the repo, so this is only needed when modifying the terminal UI.
