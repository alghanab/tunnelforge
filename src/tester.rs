use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

/// A parsed proxy configuration ready for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub name: String,
    pub config_type: String,
    pub raw_uri: String,
    pub server: String,
    pub port: u16,
    pub uuid: Option<String>,
    pub secret: Option<String>,
    pub ws_path: Option<String>,
    pub sni: Option<String>,
    pub host_header: Option<String>,
    pub tls: bool,
}

/// Geo info from ip-api.com
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeoInfo {
    pub country: String,
    pub country_code: String,
    pub city: String,
    pub isp: String,
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
    pub status: String,
    pub geo: Option<GeoInfo>,
}

/// Sort field for results
#[derive(Debug, Clone, Copy)]
pub enum SortBy {
    Latency,
    Status,
    Name,
    Type,
    Country,
    None,
}

impl SortBy {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "latency" | "ping" | "ms" => Self::Latency,
            "status" | "health" => Self::Status,
            "name" => Self::Name,
            "type" | "proto" => Self::Type,
            "country" | "geo" | "location" => Self::Country,
            _ => Self::None,
        }
    }
}

// ─── Parsing ─────────────────────────────────────────────

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

pub fn parse_single_config(uri: &str, index: usize) -> Option<ProxyConfig> {
    if uri.starts_with("vless://") {
        parse_vless(uri, index)
    } else if uri.starts_with("vmess://") {
        parse_vmess(uri, index)
    } else if uri.starts_with("trojan://") {
        parse_trojan(uri, index)
    } else if uri.starts_with("tg://") || uri.starts_with("https://t.me/proxy") {
        parse_mtproto(uri, index)
    } else {
        // Try bare host:port (must contain a dot for domain or be numeric IP)
        parse_bare(uri, index)
    }
}

fn parse_vless(uri: &str, index: usize) -> Option<ProxyConfig> {
    let rest = &uri[8..];
    let (main, fragment) = split_fragment(rest);
    let (auth_host, query) = split_query(main);
    let parts: Vec<&str> = auth_host.splitn(2, '@').collect();
    if parts.len() < 2 { return None; }

    let uuid = parts[0].to_string();
    let (server, port) = parse_host_port(parts[1], 443);
    let params = parse_query_params(query);
    let name = fragment.map(urldecode).unwrap_or_else(|| format!("vless-{}", index + 1));

    Some(ProxyConfig {
        name,
        config_type: "vless".to_string(),
        raw_uri: uri.to_string(),
        server, port,
        uuid: Some(uuid), secret: None,
        ws_path: params.get("path").cloned(),
        sni: params.get("sni").cloned(),
        host_header: params.get("host").cloned(),
        tls: params.get("security").map(|s| s == "tls").unwrap_or(true),
    })
}

fn parse_vmess(uri: &str, index: usize) -> Option<ProxyConfig> {
    let b64 = &uri[8..];
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;

    let server = json["add"].as_str()?.to_string();
    let port = json["port"].as_u64().unwrap_or(443) as u16;
    let uuid = json["id"].as_str().map(|s| s.to_string());
    let name = json["ps"].as_str().map(|s| s.to_string())
        .unwrap_or_else(|| format!("vmess-{}", index + 1));

    Some(ProxyConfig {
        name,
        config_type: "vmess".to_string(),
        raw_uri: uri.to_string(),
        server, port, uuid, secret: None,
        ws_path: json["path"].as_str().map(|s| s.to_string()),
        sni: json["sni"].as_str().map(|s| s.to_string()),
        host_header: json["host"].as_str().map(|s| s.to_string()),
        tls: json["tls"].as_str() == Some("tls") || json["tls"].as_bool().unwrap_or(false),
    })
}

fn parse_trojan(uri: &str, index: usize) -> Option<ProxyConfig> {
    let rest = &uri[9..];
    let (main, fragment) = split_fragment(rest);
    let (auth_host, query) = split_query(main);
    let parts: Vec<&str> = auth_host.splitn(2, '@').collect();
    if parts.len() < 2 { return None; }

    let password = parts[0].to_string();
    let (server, port) = parse_host_port(parts[1], 443);
    let params = parse_query_params(query);
    let name = fragment.map(urldecode).unwrap_or_else(|| format!("trojan-{}", index + 1));

    Some(ProxyConfig {
        name,
        config_type: "trojan".to_string(),
        raw_uri: uri.to_string(),
        server, port, uuid: None, secret: Some(password),
        ws_path: params.get("path").cloned(),
        sni: params.get("sni").cloned(),
        host_header: params.get("host").cloned(),
        tls: true,
    })
}

fn parse_mtproto(uri: &str, index: usize) -> Option<ProxyConfig> {
    let query = uri.find('?').map(|pos| &uri[pos + 1..])?;
    let params = parse_query_params(query);
    let server = params.get("server")?.clone();
    let port = params.get("port").and_then(|p| p.parse().ok()).unwrap_or(443);
    let secret = params.get("secret").cloned();

    Some(ProxyConfig {
        name: format!("mtproto-{}", index + 1),
        config_type: "mtproto".to_string(),
        raw_uri: uri.to_string(),
        server, port, uuid: None, secret,
        ws_path: None, sni: None, host_header: None, tls: false,
    })
}

fn parse_bare(uri: &str, index: usize) -> Option<ProxyConfig> {
    let (main, fragment) = split_fragment(uri);
    let (server, port) = parse_host_port(main, 443);
    // Must have a valid-looking host (contains a dot or is numeric IP)
    if server.is_empty() || (!server.contains('.') && !server.chars().all(|c| c.is_ascii_digit() || c == ':')) {
        return None;
    }
    let name = fragment.map(urldecode).unwrap_or_else(|| format!("conn-{}", index + 1));

    Some(ProxyConfig {
        name,
        config_type: "direct".to_string(),
        raw_uri: uri.to_string(),
        server, port,
        uuid: None, secret: None,
        ws_path: None, sni: None, host_header: None, tls: false,
    })
}

// ─── Testing ─────────────────────────────────────────────

pub async fn test_config(config: &ProxyConfig, do_http: bool) -> TestResult {
    let addr = format!("{}:{}", config.server, config.port);

    let (tcp_ok, tcp_latency_ms) = match tokio::time::timeout(
        Duration::from_secs(5), TcpStream::connect(&addr),
    ).await {
        Ok(Ok(_)) => {
            let start = Instant::now();
            let _ = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await;
            (true, start.elapsed().as_millis() as u64)
        }
        _ => (false, 0),
    };

    let (http_ok, http_latency_ms) = if tcp_ok && do_http {
        test_http(&config.server, config.port, config.tls).await
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
        if tcp_latency_ms < 300 { "healthy".to_string() } else { "slow".to_string() }
    } else {
        "dead".to_string()
    };

    TestResult {
        name: config.name.clone(),
        config_type: config.config_type.clone(),
        raw_uri: config.raw_uri.clone(),
        server: config.server.clone(),
        port: config.port,
        tcp_ok, tcp_latency_ms, http_ok, http_latency_ms,
        error, status,
        geo: None,
    }
}

async fn test_http(server: &str, port: u16, tls: bool) -> (bool, u64) {
    let url = if tls { format!("https://{}:{}/", server, port) }
              else { format!("http://{}:{}/", server, port) };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build() { Ok(c) => c, Err(_) => return (false, 0) };

    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(5), client.get(&url).send()).await {
        Ok(Ok(_)) => (true, start.elapsed().as_millis() as u64),
        _ => (false, 0),
    }
}

pub async fn test_bulk(configs: &[ProxyConfig], do_http: bool, concurrency: usize) -> Vec<TestResult> {
    use futures::stream::{self, StreamExt};
    stream::iter(configs.iter())
        .map(|cfg| async move { test_config(cfg, do_http).await })
        .buffer_unordered(concurrency)
        .collect()
        .await
}

// ─── Geo Lookup ──────────────────────────────────────────

/// Batch lookup countries for unique IPs via ip-api.com (max 100 per request, free)
pub async fn lookup_geo(results: &mut Vec<TestResult>) {
    // Collect unique server IPs
    let mut servers: Vec<String> = results.iter()
        .filter(|r| r.tcp_ok)
        .map(|r| r.server.clone())
        .collect();
    servers.sort();
    servers.dedup();

    if servers.is_empty() { return; }

    // ip-api.com batch: POST http://ip-api.com/batch with [{"query":"1.2.3.4"},...]
    let query: Vec<serde_json::Value> = servers.iter()
        .map(|ip| serde_json::json!({"query": ip}))
        .collect();

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(10));
    // Use SOCKS proxy if set in environment (socks5h:// for remote DNS)
    if let Ok(proxy_url) = std::env::var("TUNNELFORGE_SOCKS").or_else(|_| std::env::var("ALL_PROXY")) {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let client = match builder.build() { Ok(c) => c, Err(_) => return };

    let resp = match client.post("http://ip-api.com/batch?fields=status,country,countryCode,city,isp,query")
        .json(&query)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return,
    };

    let data: Vec<serde_json::Value> = match resp.json().await {
        Ok(d) => d,
        Err(_) => return,
    };

    // Build lookup map: ip -> GeoInfo
    let mut geo_map = std::collections::HashMap::new();
    for item in &data {
        if item["status"].as_str() == Some("success") {
            let ip = item["query"].as_str().unwrap_or("").to_string();
            geo_map.insert(ip, GeoInfo {
                country: item["country"].as_str().unwrap_or("").to_string(),
                country_code: item["countryCode"].as_str().unwrap_or("").to_string(),
                city: item["city"].as_str().unwrap_or("").to_string(),
                isp: item["isp"].as_str().unwrap_or("").to_string(),
            });
        }
    }

    // Apply to results
    for r in results.iter_mut() {
        if let Some(geo) = geo_map.get(&r.server) {
            r.geo = Some(geo.clone());
        }
    }
}

// ─── Sorting ─────────────────────────────────────────────

pub fn sort_results(results: &mut Vec<TestResult>, by: SortBy) {
    match by {
        SortBy::Latency => results.sort_by_key(|r| {
            if r.tcp_ok { r.tcp_latency_ms } else { u64::MAX }
        }),
        SortBy::Status => results.sort_by(|a, b| {
            let ord = |s: &str| match s { "healthy" => 0, "slow" => 1, _ => 2 };
            ord(&a.status).cmp(&ord(&b.status))
        }),
        SortBy::Name => results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        SortBy::Type => results.sort_by(|a, b| a.config_type.cmp(&b.config_type)),
        SortBy::Country => results.sort_by(|a, b| {
            let ca = a.geo.as_ref().map(|g| g.country.as_str()).unwrap_or("ZZZ");
            let cb = b.geo.as_ref().map(|g| g.country.as_str()).unwrap_or("ZZZ");
            ca.cmp(cb)
        }),
        SortBy::None => {}
    }
}

// ─── Auto-naming ─────────────────────────────────────────

/// Generate auto names from country info: "Germany-1", "US-2", etc.
/// `include_ip` appends the IP: "Germany-185.100.87.1"
pub fn auto_name_from_country(results: &mut Vec<TestResult>, include_ip: bool) {
    // Count per country for numbering
    let mut country_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    for r in results.iter_mut() {
        let geo = match &r.geo {
            Some(g) if !g.country.is_empty() => g,
            _ => continue,
        };

        let count = country_counts.entry(geo.country_code.clone()).or_insert(0);
        *count += 1;

        let base = if include_ip {
            format!("{}-{}", geo.country, r.server)
        } else {
            format!("{}-{}", geo.country_code, count)
        };
        r.name = base;
    }
}

// ─── CLI Interface ───────────────────────────────────────

pub async fn run_cli(file: Option<&str>, http: bool, sort: &str, auto_name: bool) -> Result<()> {
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
    let mut results = test_bulk(&configs, http, 10).await;

    // Geo lookup for reachable configs
    println!("{} Looking up countries...", "🌍".cyan());
    lookup_geo(&mut results).await;

    // Auto-name from country if requested
    if auto_name {
        auto_name_from_country(&mut results, false);
    }

    // Sort
    let sort_by = SortBy::from_str(sort);
    sort_results(&mut results, sort_by);

    // Print results table
    println!();
    println!("{:<4} {:<18} {:<8} {:<8} {:<22} {:<8} {:<8} {:<10} {}",
        "#", "NAME", "TYPE", "COUNTRY", "SERVER", "TCP", "LATENCY", "STATUS", "ERROR");
    println!("{}", "-".repeat(120));

    for (i, r) in results.iter().enumerate() {
        let tcp = if r.tcp_ok { "✓".green().to_string() } else { "✗".red().to_string() };
        let latency = if r.tcp_ok { format!("{}ms", r.tcp_latency_ms) } else { "-".to_string() };
        let status = match r.status.as_str() {
            "healthy" => "healthy".green().to_string(),
            "slow" => "slow".yellow().to_string(),
            "dead" => "dead".red().to_string(),
            _ => r.status.clone(),
        };
        let country = r.geo.as_ref()
            .map(|g| format!("{} {}", g.country_code, truncate(&g.country, 10)))
            .unwrap_or_else(|| "-".to_string());
        let error = r.error.as_deref().unwrap_or("-");

        println!("{:<4} {:<18} {:<8} {:<8} {:<22} {:<8} {:<8} {:<10} {}",
            i + 1, truncate(&r.name, 18), r.config_type, country,
            format!("{}:{}", r.server, r.port),
            tcp, latency, status, truncate(error, 25));
    }

    let healthy = results.iter().filter(|r| r.status == "healthy" || r.status == "slow").count();
    println!();
    println!("{} {}/{} configs reachable", "✓".green(), healthy, results.len());

    // Show country summary
    let mut countries: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &results {
        if r.tcp_ok {
            let c = r.geo.as_ref().map(|g| g.country.clone()).unwrap_or_else(|| "Unknown".to_string());
            *countries.entry(c).or_insert(0) += 1;
        }
    }
    if !countries.is_empty() {
        println!();
        println!("Countries:");
        let mut sorted: Vec<_> = countries.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (country, count) in sorted {
            println!("  {} x{}", country, count);
        }
    }

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────

fn split_fragment(s: &str) -> (&str, Option<&str>) {
    match s.find('#') { Some(pos) => (&s[..pos], Some(&s[pos + 1..])), None => (s, None) }
}

fn split_query(s: &str) -> (&str, &str) {
    match s.find('?') { Some(pos) => (&s[..pos], &s[pos + 1..]), None => (s, "") }
}

fn parse_host_port(s: &str, default_port: u16) -> (String, u16) {
    if s.starts_with('[') {
        if let Some(pos) = s.find("]:") {
            return (s[1..pos].to_string(), s[pos + 2..].parse().unwrap_or(default_port));
        }
        return (s.trim_matches('[').trim_matches(']').to_string(), default_port);
    }
    if let Some(pos) = s.rfind(':') {
        (s[..pos].to_string(), s[pos + 1..].parse().unwrap_or(default_port))
    } else {
        (s.to_string(), default_port)
    }
}

fn parse_query_params(query: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();
    for pair in query.split('&') {
        if let Some(pos) = pair.find('=') {
            params.insert(pair[..pos].to_string(), urldecode(&pair[pos + 1..]));
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
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00"), 16) {
                result.push(byte as char);
                i += 3;
                continue;
            }
        }
        result.push(if bytes[i] == b'+' { ' ' } else { bytes[i] as char });
        i += 1;
    }
    result
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max - 3]) }
}
