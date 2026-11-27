//! Visões de dados: marcadores de estado, tabelas, medidores, sparklines, recibo e progresso.

use super::text::{pad, truncate, width};
use super::{Glyph, Sty, Style};

/// Intervalo entre quadros do spinner.
pub const SPINNER_INTERVAL_MS: u64 = 100;

/// Classe de um estado. Um marcador tem 1 célula nos dois conjuntos de caracteres, fica DENTRO de
/// uma célula de valor ou coluna de tabela (nunca na coluna 0) e vem antes da palavra da API, que
/// não muda — `grep running` continua funcionando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Ok,
    Off,
    Attention,
    Failed,
    /// em transição (aceito, ainda não feito)
    Flux,
}

/// Como classificar a palavra `running`: um recurso rodando é saudável, uma operação rodando está
/// em andamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Semantics {
    #[default]
    Resource,
    Operation,
}

impl Marker {
    /// Classifica um valor de estado da API (sem diferenciar caixa). Palavra desconhecida: `None`
    /// (sem marcador).
    pub fn classify(word: &str, sem: Semantics) -> Option<Marker> {
        let w = word.trim().to_lowercase();
        Some(match w.as_str() {
            "running" if sem == Semantics::Operation => Marker::Flux,
            "running" | "online" | "active" | "completed" | "healthy" | "ok" | "sim" | "mitigated" => Marker::Ok,
            "stopped" | "offline" | "cancelled" | "canceled" | "expired" | "closed" => Marker::Off,
            "degraded" | "suspended" | "overdue" | "warn" | "warning" => Marker::Attention,
            "error" | "failed" | "fail" | "crashed" | "não" | "nao" => Marker::Failed,
            "starting" | "stopping" | "installing" | "reinstalling" | "pending" | "queued" | "ongoing" => Marker::Flux,
            _ => return None,
        })
    }

    /// O glifo de mensagem equivalente, para tabela cuja coluna de estado é a primeira.
    pub fn as_glyph(self) -> Option<Glyph> {
        match self {
            Marker::Ok => Some(Glyph::Done),
            Marker::Off => None,
            Marker::Attention => Some(Glyph::Attention),
            Marker::Failed => Some(Glyph::Failed),
            Marker::Flux => Some(Glyph::Accepted),
        }
    }
}

/// Um item do recibo.
#[derive(Debug, Clone)]
pub struct ReceiptItem {
    pub description: String,
    /// Já formatado, com o sinal colado: `R$ 59,90`, `-R$ 10,48`.
    pub price: String,
}

#[derive(Debug, Clone)]
pub struct Receipt {
    pub order_id: String,
    pub items: Vec<ReceiptItem>,
    pub total: String,
    pub balance_before: String,
    pub balance_after: String,
}

/// Arredonda metade para o par, como a especificação (e o Python que a gerou) faz: 25% de 18
/// células são 4,5 e precisam dar 4, o que `f64::round` (metade para longe do zero) não dá.
fn round_even(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        r - x.signum()
    } else {
        r
    }
}

const RECEIPT_W: usize = 48;
const METER_W: usize = 20;

impl Style {
    // -- marcadores --------------------------------------------------------------------------

    /// O marcador pintado (1 célula).
    pub fn marker(&self, m: Marker) -> String {
        let u = self.unicode();
        match m {
            Marker::Ok => self.paint(Sty::Ok, if u { "●" } else { "+" }),
            Marker::Off => self.dim(if u { "·" } else { "-" }),
            Marker::Attention => self.paint(Sty::Warn, if u { "▲" } else { "!" }),
            Marker::Failed => self.paint(Sty::Err, if u { "✗" } else { "x" }),
            Marker::Flux => self.paint(Sty::Acc, if u { "○" } else { "~" }),
        }
    }

    /// `● running`: marcador + palavra da API. Fora de um terminal estilizado, ou com palavra
    /// desconhecida, a palavra sozinha.
    pub fn state(&self, word: &str) -> String {
        self.state_with(word, Semantics::Resource)
    }

    pub fn state_with(&self, word: &str, sem: Semantics) -> String {
        match Marker::classify(word, sem) {
            Some(m) if self.fancy() => format!("{} {word}", self.marker(m)),
            _ => word.to_string(),
        }
    }

    /// Marcador DEPOIS de um número (`2 ▲`): um marcador antes de número lê-se como aritmética.
    /// Vazio fora de um terminal estilizado.
    pub fn trailing(&self, m: Marker) -> String {
        if self.fancy() {
            format!(" {}", self.marker(m))
        } else {
            String::new()
        }
    }

    // -- tabela ------------------------------------------------------------------------------

    /// Cabeçalho de tabela: negrito num terminal estilizado.
    pub fn table_header(&self, h: &str) -> String {
        self.bold(h)
    }

    /// Renderiza uma tabela já com cabeçalho em negrito e marcador nas colunas de estado.
    /// `state_cols` são os índices das colunas de estado; na primeira coluna (índice 0) o marcador
    /// é um glifo de mensagem, porque `+`/`-` na coluna 0 é sintaxe de Markdown e de diff.
    ///
    /// A estrutura é a de `Table::render`: cabeçalho MAIÚSCULO, dois espaços entre colunas,
    /// alinhado à esquerda, última coluna sem preenchimento. O que muda é só a pintura, por isso a
    /// largura das colunas é medida sem o SGR.
    pub fn table(&self, headers: &[String], rows: &[Vec<String>], state_cols: &[usize], sem: Semantics) -> String {
        let cell = |ci: usize, c: &str| -> String {
            // Célula faltando (`-`) esmaece: é ausência, não valor.
            if c == "-" && self.fancy() {
                return self.dim(c);
            }
            if !state_cols.contains(&ci) || !self.fancy() {
                return c.to_string();
            }
            match Marker::classify(c, sem) {
                Some(m) if ci == 0 => match m.as_glyph() {
                    Some(g) => format!("{} {c}", self.glyph(g)),
                    None => c.to_string(),
                },
                Some(m) => format!("{} {c}", self.marker(m)),
                None => c.to_string(),
            }
        };

        let heads: Vec<String> = headers.iter().map(|h| h.to_uppercase()).collect();
        let body: Vec<Vec<String>> = rows
            .iter()
            .map(|r| r.iter().enumerate().map(|(i, c)| cell(i, c)).collect())
            .collect();

        let mut widths: Vec<usize> = heads.iter().map(|h| width(h)).collect();
        for row in &body {
            for (i, c) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(width(c));
                }
            }
        }

        let mut out = String::new();
        let n = heads.len();
        for (i, h) in heads.iter().enumerate() {
            if i + 1 == n {
                out.push_str(&self.table_header(h));
            } else {
                out.push_str(&self.table_header(h));
                out.push_str(&" ".repeat(widths[i] - width(h) + 2));
            }
        }
        out.push('\n');
        for row in &body {
            for (i, c) in row.iter().enumerate() {
                if i >= widths.len() {
                    continue;
                }
                if i + 1 == widths.len() || i + 1 == row.len() {
                    out.push_str(c);
                } else {
                    out.push_str(&pad(c, widths[i]));
                    out.push_str("  ");
                }
            }
            out.push('\n');
        }
        out
    }

    // -- medidor e sparkline -------------------------------------------------------------------

    /// Medidor de 20 células, só quando existe um máximo real. `share` (parte de um total) não
    /// tem limiares. Fora de um terminal estilizado: string vazia (os números ficam sozinhos).
    ///
    /// Unicode: `█` cheio, `▌` meio (40 passos), `·` vazio. ASCII: `[` + 18 células de `=`/`.` +
    /// `]`; `#` nunca é usado, para um medidor não ser lido como `[#]`.
    pub fn meter(&self, pct: f64, share: bool) -> String {
        self.meter_w(pct, METER_W, share)
    }

    pub fn meter_w(&self, pct: f64, w: usize, share: bool) -> String {
        if !self.fancy() {
            return String::new();
        }
        let sty = if share || pct < 75.0 {
            None
        } else if pct < 90.0 {
            Some(Sty::M75)
        } else {
            Some(Sty::M90)
        };
        let fill = |s: &str| match sty {
            Some(st) => self.paint(st, s),
            None => s.to_string(),
        };
        let nonzero: f64 = if pct > 0.0 { 1.0 } else { 0.0 };

        if self.unicode() {
            let k = ((w * 2) as f64).min(nonzero.max(round_even(pct / 100.0 * (w * 2) as f64))) as usize;
            let (full, half) = (k / 2, k % 2);
            format!(
                "{}{}",
                fill(&format!("{}{}", "█".repeat(full), if half == 1 { "▌" } else { "" })),
                self.dim(&"·".repeat(w - full - half))
            )
        } else {
            let inner = w - 2;
            let k = (inner as f64).min(nonzero.max(round_even(pct / 100.0 * inner as f64))) as usize;
            format!("[{}{}]", fill(&"=".repeat(k)), self.dim(&".".repeat(inner - k)))
        }
    }

    /// Sparkline de valores já normalizados em 0..1 (uma célula por amostra, no máximo 30). Só em
    /// Unicode: em ASCII uma rampa `_.-=^` não se lê como curva, então o gráfico some e os
    /// números ao lado ficam (string vazia).
    pub fn spark(&self, values: &[f64]) -> String {
        if !self.fancy() || !self.unicode() {
            return String::new();
        }
        const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        values
            .iter()
            .take(30)
            .map(|v| BARS[((v.clamp(0.0, 1.0) * 8.0) as usize).min(7)])
            .collect()
    }

    /// Normaliza uma série. `from_zero`: fluxos (receita diária) partem de zero; níveis (MRR) vão
    /// do mínimo ao máximo.
    pub fn normalize(values: &[f64], from_zero: bool) -> Vec<f64> {
        let max = values.iter().cloned().fold(f64::MIN, f64::max);
        let min = if from_zero {
            0.0
        } else {
            values.iter().cloned().fold(f64::MAX, f64::min)
        };
        let span = (max - min).max(f64::EPSILON);
        values.iter().map(|v| (v - min) / span).collect()
    }

    // -- recibo --------------------------------------------------------------------------------

    /// O recibo coroado de `order pay` (48 colunas). Sem terminal estilizado, ou sem arte: `None`
    /// (a frase de hoje, `Pedido {id} pago.`). Abaixo de 48 colunas: a linha única com a coroa.
    pub fn receipt(&self, r: &Receipt) -> Option<String> {
        if !self.art() {
            return None;
        }
        let done = format!("{} Pedido {} pago.", self.glyph(Glyph::Done), r.order_id);
        if self.caps.width < RECEIPT_W {
            return Some(format!(
                "{} {} Pedido {} pago",
                self.crown(),
                self.glyph(Glyph::Done),
                r.order_id
            ));
        }
        let rule = self.rule(&super::RuleSpec::new().width(RECEIPT_W));
        let crown_w = width(if self.unicode() {
            super::art::COMPACT_UNICODE
        } else {
            super::art::COMPACT_ASCII
        });
        let head_l = "MJSV HOSTING";
        let head_r = format!("Pedido {}", r.order_id);
        let gap = RECEIPT_W.saturating_sub(crown_w + 1 + width(head_l) + width(&head_r));

        let mut lines = vec![
            format!("{} {}{}{head_r}", self.crown(), self.bold(head_l), " ".repeat(gap)),
            rule.clone(),
        ];
        let leader = if self.unicode() { "·" } else { "." };
        for it in &r.items {
            // A especificação diz "corta em 30 células", mas o próprio exemplo traz uma descrição de 33:
            // corta-se só o que não cabe (sobrando ao menos dois pontos de guia).
            let name = truncate(
                &it.description,
                RECEIPT_W.saturating_sub(width(&it.price) + 4),
                self.ellipsis(),
            );
            let n = RECEIPT_W.saturating_sub(width(&name) + width(&it.price) + 2).max(1);
            lines.push(format!("{name} {} {}", self.dim(&leader.repeat(n)), it.price));
        }
        if !r.items.is_empty() {
            lines.push(rule.clone());
        }
        let amount = |label: &str, value: &str, bold: bool| {
            let s = format!(
                "{label}{}{value}",
                " ".repeat(RECEIPT_W.saturating_sub(width(label) + width(value)))
            );
            if bold {
                self.bold(&s)
            } else {
                s
            }
        };
        lines.push(amount("Total", &r.total, true));
        lines.push(amount("Saldo", &r.balance_before, false));
        lines.push(amount("Resta", &r.balance_after, false));
        lines.push(rule);
        lines.push(done);
        Some(lines.join("\n"))
    }

    // -- progresso -----------------------------------------------------------------------------

    /// A linha de progresso de upload, UMA linha de no máximo `colunas - 1` células, para ser
    /// redesenhada no lugar. `pct` desconhecido: sem medidor (`▀ Enviando world.zip  38,1 MiB`).
    ///
    /// Só o quadro do spinner leva cor; o preenchimento fica no padrão e o vazio esmaecido.
    pub fn progress_line(&self, frame: usize, label: &str, pct: Option<f64>, detail: &str) -> String {
        let mut line = format!("{} {label}", self.spinner_frame(frame));
        if let Some(p) = pct {
            line.push_str(&format!(
                "  {}  {:>3}%",
                self.meter_w(p, METER_W, true),
                p.round() as i64
            ));
        }
        if !detail.is_empty() {
            line.push_str("  ");
            line.push_str(detail);
        }
        self.fit_line(&line)
    }

    /// Corta uma linha pintada a `colunas - 1` células sem partir uma sequência SGR.
    pub fn fit_line(&self, line: &str) -> String {
        let max = self.caps.width.saturating_sub(1);
        if width(line) <= max {
            return line.to_string();
        }
        let mut out = String::new();
        let mut used = 0;
        let mut it = line.chars().peekable();
        while let Some(c) = it.next() {
            if c == '\x1b' && it.peek() == Some(&'[') {
                out.push(c);
                for n in it.by_ref() {
                    out.push(n);
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
                continue;
            }
            let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + w > max {
                break;
            }
            used += w;
            out.push(c);
        }
        if self.color() {
            out.push_str("\x1b[0m");
        }
        out
    }

    /// `\r` + CSI K: apaga a linha redesenhável. Vazio sob `TERM=dumb` e fora de um terminal (não
    /// há redesenho nenhum nesses casos). CSI K não é SGR, então vale também sob `NO_COLOR`.
    pub fn erase_line(&self) -> &'static str {
        if self.fancy() {
            "\r\x1b[K"
        } else {
            ""
        }
    }

    /// Há redesenho no lugar? (`TERM=dumb`, pipe ou saída de máquina: só a linha final.)
    pub fn can_redraw(&self) -> bool {
        self.fancy()
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::{Style, Tool};
    use super::*;

    #[test]
    fn classes_de_estado() {
        use Marker::*;
        let c = |w| Marker::classify(w, Semantics::Resource);
        assert_eq!(c("running"), Some(Ok));
        assert_eq!(c("ACTIVE"), Some(Ok));
        assert_eq!(c("sim"), Some(Ok));
        assert_eq!(c("CANCELLED"), Some(Off));
        assert_eq!(c("degraded"), Some(Attention));
        assert_eq!(c("NÃO"), Some(Failed));
        assert_eq!(c("PENDING"), Some(Flux));
        assert_eq!(c("whatever"), None);
        assert_eq!(Marker::classify("running", Semantics::Operation), Some(Flux));
    }

    #[test]
    fn estado_so_em_terminal_estilizado() {
        assert_eq!(uni(Tool::Mjsv).state("running"), "● running");
        assert_eq!(asc(Tool::Mjsv).state("degraded"), "! degraded");
        assert_eq!(uni(Tool::Mjsv).state("desconhecido"), "desconhecido");
        assert_eq!(Style::plain(Tool::Mjsv).state("running"), "running");
    }

    #[test]
    fn medidor_tem_20_celulas_nos_dois_conjuntos() {
        for p in [0.0, 1.0, 25.0, 62.0, 75.0, 91.0, 100.0] {
            assert_eq!(width(&uni(Tool::Mjsv).meter(p, false)), 20, "{p}");
            assert_eq!(width(&asc(Tool::Mjsv).meter(p, false)), 20, "{p}");
        }
        assert_eq!(uni(Tool::Mjsv).meter(0.0, false), "····················");
        assert_eq!(uni(Tool::Mjsv).meter(100.0, false), "████████████████████");
        assert_eq!(asc(Tool::Mjsv).meter(62.0, false), "[===========.......]");
    }

    #[test]
    fn medidor_de_uso_muda_de_cor_nos_limiares() {
        let s = Style::terminal(
            Tool::Mjsv,
            super::super::Charset::Unicode,
            super::super::Depth::Ansi16,
            100,
        );
        assert!(!s.meter(74.0, false).contains("\x1b[33m"));
        assert!(s.meter(75.0, false).contains("\x1b[33m"));
        assert!(s.meter(90.0, false).contains("\x1b[31m"));
        assert!(!s.meter(95.0, true).contains("\x1b[31m"));
    }

    #[test]
    fn sparkline_so_em_unicode() {
        assert_eq!(uni(Tool::Mjsv).spark(&[0.0, 0.5, 1.0]), "▁▅█");
        assert_eq!(asc(Tool::Mjsv).spark(&[0.0, 0.5, 1.0]), "");
    }

    #[test]
    fn tabela_estilizada_alinha_pela_largura_visivel() {
        let s = Style::terminal(
            Tool::Mjsv,
            super::super::Charset::Unicode,
            super::super::Depth::TrueColor,
            100,
        );
        let out = s.table(
            &["id".into(), "estado".into()],
            &[vec!["a".into(), "running".into()], vec!["bb".into(), "stopped".into()]],
            &[1],
            Semantics::Resource,
        );
        let plain = super::super::strip_ansi(&out);
        assert_eq!(plain, "ID  ESTADO\na   ● running\nbb  · stopped\n");
        assert!(out.starts_with("\x1b[1mID\x1b[0m"));
    }

    #[test]
    fn tabela_fora_do_terminal_e_a_de_hoje() {
        let p = Style::plain(Tool::Mjsv);
        let out = p.table(
            &["id".into(), "estado".into()],
            &[vec!["a".into(), "running".into()]],
            &[1],
            Semantics::Resource,
        );
        assert_eq!(out, "ID  ESTADO\na   running\n");
    }

    #[test]
    fn linha_de_progresso_cabe_em_colunas_menos_um() {
        let s = Style::terminal(
            Tool::Mjsv,
            super::super::Charset::Unicode,
            super::super::Depth::TrueColor,
            30,
        );
        let l = s.progress_line(1, "Enviando world.zip", Some(62.0), "38,1 de 61,4 MiB");
        assert!(width(&l) <= 29, "{}", width(&l));
        assert!(l.ends_with("\x1b[0m"));
    }
}
