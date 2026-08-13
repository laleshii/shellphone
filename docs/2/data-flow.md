---
id: data-flow
title: End-to-end data flow
altitude: 2
topics:
- server
- pty
- frontend
relations:
- type: refines
  target: architecture
- type: references
  target: pty-bridge
- type: references
  target: http-server
- type: references
  target: frontend-terminal
summary: 'Detailed walkthrough of bytes flowing from the PTY through channels, WebSocket, and into xterm.js.'
---

# End-to-end data flow

## Output path (command → phone screen)

```
child process stdout/stderr
  → PTY master reader (blocking, in spawn_blocking thread)
  → broadcast::Sender<PtyEvent::Output> [4096-byte chunks]
  → relay_events task (server-owned broadcast::Sender)
  → per-connection broadcast::Receiver in send_task
  → WebSocket Binary frame
  → browser: new Uint8Array(data) → term.write()
```

## Input path (phone keyboard → command)

```
browser: term.onData(keystroke)
  → ws.send(JSON: {type: "input", data: "..."})
  → WebSocket recv_task: parse ClientMessage::Input
  → cmd_tx.send(PtyCommand::Input(bytes))
  → PTY master writer → child process stdin
```

## Resize path

```
browser: fitAddon.fit() → ResizeObserver/window resize
  → ws.send(JSON: {type: "resize", cols: N, rows: N})
  → recv_task: parse ClientMessage::Resize
  → cmd_tx.send(PtyCommand::Resize { cols, rows })
  → master.resize(PtySize { ... })
```

## Control messages (server → client)

Sent as JSON text frames, distinct from binary PTY output:
- `{"type": "refresh_token", "token": "..."}` — on first auth
- `{"type": "exit", "code": N}` — when child process exits

## Channel sizing

Both channels use a buffer of 256 messages. For output, this means up to ~1MB of buffered PTY data. If the browser can't keep up, the broadcast receiver lags and `RecvError::Lagged` is handled by continuing (skipping missed chunks).
