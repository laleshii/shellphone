---
id: frontend-terminal
title: Frontend terminal UI
altitude: 1
topics:
- frontend
relations:
- type: refines
  target: architecture
summary: The xterm.js-based mobile-friendly terminal UI with vendored assets, reconnection, and resize support.
---

# Frontend terminal UI

`frontend-src/` contains the wterm-based terminal source, built with esbuild into `frontend/` which is embedded into the Rust binary via `rust-embed`.

## Build pipeline

1. Source: `frontend-src/terminal.mjs` + `frontend-src/index.html`
2. Build: `cd frontend-src && npm run build` (esbuild bundles ESM + inlined WASM → `frontend/terminal.js`)
3. Embed: rust-embed compiles `frontend/` into the binary at `cargo build` time

## wterm integration

Uses `@wterm/dom` which provides:
- `WTerm` class: terminal constructor, `init()`, `write()`, `resize()`, `destroy()`
- `onData` callback: user input (keystrokes)
- `onResize` callback: terminal resize events
- `autoResize: true`: built-in ResizeObserver, no fit addon needed
- `bridge.usingAltScreen()`: detect alternate screen buffer (TUI apps)
- Built-in WASM (base64-inlined, ~12KB): no separate .wasm file to serve

## Mobile support

- **Toolbar**: fixed bar at bottom with Esc, Tab, arrows, Enter. Shown on touch devices only.
- **Virtual keyboard**: listens to `visualViewport` resize/scroll events, repositions toolbar and resizes terminal.
- **Touch scroll (normal buffer)**: wterm handles natively via DOM scrolling.
- **Touch scroll (alternate buffer)**: touchmove handler dispatches synthetic `WheelEvent` on the terminal element. wterm's input handler converts wheel events to SGR mouse escape sequences for the TUI app. Uses `passive: false` with `preventDefault()` to prevent the browser from stealing the gesture. `bridge.usingAltScreen()` gates alt-buffer mode.

## WebSocket protocol

Same as before: binary frames for PTY output, JSON text for control messages (`input`, `resize`, `refresh_token`, `exit`). Custom WebSocket handling (not wterm's built-in `WebSocketTransport`) to support one-time token auth and refresh tokens.
