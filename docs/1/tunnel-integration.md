---
id: tunnel-integration
title: Tunnel providers
altitude: 1
topics:
- tunnel
relations:
- type: refines
  target: architecture
summary: 'Provider-based tunnel system: cloudflared, ngrok, bore, tailscale (direct tailnet), tailscale funnel, custom command, and auto-detection.'
---

# Tunnel providers

`src/tunnel/` implements a provider-based tunnel system. Each provider spawns an external process, parses a tunnel URL from its stdout/stderr, and returns it for the QR code.

## Provider enum

`Provider::start(local_port)` dispatches to the appropriate module:

| Variant | Binary | URL pattern | Notes |
|---|---|---|---|
| `Cloudflared` | `cloudflared tunnel --url http://127.0.0.1:{port}` | `*.trycloudflare.com` | Quick tunnel, no account needed |
| `Ngrok` | `ngrok http {port} --log stdout` | `*.ngrok.*` | Requires ngrok account for custom domains |
| `Bore` | `bore local {port} --to {server}` | any `https://` | Default server: `bore.pub` |
| `Tailscale` | `tailscale status --json` (no process) | `http://<node>.<tailnet>.ts.net:{port}` | Direct tailnet access: `main::serve` binds to the IPv4 from `tailscale ip -4` instead of loopback, so only tailnet peers reach it. Plain HTTP; WireGuard encrypts the path. Nothing to enable on the tailnet. |
| `TailscaleFunnel` | `tailscale funnel {port}` | `*.ts.net` | Public exposure through Tailscale's ingress relays. Requires Funnel enabled on the tailnet, otherwise the command prints an enable link and the URL wait times out. Used by `auto`. |
| `Custom` | user-provided command with `{port}` placeholder | any `https://` | 30s URL parse timeout |
| `Auto` | probes `which` for each provider in order | — | Falls back with a helpful error message |

## Stream parsing

`parse_url_from_output` is the shared URL extraction logic. It spawns separate tasks for stdout and stderr (reading them concurrently, not sequentially — this was a critical bug fix) and sends the first matching URL through a `oneshot` channel with a configurable timeout.

## Gotcha: cloudflared quick tunnels

cloudflared prints the URL to stderr before the Cloudflare edge has propagated the tunnel routing. The URL may return HTTP 530 / error 1033 for several seconds. The client-side auto-reconnect handles this gracefully.
