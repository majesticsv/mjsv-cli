mod cli;
mod cmd;
mod prompt;

use clap::error::ErrorKind;
use clap::Parser;
use mjsv_core::client::Client;
use mjsv_core::config::{Config, Credentials};
use mjsv_core::output::{to_json, to_yaml, Format};
use mjsv_core::style::{ReportCtx, Style, Tool};
use mjsv_core::Ctx;

use cli::{Cli, Command, GlobalArgs};

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

/// O formato pedido, para os caminhos que rodam antes de existir um `Cli` (ajuda, versão, erro de
/// uso): `--output json` vale ali também, senão a saída de máquina ganharia arte.
fn format_from_env() -> Format {
    let args: Vec<String> = std::env::args().collect();
    Format::from_args(&args)
        .or_else(|| {
            Config::load()
                .ok()
                .and_then(|c| c.output)
                .as_deref()
                .and_then(Format::parse)
        })
        .unwrap_or_default()
}

/// Erros do clap: `--version` com a coroa e a tela inicial no `mjsv` sem argumentos, ambos só num
/// terminal. Todo o resto — e todo fluxo que não é terminal — sai pelo clap, byte a byte como antes.
async fn parse_error(e: clap::Error) -> ! {
    let format = format_from_env();
    match e.kind() {
        ErrorKind::DisplayVersion => {
            if let Some(line) = Style::stdout(format, Tool::Mjsv).version_line(env!("CARGO_PKG_VERSION")) {
                println!("{line}");
                std::process::exit(0);
            }
        }
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            let style = Style::stderr(format, Tool::Mjsv);
            if style.art() {
                if let Some(banner) = home_banner(&style).await {
                    eprintln!("{banner}");
                }
            }
        }
        _ => {}
    }
    e.exit()
}

/// A tela inicial: o banner HERO com as linhas reais. Sem sessão, nenhuma requisição.
async fn home_banner(style: &Style) -> Option<String> {
    let config = Config::load().ok()?;
    let credentials = Credentials::load().ok()?;
    let api_url = config.resolve_api_url(None);
    let client = Client::new(&api_url, credentials.resolve_token()).ok()?;
    let infos = mjsv_core::home::customer_infos(style, &client, config.org.as_deref()).await;
    style.home(env!("CARGO_PKG_VERSION"), &infos)
}

/// O erro de um comando, desenhado para o stderr.
fn render_failure(global: &GlobalArgs, e: &anyhow::Error) -> String {
    let config = Config::load().unwrap_or_default();
    let format = global
        .output
        .as_deref()
        .or(config.output.as_deref())
        .and_then(Format::parse)
        .unwrap_or_default();
    let api_url = config.resolve_api_url(global.api_url.as_deref());
    let args: Vec<String> = std::env::args().collect();
    let rc = ReportCtx::detect(&api_url, global.api_url.is_some(), global.verbose, &args);
    Style::stderr(format, Tool::Mjsv).render_error(e, &rc)
}

#[tokio::main]
async fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => parse_error(e).await,
    };

    init_tracing(cli.global.verbose);

    let global = cli.global.clone();
    if let Err(e) = run(cli).await {
        // Fora de um terminal isto é `erro: {e:#}` como sempre; num terminal, o cartão ER-.
        eprintln!("{}", render_failure(&global, &e));
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
