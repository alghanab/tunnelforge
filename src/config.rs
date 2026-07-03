use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ConfigStore {
    pub vps: VpsConfig,
    pub exit_nodes: HashMap<String, NodeConfig>,
    pub protocols: HashMap<String, ProtoConfig>,
    pub plans: HashMap<String, PlanConfig>,
    pub imports: HashMap<String, ImportConfig>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct VpsConfig {
    pub ip: Option<String>,
    pub domain: Option<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeConfig {
    #[serde(alias = "type")]
    pub node_type: String,
    pub server: String,
    pub key: Option<String>,
    #[serde(default = "default_port")]
    pub socks_port: u16,
    #[serde(default = "default_port")]
    pub external_port: u16,
    pub interface: String,
    pub router_mac: Option<String>,
    pub local_ip: Option<String>,
    pub exit_ip: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProtoConfig {
    #[serde(alias = "type")]
    pub proto_type: String,
    #[serde(default)]
    pub exit_node: String,
    pub port: u16,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub ws_path: Option<String>,
    #[serde(default)]
    pub sni: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub tls: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlanConfig {
    pub name: String,
    pub data_limit: String,
    pub duration: String,
    pub max_devices: u32,
    pub protocols: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImportConfig {
    pub name: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub source_link: String,
    #[serde(default)]
    pub local_port: u16,
    #[serde(default)]
    pub local_socks_port: u16,
    #[serde(default)]
    pub exit_node: Option<String>,
}

impl ConfigStore {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(serde_yaml::from_str(&content)?)
        } else {
            let cfg = Self::default();
            cfg.save()?;
            Ok(cfg)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_yaml::to_string(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

fn config_path() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
        .join(".tunnelforge").join("config.yaml")
}

fn default_port() -> u16 { 0 }
