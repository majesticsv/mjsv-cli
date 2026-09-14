mod cli;
mod cmd;
mod prompt;

use clap::Parser;
use mjsv_core::client::Client;
use mjsv_core::config::{Config, Credentials};
use mjsv_core::output::{to_json, to_yaml, Format};
use mjsv_core::Ctx;

use cli::{Cli, Command, OrgCommand};

pub fn render_value(format: Format, value: &serde_json::Value) {
    match format {
        Format::Json => println!("{}", to_json(value)),
        Format::Yaml => print!("{}", to_yaml(value)),
        Format::Table => println!("{}", to_json(value)),
    }
}

fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };

    // O log vai para stderr para não contaminar o `--output json`, que é contrato de script.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(format!("mjsv={level},mjsv_core={level}"))),
        )
        .init();
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    init_tracing(cli.global.verbose);

    if let Err(e) = run(cli).await {
        // `{e:#}` traz a cadeia do anyhow; sem isso o contexto do erro se perde.
        eprintln!("erro: {e:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let config = Config::load()?;
    let credentials = Credentials::load()?;

    let api_url = config.resolve_api_url(cli.global.api_url.as_deref());
    let token = credentials.resolve_token();

    let format = cli
        .global
        .output
        .as_deref()
        .or(config.output.as_deref())
        .and_then(Format::parse)
        .unwrap_or_default();

    let org = cli.global.org.clone().or_else(|| config.org.clone());

    let ctx = Ctx {
        client: Client::new(&api_url, token)?,
        config,
        credentials,
        format,
        print_request: cli.global.print_request,
        assume_yes: cli.global.yes,
        org,
    };

    match &cli.command {
        Command::Login(args) => cmd::auth::login(&ctx, args).await,
        Command::Logout => cmd::auth::logout(&ctx).await,
        Command::Whoami => cmd::auth::whoami(&ctx).await,
        Command::Org(OrgCommand::List) => cmd::auth::org_list(&ctx).await,
        Command::Org(OrgCommand::Use { org }) => cmd::auth::org_use(&ctx, org).await,
        Command::Vm(sub) => cmd::vm::run(&ctx, sub).await,
        Command::Gs(sub) => cmd::gs::run(&ctx, sub).await,
        Command::Eip(sub) => cmd::fw::eip(&ctx, sub).await,
        Command::Order(sub) => cmd::billing::order(&ctx, sub).await,
        Command::Ticket(sub) => cmd::billing::ticket(&ctx, sub).await,
        Command::SshKey(sub) => cmd::billing::ssh_key(&ctx, sub).await,
        Command::Token(sub) => cmd::auth::token(&ctx, sub).await,
    }
}
