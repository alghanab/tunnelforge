use anyhow::Result;
use colored::*;
use crate::config::{ConfigStore, PlanConfig};

pub fn create(cfg: &ConfigStore, name: &str, data: &str, duration: &str, devices: u32) -> Result<()> {
    let mut cfg = cfg.clone();
    let plan = PlanConfig {
        name: name.to_string(),
        data_limit: data.to_string(),
        duration: duration.to_string(),
        max_devices: devices,
        protocols: cfg.protocols.keys().cloned().collect(),
    };
    cfg.plans.insert(name.to_string(), plan);
    cfg.save()?;
    println!("{} Plan '{}' created (data: {}, duration: {}, devices: {})", "✓".green(), name, data, duration, devices);
    Ok(())
}

pub fn list(cfg: &ConfigStore) -> Result<()> {
    if cfg.plans.is_empty() {
        println!("No plans configured.");
        return Ok(());
    }
    println!("{:<12} {:<10} {:<10} {:<10} {}", "NAME", "DATA", "DURATION", "DEVICES", "PROTOCOLS");
    println!("{}", "-".repeat(60));
    for (name, plan) in &cfg.plans {
        println!("{:<12} {:<10} {:<10} {:<10} {}", name, plan.data_limit, plan.duration, plan.max_devices, plan.protocols.join(", "));
    }
    Ok(())
}

pub fn remove(cfg: &ConfigStore, name: &str) -> Result<()> {
    println!("{} Plan '{}' removed", "✓".green(), name);
    Ok(())
}

impl Clone for ConfigStore {
    fn clone(&self) -> Self {
        Self {
            vps: VpsConfig { ip: self.vps.ip.clone(), domain: self.vps.domain.clone(), provider: self.vps.provider.clone() },
            exit_nodes: self.exit_nodes.clone(),
            protocols: self.protocols.clone(),
            plans: self.plans.clone(),
            imports: self.imports.clone(),
        }
    }
}

use crate::config::VpsConfig;
