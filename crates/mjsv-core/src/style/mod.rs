//! O sistema visual do terminal: banners, glifos, cartões, confirmações, erros e visões de dados.
//!
//! Desenho aprovado em `/projects/MJSV/temp/ascii/FINAL.md` e `FINAL-outputs.md`; este módulo é a
//! implementação ÚNICA, usada pelos dois binários.
//!
//! Regras que valem para tudo aqui (contrato duro):
//!
//! * `--output json|yaml` não recebe nada: [`Style::fancy`] é falso e todo construtor devolve o
//!   texto de hoje.
//! * Um fluxo que não é TTY recebe exatamente o texto de hoje (a coluna "plain" da especificação).
//!   O estilo do stdout depende de o stdout ser TTY, o do stderr de o stderr ser TTY.
//! * `NO_COLOR`: mesmos caracteres, nenhum SGR. `MJSV_NO_BANNER`: sem banner nem ilustração
//!   (glifos e cartões ficam). `TERM=dumb`: texto puro.
//!
//! Os construtores devolvem `String` SEM quebra de linha final; quem chama usa `println!`/`eprintln!`.
//! Nada aqui escreve em fluxo algum nem guarda estado global: um [`Style`] é um valor, e os testes
//! montam o terminal que quiserem.

mod art;
mod banner;
mod caps;
mod confirm;
mod data;
mod layout;
mod msg;
mod report;
mod text;

pub use banner::{BannerKind, InfoLine};
pub use caps::{Caps, Charset, Depth, Env, MapEnv, Stream, SysEnv};
pub use confirm::{ConfirmSpec, Tier};
pub use data::{Marker, Receipt, ReceiptItem, Semantics, SPINNER_INTERVAL_MS};
pub use layout::RuleSpec;
pub use msg::{api_host, LoginView, OrgSummary, SecretBox, SecretItem};
pub use report::{first_noun, CardError, ReportCtx};
pub use text::{is_clean, pad, sanitize, strip_ansi, truncate, width};

/// Constantes de arte, para quem precisar delas cruas (testes, `--help` futuro).
pub mod consts {
    pub use super::art::*;
}

use crate::output::Format;

/// Qual binário está desenhando. Muda a cor da coroa, o tipo de régua e os cartões de erro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Mjsv,
    Mjsvadm,
}

impl Tool {
    pub fn bin(self) -> &'static str {
        match self {
            Tool::Mjsv => "mjsv",
            Tool::Mjsvadm => "mjsvadm",
        }
    }

    pub fn is_staff(self) -> bool {
        self == Tool::Mjsvadm
    }
}

/// Glifos de mensagem, na coluna 0 (1 célula em Unicode, 3 em ASCII).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// feito
    Done,
    /// falhou
    Failed,
    /// atenção
    Attention,
    /// informação, nada a fazer, cancelado
    Info,
    /// aceito, ainda não feito
    Accepted,
    /// passo ou pergunta de ROOT (só `mjsvadm`)
    Root,
}

/// Estilos nomeados da tabela de cores da especificação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sty {
    Ok,
    Err,
    Warn,
    Info,
    Acc,
    Dim,
    Bold,
    Brand,
    SBrand,
    Plate,
    SRule,
    STag,
    DRule,
    DTag,
    Sec,
    Spin,
    M75,
    M90,
    /// Linha `n` da marca do banner do cliente.
    Grad(u8),
    /// Linha `n` da marca do banner de staff.
    SGrad(u8),
}

/// O estilo de UM fluxo: capacidades mais o binário.
#[derive(Debug, Clone)]
pub struct Style {
    pub caps: Caps,
    pub tool: Tool,
}

impl Style {
    pub fn new(caps: Caps, tool: Tool) -> Style {
        Style { caps, tool }
    }

    /// Detecta no ambiente real. `format` entra porque `--output json|yaml` desliga tudo.
    pub fn detect(stream: Stream, format: Format, tool: Tool) -> Style {
        Style::new(Caps::from_stream(&SysEnv, stream, format), tool)
    }

    pub fn stdout(format: Format, tool: Tool) -> Style {
        Style::detect(Stream::Stdout, format, tool)
    }

    pub fn stderr(format: Format, tool: Tool) -> Style {
        Style::detect(Stream::Stderr, format, tool)
    }

    /// Como [`Style::detect`], mas com ambiente e TTY injetados.
    pub fn with_env(env: &dyn Env, tty: bool, width: Option<usize>, format: Format, tool: Tool) -> Style {
        Style::new(Caps::detect(env, tty, width, format.is_machine_readable(), false), tool)
    }

    /// O texto de hoje: nada de glifo, cor ou arte.
    pub fn plain(tool: Tool) -> Style {
        Style::new(Caps::plain(), tool)
    }

    /// Um terminal de teste com o que se quiser.
    pub fn terminal(tool: Tool, charset: Charset, depth: Depth, width: usize) -> Style {
        Style::new(Caps::terminal(charset, depth, width), tool)
    }

    // -- portões ---------------------------------------------------------------------------

    /// Glifos, cartões, marcadores e réguas: TTY, formato tabela e `TERM` diferente de `dumb`.
    pub fn fancy(&self) -> bool {
        self.caps.tty && !self.caps.machine && !self.caps.dumb
    }

    /// Banners e ilustrações: o que `fancy` exige, mais `MJSV_NO_BANNER` ausente.
    pub fn art(&self) -> bool {
        self.fancy() && !self.caps.no_banner
    }

    /// SGR liga: `fancy` e sem `NO_COLOR`.
    pub fn color(&self) -> bool {
        self.fancy() && self.caps.depth != Depth::None
    }

    pub fn unicode(&self) -> bool {
        self.caps.charset == Charset::Unicode
    }

    pub fn staff(&self) -> bool {
        self.tool.is_staff()
    }

    /// Largura de régua de cartão, prompt e caixa: `min(colunas - 1, 60)`.
    pub fn rule_width(&self) -> usize {
        self.caps.width.saturating_sub(1).clamp(20, 60)
    }

    /// Abaixo de 60 colunas o banner vira a coroa compacta.
    pub fn narrow(&self) -> bool {
        self.caps.width < 60
    }

    // -- pintura ---------------------------------------------------------------------------

    fn sgr(&self, s: Sty) -> Option<String> {
        if !self.color() {
            return None;
        }
        let d = self.caps.depth;
        let tc = d == Depth::TrueColor;
        let c256 = d == Depth::Ansi256;
        let pick = |t: &str, m: &str, l: &str| -> String {
            if tc {
                t
            } else if c256 {
                m
            } else {
                l
            }
            .to_string()
        };
        // O contorno fino da marca em ASCII ganha negrito para manter contraste em tema claro.
        let bold_ascii = |code: String| if self.unicode() { code } else { format!("1;{code}") };

        Some(match s {
            Sty::Ok => "32".into(),
            Sty::Err => "31".into(),
            Sty::Warn => "33".into(),
            Sty::Info | Sty::Acc => "36".into(),
            Sty::Dim => "2".into(),
            Sty::Bold => "1".into(),
            Sty::Brand => bold_ascii(pick("38;2;255;92;57", "38;5;203", "33")),
            Sty::SBrand => bold_ascii(pick("38;2;255;59;48", "38;5;196", "31")),
            Sty::Plate => pick("1;38;2;255;255;255;48;2;214;31;38", "1;38;5;231;48;5;160", "1;97;41"),
            Sty::SRule => pick("38;2;214;31;38", "38;5;160", "31"),
            Sty::STag => pick("1;38;2;214;31;38", "1;38;5;160", "1;31"),
            Sty::DRule => "31".into(),
            Sty::DTag => "1;31".into(),
            Sty::Sec => "33".into(),
            Sty::Spin => pick("38;2;254;118;65", "38;5;209", "33"),
            Sty::M75 => "33".into(),
            Sty::M90 => "31".into(),
            Sty::Grad(i) => {
                let i = (i as usize).min(7);
                let (r, g, b) = art::GRADIENT_CUSTOMER[i];
                bold_ascii(pick(
                    &format!("38;2;{r};{g};{b}"),
                    &format!("38;5;{}", art::GRADIENT_CUSTOMER_256[i / 4]),
                    "33",
                ))
            }
            Sty::SGrad(i) => {
                let i = (i as usize).min(7);
                let (r, g, b) = art::GRADIENT_STAFF[i];
                bold_ascii(pick(
                    &format!("38;2;{r};{g};{b}"),
                    &format!("38;5;{}", art::GRADIENT_STAFF_256[i / 4]),
                    "31",
                ))
            }
        })
    }

    /// Pinta `text`. Toda corrida pintada é fechada com SGR 0 antes de qualquer quebra de linha.
    pub fn paint(&self, s: Sty, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        match self.sgr(s) {
            Some(code) => format!("\x1b[{code}m{text}\x1b[0m"),
            None => text.to_string(),
        }
    }

    pub fn dim(&self, text: &str) -> String {
        self.paint(Sty::Dim, text)
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint(Sty::Bold, text)
    }

    /// SGR 0 explícito, para emitir antes de bloquear em um prompt (a cor nunca pode "vazar" para o
    /// que o usuário digita). Vazio quando não há cor.
    pub fn reset(&self) -> &'static str {
        if self.color() {
            "\x1b[0m"
        } else {
            ""
        }
    }

    // -- glifos ----------------------------------------------------------------------------

    /// O glifo pintado. Não consulta `fancy`: quem decide se há glifo é o construtor da mensagem.
    pub fn glyph(&self, g: Glyph) -> String {
        let u = self.unicode();
        match g {
            Glyph::Done => self.paint(Sty::Ok, if u { "✓" } else { "[+]" }),
            Glyph::Failed => self.paint(Sty::Err, if u { "✗" } else { "[x]" }),
            Glyph::Attention => self.paint(Sty::Warn, if u { "▲" } else { "[!]" }),
            Glyph::Info => self.paint(Sty::Info, if u { "●" } else { "[i]" }),
            Glyph::Accepted => self.paint(Sty::Acc, if u { "○" } else { "[~]" }),
            Glyph::Root => self.paint(Sty::Plate, "[#]"),
        }
    }

    /// Quadros do spinner (4, 100 ms cada), sempre com a largura do glifo do conjunto.
    pub fn spinner_frame(&self, i: usize) -> String {
        let f = if self.unicode() {
            ["▌", "▀", "▐", "▄"]
        } else {
            ["[|]", "[/]", "[-]", "[\\]"]
        };
        self.paint(Sty::Spin, f[i % 4])
    }

    /// A coroa compacta, na cor da ferramenta.
    pub fn crown(&self) -> String {
        let art = if self.unicode() {
            art::COMPACT_UNICODE
        } else {
            art::COMPACT_ASCII
        };
        self.paint(if self.staff() { Sty::SBrand } else { Sty::Brand }, art)
    }

    /// A coroa vermelha de staff (selo do CF-A3), independente da ferramenta.
    pub fn staff_crown(&self) -> String {
        let art = if self.unicode() {
            art::COMPACT_UNICODE
        } else {
            art::COMPACT_ASCII
        };
        self.paint(Sty::SBrand, art)
    }

    /// Separador dentro de frases e linhas: ` · ` ou `, `.
    pub fn sep(&self) -> &'static str {
        if self.unicode() {
            " · "
        } else {
            ", "
        }
    }

    /// Separador dentro de título de régua e placa: ` · ` ou ` / `.
    pub fn rsep(&self) -> &'static str {
        if self.unicode() {
            " · "
        } else {
            " / "
        }
    }

    /// Reticências: `…` ou `...`.
    pub fn ellipsis(&self) -> &'static str {
        if self.unicode() {
            "…"
        } else {
            "..."
        }
    }

    /// Coluna de texto de um bloco de mensagem: 2 em Unicode, 4 em ASCII.
    pub fn indent(&self) -> &'static str {
        if self.unicode() {
            "  "
        } else {
            "    "
        }
    }

    /// Coluna de texto sob `[#]`: sempre 4.
    pub fn root_indent(&self) -> &'static str {
        "    "
    }

    /// Coluna de texto sob a coroa compacta: 6 em Unicode, 8 em ASCII.
    pub fn crown_indent(&self) -> &'static str {
        if self.unicode() {
            "      "
        } else {
            "        "
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    pub fn uni(tool: Tool) -> Style {
        Style::terminal(tool, Charset::Unicode, Depth::None, 100)
    }

    pub fn asc(tool: Tool) -> Style {
        Style::terminal(tool, Charset::Ascii, Depth::None, 100)
    }
}
