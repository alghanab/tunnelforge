use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// A parsed proxy configuration ready for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub name: String,
    pub config_type: String,      // vless, vmess, trojan, mtproto
    pub raw_uri: String,          // original URI
    pub server: String,           // host
    pub port: u16,                // port
    pub uuid: Option<String>,     // for vless/vmess
    pub secret: Option<String>,   // for mtproto
    pub ws_path: Option<String>,  // websocket path
    pub sni: Option<String>,      // TLS SNI
    pub host_header: Option<String>, // WS host header
    pub tls: bool,
}

/// Result of testing a proxy config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub config_type: String,
    pub raw_uri: String,
    pub server: String,
    pub port: u16,
    pub tcp_ok: bool,
    pub tcp_latency_ms: u64,
    pub http_ok: bool,
    pub http_latency_ms: u64,
    pub error: Option<String>,
    pub status: String,  // "healthy", "slow", "dead"
}

/// Parse multiple config URIs from text (one per line, or separated by newlines)
pub fn parse_bulk_configs(text: &str) -> Vec<ProxyConfig> {
    let mut configs = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(cfg) = parse_single_config(line, i) {
            configs.push(cfg);
        }
    }
    configs
}

/// Parse a single proxy URI into a ProxyConfig
pub fn parse_single_config(uri: &str, index: usize) -> Option<ProxyConfig> {
    if uri.starts_with("vless://") {
        parse_vless(uri, index)
    } else if uri.starts_with("vmess://") {
        parse_vmess(uri, index)
    } else if uri.starts_with("trojan://") {
        parse_trojan(uri, index)
    } else if uri.starts_with("tg://") || uri.starts_with("https://t.me/proxy") {
        parse_mtproto(uri, index)
    } else if uri.contains("@") && uri.contains(":") {
        // Try as bare server:port format "type@server:port"
        parse_bare(uri, index)
    } else {
        None
    }
}

fn parse_vless(uri: &str, index: usize) -> Option<ProxyConfig> {
    // vless://uuid@server:port?encryption=none&security=tls&sni=...&type=ws&host=...&path=...#name
    let rest = &uri[8..]; // skip "vless://"
    let (main, fragment) = if let Some(pos) = rest.find('#') {
        (&rest[..pos], Some(&rest[pos + 1..]))
    } else {
        (rest, None)
    };

    let (auth_host, query) = if let Some(pos) = main.find('?') {
        (&main[..pos], &main[pos + 1..])
    } else {
        (main, "")
    };

    let parts: Vec<&str> = auth_host.splitn(2, '@').collect();
    if parts.len() < 2 { return None; }

    let uuid = parts[0].to_string();
    let server_port = parts[1];
    let (server, port) = parse_host_port(server_port, 443);

    let params = parse_query_params(query);
    let name = fragment.map(urldecode).unwrap_or_else(|| format!("vless-{}", index + 1));
    let tls = params.get("security").map(|s| s.as_str() == "tls").unwrap_or(true);
    let sni = params.get("sni").cloned();
    let ws_path = params.get("path").cloned();
    let host_header = params.get("host").cloned();

    Some(ProxyConfig {
        name,
        config_type: "vless".to_string(),
        raw_uri: uri.to_string(),
        server,
        port,
        uuid: Some(uuid),
        secret: None,
        ws_path,
        sni,
        host_header,
        tls,
    })
}

fn parse_vmess(uri: &str, index: usize) -> Option<ProxyConfig> {
    // vmess://base64(json)
    let b64 = &uri[8..];
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;

    let server = json["add"].as_str()?.to_string();
    let port = json["port"].as_u64().unwrap_or(443) as u16;
    let uuid = json["id"].as_str().map(|s| s.to_string());
    let name = json["ps"].as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("vmess-{}", index + 1));
    let ws_path = json["path"].as_str().map(|s| s.to_string());
    let host_header = json["host"].as_str().map(|s| s.to_string());
    let sni = json["sni"].as_str().map(|s| s.to_string());
    let tls = json["tls"].as_str() == Some("tls") || json["tls"].as_bool().unwrap_or(false);

    Some(ProxyConfig {
        name,
        config_type: "vmess".to_string(),
        raw_uri: uri.to_string(),
        server,
        port,
        uuid,
        secret: None,
        ws_path,
        sni,
        host_header,
        tls,
    })
}

fn parse_trojan(uri: &str, index: usize) -> Option<ProxyConfig> {
    // trojan://password@server:port?sni=...&type=ws&host=...&path=...#name
    let rest = &uri[9..]; // skip "trojan://"
    let (main, fragment) = if let Some(pos) = rest.find('#') {
        (&rest[..pos], Some(&rest[pos + 1..]))
    } else {
        (rest, None)
    };

    let (auth_host, query) = if let Some(pos) = main.find('?') {
        (&main[..pos], &main[pos + 1..])
    } else {
        (main, "")
    };

    let parts: Vec<&str> = auth_host.splitn(2, '@').collect();
    if parts.len() < 2 { return None; }

    let password = parts[0].to_string();
    let server_port = parts[1];
    let (server, port) = parse_host_port(server_port, 443);

    let params = parse_query_params(query);
    let name = fragment.map(urldecode).unwrap_or_else(|| format!("trojan-{}", index + 1));
    let sni = params.get("sni").cloned();
    let ws_path = params.get("path").cloned();
    let host_header = params.get("host").cloned();

    Some(ProxyConfig {
        name,
        config_type: "trojan".to_string(),
        raw_uri: uri.to_string(),
        server,
        port,
        uuid: None,
        secret: Some(password),
        ws_path,
        sni,
        host_header,
        tls: true,
    })
}

fn parse_mtproto(uri: &str, index: usize) -> Option<ProxyConfig> {
    // tg://proxy?server=...&port=...&secret=...
    // or https://t.me/proxy?server=...&port=...&secret=...
    let query = if uri.starts_with("tg://") {
        if let Some(pos) = uri.find('?') { &uri[pos + 1..] } else { return None; }
    } else if uri.contains("t.me/proxy") {
        if let Some(pos) = uri.find('?') { &uri[pos + 1..] } else { return None; }
    } else {
        return None;
    };

    let params = parse_query_params(query);
    let server = params.get("server")?.clone();
    let port = params.get("port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(443);
    let secret = params.get("secret").cloned();
    let name = format!("mtproto-{}", index + 1);

    Some(ProxyConfig {
        name,
        config_type: "mtproto".to_string(),
        raw_uri: uri.to_string(),
        server,
        port,
        uuid: None,
        secret,
        ws_path: None,
        sni: None,
        host_header: None,
        tls: false,
    })
}

fn parse_bare(uri: &str, index: usize) -> Option<ProxyConfig> {
    // Try format: server:port or server:port#name
    let (main, fragment) = if let Some(pos) = uri.find('#') {
        (&uri[..pos], Some(&uri[pos + 1..]))
    } else {
        (uri, None)
    };

    let (server, port) = parse_host_port(main, 443);
    if server.is_empty() { return None; }

    let name = fragment.map(urldecode).unwrap_or_else(|| format!("conn-{}", index + 1));

    Some(ProxyConfig {
        name,
        config_type: "direct".to_string(),
        raw_uri: uri.to_string(),
        server,
        port,
        uuid: None,
        secret: None,
        ws_path: None,
        sni: None,
        host_header: None,
        tls: false,
    })
}

// ─── Testing ─────────────────────────────────────────────

/// Test a single proxy config: TCP connect + optional HTTP
pub async fn test_config(config: &ProxyConfig, do_http: bool) -> TestResult {
    let addr = format!("{}:{}", config.server, config.port);

    // TCP connect test
    let (tcp_ok, tcp_latency_ms) = match tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(&addr),
    ).await {
        Ok(Ok(_stream)) => {
            // Connection succeeded, measure latency by doing a quick connect again
            let start = Instant::now();
            let _ = tokio::time::timeout(
                Duration::from_secs(3),
                TcpStream::connect(&addr),
            ).await;
            let latency = start.elapsed().as_millis() as u64;
            (true, latency)
        }
        Ok(Err(e)) => (false, 0),
        Err(_) => (false, 0), // timeout
    };

    // HTTP test (connect to port and send minimal HTTP request)
    let (http_ok, http_latency_ms) = if tcp_ok && do_http {
        test_http(&config.server, config.port, &config.sni, config.tls).await
    } else {
        (false, 0)
    };

    let error = if !tcp_ok {
        Some(format!("TCP connect to {} failed", addr))
    } else if do_http && !http_ok {
        Some("HTTP test failed".to_string())
    } else {
        None
    };

    let status = if tcp_ok && (!do_http || http_ok) {
        if tcp_latency_ms < 300 {
            "healthy".to_string()
        } else {
            "slow".to_string()
        }
    } else {
        "dead".to_string()
    };

    TestResult {
        name: config.name.clone(),
        config_type: config.config_type.clone(),
        raw_uri: config.raw_uri.clone(),
        server: config.server.clone(),
        port: config.port,
        tcp_ok,
        tcp_latency_ms,
        http_ok,
        http_latency_ms,
        error,
        status,
    }
}

/// Test HTTP connectivity to server:port
async fn test_http(server: &str, port: u16, sni: &Option<String>, tls: bool) -> (bool, u64) {
    let url = if tls {
        format!("https://{}:{}/", server, port)
    } else {
        format!("http://{}:{}/", server, port)
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build();

    let client = match client {
        Ok(c) => c,
        Err(_) => return (false, 0),
    };

    let start = Instant::now();
    match tokio::time::timeout(
        Duration::from_secs(5),
        client.get(&url).send(),
    ).await {
        Ok(Ok(resp)) => {
            let latency = start.elapsed().as_millis() as u64;
            // Any HTTP response means the server is alive
            (true, latency)
        }
        Ok(Err(_)) => (false, 0),
        Err(_) => (false, 0),
    }
}

/// Test multiple configs concurrently
pub async fn test_bulk(configs: &[ProxyConfig], do_http: bool, concurrency: usize) -> Vec<TestResult> {
    use futures::stream::{self, StreamExt};

    let results: Vec<TestResult> = stream::iter(configs.iter())
        .map(|cfg| async move { test_config(cfg, do_http).await })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    results
}

// ─── CLI Interface ───────────────────────────────────────

/// Run tester from CLI
pub async fn run_cli(file: Option<&str>, http: bool) -> Result<()> {
    use colored::*;

    let input = if let Some(path) = file {
        std::fs::read_to_string(path)?
    } else {
        let mut input = String::new();
        eprintln!("Paste proxy configs (one per line, Ctrl+D when done):");
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
        input
    };

    let configs = parse_bulk_configs(&input);
    if configs.is_empty() {
        println!("{} No valid configs found in input", "✗".red());
        return Ok(());
    }

    println!("{} Parsed {} configs, testing...", "⚡".cyan(), configs.len());

    let results = test_bulk(&configs, http, 10).await;

    // Print results table
    println!();
    println!("{:<4} {:<12} {:<8} {:<22} {:<8} {:<8} {:<10} {}",
        "#", "NAME", "TYPE", "SERVER", "TCP", "LATENCY", "STATUS", "ERROR");
    println!("{}", "-".repeat(100));

    for (i, r) in results.iter().enumerate() {
        let tcp = if r.tcp_ok { "✓".green().to_string() } else { "✗".red().to_string() };
        let latency = if r.tcp_ok { format!("{}ms", r.tcp_latency_ms) } else { "-".to_string() };
        let status = match r.status.as_str() {
            "healthy" => "healthy".green().to_string(),
            "slow" => "slow".yellow().to_string(),
            "dead" => "dead".red().to_string(),
            _ => r.status.clone(),
        };
        let error = r.error.as_deref().unwrap_or("-");

        println!("{:<4} {:<12} {:<8} {:<22} {:<8} {:<8} {:<10} {}",
            i + 1, truncate(&r.name, 12), r.config_type,
            format!("{}:{}", r.server, r.port),
            tcp, latency, status, truncate(error, 30));
    }

    let healthy = results.iter().filter(|r| r.status == "healthy" || r.status == "slow").count();
    println!();
    println!("{} {}/{} configs reachable", "✓".green(), healthy, results.len());

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────

fn parse_host_port(s: &str, default_port: u16) -> (String, u16) {
    // Handle IPv6 [addr]:port
    if s.starts_with('[') {
        if let Some(pos) = s.find("]:") {
            return (s[1..pos].to_string(), s[pos + 2..].parse().unwrap_or(default_port));
        }
        return (s[1..s.len().min(s.len())].to_string(), default_port);
    }

    if let Some(pos) = s.rfind(':') {
        let host = &s[..pos];
        let port = s[pos + 1..].parse().unwrap_or(default_port);
        (host.to_string(), port)
    } else {
        (s.to_string(), default_port)
    }
}

fn parse_query_params(query: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();
    for pair in query.split('&') {
        if let Some(pos) = pair.find('=') {
            let key = &pair[..pos];
            let val = &pair[pos + 1..];
            params.insert(key.to_string(), urldecode(val));
        }
    }
    params
}

fn urldecode(s: &str) -> String {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00"), 16,
            ) {
                result.push(byte as char);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            result.push(' ');
        } else {
            result.push(bytes[i] as char);
        }
        i += 1;
    }
    result
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
