use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "mjsv",
    version,
    about = "CLI do MJSV.IO",
    long_about = "Gerencia VMs, gameservers, firewall e faturamento do MJSV.IO pela linha de comando."
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Args, Clone)]
pub struct GlobalArgs {
    /// Organização a usar (sqid). Sobrepõe a definida por `mjsv org use`.
    #[arg(long, global = true, env = "MJSV_ORG")]
    pub org: Option<String>,

    /// Formato da saída.
    #[arg(long, global = true, value_parser = ["table", "json", "yaml"])]
    pub output: Option<String>,

    /// URL da API. Útil para apontar para um ambiente local.
    #[arg(long, global = true)]
    pub api_url: Option<String>,

    /// Mostra a requisição que seria enviada — método, URL, cabeçalhos de alvo e corpo — sem
    /// enviá-la. O bearer nunca é impresso.
    #[arg(long, global = true)]
    pub print_request: bool,

    /// Confirma operações destrutivas sem perguntar.
    #[arg(long, short = 'y', global = true)]
    pub yes: bool,

    /// Aumenta o log (-v, -vv).
    #[arg(long, short = 'v', global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Autentica e guarda a sessão em ~/.config/mjsv/credentials.toml.
    Login(LoginArgs),

    /// Encerra a sessão no servidor e apaga a credencial local.
    Logout,

    /// Mostra quem está autenticado e em qual organização.
    Whoami,

    /// Organizações.
    #[command(subcommand)]
    Org(OrgCommand),

    /// Chaves de API.
    #[command(subcommand)]
    Token(TokenCommand),
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Usuário ou e-mail. Perguntado se omitido.
    #[arg(long, short = 'u')]
    pub username: Option<String>,

    /// Autentica com uma chave de API já existente em vez de usuário e senha. O valor é lido do
    /// stdin ou de MJSV_TOKEN — nunca passe segredo em argumento, ele fica no histórico do shell.
    #[arg(long)]
    pub token: bool,
}

#[derive(Debug, Subcommand)]
pub enum OrgCommand {
    /// Lista as organizações da conta.
    List,

    /// Define a organização corrente.
    Use { org: String },
}

#[derive(Debug, Subcommand)]
pub enum TokenCommand {
    /// Lista as chaves de API (o segredo não é exibido — ele só aparece na criação).
    List,

    /// Cria uma chave de API e imprime o segredo uma única vez.
    Create { name: String },

    /// Revoga uma chave de API pelo id.
    Revoke { id: String },
}
