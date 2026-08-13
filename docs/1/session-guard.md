---
id: session-guard
title: Session guard (auth system)
altitude: 1
topics:
- auth
relations:
- type: refines
  target: architecture
summary: 'One-time token auth with refresh token for reconnection and constant-time comparison.'
---

# Session guard

`src/auth.rs` implements the layered authentication mechanism.

## Token generation

`generate_token()` fills 32 random bytes via `rand::rng().fill()` and encodes them as URL-safe base64 (no padding). This produces a 43-character token with 256 bits of entropy.

## Authentication flow

`SessionGuard::authenticate` accepts two optional tokens and returns one of three results:

1. **`NewSession { refresh_token }`** — the initial one-time token matched and was consumed. A refresh token is generated and returned for the client to store.
2. **`Reconnected`** — a valid refresh token was provided. No new refresh token is issued.
3. **`Failed`** — neither token was valid, or the initial token was already consumed.

The refresh token is stored server-side in `Arc<Mutex<Option<String>>>` and persists for the lifetime of the shellphone process. The client stores it in `localStorage` under the key `shellphone_refresh_token`. On process exit, the frontend clears it.

## Constant-time comparison

All token comparisons use `subtle::ConstantTimeEq` to prevent timing attacks. The function first checks length equality (which leaks length but not content), then does a byte-by-byte constant-time comparison.

## Security properties

- The one-time token is consumed on first use — intercepting it after connection is useless.
- The refresh token allows reconnection without a new QR scan but dies with the server process.
- The server notifies `main` via `Arc<Notify>` when a client authenticates, which triggers QR code clearing from the terminal.
