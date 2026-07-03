use clap::{Parser, Subcommand};
use crate::{config::ConfigStore, db::Database, nodes, protocols, imports, plans, users, links, enforcer, monitor, service, sub, web};

#[derive(Parser)]
#[command(name = "tunnelforge", version = "0.3.0", about = "Manage censorship bypass proxy tunnels")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Config { #[command(subcommand)] action: ConfigAction },
    Node { #[command(subcommand)] action: NodeAction },
    Proto { #[command(subcommand)] action: ProtoAction },
    Import { #[command(subcommand)] action: ImportAction },
    Plan { #[command(subcommand)] action: PlanAction },
    User { #[command(subcommand)] action: UserAction },
    Link { username: String, #[arg(long, default_value = "all")] format: String },
    Sub { username: String, #[arg(long)] output: Option<String> },
    Service { #[command(subcommand)] action: ServiceAction },
    Web {
        #[arg(long, default_value = "8080")] port: u16,
        #[arg(long, default_value = "")] path: String,
        #[arg(long)] password: Option<String>,
    },
    Status,
    Map,
    Ports,
    Enforce { #[arg(long)] dry_run: bool },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    SetVps { #[arg(long)] ip: Option<String>, #[arg(long)] domain: Option<String> },
    Show,
}

#[derive(Subcommand)]
pub enum NodeAction {
    Add {
        name: String,
        #[arg(long, default_value = "paqet")] r#type: String,
        #[arg(long)] server: Option<String>,
        #[arg(long)] key: Option<String>,
        #[arg(long)] socks_port: Option<u16>,
        #[arg(long)] external_port: Option<u16>,
    },
    List,
    Test { name: String },
    Remove { name: String },
}

#[derive(Subcommand)]
pub enum ProtoAction {
    Add {
        r#type: String,
        #[arg(long)] exit: String,
        #[arg(long, default_value = "auto")] port: String,
        #[arg(long)] force: bool,
    },
    List,
}

#[derive(Subcommand)]
pub enum ImportAction {
    Add {
        config_link: String,
        #[arg(long)] name: Option<String>,
        #[arg(long, default_value = "auto")] port: String,
        #[arg(long)] exit: Option<String>,
    },
    List,
    Remove { name: String },
    Test { name: String },
}

#[derive(Subcommand)]
pub enum PlanAction {
    Create {
        name: String,
        #[arg(long, default_value = "50GB")] data: String,
        #[arg(long, default_value = "30d")] duration: String,
        #[arg(long, default_value = "2")] devices: u32,
    },
    List,
    Remove { name: String },
}

#[derive(Subcommand)]
pub enum UserAction {
    Add { username: String, #[arg(long)] plan: String },
    List,
    Show { username: String },
    Disable { username: String },
    Enable { username: String },
    Reset {
        username: String,
        #[arg(long)] extend_days: Option<u32>,
        #[arg(long)] reset_data: bool,
    },
}

#[derive(Subcommand)]
pub enum ServiceAction {
    Apply { #[arg(long)] restart: bool },
    Start { name: String },
    Stop { name: String },
    Restart { name: String },
    Status,
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cfg = ConfigStore::load()?;
    let db = Database::open()?;

    match cli.command {
        Commands::Config { action } => match action {
            ConfigAction::SetVps { ip, domain } => crate::config::set_vps(&cfg, ip, domain),
            ConfigAction::Show => crate::config::show(&cfg),
        },
        Commands::Node { action } => match action {
            NodeAction::Add { name, r#type, server, key, socks_port, external_port } => {
                nodes::add(&name, &r#type, server, key, socks_port, external_port)
            }
            NodeAction::List => nodes::list(&cfg),
            NodeAction::Test { name } => nodes::test(&name),
            NodeAction::Remove { name } => nodes::remove(&name),
        },
        Commands::Proto { action } => match action {
            ProtoAction::Add { r#type, exit, port, force } => protocols::add(&r#type, &exit, &port, force),
            ProtoAction::List => protocols::list(&cfg),
        },
        Commands::Import { action } => match action {
            ImportAction::Add { config_link, name, port, exit } => imports::add(&config_link, name, &port, exit),
            ImportAction::List => imports::list(&cfg),
            ImportAction::Remove { name } => imports::remove(&name),
            ImportAction::Test { name } => imports::test(&name),
        },
        Commands::Plan { action } => match action {
            PlanAction::Create { name, data, duration, devices } => plans::create(&name, &data, &duration, devices),
            PlanAction::List => plans::list(&cfg),
            PlanAction::Remove { name } => plans::remove(&name),
        },
        Commands::User { action } => match action {
            UserAction::Add { username, plan } => users::add(&db, &username, &plan),
            UserAction::List => users::list(&db),
            UserAction::Show { username } => users::show(&db, &username),
            UserAction::Disable { username } => users::set_status(&db, &username, "suspended"),
            UserAction::Enable { username } => users::set_status(&db, &username, "active"),
            UserAction::Reset { username, extend_days, reset_data } => {
                users::reset(&db, &username, extend_days, reset_data)
            }
        },
        Commands::Link { username, format } => links::generate(&db, &cfg, &username, &format),
        Commands::Sub { username, output } => sub::generate(&db, &cfg, &username, output.as_deref()),
        Commands::Service { action } => match action {
            ServiceAction::Apply { restart } => service::apply(&cfg, restart),
            ServiceAction::Start { name } => service::start(&name),
            ServiceAction::Stop { name } => service::stop(&name),
            ServiceAction::Restart { name } => service::restart(&name),
            ServiceAction::Status => service::service_status(),
        },
        Commands::Web { port, path, password } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(web::start_web(&db, port, &path, password.as_deref()))
        }
        Commands::Status => monitor::status(&db),
        Commands::Map => monitor::map(&cfg),
        Commands::Ports => monitor::ports(),
        Commands::Enforce { dry_run } => enforcer::run(&db, dry_run),
    }
}
