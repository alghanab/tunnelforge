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
mod web;
mod sub;
mod tester;
mod live;

fn main() -> anyhow::Result<()> {
    cli::run()
}
