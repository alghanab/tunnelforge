# TunnelForge

**A self-hosted proxy tunnel manager for VPS owners.**

Turn any internet connection into a managed proxy service with subscriptions, user limits, and multi-protocol support.

## What Is This?

TunnelForge lets you take a VPS with a clean IP and expose proxy tunnels through it for your users. You bring the connection (paqet, v2ray config, SOCKS, HTTP, or any upstream proxy), and TunnelForge handles:

- **Exposing it** as VLESS, VMess, Trojan, or MTProto endpoints
- **Managing users** with subscription plans
- **Limiting usage** by data cap, expiry date, and simultaneous devices
- **Generating links** ready for v2rayNG, Hiddify, Nekobox, and Telegram

```
┌─────────────┐      ┌──────────────────────┐      ┌─────────────┐
│  Upstream   │      │    Your VPS          │      │   Users     │
│  Connection │─────▶│    + TunnelForge     │─────▶│   (devices) │
│             │      │                      │      │             │
│ paqet KCP   │      │ VLESS    MTProto     │      │ v2rayNG     │
│ v2ray link  │      │ VMess    Trojan      │      │ Telegram    │
│ SOCKS/HTTP  │      │ Subscriptions        │      │ Hiddify     │
│ direct      │      │ User limits          │      │ Nekobox     │
└─────────────┘      └──────────────────────┘      └─────────────┘
```

## Use Cases

- **You have a VPS with a clean IP** and want to share internet access with friends/family with usage limits
- **You have working v2ray/xray configs** and want to re-expose them through your VPS for others
- **You run paqet KCP tunnels** to exit servers and want to manage access to them
- **You need subscription management** — data caps, expiry dates, device limits for multiple users
- **You want to resell proxy access** with automated link generation and user enforcement

## Features

### Multi-Source Input

Add any upstream connection:

```bash
# paqet KCP tunnel (recommended — best DPI resistance)
tunnelforge node add london --type paqet --server 1.2.3.4:8443 --key "your-key"

# Existing v2ray/xray config link
tunnelforge import add "vless://uuid@server:443?security=tls&type=ws..."
tunnelforge import add "vmess://base64..."
tunnelforge import add "trojan://password@server:443..."

# Raw JSON config file
tunnelforge import add /path/to/config.json

# Direct SOCKS5 or HTTP proxy
tunnelforge node add myproxy --type direct --server socks5://1.2.3.4:1080
```

### Multi-Protocol Output

Expose connections as protocols your users' apps understand:

- **VLESS + WebSocket + TLS** — looks like normal HTTPS traffic
- **VMess + WebSocket** — widely supported
- **Trojan + TLS** — lightweight, TLS-based
- **MTProto** — Telegram proxy (TLS obfuscation)

### Subscription Management

Create plans and assign users:

```bash
# Create a plan: 50GB data, 30 days, 2 devices max
tunnelforge plan create basic --data 50GB --duration 30d --devices 2

# Create a user under that plan
tunnelforge user add alice --plan basic

# Generate ready-to-use links
tunnelforge link alice
# → vless://uuid@your-vps:443?...#alice
# → tg://proxy?server=your-vps&port=2096&secret=...
# → Subscription URL for v2rayNG
```

### Enforcement

TunnelForge automatically handles limit violations:

- **Data cap exceeded** → user suspended
- **Subscription expired** → user suspended
- **Too many devices** → warning (configurable: block or allow)
- **Manual disable/enable** → instant control

```bash
# Run enforcement check
tunnelforge enforce

# Check what would happen without acting
tunnelforge enforce --dry-run
```

### Connection Map

See your entire infrastructure at a glance:

```bash
tunnelforge status
# Shows: exit nodes, protocols, active users, data usage, port status

tunnelforge map
# Shows: user → protocol → exit node → internet flow
```

## Quick Start

### 1. Install

```bash
# Download binary (Linux)
curl -LO https://github.com/alghanab/tunnelforge/releases/latest/download/tunnelforge-linux-amd64
chmod +x tunnelforge-linux-amd64
mv tunnelforge-linux-amd64 /usr/local/bin/tunnelforge

# Or build from source
cargo install --git https://github.com/alghanab/tunnelforge
```

### 2. Add a Connection

```bash
# Option A: paqet KCP tunnel (recommended)
tunnelforge node add london --type paqet --server EXIT_SERVER_IP:8443 --key "your-kcp-key"

# Option B: Import existing v2ray config
tunnelforge import add "vless://uuid@server:443?security=tls&type=ws&path=/ws&host=domain&sni=domain" --name my-tunnel

# Option C: Direct SOCKS5 proxy
tunnelforge node add proxy --type direct --server socks5://1.2.3.4:1080
```

### 3. Add Protocols

```bash
# Add VLESS endpoint (auto-picks port)
tunnelforge proto add vless --exit london --port auto

# Add MTProto for Telegram
tunnelforge proto add mtproto --exit london --port auto
```

### 4. Create Users

```bash
# Create subscription plan
tunnelforge plan create monthly --data 100GB --duration 30d --devices 3

# Add users
tunnelforge user add alice --plan monthly
tunnelforge user add bob --plan monthly

# Get their links
tunnelforge link alice
```

### 5. Monitor

```bash
tunnelforge status      # Dashboard
tunnelforge user list   # All users with stats
tunnelforge enforce     # Auto-disable expired/capped users
```

## Architecture

```
Your VPS (clean IP)
├── TunnelForge (this tool)
│   ├── Exit Nodes ─── paqet / v2ray / direct connections
│   ├── Protocols ──── VLESS / VMess / Trojan / MTProto
│   ├── Subscriptions  Plans with limits
│   ├── Users ───────── Per-user tracking & enforcement
│   └── Config Store ── SQLite + YAML
│
├── xray ──── Protocol handling (VLESS, VMess, Trojan)
├── Caddy ─── TLS termination, WebSocket routing
├── paqet ─── KCP tunnel to exit servers
└── mtprotoproxy ─── Telegram proxy
```

## Why TunnelForge?

| Feature | 3x-ui | Marzban | TunnelForge |
|---------|-------|---------|-------------|
| Self-hosted | ✓ | ✓ | ✓ |
| CLI-first | ✗ | ✗ | ✓ |
| Web UI | ✓ | ✓ | planned |
| paqet KCP support | ✗ | ✗ | ✓ |
| Import v2ray links | ✗ | partial | ✓ |
| Multi-protocol | ✓ | ✓ | ✓ |
| Subscriptions | ✓ | ✓ | ✓ |
| Device limits | partial | ✓ | ✓ |
| Data caps | ✓ | ✓ | ✓ |
| Single binary | ✗ | ✗ | ✓ |
| Rust performance | ✗ | ✗ | ✓ |

## Commands

```
tunnelforge node add/list/test/remove     # Manage upstream connections
tunnelforge proto add/list/remove          # Manage output protocols
tunnelforge import add/list/expose/remove  # Import v2ray configs
tunnelforge plan create/list/remove        # Subscription plans
tunnelforge user add/list/show/disable     # User management
tunnelforge link <user>                    # Generate user links
tunnelforge status                         # Dashboard
tunnelforge map                            # Connection flow map
tunnelforge ports                          # Port scanner
tunnelforge enforce                        # Run limit enforcement
```

## Requirements

- Linux VPS (Ubuntu 22/24, Debian 12 recommended)
- xray (for VLESS/VMess/Trojan — auto-installed if missing)
- Caddy (for TLS — auto-installed if missing)
- paqet v1.0.0-alpha.17 (for KCP tunnels — auto-installed if missing)

## License

MIT
