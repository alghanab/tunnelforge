use anyhow::Result;
use colored::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use crate::config::ConfigStore;

fn config_dir() -> PathBuf { dirs::home_dir().unwrap_or(PathBuf::from(".")).join(".tunnelforge") }
fn xray_dir() -> PathBuf { config_dir().join("xray") }
fn systemd_dir() -> PathBuf { PathBuf::from("/etc/systemd/system") }

/// Generate all config files from stored state
pub fn apply(cfg: &ConfigStore, restart: bool) -> Result<()> {
    println!("{} Generating configs...", "⚡".yellow());

    fs::create_dir_all(xray_dir())?;

    let mut services_to_restart: Vec<String> = Vec::new();

    // 1. Generate xray configs for each protocol
    for (name, proto) in &cfg.protocols {
        match proto.proto_type.as_str() {
            "vless" | "vmess" | "trojan" => {
                let xray_config = build_xray_config(cfg, name, proto);
                let path = xray_dir().join(format!("{}.json", name));
                fs::write(&path, serde_json::to_string_pretty(&xray_config)?)?;
                println!("  {} xray config: {}", "✓".green(), path.display());

                // Generate systemd service
                let svc_name = format!("tunnelforge-{}", name);
                let unit = build_xray_unit(&svc_name, &path);
                let svc_path = systemd_dir().join(format!("{}.service", svc_name));
                fs::write(&svc_path, unit)?;
                services_to_restart.push(svc_name);
            }
            "mtproto" => {
                let svc_name = format!("tunnelforge-{}", name);
                let unit = build_mtproto_unit(cfg, &svc_name, name, proto);
                let svc_path = systemd_dir().join(format!("{}.service", svc_name));
                fs::write(&svc_path, unit)?;
                services_to_restart.push(svc_name);
            }
            _ => {}
        }
    }

    // 2. Generate paqet services for each node
    for (name, node) in &cfg.exit_nodes {
        if node.node_type == "paqet" {
            let svc_name = format!("tunnelforge-paqet-{}", name);
            let unit = build_paqet_unit(&svc_name, name);
            let svc_path = systemd_dir().join(format!("{}.service", svc_name));
            fs::write(&svc_path, unit)?;
            services_to_restart.push(svc_name);
        }
    }

    // 3. Generate Caddyfile
    let caddyfile = build_caddyfile(cfg);
    fs::write("/etc/caddy/Caddyfile", caddyfile)?;
    println!("  {} Caddyfile", "✓".green());

    // 4. Reload systemd
    Command::new("systemctl").arg("daemon-reload").output()?;
    println!("  {} systemd daemon-reload", "✓".green());

    // 5. Restart services if requested
    if restart {
        // Reload caddy
        let _ = Command::new("caddy").args(["reload", "--config", "/etc/caddy/Caddyfile"]).output();
        println!("  {} Caddy reloaded", "✓".green());

        for svc in &services_to_restart {
            let _ = Command::new("systemctl").args(["restart", svc]).output();
            let active = Command::new("systemctl").args(["is-active", svc])
                .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
            if active == "active" {
                println!("  {} {} started", "✓".green(), svc);
            } else {
                println!("  {} {} failed to start", "✗".red(), svc);
            }
        }
    }

    println!("\n{} Config generation complete. {} services managed.", "✓".green(), services_to_restart.len());
    if !restart {
        println!("  Run {} to apply changes.", "tunnelforge service apply --restart".cyan());
    }
    Ok(())
}

/// Start a specific service
pub fn start(service_name: &str) -> Result<()> {
    let svc = normalize_svc_name(service_name);
    let _ = Command::new("systemctl").args(["start", &svc]).output();
    check_service(&svc)
}

/// Stop a specific service
pub fn stop(service_name: &str) -> Result<()> {
    let svc = normalize_svc_name(service_name);
    let _ = Command::new("systemctl").args(["stop", &svc]).output();
    println!("{} {} stopped", "✓".green(), svc);
    Ok(())
}

/// Restart a specific service
pub fn restart(service_name: &str) -> Result<()> {
    let svc = normalize_svc_name(service_name);
    let _ = Command::new("systemctl").args(["restart", &svc]).output();
    check_service(&svc)
}

/// Show status of all TunnelForge services
pub fn service_status() -> Result<()> {
    let output = Command::new("systemctl").args(["list-units", "--type=service", "--all", "--no-pager", "--no-legend"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    println!("{:<40} {:<10} {}", "SERVICE", "STATUS", "DESCRIPTION");
    println!("{}", "-".repeat(70));

    for line in stdout.lines() {
        if line.contains("tunnelforge") || line.contains("paqet-") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let name = parts[0].replace(".service", "");
                let active = parts[2];
                let status_icon = match active {
                    "active" => "●".green(),
                    "inactive" => "○".dimmed(),
                    "failed" => "✗".red(),
                    _ => "?".yellow(),
                };
                println!("{} {:<38} {:<10} {}", status_icon, name, active, parts[3..].join(" "));
            }
        }
    }
    Ok(())
}

// ── Config Generators ───────────────────────────────────────────

fn build_xray_config(
    cfg: &ConfigStore,
    name: &str,
    proto: &crate::config::ProtoConfig,
) -> serde_json::Value {
    let node = cfg.exit_nodes.get(&proto.exit_node);
    let socks_port = node.map(|n| n.socks_port).unwrap_or(666);
    let uuid = proto.uuid.as_deref().unwrap_or("");
    let ws_path = proto.ws_path.as_deref().unwrap_or("/");
    let tag_in = format!("{}-in", name);
    let tag_out = format!("{}-out", name);

    serde_json::json!({
        "log": {"loglevel": "warning"},
        "inbounds": [{
            "listen": "127.0.0.1",
            "port": proto.port,
            "protocol": "vless",
            "settings": {
                "clients": [{"id": uuid, "encryption": "none"}],
                "decryption": "none"
            },
            "streamSettings": {
                "network": "ws",
                "wsSettings": {"path": ws_path}
            },
            "tag": tag_in
        }],
        "outbounds": [
            {
                "protocol": "socks",
                "settings": {"servers": [{"address": "127.0.0.1", "port": socks_port}]},
                "tag": tag_out
            },
            {"protocol": "freedom", "tag": "direct"}
        ],
        "routing": {
            "domainStrategy": "IPIfNonMatch",
            "rules": [
                {"type": "field", "outboundTag": "direct", "ip": ["geoip:private"]},
                {"type": "field", "outboundTag": tag_out, "network": "tcp,udp"}
            ]
        }
    })
}



fn build_caddyfile(cfg: &ConfigStore) -> String {
    let domain = cfg.vps.domain.as_deref().unwrap_or("vpn.example.com");
    let main_vps = "130.185.77.47"; // TODO: make configurable

    let mut lines = vec![format!("{} {{", domain)];

    for (name, proto) in &cfg.protocols {
        if proto.proto_type == "vless" || proto.proto_type == "vmess" {
            if let Some(ws_path) = &proto.ws_path {
                lines.push(format!("    handle {} {{", ws_path));
                lines.push(format!("        reverse_proxy localhost:{}", proto.port));
                lines.push("    }".to_string());
                lines.push("".to_string());
            }
        }
    }

    lines.push("    respond \"Not Found\" 404".to_string());
    lines.push("}".to_string());
    lines.push("".to_string());
    lines.push(":80 {".to_string());
    lines.push(format!("    reverse_proxy https://{} {{", main_vps));
    lines.push("        transport http {".to_string());
    lines.push("            tls".to_string());
    lines.push("            tls_insecure_skip_verify".to_string());
    lines.push("        }".to_string());
    lines.push("        header_up Host {host}".to_string());
    lines.push("        header_up X-Real-IP {remote_host}".to_string());
    lines.push("        header_up X-Forwarded-Proto https".to_string());
    lines.push("    }".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}

fn build_xray_unit(svc_name: &str, config_path: &PathBuf) -> String {
    format!(r#"[Unit]
Description=TunnelForge Xray ({svc_name})
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/xray run -config {config}
Restart=always
RestartSec=3
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
"#, svc_name = svc_name, config = config_path.display())
}

fn build_mtproto_unit(cfg: &ConfigStore, svc_name: &str, proto_name: &str, proto: &crate::config::ProtoConfig) -> String {
    let node = cfg.exit_nodes.get(&proto.exit_node);
    let socks_port = node.map(|n| n.socks_port).unwrap_or(666);
    let work_dir = format!("/tmp/tunnelforge-mtproto-{}", proto_name);
    let pc_conf = format!("/tmp/tunnelforge-proxychains-{}.conf", proto_name);

    // Create mtprotoproxy config
    let secret = proto.secret.as_deref().unwrap_or("00000000000000000000000000000000");
    // Strip ee prefix and 7777... suffix to get raw secret
    let raw_secret = if secret.starts_with("ee") && secret.len() > 66 {
        &secret[2..34]
    } else {
        secret
    };

    let config_py = format!("PORT = {}\n\nUSERS = {{\n    \"tg\": \"{}\",\n}}\n\nMODES = {{\n    \"classic\": False,\n    \"secure\": False,\n    \"tls\": True\n}}\n\nTLS_DOMAIN = \"www.google.com\"\n",
        proto.port, raw_secret);

    let _ = fs::create_dir_all(&work_dir);
    let _ = fs::write(format!("{}/config.py", work_dir), config_py);

    // Download mtprotoproxy if not exists
    let py_file = format!("{}/mtprotoproxy.py", work_dir);
    if !PathBuf::from(&py_file).exists() {
        let _ = Command::new("curl").args(["-sL", "-o", &py_file,
            "https://github.com/alexbers/mtprotoproxy/raw/master/mtprotoproxy.py"]).output();
    }

    // Create proxychains config
    let pc_content = format!("strict_chain\nproxy_dns\nremote_dns_subnet 224\ntcp_read_time_out 15000\ntcp_connect_time_out 8000\n\n[ProxyList]\nsocks5\t127.0.0.1 {}\n", socks_port);
    let _ = fs::write(&pc_conf, pc_content);

    // Find libproxychains
    let lib_path = find_libproxychains();

    format!(r#"[Unit]
Description=TunnelForge MTProto ({svc_name})
After=network.target

[Service]
Type=simple
WorkingDirectory={work_dir}
Environment="LD_PRELOAD={lib_path}"
Environment="PROXYCHAINS_CONF_FILE={pc_conf}"
ExecStart=/usr/bin/python3 {work_dir}/mtprotoproxy.py
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
"#, svc_name = svc_name, work_dir = work_dir, lib_path = lib_path, pc_conf = pc_conf)
}

fn build_paqet_unit(svc_name: &str, node_name: &str) -> String {
    let config_path = format!("/opt/paqctl/{}.yaml", node_name);
    format!(r#"[Unit]
Description=TunnelForge Paqet ({svc_name})
After=network.target

[Service]
Type=simple
Environment="LD_PRELOAD="
ExecStart=/root/paqet_linux_amd64 run -c {config}
Restart=always
RestartSec=5
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
"#, svc_name = svc_name, config = config_path)
}

fn find_libproxychains() -> String {
    let paths = [
        "/usr/lib/x86_64-linux-gnu/libproxychains.so.4",
        "/usr/lib/x86_64-linux-gnu/libproxychains.so.3",
        "/usr/lib/libproxychains.so.4",
    ];
    for p in &paths {
        if PathBuf::from(p).exists() {
            return p.to_string();
        }
    }
    // Try find
    if let Ok(out) = Command::new("find").args(["/usr/lib", "-name", "libproxychains.so*", "-maxdepth", "3"]).output() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if let Some(first) = stdout.lines().next() {
            return first.to_string();
        }
    }
    "/usr/lib/x86_64-linux-gnu/libproxychains.so.4".to_string()
}

fn normalize_svc_name(name: &str) -> String {
    if name.starts_with("tunnelforge-") {
        name.to_string()
    } else {
        format!("tunnelforge-{}", name)
    }
}

fn check_service(svc: &str) -> Result<()> {
    let active = Command::new("systemctl").args(["is-active", svc])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if active == "active" {
        println!("{} {} is running", "✓".green(), svc);
    } else {
        println!("{} {} is {}", "✗".red(), svc, active);
    }
    Ok(())
}
