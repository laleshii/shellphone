---
id: frontend-terminal
title: Frontend terminal UI
altitude: 1
topics:
- frontend
relations:
- type: refines
  target: architecture
summary: 'The xterm.js-based mobile-friendly terminal UI with vendored assets, reconnection, and resize support.'
---

# Frontend terminal UI

`frontend/index.html` is a single-file web app that renders a terminal using xterm.js. It's embedded into the Rust binary at compile time via `rust-embed`.

## Vendored dependencies

xterm.js and the fit addon are vendored in `frontend/vendor/` and served via `/assets/vendor/...`. No CDN dependency — the binary works fully offline.

## Connection flow

1. Checks `localStorage` for a refresh token. If present and no `?token=` in the URL, connects with the refresh token. Otherwise uses the initial token from the URL.
2. Opens a WebSocket to `/ws?token=...` or `/ws?refresh=...`, using `wss:` if the page was loaded over HTTPS.
3. On successful connection, sends a resize message with the current terminal dimensions.

## Message handling

- **Binary frames** (server → client): raw PTY output, written to the terminal via `term.write(new Uint8Array(data))`.
- **JSON text frames** (server → client): `refresh_token` (stored in localStorage) and `exit` (shows exit message, sets `sessionEnded` flag).
- **JSON text frames** (client → server): `input` (keystrokes from `term.onData`) and `resize` (from `fitAddon.fit()` on window/container resize).

## Reconnection

On WebSocket close, the client reconnects after 2 seconds using the refresh token — unless `sessionEnded` is true (process exited), in which case it shows "Session closed" and stops.

## Status indicator

A fixed-position bar at the top shows: "Connecting...", "Connected" (green, auto-hides), "Reconnecting..." (yellow), or "Session closed" (red, persistent).
