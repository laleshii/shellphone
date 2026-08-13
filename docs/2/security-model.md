---
id: security-model
title: Security model
altitude: 2
topics:
- auth
- tunnel
relations:
- type: refines
  target: session-guard
- type: references
  target: tunnel-integration
summary: 'The layered security design: token auth, session binding, refresh tokens, TLS, and tunnel encryption.'
---

# Security model

shellphone's security is layered — each layer is independent so defense in depth holds even if one is bypassed.

## Active layers

1. **Localhost binding:** The server binds to `127.0.0.1:3845` by default. Without a tunnel or `--tls --bind 0.0.0.0`, it's only reachable from the same machine.
2. **One-time token:** A 256-bit random token is generated per session, passed as a URL query parameter, and consumed on first WebSocket connection. See [[session-guard]].
3. **Constant-time comparison:** All token comparisons use `subtle::ConstantTimeEq` to prevent timing side-channels.
4. **Session binding via refresh token:** After the one-time token is consumed, reconnection is only possible with a refresh token that was delivered over the authenticated WebSocket. It dies with the server process.
5. **QR code clearing:** The token URL is removed from the terminal display once a client connects, reducing the exposure window.
6. **TLS (tunnel or built-in):** Cloudflare Tunnel provides HTTPS/WSS automatically. `--tls` generates a self-signed cert and prints the SHA-256 fingerprint for trust-on-first-use.

## Token lifecycle is not a weakness

The one-time token appears in the URL query string, which would normally be a concern (browser history, proxy logs). However, since it's consumed on first use, any captured copy is useless after connection. The only vulnerability window is between QR display and first connection — typically seconds — and requires either terminal access or network interception.
