//! Mensagens: linhas com glifo, listas vazias, sucessos, login/logout e a caixa de segredo.

use super::art::*;
use super::banner::{BannerKind, InfoLine};
use super::{Glyph, Sty, Style};

/// Um segredo dentro da caixa. Cada um fica sozinho na linha, na coluna 0, sem barra, rótulo ou
/// recuo: o triplo clique seleciona exatamente o valor e um valor longo quebra sozinho.
#[derive(Debug, Clone)]
pub struct SecretItem {
    /// Legenda (`token:`), obrigatória quando há mais de um segredo.
    pub caption: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct SecretBox {
    /// `COPIE AGORA`, `USE AGORA`...
    pub title: String,
    /// `não será mostrado de novo`, `o token expira em 60 segundos`...
    pub note: String,
    pub items: Vec<SecretItem>,
    /// Linhas `rótulo  valor` dentro da barra (`chave`, `use com`).
    pub rows: Vec<(String, String)>,
}

impl SecretBox {
    pub fn new(title: impl Into<String>, note: impl Into<String>) -> Self {
        SecretBox {
            title: title.into(),
            note: note.into(),
            ..Default::default()
        }
    }

    pub fn secret(mut self, value: impl Into<String>) -> Self {
        self.items.push(SecretItem {
            caption: None,
            value: value.into(),
        });
        self
    }

    pub fn captioned(mut self, caption: impl Into<String>, value: impl Into<String>) -> Self {
        self.items.push(SecretItem {
            caption: Some(caption.into()),
            value: value.into(),
        });
        self
    }

    pub fn row(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.rows.push((k.into(), v.into()));
        self
    }
}

impl Style {
    // -- linhas com glifo ----------------------------------------------------------------------

    /// `glifo mensagem` num terminal estilizado; a mensagem pura (o texto de hoje) fora dele.
    pub fn msg(&self, g: Glyph, text: &str) -> String {
        if self.fancy() {
            format!("{} {text}", self.glyph(g))
        } else {
            text.to_string()
        }
    }

    /// Sucesso de uma frase (`✓`).
    pub fn done(&self, text: &str) -> String {
        self.msg(Glyph::Done, text)
    }

    /// A frase diz que nada aconteceu (`●`).
    pub fn noop(&self, text: &str) -> String {
        self.msg(Glyph::Info, text)
    }

    /// A frase termina com algo que o usuário precisa fazer (`▲`).
    pub fn attention(&self, text: &str) -> String {
        self.msg(Glyph::Attention, text)
    }

    /// Aceito, ainda não feito (`○`): a VM ainda não está ligada.
    pub fn accepted(&self, text: &str) -> String {
        self.msg(Glyph::Accepted, text)
    }

    /// A linha de erro: `✗ mensagem` ou, fora de um terminal estilizado, o `erro: mensagem` de hoje.
    pub fn error_line(&self, text: &str) -> String {
        if self.fancy() {
            format!("{} {text}", self.glyph(Glyph::Failed))
        } else {
            format!("erro: {text}")
        }
    }

    /// A linha de passo de ROOT (`[#] Apagando ... …`), impressa em stderr antes de uma mutação
    /// quando nenhum prompt apareceu (`--yes`). `None` fora de um terminal estilizado: o texto de
    /// hoje não tem essa linha. Nunca se combina com outro glifo.
    pub fn root_step(&self, text: &str) -> Option<String> {
        if !self.fancy() || !self.staff() {
            return None;
        }
        Some(format!("{} {text} {}", self.glyph(Glyph::Root), self.ellipsis()))
    }

    /// `rótulo: comando` de cartão de erro/vazio, com o recuo padrão.
    pub fn next_step(&self, label: &str, cmd: &str) -> String {
        self.hint_default(label, cmd)
    }

    // -- listas vazias -------------------------------------------------------------------------

    /// Lista vazia comum: a frase de hoje, sem glifo (EM-neutral). Filtro que não achou nada
    /// também cai aqui.
    pub fn empty(&self, sentence: &str) -> String {
        sentence.to_string()
    }

    /// Lista vazia com um próximo passo (EM-hint), recuado 2 células nos dois conjuntos.
    pub fn empty_hint(&self, sentence: &str, label: &str, cmd: &str) -> String {
        self.empty_hint_plain(sentence, sentence, label, cmd)
    }

    /// Como [`Style::empty_hint`], para frases cuja versão sem terminal já embute a dica
    /// (`token list`).
    pub fn empty_hint_plain(&self, plain: &str, sentence: &str, label: &str, cmd: &str) -> String {
        if self.fancy() {
            format!("{sentence}\n{}", self.hint(label, cmd, "  "))
        } else {
            plain.to_string()
        }
    }

    /// Boa notícia vazia (EM-clear): `✓ Nenhum incidente no período.`
    pub fn empty_clear(&self, sentence: &str) -> String {
        self.done(sentence)
    }

    /// "Não deu para saber" (EM-unknown): nunca se confunde com o tudo-certo.
    pub fn empty_unknown(&self, sentence: &str) -> String {
        self.attention(sentence)
    }

    /// Organização sem nenhum serviço (EM-courtyard-example): o pátio vazio, o título e três
    /// passos. Abaixo de 60 colunas ou sem arte: a frase mais a dica de EM-hint.
    pub fn empty_courtyard(&self) -> String {
        let sentence = "Nenhum serviço.";
        if !self.art() || self.narrow() {
            return self.empty_hint_plain(sentence, sentence, "comece pelo catálogo", "mjsv product list");
        }
        let yard = if self.unicode() {
            &COURTYARD_UNICODE
        } else {
            &COURTYARD_ASCII
        };
        let rows = self.rows(
            &[
                ("escolha", "mjsv product list"),
                ("peça", "mjsv order create --product <id>"),
                ("pague", "mjsv order pay <id>"),
            ],
            "",
            false,
        );
        let mut text = vec![self.bold("Nenhum serviço ainda")];
        text.extend(rows);
        let lines: Vec<String> = yard
            .iter()
            .zip(text)
            .map(|(y, t)| format!("{}    {t}", self.dim(y)))
            .collect();
        // A largura do pátio com o texto passa de 60 colunas: medir a arte em vez de confiar no
        // limiar de `narrow()`, porque uma linha mais larga que o terminal quebra e destrói o
        // desenho. Cabendo `colunas - 1`, ele aparece; senão, a frase mais a dica.
        let largura = lines.iter().map(|l| super::text::width(l)).max().unwrap_or(0);
        if largura > self.caps.width.saturating_sub(1) {
            return self.empty_hint_plain(sentence, sentence, "comece pelo catálogo", "mjsv product list");
        }
        lines.join("\n")
    }

    // -- cartões de sucesso ----------------------------------------------------------------------

    /// `order create` / `order quote` (OK-order-created). Fora de um terminal estilizado, as três
    /// linhas de hoje.
    pub fn order_created(&self, id: &str, status: &str, total: &str) -> String {
        if !self.fancy() {
            return format!("Pedido {id} criado ({status}).\nTotal:   {total}\nPague com: mjsv order pay {id}");
        }
        let mut lines = vec![self.title_line(Glyph::Done, &format!("Pedido {id} criado"))];
        lines.extend(self.rows(
            &[
                ("status", self.state(status)),
                ("total", total.to_string()),
                ("próximo", format!("mjsv order pay {id}")),
            ],
            self.indent(),
            false,
        ));
        lines.join("\n")
    }

    /// `order pay` pelo gateway (OK-checkout). A URL é a carga: sozinha na linha, na coluna 0,
    /// nunca truncada nem quebrada pelo CLI.
    pub fn checkout(&self, id: &str, url: &str) -> String {
        if !self.fancy() {
            return format!("Pague em: {url}");
        }
        [
            self.title_line(Glyph::Accepted, &format!("Pedido {id} aguardando pagamento")),
            format!("{}{}", self.indent(), self.dim("pague em:")),
            self.bold(url),
            self.hint_default("depois", &format!("mjsv order show {id}")),
        ]
        .join("\n")
    }

    /// Caixa de segredo de lado aberto (OK-secret): régua pesada amarela, barra, valor sozinho na
    /// linha. `None` sem arte; o chamador imprime as linhas de hoje.
    ///
    /// Escrita num único `write_all` com stdout travado por quem chama: o segredo nunca fica
    /// entremeado a outra saída.
    pub fn secret_box(&self, b: &SecretBox) -> Option<String> {
        if !self.art() {
            return None;
        }
        let n = self.rule_width();
        let heavy = |top: bool| -> String {
            if self.unicode() {
                self.paint(Sty::Sec, &(if top { "▄" } else { "▀" }).repeat(n))
            } else {
                self.paint(
                    Sty::Sec,
                    &format!("{}{}", if top { "." } else { "'" }, "-".repeat(n - 1)),
                )
            }
        };
        let bar = self.paint(Sty::Sec, if self.unicode() { "▌" } else { "|" });

        let mut lines = vec![
            heavy(true),
            format!(
                "{bar} {} {}{}{}",
                self.glyph(Glyph::Attention),
                self.bold(&b.title),
                self.sep(),
                b.note
            ),
        ];
        let mut last_bar = false;
        for it in &b.items {
            match &it.caption {
                Some(c) => {
                    lines.push(format!("{bar} {}", self.dim(c)));
                    lines.push(self.bold(&it.value));
                    last_bar = false;
                }
                None => {
                    lines.push(bar.clone());
                    lines.push(self.bold(&it.value));
                    lines.push(bar.clone());
                    last_bar = true;
                }
            }
        }
        if !b.rows.is_empty() {
            if !last_bar {
                lines.push(bar.clone());
            }
            lines.extend(self.rows(&b.rows, &format!("{bar} "), false));
        }
        lines.push(heavy(false));
        Some(lines.join("\n"))
    }

    /// Comandos que só imprimem o JSON da API: uma linha de glifo em STDERR
    /// (para o JSON do stdout continuar encanável). Proposta da especificação, bloqueada por texto
    /// novo por comando; a forma degradada é `✓ {comando}: concluído`.
    pub fn staff_done_line(&self, text: &str) -> Option<String> {
        self.fancy()
            .then(|| format!("{} {}", self.glyph(Glyph::Done), super::text::sanitize(text)))
    }

    /// `mjsvadm whoami` sem a flag de sistema (ER-flag): aviso depois dos campos, exit 0. Fora de um
    /// terminal estilizado, a frase de hoje.
    pub fn no_system_flag(&self) -> String {
        if !self.fancy() {
            return "Esta conta não tem flag de sistema — as rotas administrativas vão responder 403.".into();
        }
        format!(
            "{}\n{}As rotas administrativas vão responder 403.",
            self.title_line(Glyph::Attention, "Esta conta não tem flag de sistema"),
            self.indent()
        )
    }

    // -- login e logout --------------------------------------------------------------------------

    /// O resultado do login. Ver [`LoginView`].
    pub fn login_result(&self, v: &LoginView) -> String {
        let who = format!("Autenticado como {} ({})", v.username, v.user_id);
        let (org_plain, org_kv, next): (String, Option<(String, String)>, Option<String>) = match &v.orgs {
            OrgSummary::None => (
                "Nenhuma organização vinculada a esta conta.".into(),
                Some(("org".into(), "nenhuma".into())),
                Some("mjsv org create <nome>".into()),
            ),
            OrgSummary::One { name, id } => (
                format!("Organização: {name} ({id})"),
                Some(("org".into(), format!("{name} ({id})"))),
                None,
            ),
            OrgSummary::Many(n) => (
                format!("{n} organizações — escolha com `mjsv org use <id>` (veja `mjsv org list`)."),
                Some(("org".into(), format!("{n} disponíveis"))),
                Some("mjsv org use <id>".into()),
            ),
        };

        if !self.fancy() {
            return format!("{who}\n{org_plain}");
        }
        if !self.art() {
            // Glifo e as linhas de hoje: só a arte some com `MJSV_NO_BANNER`.
            return format!("{} {who}\n{org_plain}", self.glyph(Glyph::Done));
        }

        let (k, val) = org_kv.unwrap();
        if v.first && !self.narrow() {
            let mut infos = vec![InfoLine::done(who)];
            if matches!(v.orgs, OrgSummary::One { .. }) {
                infos.push(InfoLine::text(v.host.clone()));
            }
            infos.push(InfoLine::kv(k, val));
            if let Some(n) = next {
                infos.push(InfoLine::kv("próximo", n));
            }
            return self
                .banner(BannerKind::Hero, &format!("mjsv {}", v.version), &infos)
                .unwrap_or_default();
        }

        let mut details = vec![self.kv(&k, &val)];
        if let Some(n) = next {
            details.push(self.kv("próximo", &n));
        }
        self.compact_block(&format!("{} {}", self.glyph(Glyph::Done), self.bold(&who)), &details)
    }

    /// `mjsv logout` (OK-logout): uma linha coroada.
    pub fn logout_result(&self) -> String {
        if !self.fancy() {
            return "Sessão encerrada.".into();
        }
        if !self.art() {
            return self.done("Sessão encerrada.");
        }
        format!(
            "{} {} {}",
            self.crown(),
            self.glyph(Glyph::Done),
            self.bold("Sessão encerrada")
        )
    }

    /// `mjsv --version`: a coroa e `mjsv 0.3.0`; em `mjsvadm`, a marca `══ STAFF · ROOT ══` ao lado.
    /// `None` fora de um terminal com arte (o clap imprime a versão de hoje).
    pub fn version_line(&self, version: &str) -> Option<String> {
        if !self.art() {
            return None;
        }
        let name = format!("{} {version}", self.tool.bin());
        let mut line = format!("{} {}", self.crown(), self.bold(&name));
        if self.staff() {
            let (ch, plate) = if self.unicode() {
                ("══", PLATE_UNICODE)
            } else {
                ("==", PLATE_ASCII)
            };
            line.push_str(&format!(
                "  {}{}{}",
                self.paint(Sty::SRule, ch),
                self.paint(Sty::Plate, plate),
                self.paint(Sty::SRule, ch)
            ));
        }
        Some(line)
    }

    /// O banner da tela inicial do binário, com as linhas de informação dadas.
    pub fn home(&self, version: &str, infos: &[InfoLine]) -> Option<String> {
        let kind = if self.staff() {
            BannerKind::Staff
        } else {
            BannerKind::Hero
        };
        self.banner(kind, &format!("{} {version}", self.tool.bin()), infos)
    }
}

/// Resumo de organizações do login.
#[derive(Debug, Clone)]
pub enum OrgSummary {
    None,
    One { name: String, id: String },
    Many(usize),
}

/// O que o login precisa para se desenhar.
#[derive(Debug, Clone)]
pub struct LoginView {
    pub username: String,
    pub user_id: String,
    pub orgs: OrgSummary,
    /// Não existia arquivo de credenciais antes: o banner completo.
    pub first: bool,
    pub host: String,
    pub version: String,
}

/// O host da API para mostrar: `host` ou `host:porta` quando a porta não é a do esquema.
pub fn api_host(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("").to_string();
            match u.port() {
                Some(p) => format!("{host}:{p}"),
                None => host,
            }
        }
        Err(_) => url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::{Charset, Depth, Style, Tool};
    use super::*;

    #[test]
    fn patio_so_aparece_se_couber_no_terminal() {
        let largo = Style::terminal(Tool::Mjsv, Charset::Unicode, Depth::None, 100);
        assert!(largo.empty_courtyard().lines().count() > 2);
        // 60 a 65 colunas: o pátio com o texto tem 66 células e quebraria o desenho.
        let medio = Style::terminal(Tool::Mjsv, Charset::Unicode, Depth::None, 62);
        assert!(medio.empty_courtyard().starts_with("Nenhum serviço."));
        for l in largo.empty_courtyard().lines() {
            assert!(super::super::text::width(l) <= 99);
        }
    }

    #[test]
    fn glifos_so_em_terminal_estilizado() {
        assert_eq!(uni(Tool::Mjsv).done("feito."), "✓ feito.");
        assert_eq!(asc(Tool::Mjsv).accepted("x"), "[~] x");
        assert_eq!(Style::plain(Tool::Mjsv).done("feito."), "feito.");
        assert_eq!(Style::plain(Tool::Mjsv).error_line("boom"), "erro: boom");
        assert_eq!(uni(Tool::Mjsv).error_line("boom"), "✗ boom");
    }

    #[test]
    fn passo_de_root_so_em_staff_e_so_com_terminal() {
        assert_eq!(
            uni(Tool::Mjsvadm).root_step("Apagando o usuário x").unwrap(),
            "[#] Apagando o usuário x …"
        );
        assert_eq!(asc(Tool::Mjsvadm).root_step("x").unwrap(), "[#] x ...");
        assert!(uni(Tool::Mjsv).root_step("x").is_none());
        assert!(Style::plain(Tool::Mjsvadm).root_step("x").is_none());
    }

    #[test]
    fn vazio_com_dica() {
        assert_eq!(
            uni(Tool::Mjsv).empty_hint(
                "Nenhuma VM nesta organização.",
                "comece pelo catálogo",
                "mjsv product list"
            ),
            "Nenhuma VM nesta organização.\n  comece pelo catálogo: mjsv product list"
        );
        let p = Style::plain(Tool::Mjsv);
        assert_eq!(
            p.empty_hint("Nenhuma VM nesta organização.", "x", "y"),
            "Nenhuma VM nesta organização."
        );
        assert_eq!(
            p.empty_hint_plain(
                "Nenhuma chave de API. Crie uma com `mjsv token create <nome>`.",
                "Nenhuma chave de API.",
                "crie uma",
                "mjsv token create <nome>"
            ),
            "Nenhuma chave de API. Crie uma com `mjsv token create <nome>`."
        );
    }

    #[test]
    fn host_da_api() {
        assert_eq!(api_host("https://courtyard-api.mjsv.io"), "courtyard-api.mjsv.io");
        assert_eq!(api_host("http://localhost:5000/"), "localhost:5000");
    }
}
