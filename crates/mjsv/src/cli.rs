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

    /// Máquinas virtuais.
    #[command(subcommand)]
    Vm(VmCommand),

    /// Gameservers.
    #[command(subcommand)]
    Gs(GsCommand),

    /// IPs elásticos de uma VM.
    #[command(subcommand)]
    Eip(EipCommand),

    /// Pedidos e faturas.
    #[command(subcommand)]
    Order(OrderCommand),

    /// Tickets de suporte.
    #[command(subcommand)]
    Ticket(TicketCommand),

    /// Chaves SSH da organização.
    #[command(subcommand, name = "ssh-key")]
    SshKey(SshKeyCommand),

    /// Chaves de API.
    #[command(subcommand)]
    Token(TokenCommand),
}

#[derive(Debug, Subcommand)]
pub enum OrderCommand {
    /// Lista os pedidos da organização.
    List,

    /// Lista apenas os pedidos pendentes.
    Pending,

    /// Mostra um pedido.
    Show { id: String },

    /// Cancela um pedido. Pede confirmação.
    Cancel { id: String },

    /// Baixa o PDF da fatura.
    Invoice {
        id: String,

        /// Caminho de saída. Padrão: `fatura-<id>.pdf` no diretório atual.
        #[arg(long, short = 'o')]
        output: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum TicketCommand {
    /// Lista os tickets da organização.
    List,

    /// Lista as categorias disponíveis (o id é exigido por `ticket open`).
    Categories,

    /// Mostra um ticket.
    Show { id: String },

    /// Mostra a conversa de um ticket.
    Messages { id: String },

    /// Abre um ticket.
    Open {
        #[arg(long)]
        title: String,

        #[arg(long)]
        message: String,

        /// Id da categoria — veja `mjsv ticket categories`.
        #[arg(long)]
        category: String,

        /// Serviço relacionado, se houver.
        #[arg(long)]
        service: Option<String>,
    },

    /// Responde um ticket.
    Reply { id: String, message: String },

    /// Fecha um ticket. Pede confirmação.
    Close { id: String },
}

#[derive(Debug, Subcommand)]
pub enum SshKeyCommand {
    /// Lista as chaves da organização.
    List,

    /// Adiciona uma chave a partir de um arquivo `.pub`.
    Add {
        #[arg(long)]
        name: String,

        /// Caminho do arquivo da chave PÚBLICA (ex.: ~/.ssh/id_ed25519.pub).
        #[arg(long)]
        file: String,
    },

    /// Remove uma chave. Pede confirmação.
    Delete { id: String },
}

/// Como nas VMs, o id é o do **serviço** (`/gameserver/:gameServerServiceId`).
#[derive(Debug, Subcommand)]
pub enum GsCommand {
    /// Lista os gameservers da organização.
    List,

    /// Mostra os detalhes de um gameserver.
    Show { id: String },

    /// Liga o servidor.
    Start { id: String },

    /// Para o servidor.
    Stop { id: String },

    /// Reinicia o servidor.
    Restart { id: String },

    /// Mata o processo sem salvar. Pede confirmação.
    Kill { id: String },

    /// Reexecuta o script de instalação. Pede confirmação.
    Reinstall { id: String },

    /// Metadados do servidor.
    Metadata { id: String },

    /// Bancos de dados do servidor.
    Databases { id: String },

    /// Agendamentos.
    Schedules { id: String },

    /// Credenciais de SFTP.
    Sftp { id: String },
}

#[derive(Debug, Subcommand)]
pub enum EipCommand {
    /// Lista os EIPs de uma VM (id do serviço).
    List { vm: String },
}

#[derive(Debug, Subcommand)]
pub enum FirewallCommand {
    /// Lista as regras e as faixas de gameguard.
    List { vm: String },

    /// Cria uma regra.
    Add {
        /// Id do serviço da VM.
        vm: String,

        #[arg(long, value_parser = ["allow", "drop"])]
        action: String,

        #[arg(long, value_parser = ["tcp", "udp"])]
        protocol: String,

        /// `443` ou `27015-27020`.
        #[arg(long)]
        ports: String,

        /// CIDR de origem. Ausente = qualquer origem (não use `0.0.0.0/0`, a API recusa).
        #[arg(long)]
        source: Option<String>,

        #[arg(long)]
        description: Option<String>,

        /// Posição desejada na lista (1-based). Ausente = no fim.
        #[arg(long)]
        position: Option<u32>,
    },

    /// Remove uma regra. Pede confirmação.
    Delete { vm: String, rule_id: String },
}

/// Todo subcomando recebe o **id do serviço** (a coluna `service` de `mjsv vm list`), porque as
/// rotas da API são `/vm/:vmServiceId` — chaveadas pelo serviço, não pelo id da VM.
#[derive(Debug, Subcommand)]
pub enum VmCommand {
    /// Lista as VMs da organização.
    List,

    /// Mostra os detalhes de uma VM.
    Show { id: String },

    /// Liga a VM.
    Start { id: String },

    /// Desliga pelo sistema convidado (ACPI).
    Stop { id: String },

    /// Reinicia a VM.
    Reboot { id: String },

    /// Corta a energia sem avisar o convidado. Pede confirmação.
    Kill { id: String },

    /// Estado de energia atual, direto do hypervisor.
    Power { id: String },

    /// Métricas de CPU, memória, disco e rede.
    Metrics { id: String },

    /// Dados de conexão do console VNC.
    Console { id: String },

    /// Firewall do EIP da VM.
    #[command(subcommand)]
    Firewall(FirewallCommand),
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
