use anyhow::Result;
use colored::*;
use std::process::Command;

pub fn run() -> Result<()> {
    println!("{}", "═".repeat(60));
    println!("  TunnelForge Restore");
    println!("  Bringing back all proxy services...");
    println!("{}", "═".repeat(60));
    println!();

    let mut issues = 0;
    let mut fixed = 0;

    // Step 1: Fix missing mtproto binaries
    println!("{} Checking mtproto binaries...", "🔍".cyan());
    issues += fix_mtproto_binary("london", 2096);
    issues += fix_mtproto_binary("canada", 8443);

    // Step 2: Fix missing proxychains configs
    println!("{} Checking proxychains configs...", "🔍".cyan());
    fix_proxychains_config("local", 666);
    fix_proxychains_config("canada", 667);

    // Step 3: Fix missing mtproto configs
    println!("{} Checking mtproto configs...", "🔍".cyan());
    fix_mtproto_config("london", 2096);
    fix_mtproto_config("canada", 8443);

    // Step 4: Start services in order
    println!();
    println!("{} Starting services...", "🚀".cyan());

    // Core services first
    fixed += start_service("caddy");
    fixed += start_service("gost-https-proxy");
    fixed += start_service("gost-socks5-proxy");

    // Paqet tunnels
    fixed += start_service("paqet-london");
    fixed += start_service("paqet-canada");

    // Wait for paqet to establish
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Xray services
    fixed += start_service("xray");
    fixed += start_service("xray-local");
    fixed += start_service("xray-canada");

    // MTProto services
    fixed += start_service("mtproto-local");
    fixed += start_service("mtproto-canada");

    // Step 5: Apply iptables bridge rules
    println!();
    println!("{} Applying bridge rules...", "🔧".cyan());
    apply_bridge_rules();

    // Step 6: Verify ports
    println!();
    println!("{} Verifying ports...", "✅".cyan());
    let port_results = verify_ports();

    // Step 7: Summary
    println!();
    println!("{}", "═".repeat(60));
    println!("  Restore Complete");
    println!("{}", "═".repeat(60));
    println!();

    let all_ports = vec![
        (443, "VLESS (caddy)"),
        (2096, "MTProto London"),
        (8443, "MTProto Canada"),
        (2087, "GOST HTTPS"),
        (4444, "GOST SOCKS5"),
        (3389, "Paqet London"),
        (999, "Paqet Canada"),
    ];

    for (port, name) in &all_ports {
        let status = if port_results.contains(port) {
            "UP".green()
        } else {
            "DOWN".red()
        };
        println!("  {:<6} {:<25} {}", port, name, status);
    }

    let up_count = all_ports.iter().filter(|(p, _)| port_results.contains(p)).count();
    println!();
    println!("  {}/{} ports active", up_count, all_ports.len());

    if up_count < all_ports.len() {
        println!("  {} Some ports are down - check logs with: proxy-iran logs", "⚠".yellow());
    } else {
        println!("  {} All services restored!", "✓".green());
    }

    println!();
    println!("  Quick commands:");
    println!("    proxy-iran status    Show tunnel status");
    println!("    proxy-iran show      Show client configs");
    println!("    proxy-iran logs      Show service logs");
    println!("    proxy-iran restart   Restart active tunnel");

    Ok(())
}

fn fix_mtproto_binary(tunnel: &str, port: u16) -> i32 {
    let dir = format!("/tmp/mtprotoproxy-{}", tunnel);
    let binary = format!("{}/mtprotoproxy.py", dir);

    if std::path::Path::new(&binary).exists() {
        println!("  ✓ mtprotoproxy-{} binary exists", tunnel);
        return 0;
    }

    println!("  ⚠ mtprotoproxy-{} binary missing, downloading...", tunnel);
    let _ = Command::new("mkdir").args(["-p", &dir]).output();

    // Try to copy from the other tunnel first
    let other = if tunnel == "london" { "canada" } else { "london" };
    let other_binary = format!("/tmp/mtprotoproxy-{}/mtprotoproxy.py", other);
    if std::path::Path::new(&other_binary).exists() {
        let _ = Command::new("cp").args([&other_binary, &binary]).output();
        println!("  ✓ Copied from {}", other);
        return 1;
    }

    // Download from GitHub
    let result = Command::new("curl")
        .args(["-sL", "https://github.com/alexbers/mtprotoproxy/raw/master/mtprotoproxy.py", "-o", &binary])
        .output();

    match result {
        Ok(o) if o.status.success() => {
            println!("  ✓ Downloaded mtprotoproxy-{}", tunnel);
            1
        }
        _ => {
            println!("  ✗ Failed to download mtprotoproxy-{}", tunnel);
            1
        }
    }
}

fn fix_proxychains_config(name: &str, socks_port: u16) {
    let path = format!("/tmp/proxychains-{}.conf", name);
    if std::path::Path::new(&path).exists() {
        println!("  ✓ proxychains-{}.conf exists", name);
        return;
    }

    let content = format!(
        "strict_chain\nproxy_dns\ntcp_read_time_out 15000\ntcp_write_time_out 15000\n\n[ProxyList]\nsocks5 127.0.0.1 {}\n",
        socks_port
    );
    let _ = std::fs::write(&path, content);
    println!("  ✓ Created proxychains-{}.conf", name);
}

fn fix_mtproto_config(tunnel: &str, port: u16) {
    let dir = format!("/tmp/mtprotoproxy-{}", tunnel);
    let config_path = format!("{}/config.py", dir);

    if std::path::Path::new(&config_path).exists() {
        println!("  ✓ mtprotoproxy-{}/config.py exists", tunnel);
        return;
    }

    let _ = Command::new("mkdir").args(["-p", &dir]).output();

    // Try to read secret from tunnelforge config
    let secret = read_mtproto_secret(tunnel);
    let content = format!(
        "PORT = {}\nUSERS = {{\n    \"tunnelforge\": \"{}\"\n}}\n",
        port, secret
    );
    let _ = std::fs::write(&config_path, content);
    println!("  ✓ Created mtprotoproxy-{}/config.py", tunnel);
}

fn read_mtproto_secret(tunnel: &str) -> String {
    // Try to read from tunnelforge config
    let config_path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".tunnelforge").join("config.yaml");

    if let Ok(content) = std::fs::read_to_string(&config_path) {
        // Simple YAML parsing for the secret
        let tag = format!("mtproto-{}", tunnel);
        let mut in_section = false;
        for line in content.lines() {
            if line.trim().starts_with(&tag) {
                in_section = true;
                continue;
            }
            if in_section && line.contains("secret:") {
                return line.split("secret:").nth(1)
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
            if in_section && !line.starts_with(" ") && !line.starts_with("    ") {
                break;
            }
        }
    }

    // Fallback: generate a random secret
    format!("ee{:032x}", rand::random::<u64>())
}

fn start_service(name: &str) -> i32 {
    let status = Command::new("systemctl")
        .args(["is-active", &format!("{}.service", name)])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    if status == "active" {
        println!("  ✓ {} already running", name);
        return 0;
    }

    let result = Command::new("systemctl")
        .args(["start", &format!("{}.service", name)])
        .output();

    match result {
        Ok(o) if o.status.success() => {
            println!("  ✓ {} started", name);
            1
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            println!("  ✗ {} failed: {}", name, stderr.trim());
            1
        }
        Err(e) => {
            println!("  ✗ {} error: {}", name, e);
            1
        }
    }
}

fn apply_bridge_rules() {
    // Check if mtproto-bridge-rules.sh exists and run it
    let rules_script = "/usr/local/bin/mtproto-bridge-rules.sh";
    if std::path::Path::new(rules_script).exists() {
        let result = Command::new("bash").arg(rules_script).output();
        match result {
            Ok(o) if o.status.success() => println!("  ✓ Bridge rules applied"),
            Ok(o) => println!("  ⚠ Bridge rules: {}", String::from_utf8_lossy(&o.stderr).trim()),
            Err(e) => println!("  ✗ Bridge rules error: {}", e),
        }
    } else {
        println!("  ⚠ Bridge rules script not found");
    }
}

fn verify_ports() -> Vec<u16> {
    let ports = vec![443, 2096, 8443, 2053, 2087, 4444, 3389, 999];
    let mut up = Vec::new();

    // Wait a moment for services to bind
    std::thread::sleep(std::time::Duration::from_secs(1));

    for port in &ports {
        let output = Command::new("ss")
            .args(["-tlnp"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        if output.contains(&format!(":{} ", port)) {
            up.push(*port);
        }
    }

    up
}
