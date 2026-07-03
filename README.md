# TunnelForge

**A self-hosted proxy tunnel manager for VPS owners.**

Turn any internet connection into a managed proxy service with subscriptions, user limits, and multi-protocol support.

## What Is This?

TunnelForge lets you take a VPS and expose proxy tunnels through it for your users. You can either:

- **Use your VPS's own internet** (direct mode) — if your VPS has clean, unblocked internet
- **Route through external exit servers** (paqet mode) — if your VPS IP is blocked/dirty and you need a clean exit

You bring the connection, TunnelForge handles the rest: VLESS, VMess, Trojan, MTProto endpoints, subscription management, user limits, and link generation.

```
┌─────────────┐      ┌──────────────────────┐      ┌─────────────┐
│  Upstream   │      │    Your VPS          │      │   Users     │
│  Connection │─────▶│    + TunnelForge     │─────▶│   (devices) │
│             │      │                      │      │             │
│ paqet KCP   │      │ VLESS    MTProto     │      │ v2rayNG     │
│ v2ray link  │      │ VMess    Trojan      │      │ Telegram    │
│ SOCKS/HTTP  │      │ Subscriptions        │      │ Hiddify     │
│ DIRECT      │      │ User limits          │      │ Nekobox     │
└─────────────┘      └──────────────────────┘      └─────────────┘
```

## Quick Start

### 1. Install

```bash
# Download binary (Linux x86_64)
curl -LO https://github.com/alghanab/tunnelforge/releases/latest/download/tunnelforge-linux-amd64
chmod +x tunnelforge-linux-amd64
sudo mv tunnelforge-linux-amd64 /usr/local/bin/tunnelforge

# Verify
tunnelforge --version
```

### 2. Set Up Your VPS Info

```bash
# Set your VPS IP and domain
tunnelforge config set-vps --ip YOUR_VPS_IP --domain vpn.yourdomain.com
```

### 3. Choose Your Mode

#### Mode A: Direct (Your VPS has clean internet)

If your VPS IP is not blocked by ISPs, you can expose your VPS's internet directly:

```bash
# Add your VPS as a direct exit node
tunnelforge node add local --type direct

# Add VLESS protocol (auto-picks port)
tunnelforge proto add vless --exit local --port auto

# Add MTProto for Telegram
tunnelforge proto add mtproto --exit local --port auto

# Apply configs and start services
tunnelforge service apply --restart
```

#### Mode B: Paqet (Route through external exit server)

If your VPS IP is blocked, use paqet KCP tunnels to route through a clean exit server:

```bash
# Add paqet exit node
tunnelforge node add london --type paqet --server EXIT_IP:8443 --key "your-kcp-key"

# Add protocols
tunnelforge proto add vless --exit london --port auto
tunnelforge proto add mtproto --exit london --port auto

# Apply and start
tunnelforge service apply --restart
```

#### Mode C: Import (You have existing v2ray configs)

If you already have working v2ray/xray config links:

```bash
# Import a vless link
tunnelforge import add "vless://uuid@server:443?security=tls&type=ws&path=/ws&host=domain&sni=domain"

# Import a vmess link
tunnelforge import add "vmess://base64encoded"

# Import a trojan link
tunnelforge import add "trojan://password@server:443"

# Or import a JSON config file
tunnelforge import add /path/to/config.json --name my-config

# Expose it through your VPS
tunnelforge import expose my-config --as vless --port auto
tunnelforge service apply --restart
```

### 4. Create Users

```bash
# Create subscription plan: 50GB, 30 days, 2 devices max
tunnelforge plan create basic --data 50GB --duration 30d --devices 2

# Add users
tunnelforge user add alice --plan basic
tunnelforge user add bob --plan basic

# Get their links
tunnelforge link alice
```

### 5. Monitor

```bash
tunnelforge status          # Dashboard
tunnelforge service status  # All services
tunnelforge user list       # All users with stats
tunnelforge enforce         # Auto-disable expired/capped users
tunnelforge map             # Connection flow
```

## Domain Setup (Required for VLESS+TLS)

VLESS with TLS requires a domain with a valid certificate. TunnelForge uses Caddy for automatic TLS via Let's Encrypt.

### Step 1: Get a Domain

Any domain works. Free options:
- **nip.io** — `YOUR_IP.nip.io` (no setup needed, but no TLS)
- **Cloudflare** — free DNS management
- **Freenom** — free domains (.tk, .ml, etc.)

### Step 2: Configure DNS

Point a subdomain to your VPS IP:

```
Type: A
Name: vpn (or any subdomain)
Value: YOUR_VPS_IP
TTL: 300
```

Example: `vpn.yourdomain.com → 1.2.3.4`

### Step 3: Cloudflare Users (Important!)

If using Cloudflare for DNS:

1. Set the DNS record to **grey cloud** (DNS only, NOT proxied)
   - Orange cloud = traffic goes through Cloudflare (blocked by Iranian ISPs)
   - Grey cloud = traffic goes directly to your VPS ✓

2. Set SSL mode to **Flexible** (in Cloudflare SSL/TLS settings)
   - This lets Caddy handle the TLS on your VPS

3. The domain must resolve directly to your VPS IP, not Cloudflare IPs

### Step 4: Configure TunnelForge

```bash
tunnelforge config set-vps --domain vpn.yourdomain.com
tunnelforge service apply --restart
```

Caddy will automatically get a Let's Encrypt certificate for your domain.

### Step 5: Verify

```bash
# Check Caddy got the cert
curl -I https://vpn.yourdomain.com

# Check VLESS works
tunnelforge link alice
# Use the generated link in v2rayNG
```

### DNS Provider Guides

| Provider | Setup |
|----------|-------|
| Cloudflare | Add A record, grey cloud, Flexible SSL |
| Namecheap | Add A record in Advanced DNS |
| GoDaddy | Add A record in DNS Management |
| Google Domains | Add A record in DNS → Custom records |
| FreeDNS | Add A record at freedns.afraid.org |

## Features

### Exit Nodes

- **paqet** — KCP tunnel to external exit server (best DPI resistance)
- **direct** — Use VPS's own internet (if VPS has clean IP)
- **import** — Use existing v2ray/xray config as upstream

### Protocols

- **VLESS + WebSocket + TLS** — Looks like HTTPS traffic
- **VMess + WebSocket** — Widely supported
- **Trojan + TLS** — Lightweight
- **MTProto** — Telegram proxy with TLS obfuscation

### Subscriptions

- Plans with data limits, duration, device count
- Per-user tracking (bandwidth, IPs, expiry)
- Auto-disable on limit exceeded
- Subscription URL for v2rayNG/Hiddify auto-import

### Service Management

- Auto-generate xray configs, Caddyfile, systemd services
- One command to apply all changes
- Start/stop/restart individual services

## Commands

```
tunnelforge config set-vps           # Set VPS IP and domain
tunnelforge node add/list/test       # Exit node management
tunnelforge proto add/list           # Protocol management
tunnelforge import add/list/expose   # Import v2ray configs
tunnelforge plan create/list         # Subscription plans
tunnelforge user add/list/show       # User management
tunnelforge link <user>              # Generate user links
tunnelforge sub <user>               # Subscription URL (base64)
tunnelforge service apply/start/stop # Service management
tunnelforge status                   # Dashboard
tunnelforge map                      # Connection flow
tunnelforge ports                    # Port scanner
tunnelforge enforce                  # Run limit enforcement
tunnelforge web                      # Start web dashboard
```

## Architecture

```
Your VPS
├── TunnelForge (this tool)
│   ├── Exit Nodes ─── paqet / direct / imported connections
│   ├── Protocols ──── VLESS / VMess / Trojan / MTProto
│   ├── Users ───────── Per-user tracking & enforcement
│   └── Config ──────── ~/.tunnelforge/ (YAML + SQLite)
│
├── xray ──── Protocol handling
├── Caddy ─── TLS termination, WebSocket routing
├── paqet ─── KCP tunnel (optional)
└── mtprotoproxy ─── Telegram proxy
```

## Requirements

- Linux VPS (Ubuntu 22/24, Debian 12)
- Root access
- Domain with DNS pointing to VPS (for TLS)
- xray, Caddy (auto-installed if missing)
- paqet v1.0.0-alpha.17 (only for paqet mode)

## License

MIT
