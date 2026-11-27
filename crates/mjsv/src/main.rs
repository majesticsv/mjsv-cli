mod cli;
mod cmd;
mod prompt;

use clap::Parser;
use mjsv_core::client::Client;
use mjsv_core::config::{Config, Credentials};
use mjsv_core::output::{to_json, to_yaml, Format};
use mjsv_core::Ctx;

use cli::{Cli, Command};

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
    // Antes de qualquer leitura de configuração: gerar completions não precisa de credencial nem
    // de rede, e falhar aqui por falta de config seria absurdo.
    if let Command::Completions { shell } = &cli.command {
        let mut cmd = <Cli as clap::CommandFactory>::command();

        // Gera em memória e só então escreve. O `generate` faz `.expect()` no writer, então
        // escrever direto em stdout vira um panic feio em `mjsv completions bash | head` — o
        // consumidor fecha o pipe e o programa "quebra" no que é uso normal de shell.
        let mut buf: Vec<u8> = Vec::new();
        clap_complete::generate(*shell, &mut cmd, "mjsv", &mut buf);

        use std::io::Write;
        match std::io::stdout().write_all(&buf) {
            Ok(()) => {}
            // Pipe fechado pelo outro lado não é erro: é `| head`.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
            Err(e) => return Err(e.into()),
        }

        return Ok(());
    }

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
        Command::Whoami { full } => cmd::account::whoami(&ctx, *full).await,
        Command::Org(sub) => cmd::org::run(&ctx, sub).await,
        Command::Vm(sub) => cmd::vm::run(&ctx, sub).await,
        Command::Gs(sub) => cmd::gs::run(&ctx, sub).await,
        Command::Eip(sub) => cmd::fw::eip(&ctx, sub).await,
        Command::Order(sub) => cmd::billing::order(&ctx, sub).await,
        Command::Ticket(sub) => cmd::billing::ticket(&ctx, sub).await,
        Command::SshKey(sub) => cmd::billing::ssh_key(&ctx, sub).await,
        Command::Token(sub) => cmd::auth::token(&ctx, sub).await,
        Command::Vpc(sub) => cmd::vpc::run(&ctx, sub).await,
        Command::Volume(sub) => cmd::volume::run(&ctx, sub).await,
        Command::Protection(sub) => cmd::protection::run(&ctx, sub).await,
        Command::Notification(sub) => cmd::account::notification(&ctx, sub).await,
        Command::Session(sub) => cmd::account::session(&ctx, sub).await,
        Command::Profile(sub) => cmd::account::profile(&ctx, sub).await,

        Command::Product(sub) => cmd::commerce::product(&ctx, sub).await,
        Command::Payment(sub) => cmd::commerce::payment(&ctx, sub).await,

        // Tratado antes de montar o contexto.
        Command::Completions { .. } => Ok(()),
    }
}
