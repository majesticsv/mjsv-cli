//! Confirmações em quatro níveis (CF-0..CF-3 do cliente, CF-A0..CF-A3 do staff).
//!
//! Cada pergunta é descrita por um [`ConfirmSpec`] e desenhada por [`ConfirmSpec::render`]. O texto
//! "de hoje" (`sentence`) é sempre a base: fora de um terminal estilizado a pergunta é exatamente
//! `{sentence} [s/N] ` (nível 3: `{sentence} Digite {nome} para confirmar: `), porque a
//! segurança não pode depender de haver TTY.

use unicode_normalization::UnicodeNormalization;

use super::text::{is_clean, width, wrap_words};
use super::{Glyph, RuleSpec, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Leve: a linha de hoje.
    T0,
    /// Impacto no serviço, recuperável.
    T1,
    /// Destrutivo sobre um recurso.
    T2,
    /// Sem volta: digitar o nome.
    T3,
}

#[derive(Debug, Clone)]
pub struct ConfirmSpec {
    pub tier: Tier,
    /// A pergunta COMPLETA de hoje, byte a byte.
    pub sentence: String,
    /// Cabeça do bloco (verbo + objeto). Por padrão, a frase sem o `ask` final.
    pub head: Option<String>,
    /// A pergunta curta da última linha (`Continuar?`, `Pagar?`, `Ajustar?`).
    pub ask: String,
    pub rows: Vec<(String, String)>,
    /// Linha de consequência (`APAGA o disco e não tem volta.`).
    pub note: Option<String>,
    /// Tag da régua (`DESTRUTIVO`, `SEM VOLTA`, `ORG nUME7`).
    pub tag: Option<String>,
    /// Nível 3: o texto que o usuário digita.
    pub typed: Option<String>,
}

impl ConfirmSpec {
    pub fn new(tier: Tier, sentence: impl Into<String>) -> Self {
        let tag = match tier {
            Tier::T3 => Some("SEM VOLTA".to_string()),
            _ => None,
        };
        ConfirmSpec {
            tier,
            sentence: sentence.into(),
            head: None,
            ask: "Continuar?".into(),
            rows: Vec::new(),
            note: None,
            tag,
            typed: None,
        }
    }

    /// A pergunta de hoje, sem enfeite: o nível padrão de quem ainda não classificou o comando.
    pub fn simple(sentence: impl Into<String>) -> Self {
        Self::new(Tier::T0, sentence)
    }

    pub fn head(mut self, h: impl Into<String>) -> Self {
        self.head = Some(h.into());
        self
    }

    pub fn ask(mut self, a: impl Into<String>) -> Self {
        self.ask = a.into();
        self
    }

    pub fn row(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.rows.push((k.into(), v.into()));
        self
    }

    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.note = Some(n.into());
        self
    }

    pub fn tag(mut self, t: impl Into<String>) -> Self {
        self.tag = Some(t.into());
        self
    }

    /// Sem tag (régua de placa sem tag: dinheiro e identidade, onde nada é destruído).
    pub fn no_tag(mut self) -> Self {
        self.tag = None;
        self
    }

    /// `DESTRUTIVO` na régua (nível 2).
    pub fn destructive(self) -> Self {
        self.tag("DESTRUTIVO")
    }

    /// Nível 3: o nome é o do recurso como o servidor devolve; um nome vazio ou com controle, bidi
    /// ou largura zero é trocado pelo id (um nome que o usuário não consegue digitar não protege).
    pub fn typed(mut self, name: &str, id: &str) -> Self {
        self.typed = Some(if name.trim().is_empty() || !is_clean(name) {
            id.to_string()
        } else {
            name.to_string()
        });
        self
    }

    fn derived_head(&self) -> String {
        if let Some(h) = &self.head {
            return h.clone();
        }
        let suffix = format!(" {}", self.ask);
        self.sentence
            .strip_suffix(&suffix)
            .unwrap_or(&self.sentence)
            .to_string()
    }

    /// O texto que o usuário precisa digitar, se houver.
    pub fn expected(&self) -> Option<&str> {
        match self.tier {
            Tier::T3 => self.typed.as_deref(),
            _ => None,
        }
    }

    /// Confere a resposta. `Ok(())` confirma.
    pub fn check(&self, answer: &str) -> Result<(), crate::prompt::PromptError> {
        use crate::prompt::PromptError;
        match self.expected() {
            Some(want) => {
                let got = answer.trim();
                if got.is_empty() {
                    return Err(PromptError::Cancelled);
                }
                // NFC dos dois lados, sem dobrar caixa: "Acme" e "acme" são nomes diferentes.
                if got.nfc().eq(want.trim().nfc()) {
                    Ok(())
                } else {
                    Err(PromptError::Mismatch)
                }
            }
            None => {
                if matches!(answer.trim().to_lowercase().as_str(), "s" | "sim" | "y" | "yes") {
                    Ok(())
                } else {
                    Err(PromptError::Cancelled)
                }
            }
        }
    }

    /// Quebra uma linha de prosa em `colunas - 1` (ver [`wrap_words`]).
    fn fit(&self, s: &Style, line: String, hang: &str) -> Vec<String> {
        wrap_words(&line, s.caps.width.saturating_sub(1).max(20), hang)
    }

    /// O texto da pergunta, pronto para escrever (termina em espaço, sem quebra de linha final).
    pub fn render(&self, s: &Style) -> String {
        let yn = || s.bold("[s/N]");
        let typed_plain = |name: &str| format!("{} Digite {name} para confirmar: ", self.sentence);

        if !s.fancy() {
            return match self.expected() {
                Some(name) => typed_plain(name),
                None => format!("{} [s/N] ", self.sentence),
            };
        }

        let staff = s.staff();
        let ind = if staff { s.root_indent() } else { s.indent() };
        // Cabeça do bloco: `[#]` em staff, `▲` no cliente.
        let lead = if staff {
            s.glyph(Glyph::Root)
        } else {
            s.glyph(Glyph::Attention)
        };
        let ask_line = |ind: &str| format!("{ind}{} {}", self.ask, yn());
        // O primeiro valor (o recurso) é negrito a partir do nível 2.
        let rows: Vec<String> = s.rows(&self.rows, ind, matches!(self.tier, Tier::T2 | Tier::T3));

        let rule = |closing: bool| {
            let mut spec = RuleSpec::new();
            if staff {
                spec = spec.staff();
                if !closing {
                    spec = spec.plate();
                }
            } else {
                spec = spec.danger();
            }
            if !closing {
                if let Some(t) = &self.tag {
                    spec = spec.tag(t.clone());
                }
                if staff && self.tier == Tier::T3 {
                    spec = spec.crown();
                }
            }
            s.rule(&spec)
        };

        let mut lines: Vec<String> = Vec::new();
        match (self.tier, staff) {
            // CF-0: a linha de hoje, com `[s/N]` em negrito; quebra antes da pergunta curta se
            // passaria de 72 colunas.
            (Tier::T0, false) => {
                let full = format!("{} [s/N] ", self.sentence);
                let tail = format!(" {}", self.ask);
                if width(&full) > 72 && self.sentence.ends_with(&tail) {
                    let body = &self.sentence[..self.sentence.len() - tail.len()];
                    lines.extend(self.fit(s, body.to_string(), ind));
                    lines.push(format!("{} {}", self.ask, yn()));
                } else {
                    lines.extend(self.fit(s, format!("{} {}", self.sentence, yn()), ""));
                }
            }
            // CF-A0: `[#]` e a pergunta de hoje.
            (Tier::T0, true) => lines.extend(self.fit(s, format!("{lead} {} {}", self.sentence, yn()), ind)),
            // CF-1: aviso + frase quebrada na coluna de texto; com linhas, a cabeça em negrito.
            (Tier::T1, false) => {
                if self.rows.is_empty() {
                    lines.extend(self.fit(s, format!("{lead} {}", self.derived_head()), ind));
                } else {
                    lines.extend(self.fit(s, format!("{lead} {}", s.bold(&self.derived_head())), ind));
                    lines.extend(rows);
                }
                lines.push(ask_line(ind));
            }
            // CF-A1: `[#]` + a pergunta + a linha `org:`; sem linhas, a pergunta inteira numa linha.
            (Tier::T1, true) => {
                if self.rows.is_empty() {
                    lines.extend(self.fit(s, format!("{lead} {} {}", self.sentence, yn()), ind));
                } else {
                    lines.extend(self.fit(s, format!("{lead} {}", s.bold(&self.derived_head())), ind));
                    lines.extend(rows);
                    lines.push(ask_line(ind));
                }
            }
            // Níveis 2 e 3: régua + bloco; sem linhas (a consulta falhou) é o nível 1 sob a MESMA
            // régua com tag: a tag nunca some e apagar nunca trava numa consulta cosmética.
            (Tier::T2, _) | (Tier::T3, _) => {
                lines.push(rule(false));
                if self.rows.is_empty() {
                    lines.extend(self.fit(s, format!("{lead} {}", self.derived_head()), ind));
                } else {
                    lines.extend(self.fit(s, format!("{lead} {}", s.bold(&self.derived_head())), ind));
                    lines.extend(rows);
                }
                if let Some(n) = &self.note {
                    lines.push(format!("{ind}{n}"));
                }
                match self.expected() {
                    Some(name) => {
                        lines.push(rule(true));
                        lines.push(format!("{ind}Digite {} para confirmar:", s.bold(name)));
                    }
                    None => lines.push(ask_line(ind)),
                }
            }
        }

        let mut out = lines.join("\n");
        out.push(' ');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::{Charset, Depth, Style, Tool};
    use super::*;

    #[test]
    fn prosa_quebra_em_colunas_menos_um_com_recuo() {
        let st = Style::terminal(Tool::Mjsv, Charset::Unicode, Depth::None, 40);
        let sp = ConfirmSpec::new(
            Tier::T1,
            "Matar a VM k3TqZ agora? O processo é interrompido sem aviso. Continuar?",
        );
        let texto = sp.render(&st);
        for l in texto.lines() {
            assert!(width(l) <= 39, "{l}");
        }
        assert!(texto.lines().count() >= 3, "{texto}");
    }

    #[test]
    fn fora_do_terminal_e_a_frase_de_hoje() {
        let p = Style::plain(Tool::Mjsv);
        let sp = ConfirmSpec::new(
            Tier::T2,
            "Reinstalar a VM k3TqZ com a imagem ubuntu-24.04 APAGA o disco e não tem volta. Continuar?",
        )
        .row("VM", "web-01");
        assert_eq!(
            sp.render(&p),
            "Reinstalar a VM k3TqZ com a imagem ubuntu-24.04 APAGA o disco e não tem volta. Continuar? [s/N] "
        );
        let t3 = ConfirmSpec::new(
            Tier::T3,
            "Apagar a organização acme? Só o dono consegue e os serviços ativos ficam sem dono.",
        )
        .typed("acme", "nUME7");
        assert_eq!(
            t3.render(&p),
            "Apagar a organização acme? Só o dono consegue e os serviços ativos ficam sem dono. Digite acme para confirmar: "
        );
    }

    #[test]
    fn nivel_0_quebra_antes_da_pergunta_quando_passa_de_72() {
        let s = uni(Tool::Mjsv);
        let sp =
            ConfirmSpec::simple("Enviar map.zip para /map.zip sobrescreve o arquivo se ele já existir. Continuar?");
        assert_eq!(
            super::super::strip_ansi(&sp.render(&s)),
            "Enviar map.zip para /map.zip sobrescreve o arquivo se ele já existir.\nContinuar? [s/N] "
        );
        let curto = ConfirmSpec::simple("Fechar o ticket 4821?");
        assert_eq!(curto.render(&s), "Fechar o ticket 4821? [s/N] ");
    }

    #[test]
    fn nivel_3_digita_o_nome_com_nfc_e_sem_dobrar_caixa() {
        let sp = ConfirmSpec::new(Tier::T3, "x").typed("São Paulo", "gru");
        assert!(sp.check("São Paulo").is_ok());
        // "ã" decomposto (a + til combinante) vale o mesmo que o pré-composto.
        assert!(sp.check("Sa\u{303}o Paulo").is_ok());
        assert!(sp.check("são paulo").is_err());
        assert!(matches!(sp.check(""), Err(crate::prompt::PromptError::Cancelled)));
        assert!(matches!(sp.check("outro"), Err(crate::prompt::PromptError::Mismatch)));
    }

    #[test]
    fn nome_vazio_ou_com_controle_vira_o_id() {
        let sp = ConfirmSpec::new(Tier::T3, "x").typed("a\u{202e}b", "nUME7");
        assert_eq!(sp.expected(), Some("nUME7"));
        let sp = ConfirmSpec::new(Tier::T3, "x").typed("  ", "nUME7");
        assert_eq!(sp.expected(), Some("nUME7"));
    }

    #[test]
    fn respostas_aceitas_nos_niveis_0_a_2() {
        let sp = ConfirmSpec::simple("x?");
        for ok in ["s", "S", "sim", "y", "YES", " s "] {
            assert!(sp.check(ok).is_ok(), "{ok}");
        }
        for no in ["", "n", "nao", "talvez"] {
            assert!(sp.check(no).is_err(), "{no}");
        }
    }
}
