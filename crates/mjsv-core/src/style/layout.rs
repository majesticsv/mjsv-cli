//! Peças de layout: réguas, linhas `rótulo: valor`, dicas e marcadores de lista.
//!
//! Anatomia única (FINAL-outputs.md, Convenções 2 e 3): glifo na coluna 0, corpo na coluna de
//! texto, rótulos esmaecidos com os dois pontos, nenhuma linha em branco dentro do bloco.

use super::text::width;
use super::{Glyph, Sty, Style};

/// Descrição de uma régua. `Default` é a régua nua, só traço.
#[derive(Debug, Clone, Default)]
pub struct RuleSpec {
    pub title: Option<String>,
    /// `═`/`=` em vermelho de staff (a ferramenta administrativa).
    pub staff: bool,
    /// Régua de perigo do cliente (SGR 31, tag em negrito vermelho).
    pub danger: bool,
    /// Placa ` STAFF · ROOT ` (só com `staff`).
    pub plate: bool,
    /// Tag depois da placa (`DESTRUTIVO`, `SEM VOLTA`, `ORG nUME7`).
    pub tag: Option<String>,
    /// Coroa vermelha de staff fechando a régua (selo do CF-A3).
    pub crown: bool,
    /// Largura; por padrão `min(colunas - 1, 60)`.
    pub width: Option<usize>,
}

impl RuleSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = Some(t.into());
        self
    }

    pub fn staff(mut self) -> Self {
        self.staff = true;
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn plate(mut self) -> Self {
        self.plate = true;
        self
    }

    pub fn tag(mut self, t: impl Into<String>) -> Self {
        self.tag = Some(t.into());
        self
    }

    pub fn crown(mut self) -> Self {
        self.crown = true;
        self
    }

    pub fn width(mut self, w: usize) -> Self {
        self.width = Some(w);
        self
    }
}

impl Style {
    /// A régua. O peso diz a ferramenta sem cor: `─`/`-` para `mjsv`, `═`/`=` para `mjsvadm`.
    /// Régua só leva texto (título ou tag), nunca glifo; a única marca é a coroa do CF-A3.
    pub fn rule(&self, spec: &RuleSpec) -> String {
        let u = self.unicode();
        let ch = match (spec.staff, u) {
            (true, true) => "═",
            (true, false) => "=",
            (false, true) => "─",
            (false, false) => "-",
        };
        let rs = if spec.staff {
            Sty::SRule
        } else if spec.danger {
            Sty::DRule
        } else {
            Sty::Dim
        };
        let n = spec.width.unwrap_or_else(|| self.rule_width());

        if !(spec.plate || spec.tag.is_some() || spec.title.is_some()) {
            return self.paint(rs, &ch.repeat(n));
        }

        // Segmentos (estilo, texto): a largura é somada antes de pintar, para o preenchimento
        // caber exatamente em `n`.
        let mut segs: Vec<(Option<Sty>, String)> = vec![(Some(rs), ch.repeat(2))];
        if spec.plate {
            let plate = if u {
                super::art::PLATE_UNICODE
            } else {
                super::art::PLATE_ASCII
            };
            segs.push((Some(Sty::Plate), plate.to_string()));
            segs.push((Some(rs), ch.repeat(2)));
        }
        if let Some(tag) = &spec.tag {
            segs.push((None, " ".into()));
            segs.push((Some(if spec.staff { Sty::STag } else { Sty::DTag }), tag.clone()));
            segs.push((None, " ".into()));
        }
        if let Some(title) = &spec.title {
            segs.push((None, " ".into()));
            segs.push((Some(Sty::Bold), title.clone()));
            segs.push((None, " ".into()));
        }
        let used: usize = segs.iter().map(|(_, t)| width(t)).sum();

        // Abaixo de 60 colunas a coroa do selo sai antes de qualquer outra coisa: a tag nunca.
        let crown_w = width(if u {
            super::art::COMPACT_UNICODE
        } else {
            super::art::COMPACT_ASCII
        }) + 1;
        let with_crown = spec.crown && !self.narrow() && used + crown_w < n;
        let tail = if with_crown { crown_w } else { 0 };
        segs.push((Some(rs), ch.repeat(n.saturating_sub(used + tail))));

        let mut out: String = segs
            .iter()
            .map(|(sty, t)| match sty {
                Some(sty) => self.paint(*sty, t),
                None => t.clone(),
            })
            .collect();
        if with_crown {
            out.push(' ');
            out.push_str(&self.staff_crown());
        }
        out
    }

    /// Régua de título de uma visão de dados (`── VM web-01 ───`); `None` fora de um terminal
    /// estilizado, onde o bloco começa direto nas linhas de hoje.
    pub fn title_rule(&self, title: &str) -> Option<String> {
        if !self.fancy() {
            return None;
        }
        let mut spec = RuleSpec::new().title(title);
        spec.staff = self.staff();
        Some(self.rule(&spec))
    }

    /// Linhas `rótulo:  valor` de um bloco de mensagem, no recuo `indent`. O rótulo vai esmaecido
    /// com os dois pontos e o valor começa em `maior rótulo + 3`. `bold_first` destaca o primeiro
    /// valor (o recurso de um prompt).
    pub fn rows<K: AsRef<str>, V: AsRef<str>>(&self, pairs: &[(K, V)], indent: &str, bold_first: bool) -> Vec<String> {
        let w = pairs.iter().map(|(k, _)| width(k.as_ref())).max().unwrap_or(0) + 3;
        pairs
            .iter()
            .enumerate()
            .map(|(j, (k, v))| {
                let k = k.as_ref();
                let v = v.as_ref();
                let value = if bold_first && j == 0 {
                    self.bold(v)
                } else {
                    v.to_string()
                };
                format!(
                    "{indent}{}{}{value}",
                    self.dim(&format!("{k}:")),
                    " ".repeat(w - width(k) - 1)
                )
            })
            .collect()
    }

    /// Linhas de um cartão de dados: `Rótulo:   valor` na coluna 0, com os rótulos capitalizados
    /// de hoje (a exceção declarada à regra do recuo, para preservar a saída existente).
    pub fn card_rows<K: AsRef<str>, V: AsRef<str>>(&self, pairs: &[(K, V)]) -> Vec<String> {
        self.rows(pairs, "", false)
    }

    /// Texto vindo da API, limpo de controles só quando a saída é estilizada: fora dela o byte a
    /// byte de hoje vale mais que a higiene (e o destino não é um terminal que interprete escapes).
    pub fn safe(&self, s: &str) -> String {
        if self.fancy() {
            super::text::sanitize(s)
        } else {
            s.to_string()
        }
    }

    /// Cartão de dados completo: régua de título e `Rótulo:   valor` num terminal estilizado; fora
    /// dele, `Rótulo:` preenchido até `plain_width` colunas, o recuo que cada comando já tinha (por
    /// isso a largura é parâmetro: a saída encanada não pode mudar um espaço). Rótulo vazio é
    /// continuação do valor anterior, recuada na coluna do valor.
    pub fn data_card(&self, title: &str, pairs: &[(&str, String)], plain_width: usize) -> Vec<String> {
        let Some(rule) = self.title_rule(&super::text::sanitize(title)) else {
            return pairs
                .iter()
                .map(|(k, v)| {
                    if k.is_empty() {
                        format!("{}{v}", " ".repeat(plain_width))
                    } else {
                        format!("{}{v}", super::text::pad(&format!("{k}:"), plain_width))
                    }
                })
                .collect();
        };
        let labelled: Vec<(&str, &str)> = pairs
            .iter()
            .filter(|(k, _)| !k.is_empty())
            .map(|(k, v)| (*k, v.as_str()))
            .collect();
        let col = labelled.iter().map(|(k, _)| width(k)).max().unwrap_or(0) + 3;
        let mut ready = self.card_rows(&labelled).into_iter();
        let mut out = vec![rule];
        out.extend(pairs.iter().map(|(k, v)| {
            if k.is_empty() {
                format!("{}{v}", " ".repeat(col))
            } else {
                ready.next().unwrap_or_default()
            }
        }));
        out
    }

    /// Escreve a caixa de segredo num único `write_all` com stdout travado: o segredo nunca fica
    /// entremeado a outra saída. `false` quando não há arte (o chamador imprime as linhas de hoje).
    pub fn print_secret_box(&self, b: &super::msg::SecretBox) -> bool {
        use std::io::Write;
        let Some(text) = self.secret_box(b) else {
            return false;
        };
        let mut o = std::io::stdout().lock();
        let _ = o.write_all(format!("{text}\n").as_bytes());
        let _ = o.flush();
        true
    }

    /// Uma dica: `rótulo: comando`. O comando é o texto padrão, nunca em negrito, sem crase, e é a
    /// última coisa da linha. `tente:` e `abra:` são reservados a erros.
    pub fn hint(&self, label: &str, cmd: &str, indent: &str) -> String {
        format!("{indent}{} {cmd}", self.dim(&format!("{label}:")))
    }

    /// Dica no recuo padrão do bloco.
    pub fn hint_default(&self, label: &str, cmd: &str) -> String {
        self.hint(label, cmd, self.indent())
    }

    /// `rótulo: valor` de linha de banner (rótulo esmaecido, valor no padrão).
    pub fn kv(&self, label: &str, value: &str) -> String {
        format!("{}{value}", self.dim(&format!("{label}: ")))
    }

    /// Marcador de lista (`·` / `-`), esmaecido. Nunca é marcador de estado: esses vivem dentro de
    /// uma célula de valor.
    pub fn bullet(&self) -> String {
        self.dim(if self.unicode() { "·" } else { "-" })
    }

    /// Um cartão de mensagem completo na anatomia única: `glifo título`, linhas de causa no recuo,
    /// linhas `rótulo  valor` e uma dica. Fora de um terminal estilizado devolve `plain`, o texto de
    /// hoje (o cartão nunca inventa texto que um script possa passar a ler).
    pub fn card<K: AsRef<str>, V: AsRef<str>>(
        &self,
        plain: &str,
        glyph: Glyph,
        title: &str,
        causes: &[&str],
        rows: &[(K, V)],
        hint: Option<(&str, &str)>,
    ) -> String {
        if !self.fancy() {
            return plain.to_string();
        }
        let mut lines = vec![self.title_line(glyph, title)];
        lines.extend(causes.iter().map(|c| format!("{}{c}", self.indent())));
        lines.extend(self.rows(rows, self.indent(), false));
        if let Some((label, cmd)) = hint {
            lines.push(self.hint_default(label, cmd));
        }
        lines.join("\n")
    }

    /// `glifo título` de abertura de bloco, o título em negrito.
    pub fn title_line(&self, g: Glyph, title: &str) -> String {
        format!("{} {}", self.glyph(g), self.bold(title))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::Tool;
    use super::*;

    #[test]
    fn cartao_de_dados_estiliza_no_terminal_e_preserva_o_recuo_fora_dele() {
        let pares = [
            ("Nome", "db-01".to_string()),
            ("", "obs".to_string()),
            ("SO", "Debian".to_string()),
        ];
        let linhas = uni(Tool::Mjsv).data_card("VM db-01", &pares, 12);
        assert!(linhas[0].starts_with("── VM db-01 "), "{linhas:?}");
        assert!(
            linhas[2].ends_with("obs") && linhas[2].starts_with("      "),
            "{linhas:?}"
        );
        let plano = Style::plain(Tool::Mjsv).data_card("VM db-01", &pares, 12);
        assert_eq!(plano, ["Nome:       db-01", "            obs", "SO:         Debian"]);
        assert_eq!(Style::plain(Tool::Mjsv).safe("a\u{1b}[31mb"), "a\u{1b}[31mb");
    }

    #[test]
    fn regua_de_titulo_do_cliente_e_de_staff() {
        let r = uni(Tool::Mjsv).rule(&RuleSpec::new().title("VM web-01"));
        assert_eq!(r, format!("── VM web-01 {}", "─".repeat(60 - 13)));
        let r = asc(Tool::Mjsvadm).rule(&RuleSpec::new().staff().title("Infra"));
        assert_eq!(r, format!("== Infra {}", "=".repeat(60 - 9)));
        assert_eq!(width(&r), 60);
    }

    #[test]
    fn regua_com_placa_tag_e_coroa() {
        let s = uni(Tool::Mjsvadm);
        let r = s.rule(&RuleSpec::new().staff().plate().tag("SEM VOLTA").crown());
        assert_eq!(width(&r), 60);
        assert!(r.starts_with("══ STAFF · ROOT ══ SEM VOLTA ═"), "{r}");
        assert!(r.ends_with(" █▄█▄█"), "{r}");

        let a = asc(Tool::Mjsvadm).rule(&RuleSpec::new().staff().plate().tag("SEM VOLTA").crown());
        assert!(a.starts_with("== STAFF / ROOT == SEM VOLTA ="), "{a}");
        assert!(a.ends_with(r" \_/^\_/"), "{a}");
        assert_eq!(width(&a), 60);
    }

    #[test]
    fn abaixo_de_60_colunas_a_coroa_sai_e_a_tag_fica() {
        let s = Style::terminal(
            Tool::Mjsvadm,
            super::super::Charset::Unicode,
            super::super::Depth::None,
            50,
        );
        let r = s.rule(&RuleSpec::new().staff().plate().tag("SEM VOLTA").crown());
        assert!(!r.contains('█'), "{r}");
        assert!(r.contains("SEM VOLTA"));
        assert_eq!(width(&r), 49);
    }

    #[test]
    fn cartao_generico_e_texto_de_hoje_fora_do_terminal() {
        let none: [(&str, &str); 0] = [];
        let c = uni(Tool::Mjsv).card(
            "x: y",
            Glyph::Done,
            "Feito",
            &["causa."],
            &none,
            Some(("próximo", "mjsv vm list")),
        );
        assert_eq!(c, "✓ Feito\n  causa.\n  próximo: mjsv vm list");
        assert_eq!(
            Style::plain(Tool::Mjsv).card("x: y", Glyph::Done, "Feito", &[], &none, None),
            "x: y"
        );
    }

    #[test]
    fn linhas_alinham_o_valor_em_maior_rotulo_mais_3() {
        let s = uni(Tool::Mjsv);
        let rows = s.rows(&[("VM", "web-01"), ("imagem", "x")], "  ", false);
        assert_eq!(rows, vec!["  VM:      web-01", "  imagem:  x"]);
        let rows = s.card_rows(&[("Nome", "web-01"), ("Recursos", "2 vCPU")]);
        assert_eq!(rows, vec!["Nome:      web-01", "Recursos:  2 vCPU"]);
    }
}
