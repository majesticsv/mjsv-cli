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
    Whoami {
        /// Consulta o perfil completo (`GET /user/me`): saldo, 2FA, e-mail verificado e dados de cobrança.
        #[arg(long)]
        full: bool,
    },

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

    /// Redes privadas (VPCs) da organização.
    #[command(subcommand)]
    Vpc(VpcCommand),

    /// Volumes de armazenamento extra.
    #[command(subcommand)]
    Volume(VolumeCommand),

    /// Proteção anti-DDoS: endereços protegidos, telemetria, incidentes e gameguard.
    #[command(subcommand)]
    Protection(ProtectionCommand),

    /// Notificações da organização.
    ///
    /// Exigem a permissão ALL na organização: um cargo só com ORG_ADMIN toma 403.
    #[command(subcommand)]
    Notification(NotificationCommand),

    /// Sessões de login da conta.
    #[command(subcommand)]
    Session(SessionCommand),

    /// Perfil e dados de cobrança do usuário autenticado.
    #[command(subcommand)]
    Profile(ProfileCommand),
    /// Catálogo de produtos (público, não exige login).
    #[command(subcommand)]
    Product(ProductCommand),

    /// Formas de pagamento. Para pagar um pedido use `mjsv order pay`.
    #[command(subcommand)]
    Payment(PaymentCommand),

    /// Gera o script de completions do shell.
    ///
    /// Exemplo: `mjsv completions bash > /etc/bash_completion.d/mjsv`
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
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

    /// Apaga um pedido ainda não pago e seus serviços aguardando pagamento. Pede confirmação.
    ///
    /// Diferente de `cancel`: apagar devolve o saldo reservado e some com o pedido; só vale para
    /// pedido sem serviço já provisionado.
    Delete { id: String },

    /// Cria um pedido (HTTP 201). Depois pague com `mjsv order pay`.
    Create(OrderBodyArgs),

    /// Simula um pedido ou troca de plano sem criar nada (preço, cupom, renovação).
    Quote(OrderBodyArgs),

    /// Agrupa 2 ou mais pedidos pendentes (mesma moeda) em um só para pagar de uma vez.
    ///
    /// Desfazer = cancelar o pedido-pai devolvido.
    Group {
        /// Ids dos pedidos (mínimo 2, sem repetir).
        #[arg(required = true, num_args = 2..)]
        orders: Vec<String>,
    },

    /// Paga um pedido: imprime a URL de pagamento, ou conclui na hora com saldo.
    Pay {
        /// Id do pedido.
        id: String,

        /// Forma de pagamento (veja `mjsv payment methods`).
        #[arg(long, default_value = "MercadoPago", value_parser = ["MercadoPago", "Balance"])]
        method: String,

        /// Usa também o saldo da carteira para cobrir parte do valor. Pede confirmação.
        #[arg(long)]
        use_balance: bool,
    },

    /// Consulta um cupom pelo código (prévia; a elegibilidade final sai em `order quote`).
    Coupon { code: String },

    /// Campos de filtro/ordenação aceitos na listagem de pedidos (público).
    Metadata,

    /// Gera a fatura em PDF pela rota POST `generate-pdf`.
    ///
    /// O conteúdo é o mesmo de `invoice` (GET `invoice-pdf`); `invoice` é o caminho normal e este
    /// existe para quem precisa exercitar a rota de geração.
    GeneratePdf {
        /// Id do pedido.
        id: String,

        /// Caminho de saída. Padrão: `fatura-<id>.pdf` no diretório atual.
        #[arg(long = "out-file", short = 'o')]
        output: Option<String>,
    },

    /// Baixa o PDF da fatura.
    Invoice {
        id: String,

        /// Caminho de saída. Padrão: `fatura-<id>.pdf` no diretório atual.
        #[arg(long = "out-file", short = 'o')]
        output: Option<String>,
    },
}

/// Corpo de `order create` / `order quote`: flags simples ou um JSON completo.
#[derive(Debug, Args)]
pub struct OrderBodyArgs {
    /// Id do produto (veja `mjsv product list`).
    #[arg(long, required_unless_present = "file", conflicts_with = "file")]
    pub product: Option<String>,

    /// Opção escolhida, `idDoComponente=valor` (repetível). O id vem de `mjsv product get`;
    /// o valor é o id da opção discreta ou o número, se o componente for contínuo.
    #[arg(long = "option", conflicts_with = "file")]
    pub options: Vec<String>,

    /// Moeda (ISO 4217).
    #[arg(long, default_value = "BRL")]
    pub currency: String,

    /// Id do **serviço** a trocar de plano (em vez de contratar um novo).
    #[arg(long, conflicts_with = "file")]
    pub service: Option<String>,

    /// Código de cupom. Não vale para troca de plano.
    #[arg(long, conflicts_with = "file")]
    pub coupon: Option<String>,

    /// Saldo da carteira a usar, em centavos (limitado ao saldo e ao valor do pedido).
    #[arg(long, conflicts_with = "file")]
    pub wallet: Option<u64>,

    /// Corpo completo em JSON (`-` lê da entrada padrão), para vários itens no mesmo pedido.
    #[arg(long, short = 'f')]
    pub file: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ProductCommand {
    /// Lista os produtos do catálogo. Filtro de exemplo: `--filter serviceType=VIRTUALMACHINE`.
    List {
        #[command(flatten)]
        page: PageArgs,
    },

    /// Mostra um produto com seus componentes de preço e opções (os ids de `order create`).
    Get { id: String },

    /// Lista as ISOs disponíveis para um produto de VM.
    Isos {
        /// Id do produto (tem de ser de VM).
        id: String,

        /// Id da região. Obrigatório em produto multirregião, a menos que use `--region-option`.
        #[arg(long)]
        region: Option<String>,

        /// Id da opção de região do produto (o que o checkout conhece).
        #[arg(long)]
        region_option: Option<String>,
    },

    /// Campos de filtro/ordenação aceitos em `product list`.
    Metadata,
}

#[derive(Debug, Subcommand)]
pub enum PaymentCommand {
    /// Lista as formas de pagamento disponíveis.
    Methods,
}

#[derive(Debug, Subcommand)]
pub enum TicketCommand {
    /// Lista os tickets da organização, ou só os de uma categoria ou de um serviço.
    List {
        /// Só a categoria com este id **numérico** (o inteiro do banco, não o id mostrado em
        /// `ticket categories`, que é um sqid — a API ainda não decodifica).
        #[arg(long, conflicts_with = "service")]
        category: Option<u64>,

        /// Só os tickets deste **serviço** (id do serviço, não do recurso).
        #[arg(long)]
        service: Option<String>,

        #[command(flatten)]
        page: PageArgs,
    },

    /// Mostra uma mensagem pelo id dela (resolvido dentro da organização).
    Message { id: String },

    /// Reabre um ticket fechado.
    Reopen { id: String },

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

    /// Mostra o gameserver a partir do id do serviço (rota de escopo SERVICE).
    ///
    /// Útil quando só se tem o id do serviço de um pedido, sem saber o tipo do recurso.
    ByService {
        /// Id do serviço.
        service: String,
    },

    /// Edita variáveis de ambiente (startup) do servidor.
    Startup {
        /// Id do serviço do gameserver.
        id: String,

        /// Variável no formato CHAVE=VALOR. Repita para várias.
        #[arg(long = "env", value_name = "CHAVE=VALOR", required = true)]
        env: Vec<String>,
    },

    /// Gerência de arquivos do servidor.
    #[command(subcommand)]
    File(GsFileCommand),

    /// Bancos de dados do servidor.
    #[command(subcommand)]
    Db(GsDbCommand),

    /// Agendamentos (cron) e suas tarefas.
    #[command(subcommand)]
    Schedule(GsScheduleCommand),

    /// Add-ons (mods/plugins): instalados no servidor e catálogo.
    #[command(subcommand)]
    Addon(GsAddonCommand),

    /// Catálogo de modpacks.
    #[command(subcommand)]
    Modpack(GsModpackCommand),

    /// Passos de configuração de um jogo no catálogo.
    Setup {
        /// Jogo (ex.: minecraft).
        game: String,
    },

    /// Campos configuráveis de um jogo no catálogo.
    Fields {
        /// Jogo (ex.: minecraft).
        game: String,
    },

    /// Opções disponíveis para um campo de um jogo.
    Options {
        /// Jogo (ex.: minecraft).
        game: String,

        /// Nome do campo, como listado por `gs fields`.
        field: String,

        /// Filtro repassado ao catálogo, no formato CHAVE=VALOR (ex.: version=1.21). Repita para vários.
        #[arg(long = "param", value_name = "CHAVE=VALOR")]
        param: Vec<String>,
    },
}

/// Todos os comandos de arquivo usam o id do **serviço** do gameserver; os caminhos são absolutos
/// dentro do servidor (ex.: `/plugins`).
#[derive(Debug, Subcommand)]
pub enum GsFileCommand {
    /// Lista o conteúdo de um diretório.
    Ls {
        /// Id do serviço do gameserver.
        id: String,

        /// Diretório absoluto dentro do servidor.
        #[arg(default_value = "/")]
        dir: String,
    },

    /// Imprime o conteúdo de um arquivo de texto.
    Cat {
        id: String,

        /// Caminho absoluto do arquivo.
        path: String,
    },

    /// Obtém o link temporário de download de um arquivo e o imprime.
    Download {
        id: String,

        /// Caminho absoluto do arquivo.
        path: String,
    },

    /// Envia um arquivo local para o servidor.
    Upload {
        id: String,

        /// Arquivo local.
        local: String,

        /// Caminho absoluto de destino, incluindo o nome do arquivo.
        remote: String,
    },

    /// Renomeia (ou move dentro do mesmo diretório) um arquivo ou pasta.
    Mv {
        id: String,

        /// Diretório que contém o item.
        dir: String,

        /// Nome atual.
        old: String,

        /// Novo nome.
        new: String,
    },

    /// Cria uma pasta.
    Mkdir {
        id: String,

        /// Diretório pai.
        dir: String,

        /// Nome da nova pasta.
        name: String,
    },

    /// Compacta itens de um diretório.
    Compress {
        id: String,

        /// Diretório que contém os itens.
        dir: String,

        /// Nomes dos itens a compactar.
        #[arg(required = true)]
        names: Vec<String>,
    },

    /// Descompacta um arquivo no próprio diretório.
    Decompress {
        id: String,

        /// Diretório que contém o arquivo.
        dir: String,

        /// Nome do arquivo compactado.
        archive: String,
    },

    /// Apaga arquivos ou pastas. Pede confirmação.
    Rm {
        id: String,

        /// Diretório que contém os itens.
        dir: String,

        /// Nomes dos itens a apagar.
        #[arg(required = true)]
        names: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum GsDbCommand {
    /// Cria um banco. A senha é entregue só nesta resposta.
    Create {
        /// Id do serviço do gameserver.
        id: String,

        /// Nome do banco.
        name: String,
    },

    /// Revela a senha de um banco (a ação fica no log de auditoria).
    Password {
        id: String,

        /// Nome do banco, como listado por `gs databases`.
        name: String,
    },

    /// Apaga um banco. Pede confirmação.
    Delete {
        id: String,

        /// Nome do banco (o backend resolve o id interno a partir do nome).
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum GsScheduleCommand {
    /// Mostra um agendamento e suas tarefas.
    Show {
        /// Id do serviço do gameserver.
        id: String,

        /// Id do agendamento.
        schedule: String,
    },

    /// Cria um agendamento.
    Create {
        id: String,

        #[arg(long)]
        name: String,

        /// Expressão cron de 5 campos (minuto hora dia-do-mês mês dia-da-semana).
        #[arg(long)]
        cron: String,

        /// Só executa com o servidor ligado.
        #[arg(long)]
        only_online: bool,

        /// Cria desativado.
        #[arg(long)]
        disabled: bool,
    },

    /// Edita um agendamento. Só os campos informados mudam.
    Edit {
        id: String,

        /// Id do agendamento.
        schedule: String,

        #[arg(long)]
        name: Option<String>,

        /// Expressão cron de 5 campos.
        #[arg(long)]
        cron: Option<String>,

        /// Ativa (true) ou desativa (false).
        #[arg(long)]
        enabled: Option<bool>,

        /// Só executa com o servidor ligado (true/false).
        #[arg(long)]
        only_online: Option<bool>,
    },

    /// Apaga um agendamento. Pede confirmação.
    Delete { id: String, schedule: String },

    /// Tarefas de um agendamento.
    #[command(subcommand)]
    Task(GsTaskCommand),
}

#[derive(Debug, Subcommand)]
pub enum GsTaskCommand {
    /// Adiciona uma tarefa ao agendamento.
    Add {
        /// Id do serviço do gameserver.
        id: String,

        /// Id do agendamento.
        schedule: String,

        #[arg(long, value_parser = ["command", "power", "backup"])]
        action: String,

        /// Comando, ação de energia ou nome do backup, conforme a ação.
        #[arg(long)]
        payload: String,

        /// Atraso em segundos depois da tarefa anterior.
        #[arg(long)]
        offset: Option<i64>,

        /// Segue para a próxima tarefa mesmo se esta falhar.
        #[arg(long)]
        continue_on_failure: bool,
    },

    /// Edita uma tarefa. Só os campos informados mudam.
    Edit {
        id: String,
        schedule: String,

        /// Id da tarefa.
        task: String,

        #[arg(long, value_parser = ["command", "power", "backup"])]
        action: Option<String>,

        #[arg(long)]
        payload: Option<String>,

        #[arg(long)]
        offset: Option<i64>,

        #[arg(long)]
        continue_on_failure: Option<bool>,
    },

    /// Apaga uma tarefa. Pede confirmação.
    Delete { id: String, schedule: String, task: String },
}

#[derive(Debug, Subcommand)]
pub enum GsAddonCommand {
    /// Pesquisa add-ons no catálogo.
    Search {
        /// Jogo (ex.: minecraft).
        game: String,

        /// Texto da busca.
        query: String,

        /// Filtro extra repassado ao catálogo (provider, kind, minecraft_version, modloader, page...).
        #[arg(long = "param", value_name = "CHAVE=VALOR")]
        param: Vec<String>,
    },

    /// Categorias de add-ons do catálogo.
    Categories {
        game: String,

        /// Parâmetro extra repassado ao catálogo.
        #[arg(long = "param", value_name = "CHAVE=VALOR")]
        param: Vec<String>,
    },

    /// Detalhes de um add-on do catálogo.
    Show {
        game: String,

        /// Provedor (ex.: modrinth).
        provider: String,

        /// Id do projeto no provedor.
        project: String,

        /// Parâmetro extra repassado ao catálogo.
        #[arg(long = "param", value_name = "CHAVE=VALOR")]
        param: Vec<String>,
    },

    /// Versões de um add-on do catálogo.
    Versions {
        game: String,
        provider: String,
        project: String,

        /// Filtro extra (minecraft_version, modloader...).
        #[arg(long = "param", value_name = "CHAVE=VALOR")]
        param: Vec<String>,
    },

    /// Resolve dependências de uma lista de add-ons (corpo JSON do catálogo).
    Resolve {
        game: String,

        /// Arquivo JSON com o corpo, ou `-` para stdin.
        #[arg(long)]
        file: String,
    },

    /// Consulta metadados de uma lista de add-ons (corpo JSON do catálogo).
    Lookup {
        game: String,

        /// Arquivo JSON com o corpo, ou `-` para stdin.
        #[arg(long)]
        file: String,
    },

    /// Consulta atualizações no catálogo para uma lista de add-ons (corpo JSON do catálogo).
    Updates {
        game: String,

        /// Arquivo JSON com o corpo, ou `-` para stdin.
        #[arg(long)]
        file: String,
    },

    /// Lock de add-ons do servidor (instalados e pendentes).
    Installed {
        /// Id do serviço do gameserver.
        id: String,
    },

    /// Adiciona um add-on ao servidor. Só vale após reiniciar.
    ///
    /// Instalar edita a lista desejada; quem baixa o arquivo é o servidor no próximo boot.
    Install {
        id: String,

        /// Provedor (ex.: modrinth).
        #[arg(long)]
        provider: String,

        /// Id do projeto no provedor.
        #[arg(long)]
        project: String,

        /// Versão fixa; sem ela, acompanha a mais recente compatível.
        #[arg(long)]
        version: Option<String>,
    },

    /// Remove um add-on do servidor pelo nome do arquivo. Pede confirmação.
    Remove {
        id: String,

        /// Nome do arquivo, como em `addon installed`.
        file: String,
    },

    /// Atualiza no servidor a lista de atualizações disponíveis.
    CheckUpdates { id: String },
}

#[derive(Debug, Subcommand)]
pub enum GsModpackCommand {
    /// Pesquisa modpacks no catálogo.
    Search {
        game: String,

        /// Texto da busca.
        query: String,

        /// Filtro extra repassado ao catálogo (provider, page, limit...).
        #[arg(long = "param", value_name = "CHAVE=VALOR")]
        param: Vec<String>,
    },

    /// Detalhes de um modpack.
    Show {
        game: String,
        provider: String,

        /// Id do modpack no provedor.
        modpack: String,

        /// Parâmetro extra repassado ao catálogo.
        #[arg(long = "param", value_name = "CHAVE=VALOR")]
        param: Vec<String>,
    },

    /// Resolve um modpack (corpo JSON do catálogo).
    Resolve {
        game: String,

        /// Arquivo JSON com o corpo, ou `-` para stdin.
        #[arg(long)]
        file: String,
    },

    /// Consulta metadados de um modpack (corpo JSON do catálogo).
    Lookup {
        game: String,

        /// Arquivo JSON com o corpo, ou `-` para stdin.
        #[arg(long)]
        file: String,
    },
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

    /// Altera uma regra (só os campos informados). Pede confirmação.
    RuleUpdate {
        /// Id do serviço da VM.
        vm: String,

        /// Id da regra — coluna `id` de `firewall list`.
        rule_id: String,

        #[arg(long, value_parser = ["allow", "drop"])]
        action: Option<String>,

        #[arg(long, value_parser = ["tcp", "udp"])]
        protocol: Option<String>,

        /// `443` ou `27015-27020`.
        #[arg(long)]
        ports: Option<String>,

        /// CIDR de origem. `any` volta para qualquer origem (a API recusa `0.0.0.0/0`).
        #[arg(long)]
        source: Option<String>,

        #[arg(long)]
        description: Option<String>,

        /// Nova posição na lista (1-based).
        #[arg(long)]
        position: Option<u32>,

        /// Liga ou desliga a regra sem apagá-la.
        #[arg(long)]
        enabled: Option<bool>,
    },

    /// Cria ou substitui uma faixa de gameguard (mitigação por perfil de jogo).
    GameguardSet {
        /// Id do serviço da VM.
        vm: String,

        /// `27015` ou `27015-27020`.
        #[arg(long)]
        ports: String,

        #[arg(long, value_parser = GAMEGUARD_PROFILES)]
        profile: String,
    },

    /// Remove uma faixa de gameguard. Pede confirmação.
    GameguardRm {
        /// Id do serviço da VM.
        vm: String,

        /// Id da faixa — seção `gameguard` de `firewall list`.
        range_id: String,
    },
}

/// Perfis aceitos pelo DTO de gameguard, tanto da VM quanto do serviço de proteção.
pub const GAMEGUARD_PROFILES: [&str; 13] = [
    "UDP_GENERIC",
    "TCP_GENERIC",
    "BOTH_GENERIC",
    "SRCDS",
    "SRCDS_L4D2",
    "HLDS",
    "CS2",
    "RUST",
    "SCPSL",
    "FIVEM",
    "MINECRAFT_JAVA",
    "MINECRAFT_BEDROCK",
    "MINECRAFT_JAVA_BEDROCK",
];

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

    /// Estado do console VNC (o acesso em si é pelo painel).
    Console { id: String },

    /// Firewall do EIP da VM.
    #[command(subcommand)]
    Firewall(FirewallCommand),

    /// Resolve o id do serviço para o id da VM (o que `volume available` e `volume products` usam).
    Resolve { id: String },

    /// Incidentes de ataque contra os IPs da VM.
    Incidents {
        /// Id do serviço da VM.
        id: String,

        #[command(flatten)]
        filter: IncidentFilter,
    },

    /// Detalhe de um incidente (id vindo de `vm incidents`).
    Incident {
        /// Id do serviço da VM.
        id: String,

        incident: String,
    },

    /// Imagens/ISOs que a VM pode usar no `rebuild` (a coluna `id` vai em `--image`).
    Images { id: String },

    /// Catálogo de imagens de nuvem da organização (todas as localizações).
    CloudImages,

    /// VPCs para as quais a VM pode ser movida (a coluna `id` vai em `move --vpc`).
    Vpcs { id: String },

    /// Move a VM para outra VPC. Pede confirmação.
    Move {
        /// Id do serviço da VM.
        id: String,

        /// Id da VPC de destino — veja `mjsv vm vpcs`.
        #[arg(long)]
        vpc: String,
    },

    /// Reinstala o sistema a partir de uma imagem e APAGA O DISCO. Pede confirmação.
    Rebuild {
        /// Id do serviço da VM.
        id: String,

        /// Id da imagem — veja `mjsv vm images`.
        #[arg(long)]
        image: String,

        /// Pergunta a nova senha de root em vez de gerar uma (recupere a gerada com `root-password`).
        #[arg(long)]
        password: bool,
    },

    /// Troca a senha de root e imprime a nova uma única vez. Pede confirmação.
    ResetPassword {
        /// Id do serviço da VM.
        id: String,

        /// Pergunta a senha em vez de gerar uma aleatória.
        #[arg(long)]
        password: bool,
    },

    /// Imprime a senha de root atual (cada consulta fica registrada em auditoria).
    RootPassword { id: String },

    /// Ressincroniza o IP privado da VM com o lease do DHCP.
    SyncDhcpIp {
        id: String,

        /// IP manual, em vez de resolver pelo lease.
        #[arg(long)]
        ip: Option<String>,
    },

    /// Gera um token de console VNC de curta duração e imprime a senha do VNC.
    VncToken { id: String },

    /// Altera a senha do VNC. Mudar vCPU/memória (plano) é feito por pedido: `mjsv order quote`.
    Update {
        /// Id do serviço da VM.
        id: String,

        /// Pergunta a nova senha do VNC.
        #[arg(long, required = true)]
        vnc_password: bool,
    },

    /// Define as chaves SSH da VM, SUBSTITUINDO o conjunto atual. Pede confirmação.
    SshKeys {
        /// Id do serviço da VM.
        id: String,

        /// Id de uma chave da organização (`mjsv ssh-key list`). Repita para várias.
        #[arg(long = "key")]
        keys: Vec<String>,

        /// Remove todas as chaves.
        #[arg(long, conflicts_with = "keys")]
        clear: bool,
    },
}

/// Filtros comuns de incidentes (VM e proteção). Tempos em segundos Unix.
#[derive(Debug, Args)]
pub struct IncidentFilter {
    #[arg(long, value_parser = ["active", "ended"])]
    pub status: Option<String>,

    /// Só incidentes deste IPv4 (precisa ser da VM/serviço).
    #[arg(long)]
    pub ip: Option<String>,

    /// Início da janela (segundos Unix). Padrão da API: últimos 30 dias.
    #[arg(long)]
    pub from: Option<u64>,

    #[arg(long)]
    pub to: Option<u64>,

    /// Itens por página (1-25).
    #[arg(long)]
    pub limit: Option<u32>,

    /// `nextCursor` da página anterior.
    #[arg(long)]
    pub cursor: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum VpcCommand {
    /// Lista as VPCs da organização.
    List,

    /// Cria uma VPC (a API aceita no máximo 2 por região).
    Create {
        /// Id da região — veja `mjsv vpc regions`.
        #[arg(long)]
        region: String,

        #[arg(long)]
        name: String,

        /// Sub-rede IPv4. Padrão da API: 172.16.0.0/24.
        #[arg(long)]
        subnet: Option<String>,

        /// Delega um prefixo IPv6 automaticamente.
        #[arg(long)]
        auto_ipv6: Option<bool>,

        /// Liga o DHCP IPv4 da rede.
        #[arg(long)]
        dhcp: Option<bool>,

        #[arg(long)]
        dhcp_start: Option<String>,

        #[arg(long)]
        dhcp_end: Option<String>,
    },

    /// Apaga uma VPC vazia (sem VMs nem EIPs). Pede confirmação.
    Delete {
        /// Id da VPC — coluna `id` de `vpc list`.
        id: String,
    },

    /// Regiões disponíveis para criar VPCs.
    Regions,
}

#[derive(Debug, Subcommand)]
pub enum VolumeCommand {
    /// Lista os volumes da organização.
    List,

    /// Volumes desanexados que podem ser ligados a uma VM (mesmo nó).
    Available {
        /// Id do serviço da VM.
        #[arg(long)]
        vm: String,
    },

    /// Planos de volume disponíveis na região da VM.
    Products {
        /// Id do serviço da VM.
        #[arg(long)]
        vm: String,
    },

    /// Anexa um volume a uma VM.
    Attach {
        /// Id do volume — coluna `id` de `volume list`.
        volume: String,

        /// Id do serviço da VM.
        #[arg(long)]
        vm: String,
    },

    /// Desanexa um volume. Desmonte no sistema convidado antes. Pede confirmação.
    Detach {
        /// Id do volume.
        volume: String,
    },
}

/// O id é o do **serviço** de proteção, o mesmo que vai no cabeçalho de alvo.
#[derive(Debug, Subcommand)]
pub enum ProtectionCommand {
    /// Mostra o serviço de proteção e seus endereços.
    Show { service: String },

    /// Telemetria de tráfego (a API limita a janela a 90 dias).
    Telemetry {
        service: String,

        /// Início, em segundos Unix.
        #[arg(long)]
        from: u64,

        /// Fim, em segundos Unix.
        #[arg(long)]
        to: u64,

        #[arg(long)]
        ip: Option<String>,
    },

    /// Incidentes de ataque.
    Incidents {
        service: String,

        #[command(flatten)]
        filter: IncidentFilter,
    },

    /// Detalhe de um incidente.
    Incident { service: String, incident: String },

    /// Firewall (faixas de gameguard) de um endereço protegido.
    Firewall {
        service: String,

        /// Id do endereço — seção `addresses` de `protection show`.
        addr: String,
    },

    /// Faixas de gameguard de um endereço protegido.
    #[command(subcommand)]
    Gameguard(ProtectionGameguardCommand),
}

#[derive(Debug, Subcommand)]
pub enum ProtectionGameguardCommand {
    /// Cria ou substitui uma faixa.
    Set {
        service: String,
        addr: String,

        /// `27015` ou `27015-27020`.
        #[arg(long)]
        ports: String,

        #[arg(long, value_parser = GAMEGUARD_PROFILES)]
        profile: String,
    },

    /// Remove uma faixa. Pede confirmação.
    Rm {
        service: String,
        addr: String,

        /// Id da faixa — veja `protection firewall`.
        range: String,
    },
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

/// Paginação e filtro das listagens (`page`, `count`, `search`, `orderBy`, `order`, `filter[..]`).
#[derive(Debug, Args, Clone, Default)]
pub struct PageArgs {
    /// Página (começa em 1).
    #[arg(long)]
    pub page: Option<u32>,

    /// Itens por página. A API recusa 100 ou mais.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..100))]
    pub count: Option<u32>,

    /// Busca livre.
    #[arg(long)]
    pub search: Option<String>,

    /// Campo de ordenação.
    #[arg(long)]
    pub order_by: Option<String>,

    /// Sentido da ordenação.
    #[arg(long, value_parser = ["asc", "desc"])]
    pub order: Option<String>,

    /// Filtro `campo=valor` ou `campo[op]=valor` (op: equals, not, in, notIn, lt, lte, gt, gte,
    /// contains, startsWith, endsWith, isNull). Pode repetir.
    #[arg(long = "filter")]
    pub filters: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum OrgCommand {
    /// Lista as organizações da conta.
    List,

    /// Define a organização corrente.
    Use { org: String },

    /// Cria uma organização (4 a 32 caracteres). A conta pode ser dona de cerca de 11.
    Create { name: String },

    /// Mostra uma organização (padrão: a corrente). Exige permissão ALL.
    Show { org: Option<String> },

    /// Renomeia a organização (4 a 32 caracteres). Exige ORG_ADMIN.
    Rename { org: String, name: String },

    /// Apaga a organização (exclusão lógica). Só o dono consegue. Pede confirmação.
    Delete { org: String },

    /// Sai da organização. O dono não pode sair. Pede confirmação.
    Leave { org: String },

    /// Registro de auditoria da organização corrente. Exige a permissão LOGS_VIEW.
    AuditLog {
        #[command(flatten)]
        page: PageArgs,
    },

    /// Membros da organização corrente.
    #[command(subcommand)]
    Member(OrgMemberCommand),

    /// Cargos da organização corrente.
    #[command(subcommand)]
    Role(OrgRoleCommand),

    /// Convites pendentes da organização corrente.
    #[command(subcommand)]
    Invite(OrgInviteCommand),

    /// Serviços cobrados da organização corrente (todos os tipos).
    #[command(subcommand)]
    Service(OrgServiceCommand),
}

#[derive(Debug, Subcommand)]
pub enum OrgMemberCommand {
    /// Lista os membros. Exige permissão ALL.
    List {
        #[command(flatten)]
        page: PageArgs,
    },

    /// Adiciona um membro pelo e-mail. Quem ainda não tem conta recebe um convite. Exige ORG_ADMIN.
    Add { email: String },

    /// Remove um membro (id do usuário, veja `org member list`). O dono não pode ser removido. Pede confirmação.
    Remove { user: String },

    /// Atribui um cargo a um membro. Não dá para atribuir cargo igual ou acima do seu.
    RoleAdd {
        /// Id do usuário.
        user: String,

        /// Id do cargo (veja `org role list`).
        #[arg(long)]
        role: String,
    },

    /// Tira um cargo de um membro. Pede confirmação.
    RoleRemove {
        /// Id do usuário.
        user: String,

        /// Id do cargo (veja `org role list`).
        #[arg(long)]
        role: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum OrgRoleCommand {
    /// Lista os cargos. Exige permissão ALL.
    List {
        #[command(flatten)]
        page: PageArgs,
    },

    /// Lista os nomes de permissão aceitos em `--flags`.
    Flags,

    /// Cria um cargo. A posição é única na organização; 0 é o dono.
    Create {
        /// 4 a 16 caracteres.
        #[arg(long)]
        name: String,

        #[arg(long)]
        position: u32,

        #[command(flatten)]
        perms: PermArgs,
    },

    /// Atualiza um cargo.
    ///
    /// Só nome e posição. A API ainda não aceita alterar permissões aqui: `--flags`/`--mask` são
    /// recusados pelo CLI; para mudar permissões, apague e recrie o cargo.
    Update {
        /// Id do cargo.
        role: String,

        #[arg(long)]
        name: Option<String>,

        #[arg(long)]
        position: Option<u32>,

        #[command(flatten)]
        perms: PermArgs,
    },

    /// Apaga um cargo. Pede confirmação.
    Delete { role: String },

    /// Troca a posição de dois cargos. Não vale para cargos na sua posição ou acima.
    Swap { role_a: String, role_b: String },
}

/// As permissões de um cargo: ou nomes, ou a máscara já calculada.
#[derive(Debug, Args, Clone, Default)]
pub struct PermArgs {
    /// Nomes de permissão separados por vírgula (veja `org role flags`). Ex.: `members-list,ticket-view`.
    #[arg(long, value_delimiter = ',', conflicts_with = "mask")]
    pub flags: Vec<String>,

    /// Máscara decimal já calculada (bit = número da permissão - 1).
    #[arg(long)]
    pub mask: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum OrgInviteCommand {
    /// Lista os convites pendentes (não expirados nem aceitos). Exige ORG_ADMIN.
    List,

    /// Revoga um convite pendente (id do convite). Pede confirmação.
    Revoke { invite: String },
}

/// O id é o do **serviço** (coluna `service` de `vm list`/`gs list`), não o do recurso.
#[derive(Debug, Subcommand)]
pub enum OrgServiceCommand {
    /// Lista os serviços da organização. Exige ORG_ADMIN.
    List {
        #[command(flatten)]
        page: PageArgs,
    },

    /// Mostra um serviço, com a configuração contratada.
    Show { service: String },

    /// Renomeia um serviço (3 a 64 caracteres). Serviço cancelado dá 404.
    Rename { service: String, name: String },

    /// Cancela um serviço. `--now` apaga os recursos na hora e é irreversível. Pede confirmação.
    #[command(group(clap::ArgGroup::new("modo").required(true).args(["now", "end_of_period"])))]
    Cancel {
        service: String,

        /// Cancela imediatamente.
        #[arg(long)]
        now: bool,

        /// Cancela ao fim do período já pago (dá para desfazer com `cancel-undo`).
        #[arg(long)]
        end_of_period: bool,

        /// Motivo (até 255 caracteres).
        #[arg(long)]
        reason: Option<String>,
    },

    /// Desfaz um cancelamento agendado para o fim do período.
    CancelUndo { service: String },
}

#[derive(Debug, Subcommand)]
pub enum NotificationCommand {
    /// Lista as notificações da organização corrente.
    List {
        /// Só um tipo.
        #[arg(long = "type", value_parser = ["PAYMENT", "GAME_SERVER", "VPS", "TEAM_ROLES", "ORDERS"])]
        kind: Option<String>,

        /// Só as não lidas.
        #[arg(long)]
        unread: bool,

        #[command(flatten)]
        page: PageArgs,
    },

    /// Conta as notificações não lidas.
    Count {
        /// Separa por tipo.
        #[arg(long)]
        by_type: bool,
    },

    /// Marca uma notificação como lida.
    Read { id: String },

    /// Marca todas como lidas.
    ReadAll,

    /// Apaga uma notificação. Pede confirmação.
    Delete { id: String },

    /// Apaga todas as lidas. Pede confirmação.
    ///
    /// Bug conhecido do backend: a rota é capturada por `DELETE .../:notificationId` e responde 404.
    #[command(hide = true)]
    ClearRead,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// Lista as sessões de login da conta.
    List,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum ProfileCommand {
    /// Mostra o perfil (saldo, 2FA, dados de cobrança).
    Show,

    /// Atualiza nome e dados de cobrança. Só o que for informado muda; `""` limpa os campos de cobrança.
    /// E-mail e senha não mudam por aqui.
    Update {
        #[arg(long)]
        name: Option<String>,

        #[arg(long, value_parser = ["CPF", "CNPJ"])]
        tax_id_type: Option<String>,

        /// CPF/CNPJ; o dígito verificador é validado pela API.
        #[arg(long)]
        tax_id: Option<String>,

        /// Razão social ou nome completo (3 a 120).
        #[arg(long)]
        legal_name: Option<String>,

        /// CEP, com ou sem hífen.
        #[arg(long)]
        billing_zip_code: Option<String>,

        #[arg(long)]
        billing_street: Option<String>,

        #[arg(long)]
        billing_number: Option<String>,

        #[arg(long)]
        billing_complement: Option<String>,

        #[arg(long)]
        billing_district: Option<String>,

        /// Código IBGE do município (7 dígitos).
        #[arg(long)]
        billing_city_code: Option<String>,

        #[arg(long)]
        billing_city_name: Option<String>,

        /// UF, 2 letras.
        #[arg(long)]
        billing_state: Option<String>,

        /// Telefone com 10 a 13 dígitos, `+` opcional.
        #[arg(long)]
        billing_phone: Option<String>,
    },
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

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    /// Pega nomes/flags duplicados entre os subcomandos, o tipo de erro que só aparece em runtime.
    #[test]
    fn definicao_do_clap_e_consistente() {
        Cli::command().debug_assert();
    }
}
