use clap::{Parser, Subcommand};

use crate::{config::ConfigStore, db::Database};

#[derive(Parser)]
#[command(name = "tunnelforge", version, about = "Manage censorship bypass proxy tunnels")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage exit nodes (paqet tunnels, direct proxies)
    Node {
        #[command(subcommand)]
        action: NodeAction,
    },
    /// Manage proxy protocols (VLESS, MTProto, etc.)
    Proto {
        #[command(subcommand)]
        action: ProtoAction,
    },
    /// Import and expose existing v2ray/xray configs
    Import {
        #[command(subcommand)]
        action: ImportAction,
    },
    /// Manage subscription plans
    Plan {
        #[command(subcommand)]
        action: PlanAction,
    },
    /// Manage users and subscriptions
    User {
        #[command(subcommand)]
        action: UserAction,
    },
    /// Generate connection links for a user
    Link {
        username: String,
        #[arg(long, default_value = "all")]
        format: String,
    },
    /// Show full status dashboard
    Status,
    /// Show connection flow map
    Map,
    /// Scan and show ports
    Ports,
    /// Run limit enforcement
    Enforce {
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum NodeAction {
    /// Add an exit node
    Add {
        name: String,
        #[arg(long, default_value = "paqet")]
        r#type: String,
        #[arg(long)]
        server: Option<String>,
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        socks_port: Option<u16>,
        #[arg(long)]
        external_port: Option<u16>,
    },
    /// List all exit nodes
    List,
    /// Test exit node connectivity
    Test { name: String },
    /// Remove an exit node
    Remove { name: String },
}

#[derive(Subcommand)]
pub enum ProtoAction {
    /// Add a proxy protocol
    Add {
        r#type: String,
        #[arg(long)]
        exit: String,
        #[arg(long, default_value = "auto")]
        port: String,
        #[arg(long)]
        r#force: bool,
    },
    /// List all protocols
    List,
}

#[derive(Subcommand)]
pub enum ImportAction {
    /// Import a v2ray config
    Add {
        config_link: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "auto")]
        port: String,
        #[arg(long)]
        exit: Option<String>,
    },
    /// List imported configs
    List,
    /// Remove an import
    Remove { name: String },
    /// Test an import
    Test { name: String },
}

#[derive(Subcommand)]
pub enum PlanAction {
    /// Create a subscription plan
    Create {
        name: String,
        #[arg(long, default_value = "50GB")]
        data: String,
        #[arg(long, default_value = "30d")]
        duration: String,
        #[arg(long, default_value = "2")]
        devices: u32,
    },
    /// List all plans
    List,
    /// Remove a plan
    Remove { name: String },
}

#[derive(Subcommand)]
pub enum UserAction {
    /// Add a user
    Add {
        username: String,
        #[arg(long)]
        plan: String,
    },
    /// List all users
    List,
    /// Show user details
    Show { username: String },
    /// Disable a user
    Disable { username: String },
    /// Enable a user
    Enable { username: String },
    /// Reset user data or extend expiry
    Reset {
        username: String,
        #[arg(long)]
        extend_days: Option<u32>,
        #[arg(long)]
        reset_data: bool,
    },
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cfg = ConfigStore::load()?;
    let db = Database::open()?;

    match cli.command {
        Commands::Node { action } => match action {
            NodeAction::Add { name, r#type, server, key, socks_port, external_port } => {
                nodes::add(&cfg, &name, &r#type, server, key, socks_port, external_port)
            }
            NodeAction::List => nodes::list(&cfg),
            NodeAction::Test { name } => nodes::test(&cfg, &name),
            NodeAction::Remove { name } => nodes::remove(&cfg, &name),
        },
        Commands::Proto { action } => match action {
            ProtoAction::Add { r#type, exit, port, force } => {
                protocols::add(&cfg, &r#type, &exit, &port, force)
            }
            ProtoAction::List => protocols::list(&cfg),
        },
        Commands::Import { action } => match action {
            ImportAction::Add { config_link, name, port, exit } => {
                imports::add(&cfg, &config_link, name, &port, exit)
            }
            ImportAction::List => imports::list(&cfg),
            ImportAction::Remove { name } => imports::remove(&cfg, &name),
            ImportAction::Test { name } => imports::test(&name),
        },
        Commands::Plan { action } => match action {
            PlanAction::Create { name, data, duration, devices } => {
                plans::create(&cfg, &name, &data, &duration, devices)
            }
            PlanAction::List => plans::list(&cfg),
            PlanAction::Remove { name } => plans::remove(&cfg, &name),
        },
        Commands::User { action } => match action {
            UserAction::Add { username, plan } => users::add(&db, &cfg, &username, &plan),
            UserAction::List => users::list(&db, &cfg),
            UserAction::Show { username } => users::show(&db, &cfg, &username),
            UserAction::Disable { username } => users::set_status(&db, &username, "suspended"),
            UserAction::Enable { username } => users::set_status(&db, &username, "active"),
            UserAction::Reset { username, extend_days, reset_data } => {
                users::reset(&db, &username, extend_days, reset_data)
            }
        },
        Commands::Link { username, format } => links::generate(&db, &cfg, &username, &format),
        Commands::Status => monitor::status(&db, &cfg),
        Commands::Map => monitor::map(&db, &cfg),
        Commands::Ports => monitor::ports(),
        Commands::Enforce { dry_run } => enforcer::run(&db, &cfg, dry_run),
    }
}
