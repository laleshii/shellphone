---
id: herdr-bridge
title: herdr bridge
altitude: 1
topics:
- server
- frontend
relations:
- type: refines
  target: architecture
- type: depends_on
  target: http-server
- type: depends_on
  target: session-guard
- type: references
  target: frontend-terminal
summary: 'How `shellphone herdr` mirrors a running herdr session to the phone: socket API for the pane tree, a per-pane `terminal session control` child for bytes, and the mobile pane picker.'
---

# herdr bridge

`shellphone herdr` replaces the single PTY of `shellphone run` with a live view of a running [herdr](https://herdr.dev) server: every workspace, tab and pane, with agent status, and one pane attached at a time as a terminal sized for the phone.

The module lives in `src/herdr/`. Nothing in it talks to a PTY; herdr owns all the processes.

## Two channels into herdr

herdr exposes two surfaces and the bridge uses both:

| Surface | Used for | Implemented in |
|---|---|---|
| Unix socket API (`~/.config/herdr/herdr.sock`, newline-delimited JSON `{id, method, params}`) | `session.snapshot` for the pane tree, `events.subscribe` to learn when it changes, `pane.focus` for "focus on desktop" | `api.rs` |
| `herdr terminal session control <pane> --takeover --cols N --rows N` child process | Live ANSI frames of one pane rendered at phone size, keyboard input, resize, scrollback | `controller.rs` |

The controller child prints newline-delimited `terminal.frame` records (`bytes` is base64 ANSI, `full` marks a whole-screen repaint) and `terminal.closed`. It reads `terminal.input` (`text` or base64 `bytes`), `terminal.resize`, `terminal.scroll` (`direction`, `lines`) and `terminal.release` on stdin.

Read-only `terminal session observe` was rejected: it crops the desktop-sized screen instead of re-laying it out, so a 40-column phone would only see a corner of a 148-column pane. Control mode with `--takeover` resizes the real terminal to the phone while attached; herdr restores the desktop size as soon as the controller releases (verified against 0.9.0).

## Hub

`hub.rs` holds the single shared state behind `Arc<Hub>`:

- the last `Snapshot` (trimmed `session.snapshot`: workspaces, tabs, panes, focused ids)
- at most one `Controller` (the attached pane) plus the last requested cols/rows
- a `broadcast` channel of `HubEvent`s: `Snapshot`, `Attached`, `Detached`, `Frame`, `Closed`

A watcher task subscribes to structural events (workspace/tab/pane created, closed, focused, renamed, moved, agent detected) and to `pane.agent_status_changed` per known pane. Any event marks the snapshot dirty; after a 120 ms debounce it re-fetches `session.snapshot` and broadcasts it. A 3 s poll covers anything the subscription misses. Because the per-pane status subscription needs pane ids, the watcher re-subscribes whenever a pane is created, closed, moved or exits. Five consecutive failed connections mean the herdr server is gone and the hub emits `Closed`, which ends the process.

Switching panes is `attach(pane_id)`: release the old controller (send `terminal.release`, wait, kill), spawn a new one at the current size, broadcast `Attached`. Waiting for the old reader task to finish before broadcasting means no stale frame from the previous pane can arrive after the clear.

## Repaint on reconnect

The controller keeps a replay buffer: the last full frame plus every delta since. A browser that connects or reconnects receives a clear sequence and that buffer, so it sees the current screen without disturbing the desktop. If the buffer grows past 4 MB the hub instead asks herdr for a fresh full frame by resending the current size (herdr answers a same-size resize with a full frame). The same path handles a lagging broadcast receiver.

## WebSocket protocol (herdr mode)

`src/herdr/protocol.rs`. Binary frames carry ANSI bytes for the attached pane, as in PTY mode. Text frames:

Client to server: `input {data}`, `resize {cols, rows}`, `attach {pane_id}`, `detach`, `scroll {direction, lines}`, `focus {pane_id}`.

Server to client: `refresh_token` (shared with PTY mode), `snapshot {workspaces, tabs, panes, focused_*_id, attached_pane_id}`, `attached {pane_id}` (preceded by a binary clear), `detached {pane_id, reason}`, `error {message}`, `exit`.

`server.rs` picks the backend per process via `BackendConfig::Pty | Herdr`; the same [[session-guard]] one-time token and refresh token protect both, and `GET /` serves `herdr.html` instead of `index.html` in herdr mode.

## Frontend

`frontend-src/herdr.mjs` and `herdr.html` build to `frontend/herdr.js` and `frontend/herdr.html`. The top bar shows workspace › tab and the pane title with a status dot; ‹ › cycle through panes in sidebar order; ☰ opens a full-screen list grouped by workspace and tab with a "Needs attention" section first (blocked, then done); ⌖ focuses the attached pane in the desktop herdr. On first snapshot the page attaches `?pane=` if given, else herdr's focused pane.

## Touch scrolling

herdr, not wterm, owns the scrollback: frames are absolute-positioned repaints of the viewport, so wterm's own buffer never fills and local scrolling has nothing to show. Swipes therefore become `scroll {direction, lines}` messages. Three details were learned the hard way:

- **Pointer capture, not touch events.** Every incoming frame re-renders wterm's rows, which detaches the DOM node a touch started on. The browser keeps aiming the rest of that gesture at the detached node, so `touchmove` listeners on `#terminal` saw exactly one move per swipe. `pointerdown` now calls `setPointerCapture`, which pins the whole gesture to the container; `touch-action: none` on `#terminal` and a `preventDefault` on `touchmove` stop the page from scrolling instead.
- **One herdr command per line.** In the normal buffer `terminal.scroll` honours `lines` exactly. On the alternate screen (Claude Code, less, vim) herdr turns each command into a single wheel tick and ignores `lines`, so the hub expands one `scroll` message into that many single-line commands written in one batch. herdr applies rapid batches fully and renders at ~60 frames/s, so this costs nothing measurable.
- **Ratio and fling.** Moves are coalesced per animation frame and sent 1:1 (one finger row scrolls one buffer row; 3x felt far too fast on a real phone). Release with velocity starts a fling that keeps queueing rows with 0.95 friction per frame. On the alternate screen the app decides how far one tick moves, so full-screen apps scroll somewhat faster than shells.

A drag can be reproduced without a phone through Chromium's DevTools protocol: `Emulation.setTouchEmulationEnabled` followed by `Input.dispatchTouchEvent` exercises the real touch pipeline, which is how the detached-target bug was found (agent-browser's `set device` only emulates viewport and user agent, and its iOS provider needs Xcode).

## Socket resolution

`--socket` wins, then `--session <name>` maps to `<config>/sessions/<name>/herdr.sock`, then `$HERDR_SOCKET_PATH` (set inside herdr panes, so running shellphone from a pane targets that session), then `<config>/herdr.sock`. `<config>` is `$XDG_CONFIG_HOME/herdr` or `~/.config/herdr`. The controller child gets the resolved socket via `HERDR_SOCKET_PATH` and has the pane-context variables removed so it never targets the caller's pane by accident.
