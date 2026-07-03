mod cli;
mod config;
mod db;
mod nodes;
mod protocols;
mod users;
mod plans;
mod imports;
mod links;
mod enforcer;
mod monitor;
mod service;

use cli::run;

fn main() -> anyhow::Result<()> {
    run()
}
