//! Erros: o cartão ER- de cada falha, e a linha `erro: ...` de hoje fora de um terminal.
//!
//! `main()` dos dois binários chama [`Style::render_error`] e imprime o resultado em stderr. A
//! regra de ouro: fora de um terminal estilizado a saída é `erro: {e:#}` como sempre (com as três
//! correções de texto da especificação: sem o `0:` do "nenhuma organização", login recusado com
//! texto próprio, e "exige um terminal" com o conselho certo).

use std::error::Error as _;
use std::fmt;

use crate::config::{Credentials, API_URL_ENV, TOKEN_ENV};
use crate::error::{ApiError, LoginRefused, OrgNotInAccount, TokenRefused};
use crate::prompt::{NoTerminalHint, PromptError};

use super::text::{sanitize, width, wrap_words};
use super::{Glyph, Style};

/// Um erro que carrega o próprio cartão. É a saída de emergência para código de comando que quer
/// um cartão sem mexer no core: `Err(CardError::new(plain, "Título").cause(..).hint(..).into())`.
#[derive(Debug, Clone)]
pub struct CardError {
    /// O texto de hoje, byte a byte (`Display`): é o que sai fora de um terminal estilizado.
    pub plain: String,
    pub title: String,
    pub glyph: Glyph,
    /// Linhas de causa, já com o texto final (o recuo é posto na hora de desenhar).
    pub causes: Vec<String>,
    pub hint: Option<(String, String)>,
}

impl CardError {
    pub fn new(plain: impl Into<String>, title: impl Into<String>) -> Self {
        CardError {
            plain: plain.into(),
            title: title.into(),
            glyph: Glyph::Failed,
            causes: Vec::new(),
            hint: None,
        }
    }

    pub fn cause(mut self, c: impl Into<String>) -> Self {
        self.causes.push(c.into());
        self
    }

    pub fn hint(mut self, label: impl Into<String>, cmd: impl Into<String>) -> Self {
        self.hint = Some((label.into(), cmd.into()));
        self
    }

    /// Glifo de atenção (`▲`) em vez de falha: não é erro de uso.
    pub fn attention(mut self) -> Self {
        self.glyph = Glyph::Attention;
        self
    }
}

impl fmt::Display for CardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.plain)
    }
}

impl std::error::Error for CardError {}

/// O que o renderizador precisa saber além do próprio erro.
#[derive(Debug, Clone, Default)]
pub struct ReportCtx {
    /// URL base da API em uso.
    pub api_url: String,
    /// Veio de `--api-url` ou `MJSV_API_URL` (a ponte mostra de onde).
    pub api_url_overridden: bool,
    /// O token em uso veio da variável `MJSV_TOKEN`.
    pub token_from_env: bool,
    /// Existe um token no arquivo de credenciais.
    pub stored_token: bool,
    /// `-v` / `-vv`: a linha original entra, esmaecida, depois do cartão.
    pub verbose: u8,
    /// O primeiro substantivo da linha de comando (`vm`, `org`...), para a dica de 404.
    pub noun: Option<String>,
}

/// Substantivos de topo que têm `list`, por binário (a dica de 404 só existe quando o comando
/// existe de verdade: "nunca invente uma dica").
const CUSTOMER_LIST_NOUNS: &[&str] = &[
    "vm",
    "gs",
    "volume",
    "vpc",
    "org",
    "order",
    "ticket",
    "ssh-key",
    "token",
    "session",
    "notification",
    "product",
    "eip",
];
const STAFF_LIST_NOUNS: &[&str] = &[
    "node",
    "vm",
    "org",
    "user",
    "region",
    "machine",
    "operation",
    "order",
    "coupon",
    "product",
    "ticket",
    "router",
    "gs",
    "service",
    "audit",
    "warn",
    "volume",
    "vpc",
    "eip",
    "eipv6",
    "ip-pool",
    "ipv6-pool",
];

impl ReportCtx {
    /// Detecção no ambiente real: a origem do token e a existência do arquivo de credenciais são
    /// decididas localmente, sem uma requisição a mais.
    pub fn detect(api_url: &str, api_url_flag: bool, verbose: u8, args: &[String]) -> ReportCtx {
        let env_token = std::env::var(TOKEN_ENV).ok().is_some_and(|v| !v.trim().is_empty());
        let stored = Credentials::load()
            .ok()
            .and_then(|c| c.token)
            .is_some_and(|t| !t.trim().is_empty());
        let env_url = std::env::var(API_URL_ENV).ok().is_some_and(|v| !v.trim().is_empty());

        ReportCtx {
            api_url: api_url.to_string(),
            api_url_overridden: api_url_flag || env_url,
            token_from_env: env_token,
            stored_token: stored,
            verbose,
            noun: first_noun(args),
        }
    }
}

/// A primeira palavra que não é flag depois do nome do programa.
pub fn first_noun(args: &[String]) -> Option<String> {
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if !a.starts_with('-') {
            return Some(a.clone());
        }
        // Flags globais com valor separado.
        if matches!(a.as_str(), "--org" | "--output" | "--api-url") {
            it.next();
        }
    }
    None
}

impl Style {
    /// Desenha qualquer erro de um comando. Sem quebra de linha final.
    pub fn render_error(&self, err: &anyhow::Error, rc: &ReportCtx) -> String {
        let plain = format!("erro: {err:#}");
        if !self.fancy() {
            // Saída de máquina com o stderr num terminal: o texto continua o de hoje, mas a mensagem
            // da API não pode levar ESC/OSC/bidi até a tela. Erro vindo da API perde também as
            // quebras de linha (uma linha falsa seria indistinguível); os demais mantêm as suas.
            // Sem terminal nada muda, byte a byte.
            if self.caps.tty {
                return if err.chain().any(|c| c.is::<ApiError>()) {
                    sanitize(&plain)
                } else {
                    plain.split('\n').map(sanitize).collect::<Vec<_>>().join("\n")
                };
            }
            return plain;
        }

        // Só cartão quando o erro tipado é o mais externo: com `.context(...)` por cima, a linha
        // `erro: contexto: causa` carrega mais informação que o cartão e sobrevive inteira.
        let top_is_typed = err.chain().next().is_some_and(|top| {
            top.is::<ApiError>()
                || top.is::<PromptError>()
                || top.is::<CardError>()
                || top.is::<LoginRefused>()
                || top.is::<TokenRefused>()
                || top.is::<OrgNotInAccount>()
                || (top.is::<serde_json::Error>() && err.chain().count() == 1)
        });
        // "organização X não encontrada" é um 404 decidido no cliente (erro de texto puro, sem
        // contexto por cima): mesmo cartão do 404.
        let local_404 = (err.chain().count() == 1 && !top_is_typed)
            .then(|| local_not_found(&err.to_string()))
            .flatten();
        if !top_is_typed && local_404.is_none() {
            return self.error_line(&sanitize(&format!("{err:#}")));
        }

        let lines = if let Some((title, noun)) = local_404 {
            let mut l = vec![self.title_line(Glyph::Failed, &sanitize(&title))];
            if self.list_noun_known(noun) {
                l.push(self.hint_default("tente", &format!("{} {noun} list", self.tool.bin())));
            }
            l
        } else if let Some(e) = err.downcast_ref::<ApiError>() {
            self.api_card(e, rc)
        } else if let Some(e) = err.downcast_ref::<serde_json::Error>() {
            // Corpo de forma errada decodificado pelo próprio comando: o mesmo cartão de ER-decode.
            self.api_card(
                &ApiError::Decode {
                    path: crate::client::last_path(),
                    message: e.to_string(),
                },
                rc,
            )
        } else if let Some(e) = err.downcast_ref::<PromptError>() {
            self.prompt_card(e)
        } else if let Some(e) = err.downcast_ref::<LoginRefused>() {
            vec![self.error_line(&capitalize(&e.to_string()))]
        } else if err.downcast_ref::<TokenRefused>().is_some() {
            vec![
                self.title_line(Glyph::Failed, "Chave de API recusada (401)"),
                self.hint_default("tente", "mjsv login --token < chave.txt"),
            ]
        } else if let Some(e) = err.downcast_ref::<OrgNotInAccount>() {
            self.org_use_card(e)
        } else if let Some(e) = err.downcast_ref::<CardError>() {
            self.custom_card(e)
        } else {
            vec![self.error_line(&sanitize(&format!("{err:#}")))]
        };

        let multi = lines.len() > 1;
        let mut out = lines.join("\n");
        if rc.verbose > 0 && multi {
            out.push('\n');
            out.push_str(&format!("{}{}", self.indent(), self.dim(&plain)));
        }
        out
    }

    fn custom_card(&self, e: &CardError) -> Vec<String> {
        let mut l = vec![self.title_line(e.glyph, &sanitize(&e.title))];
        l.extend(e.causes.iter().map(|c| format!("{}{}", self.indent(), sanitize(c))));
        if let Some((k, v)) = &e.hint {
            l.push(self.hint_default(k, v));
        }
        l
    }

    fn org_use_card(&self, e: &OrgNotInAccount) -> Vec<String> {
        let (avail, cmd) = if e.available.is_empty() {
            ("nenhuma".to_string(), "mjsv org create <nome>")
        } else {
            (sanitize(&e.available.join(", ")), "mjsv org use <id>")
        };
        vec![
            self.title_line(
                Glyph::Failed,
                &format!("Organização {} não pertence a esta conta", sanitize(&e.org)),
            ),
            format!("{}{} {avail}", self.indent(), self.dim("disponíveis:")),
            self.hint_default("tente", cmd),
        ]
    }

    fn prompt_card(&self, e: &PromptError) -> Vec<String> {
        match e {
            PromptError::NoTerminal { hint, .. } => {
                let mut l = vec![self.title_line(Glyph::Failed, "Esta pergunta exige um terminal")];
                match hint {
                    NoTerminalHint::Token => l.push(self.hint_default("tente", "mjsv login --token < chave.txt")),
                    // Nunca se imprime um `... --yes` pronto para colar.
                    NoTerminalHint::Yes => l.push(format!("{}Em script, repita com --yes.", self.indent())),
                }
                l
            }
            PromptError::NeedsYes { question } => vec![
                self.error_line(&sanitize(question)),
                format!("{}em saída não interativa, confirme com --yes", self.indent()),
            ],
            // Cancelar é um resultado válido: glifo de informação. O que foi digitado nunca é
            // repetido (uma senha colada não pode ir parar no scrollback).
            PromptError::Cancelled => vec![format!("{} Cancelado. Nada foi alterado.", self.glyph(Glyph::Info))],
            PromptError::Mismatch => vec![format!("{} Não confere. Nada foi alterado.", self.glyph(Glyph::Failed))],
        }
    }

    fn api_card(&self, e: &ApiError, rc: &ReportCtx) -> Vec<String> {
        let i = self.indent();
        match e {
            ApiError::Unauthorized => {
                if self.art() && !self.staff() && !rc.token_from_env && !rc.stored_token {
                    // ER-never: nada expirou, a pessoa ainda não entrou.
                    let mut l = vec![format!("{} {}", self.crown(), self.bold("Você ainda não entrou"))];
                    l.extend(self.rows(
                        &[("com senha", "mjsv login"), ("com chave", "mjsv login --token")],
                        self.crown_indent(),
                        false,
                    ));
                    l
                } else if rc.token_from_env {
                    vec![
                        self.title_line(Glyph::Failed, "Token de MJSV_TOKEN recusado"),
                        format!("{i}A API recusou a chave da variável de ambiente (401)."),
                        self.hint_default("tente", "unset MJSV_TOKEN && mjsv login"),
                    ]
                } else {
                    vec![
                        self.title_line(Glyph::Failed, "Sessão expirada ou token revogado"),
                        self.hint_default("tente", "mjsv login"),
                    ]
                }
            }
            ApiError::Forbidden { path, target } => {
                let kind = target.kind.as_str();
                let (cause, hint) = if self.staff() {
                    (
                        "Sem flag de sistema na conta, toda rota administrativa responde 403.",
                        "mjsvadm whoami",
                    )
                } else {
                    (
                        "Quase sempre é alvo errado: confira o escopo do comando e o --org.",
                        "mjsv org list",
                    )
                };
                let pair = format!(
                    "{i}{} {}   {} {}",
                    self.dim("alvo enviado   MJSV-TargetType:"),
                    self.bold(kind),
                    self.dim("MJSV-TargetId:"),
                    self.bold(&target.id)
                );
                let mut l = vec![self.title_line(Glyph::Failed, &format!("Acesso negado a {}", sanitize(path)))];
                // A linha do par nunca é cortada: se não cabe, os dois pares vão em duas linhas.
                if width(&pair) > self.caps.width {
                    l.push(format!(
                        "{i}{} {}",
                        self.dim("alvo enviado   MJSV-TargetType:"),
                        self.bold(kind)
                    ));
                    l.push(format!("{i}{} {}", self.dim("MJSV-TargetId:"), self.bold(&target.id)));
                } else {
                    l.push(pair);
                }
                // A explicação é prosa: quebra em `colunas - 1` com o recuo do bloco.
                l.extend(wrap_words(
                    &format!("{i}{cause}"),
                    self.caps.width.saturating_sub(1).max(20),
                    i,
                ));
                l.push(self.hint_default("tente", hint));
                l
            }
            ApiError::OtpSetupRequired => vec![
                self.title_line(Glyph::Attention, "Esta operação exige 2FA"),
                format!("{i}A conta ainda não tem autenticação em dois fatores."),
                self.hint_default("abra", "https://mjsv.io/dashboard/me"),
            ],
            ApiError::NotFound(msg) => {
                let mut l = vec![self.title_line(Glyph::Failed, &format!("Não encontrado: {}", sanitize(msg)))];
                if let Some(h) = self.list_hint(rc) {
                    l.push(self.hint_default("tente", &h));
                }
                l
            }
            ApiError::NoOrg => {
                let mut l = vec![
                    self.title_line(Glyph::Failed, "Nenhuma organização selecionada"),
                    self.hint_default("tente", "mjsv org list"),
                ];
                // A única continuação do sistema: o segundo comando alinhado ao primeiro.
                l.push(format!("{i}{}mjsv org use <id>", " ".repeat("tente: ".len())));
                l
            }
            ApiError::Api { status, message } => self.status_card(*status, message, rc),
            ApiError::Transport { url, source } => self.transport_card(url, source, rc),
            ApiError::Decode { path, message } => vec![
                self.title_line(Glyph::Failed, "Resposta inesperada da API"),
                format!("{i}{}: {}", sanitize(path), sanitize(message)),
                self.hint_default("tente", &format!("{} --version", self.tool.bin())),
            ],
        }
    }

    /// Para os testes de especificação: o cartão de rede sem precisar construir um `reqwest::Error`.
    #[doc(hidden)]
    pub fn render_net_card_for_tests(&self, timed_out: bool, path: &str, cause: &str, rc: &ReportCtx) -> String {
        let host = super::msg::api_host(&rc.api_url);
        let (title, state) = if timed_out {
            ("A API não respondeu em 60 s", BridgeState::Silent)
        } else {
            ("Sem conexão com a API", BridgeState::Raised)
        };
        self.net_card(title, state, &host, path, cause, rc).join("\n")
    }

    fn list_noun_known(&self, noun: &str) -> bool {
        let known = if self.staff() {
            STAFF_LIST_NOUNS
        } else {
            CUSTOMER_LIST_NOUNS
        };
        known.contains(&noun)
    }

    fn list_hint(&self, rc: &ReportCtx) -> Option<String> {
        let noun = rc.noun.as_deref()?;
        self.list_noun_known(noun)
            .then(|| format!("{} {noun} list", self.tool.bin()))
    }

    fn status_card(&self, status: u16, message: &str, rc: &ReportCtx) -> Vec<String> {
        let i = self.indent();
        let message = sanitize(message);
        match status {
            400 if message.contains("; ") => {
                let items: Vec<&str> = message.split("; ").collect();
                let mut l = vec![self.title_line(Glyph::Failed, "A API recusou os dados (400)")];
                l.extend(items.iter().take(6).map(|it| format!("{i}{} {it}", self.bullet())));
                if items.len() > 6 {
                    l.push(format!(
                        "{i}{} {} mais {}",
                        self.bullet(),
                        self.ellipsis(),
                        items.len() - 6
                    ));
                }
                l
            }
            500..=599 => {
                let mut l = vec![self.title_line(Glyph::Failed, &format!("A API falhou ({status})"))];
                if let Some(b) = self.bridge(BridgeState::Failed, rc) {
                    l.push(b);
                }
                if message != "erro sem mensagem" {
                    l.push(format!("{i}{message}"));
                }
                l.push(format!("{i}A falha é do nosso lado. Repita o comando em instantes."));
                l
            }
            // `status: 0` não existe mais (virou `NoOrg`); qualquer outro 4xx é a linha de hoje.
            _ => vec![self.error_line(&format!("{status}: {message}"))],
        }
    }

    fn transport_card(&self, url: &str, source: &reqwest::Error, rc: &ReportCtx) -> Vec<String> {
        let parsed = reqwest::Url::parse(url).ok();
        let path = parsed.as_ref().map(|u| u.path().to_string()).unwrap_or_default();
        let host = parsed
            .as_ref()
            .map(|_| super::msg::api_host(url))
            .unwrap_or_else(|| url.to_string());

        let timed_out = source.is_timeout();
        let cause = if timed_out {
            "operation timed out".to_string()
        } else {
            transport_cause(source)
        };
        let (title, state) = if timed_out {
            ("A API não respondeu em 60 s", BridgeState::Silent)
        } else {
            ("Sem conexão com a API", BridgeState::Raised)
        };

        self.net_card(title, state, &host, &path, &cause, rc)
    }

    /// O cartão de rede já com texto: título, ponte, causa. Abaixo de 60 colunas (ou sem arte) a
    /// ponte sai e o host passa para a causa (`{host}{path}: {causa}`).
    fn net_card(
        &self,
        title: &str,
        state: BridgeState,
        host: &str,
        path: &str,
        cause: &str,
        rc: &ReportCtx,
    ) -> Vec<String> {
        let i = self.indent();
        let mut l = vec![self.title_line(Glyph::Failed, title)];
        match self.bridge(state, rc) {
            Some(b) => {
                l.push(b);
                l.push(format!("{i}{}: {}", sanitize(path), sanitize(cause)));
            }
            None => l.push(format!("{i}{}{}: {}", sanitize(host), sanitize(path), sanitize(cause))),
        }
        l
    }

    /// A ponte de uma linha: a ferramenta, uma ponte, o host da API e ONDE quebrou.
    fn bridge(&self, state: BridgeState, rc: &ReportCtx) -> Option<String> {
        if !self.art() || self.narrow() {
            return None;
        }
        let u = self.unicode();
        let ch = match (self.staff(), u) {
            (true, true) => "═",
            (true, false) => "=",
            (false, true) => "─",
            (false, false) => "-",
        };
        let dot = if u { "·" } else { "." };
        let x = self.glyph(Glyph::Failed);
        let mid = match state {
            BridgeState::Raised => format!(
                "{} {x} {}",
                self.dim(&format!("{} {dot}", ch.repeat(4))),
                self.dim(&format!("{dot} {}", ch.repeat(4)))
            ),
            BridgeState::Silent => self.dim(&format!(
                "{}{}",
                ch.repeat(if u { 7 } else { 9 }),
                format!(" {dot}").repeat(4)
            )),
            BridgeState::Failed => format!("{} {x}", self.dim(&ch.repeat(13))),
        };
        let mut host = super::msg::api_host(&rc.api_url);
        if rc.api_url_overridden {
            host.push_str(" (MJSV_API_URL)");
        }
        Some(format!("{}{} {mid} {host}", self.indent(), self.tool.bin()))
    }
}

#[derive(Clone, Copy)]
enum BridgeState {
    Raised,
    Silent,
    Failed,
}

/// "organização k3PqD não encontrada" / "produto 7 não encontrado": 404 decidido no cliente.
/// Devolve o título capitalizado e o substantivo do `list`.
fn local_not_found(msg: &str) -> Option<(String, &'static str)> {
    let (noun, ends) = if msg.starts_with("organização ") {
        ("org", " não encontrada")
    } else if msg.starts_with("produto ") {
        ("product", " não encontrado")
    } else {
        return None;
    };
    (msg.ends_with(ends) && !msg.contains('\n')).then(|| (capitalize(msg), noun))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A causa de um erro de transporte, sem o invólucro `error sending request` do reqwest.
fn transport_cause(e: &reqwest::Error) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = e.source();
    while let Some(c) = cur {
        let s = c.to_string();
        // `client error (Connect)` é o rótulo do hyper, sem informação.
        if !s.starts_with("client error") {
            parts.push(s);
        }
        cur = c.source();
    }
    if parts.is_empty() {
        let s = e.to_string();
        // Tira a URL, que a ponte e o caminho já mostram.
        return match s.find(" for url") {
            Some(p) => s[..p].to_string(),
            None => s,
        };
    }
    // O sistema operacional repete a causa em prosa ("Name or service not known"): a primeira
    // metade já diz tudo.
    parts.first().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::{Style, Tool};
    use super::*;
    use crate::target::Target;

    fn rc() -> ReportCtx {
        ReportCtx {
            api_url: "https://courtyard-api.mjsv.io".into(),
            stored_token: true,
            ..Default::default()
        }
    }

    fn render(s: &Style, e: impl Into<anyhow::Error>, rc: &ReportCtx) -> String {
        super::super::strip_ansi(&s.render_error(&e.into(), rc))
    }

    #[test]
    fn saida_de_maquina_em_terminal_saneia_a_mensagem_da_api() {
        use super::super::caps::{Caps, MapEnv};
        let m = Style::new(
            Caps::detect(&MapEnv::new(&[]), true, Some(100), true, false),
            Tool::Mjsv,
        );
        let e = anyhow::Error::from(ApiError::Unauthorized).context("a\x1b]0;x\x07b\u{202e}c\r\nFAKE");
        let out = m.render_error(&e, &rc());
        assert!(!out.contains(['\x1b', '\x07', '\u{202e}', '\r', '\n']), "{out:?}");
        // Sem terminal: o texto de hoje, intacto.
        let p = Style::plain(Tool::Mjsv);
        assert_eq!(p.render_error(&anyhow::anyhow!("a\nb"), &rc()), "erro: a\nb");
    }

    #[test]
    fn fora_do_terminal_e_o_texto_de_hoje() {
        let p = Style::plain(Tool::Mjsv);
        assert_eq!(
            render(&p, ApiError::Unauthorized, &rc()),
            "erro: sessão expirada ou token revogado — rode `mjsv login`"
        );
        // Correção: sem o `0:` vazado.
        assert_eq!(
            render(&p, ApiError::NoOrg, &rc()),
            "erro: nenhuma organização selecionada — rode `mjsv org list` e depois `mjsv org use <id>`"
        );
        assert_eq!(
            render(
                &p,
                LoginRefused {
                    stage: crate::error::LoginStage::Password
                },
                &rc()
            ),
            "erro: login recusado: usuário, senha ou código 2FA não conferem"
        );
        assert_eq!(
            render(&p, anyhow::anyhow!("nada a alterar"), &rc()),
            "erro: nada a alterar"
        );
        assert_eq!(render(&p, PromptError::Cancelled, &rc()), "erro: cancelado");
    }

    #[test]
    fn corpo_de_forma_errada_vira_o_cartao_de_decode_so_no_terminal() {
        let e = serde_json::from_value::<Vec<u8>>(serde_json::json!("<html>")).unwrap_err();
        let texto = e.to_string();
        let card = render(&uni(Tool::Mjsv), e, &rc());
        assert!(card.starts_with("✗ Resposta inesperada da API"), "{card}");
        assert!(card.contains("tente: mjsv --version"), "{card}");
        let e = serde_json::from_value::<Vec<u8>>(serde_json::json!("<html>")).unwrap_err();
        assert_eq!(render(&Style::plain(Tool::Mjsv), e, &rc()), format!("erro: {texto}"));
    }

    #[test]
    fn sessao_expirada_em_unicode_e_ascii() {
        assert_eq!(
            render(&uni(Tool::Mjsv), ApiError::Unauthorized, &rc()),
            "✗ Sessão expirada ou token revogado\n  tente: mjsv login"
        );
        assert_eq!(
            render(&asc(Tool::Mjsv), ApiError::Unauthorized, &rc()),
            "[x] Sessão expirada ou token revogado\n    tente: mjsv login"
        );
    }

    #[test]
    fn nunca_entrou_tem_a_coroa_e_o_401_do_ambiente_o_proprio_cartao() {
        let never = ReportCtx {
            stored_token: false,
            ..rc()
        };
        assert_eq!(
            render(&uni(Tool::Mjsv), ApiError::Unauthorized, &never),
            "█▄█▄█ Você ainda não entrou\n      com senha:  mjsv login\n      com chave:  mjsv login --token"
        );
        // Em staff não há variante: é o 401 comum.
        assert!(render(&uni(Tool::Mjsvadm), ApiError::Unauthorized, &never).starts_with("✗ Sessão expirada"));

        let env = ReportCtx {
            token_from_env: true,
            ..rc()
        };
        assert_eq!(
            render(&uni(Tool::Mjsv), ApiError::Unauthorized, &env),
            "✗ Token de MJSV_TOKEN recusado\n  A API recusou a chave da variável de ambiente (401).\n  tente: unset MJSV_TOKEN && mjsv login"
        );
    }

    #[test]
    fn sem_organizacao_e_403() {
        assert_eq!(
            render(&uni(Tool::Mjsv), ApiError::NoOrg, &rc()),
            "✗ Nenhuma organização selecionada\n  tente: mjsv org list\n         mjsv org use <id>"
        );
        let e = ApiError::Forbidden {
            path: "/vm/k3TqZ/power".into(),
            target: Box::new(Target::org("nUME7")),
        };
        assert_eq!(
            render(&uni(Tool::Mjsv), e, &rc()),
            "✗ Acesso negado a /vm/k3TqZ/power\n  alvo enviado   MJSV-TargetType: ORG   MJSV-TargetId: nUME7\n  Quase sempre é alvo errado: confira o escopo do comando e o --org.\n  tente: mjsv org list"
        );
        let e = ApiError::Forbidden {
            path: "/vm/k3TqZ/power".into(),
            target: Box::new(Target::root()),
        };
        let out = render(&uni(Tool::Mjsvadm), e, &rc());
        assert!(out.contains("MJSV-TargetType: ROOT   MJSV-TargetId: 0000"), "{out}");
        assert!(out.ends_with("tente: mjsvadm whoami"), "{out}");
    }

    #[test]
    fn validacao_400_lista_ate_seis() {
        let e = ApiError::Api {
            status: 400,
            message: "E-mail inválido; Senha não pode estar vazia".into(),
        };
        assert_eq!(
            render(&uni(Tool::Mjsv), e, &rc()),
            "✗ A API recusou os dados (400)\n  · E-mail inválido\n  · Senha não pode estar vazia"
        );
        let many = (1..=8).map(|n| format!("e{n}")).collect::<Vec<_>>().join("; ");
        let out = render(
            &asc(Tool::Mjsv),
            ApiError::Api {
                status: 400,
                message: many,
            },
            &rc(),
        );
        assert!(out.ends_with("    - ... mais 2"), "{out}");
    }

    #[test]
    fn cinco_xx_tem_a_ponte_quebrada_no_fim() {
        let e = ApiError::Api {
            status: 502,
            message: "erro sem mensagem".into(),
        };
        assert_eq!(
            render(&uni(Tool::Mjsv), e, &rc()),
            "✗ A API falhou (502)\n  mjsv ───────────── ✗ courtyard-api.mjsv.io\n  A falha é do nosso lado. Repita o comando em instantes."
        );
        let e = ApiError::Api {
            status: 502,
            message: "erro sem mensagem".into(),
        };
        let out = render(&asc(Tool::Mjsvadm), e, &rc());
        assert!(out.contains("mjsvadm ============= [x] courtyard-api.mjsv.io"), "{out}");
    }

    #[test]
    fn cartao_proprio_e_verbose() {
        let e = CardError::new(
            "--org não se aplica a este comando (o alvo é sempre ROOT/0000)",
            "--org não se aplica a este comando",
        )
        .cause("No mjsvadm, o alvo é sempre ROOT/0000; use o filtro do comando.")
        .hint("tente", "mjsvadm vm list --help");
        let out = render(&uni(Tool::Mjsvadm), e.clone(), &rc());
        assert_eq!(
            out,
            "✗ --org não se aplica a este comando\n  No mjsvadm, o alvo é sempre ROOT/0000; use o filtro do comando.\n  tente: mjsvadm vm list --help"
        );
        let verbose = ReportCtx { verbose: 1, ..rc() };
        let out = render(&uni(Tool::Mjsvadm), e, &verbose);
        assert!(
            out.ends_with("\n  erro: --org não se aplica a este comando (o alvo é sempre ROOT/0000)"),
            "{out}"
        );
    }

    #[test]
    fn erro_com_contexto_vira_linha_unica_com_a_cadeia() {
        let e = anyhow::Error::new(ApiError::Unauthorized).context("falha ao listar");
        assert_eq!(
            render(&uni(Tool::Mjsv), e, &rc()),
            "✗ falha ao listar: sessão expirada ou token revogado — rode `mjsv login`"
        );
    }

    #[test]
    fn cancelamento_usa_o_glifo_de_informacao() {
        assert_eq!(
            render(&uni(Tool::Mjsv), PromptError::Cancelled, &rc()),
            "● Cancelado. Nada foi alterado."
        );
        assert_eq!(
            render(&asc(Tool::Mjsv), PromptError::Mismatch, &rc()),
            "[x] Não confere. Nada foi alterado."
        );
    }

    #[test]
    fn texto_da_api_e_saneado() {
        let e = ApiError::NotFound("/vm/\x1b[31mx".into());
        let out = uni(Tool::Mjsv).render_error(&e.into(), &rc());
        assert!(!out.contains("\x1b[31m"));
    }

    #[test]
    fn o_substantivo_da_dica_de_404() {
        assert_eq!(
            first_noun(&["mjsv".into(), "--org".into(), "x".into(), "vm".into(), "show".into()]).as_deref(),
            Some("vm")
        );
        let r = ReportCtx {
            noun: Some("vm".into()),
            ..rc()
        };
        assert_eq!(
            render(&uni(Tool::Mjsv), ApiError::NotFound("/vm/k3TqX".into()), &r),
            "✗ Não encontrado: /vm/k3TqX\n  tente: mjsv vm list"
        );
        let r = ReportCtx {
            noun: Some("whoami".into()),
            ..rc()
        };
        assert_eq!(
            render(&uni(Tool::Mjsv), ApiError::NotFound("/x".into()), &r),
            "✗ Não encontrado: /x"
        );
    }
}
