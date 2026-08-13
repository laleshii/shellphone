---
id: pty-bridge
title: PTY bridge
altitude: 1
topics:
- pty
relations:
- type: refines
  target: architecture
summary: 'How the pseudo-terminal is spawned with resize support, exit detection, and async channels.'
---

# PTY bridge

`src/pty_bridge.rs` spawns the user's command in a pseudo-terminal and exposes it through async channels.

## Spawning

The command string is passed to `sh -c <command>` via `portable_pty::CommandBuilder`. Shell features (pipes, redirects, env vars) work out of the box.

## Channel architecture

`spawn()` returns three handles:

- **`CommandTx`** (`mpsc::Sender<PtyCommand>`) — accepts `Input(Vec<u8>)` for stdin data and `Resize { cols, rows }` for terminal size changes. A dedicated task reads from this channel: input is written to the PTY master, resize calls `master.resize()`.
- **`EventRx`** (`broadcast::Receiver<PtyEvent>`) — emits `Output(Vec<u8>)` for stdout/stderr chunks (up to 4096 bytes) and `Exit(Option<u32>)` when the child process terminates. The PTY reader runs in `spawn_blocking` because `portable_pty` readers are synchronous.
- **`ExitRx`** (`oneshot::Receiver<Option<u32>>`) — a separate signal for `main` to select on alongside ctrl+c, so the process exits when the command finishes.

## Child process lifecycle

A `spawn_blocking` task calls `child.wait()` and sends both a `PtyEvent::Exit` (for WebSocket clients) and a oneshot signal (for `main`). The 500ms delay in `main` after receiving the exit signal ensures the WebSocket exit message reaches the client before the server shuts down.
