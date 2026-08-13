---
id: websocket-protocol
title: WebSocket protocol
altitude: 2
topics:
- server
- frontend
relations:
- type: refines
  target: http-server
- type: references
  target: frontend-terminal
summary: 'The typed JSON message protocol over WebSocket: ClientMessage and ServerMessage.'
---

# WebSocket protocol

`src/protocol.rs` defines the typed message format used over the WebSocket connection. Binary frames carry raw PTY output; text frames carry JSON control messages.

## ClientMessage (browser → server)

```rust
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}
```

- **Input:** keystroke data from `term.onData`. Serialized as `{"type": "input", "data": "ls\n"}`.
- **Resize:** terminal dimensions after `fitAddon.fit()`. Serialized as `{"type": "resize", "cols": 120, "rows": 40}`.

## ServerMessage (server → browser)

```rust
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    RefreshToken { token: String },
    Exit { code: Option<u32> },
}
```

- **RefreshToken:** sent once after successful initial authentication. The client stores the token in localStorage for reconnection.
- **Exit:** sent when the child process terminates. The client shows the exit code and stops reconnection attempts.

## Design decision

PTY output stays as binary WebSocket frames for performance — no JSON wrapping overhead for high-throughput terminal output. Control messages are infrequent and benefit from structured typing.
