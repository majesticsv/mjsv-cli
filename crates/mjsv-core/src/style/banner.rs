//! Banners (HERO e STAFF), coroa compacta e a tela inicial.

use super::art::*;
use super::text::{truncate, width};
use super::{Glyph, Sty, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Hero,
    Staff,
}

/// Uma linha de informação ao lado do banner (linhas 5-7, coluna 24, orçamento de 48 colunas).
///
/// Só a linha 5 pode levar glifo; as demais são `rótulo: valor`.
#[derive(Debug, Clone)]
pub enum InfoLine {
    /// `rótulo: valor` (rótulo esmaecido).
    Kv(String, String),
    /// Texto simples (o endpoint).
    Text(String),
    /// `✓ texto` em negrito: a confirmação de login.
    Done(String),
}

impl InfoLine {
    pub fn kv(label: impl Into<String>, value: impl Into<String>) -> Self {
        InfoLine::Kv(label.into(), value.into())
    }

    pub fn text(t: impl Into<String>) -> Self {
        InfoLine::Text(t.into())
    }

    pub fn done(t: impl Into<String>) -> Self {
        InfoLine::Done(t.into())
    }
}

impl Style {
    fn render_info(&self, line: &InfoLine, budget: usize) -> String {
        let ell = self.ellipsis();
        match line {
            InfoLine::Kv(k, v) => {
                let v = truncate(v, budget.saturating_sub(width(k) + 2), ell);
                self.kv(k, &v)
            }
            InfoLine::Text(t) => truncate(t, budget, ell),
            InfoLine::Done(t) => format!(
                "{} {}",
                self.glyph(Glyph::Done),
                self.bold(&truncate(
                    t,
                    budget.saturating_sub(width(self.glyph(Glyph::Done).as_str()) + 1),
                    ell
                ))
            ),
        }
    }

    /// As 8 linhas do banner (sem a linha em branco que o precede). `version` é a cadeia que vai
    /// depois do rótulo (`mjsv 0.3.0`). Largura de texto truncada em `min(colunas, 72)`.
    pub fn banner_lines(&self, kind: BannerKind, version: &str, infos: &[InfoLine]) -> Vec<String> {
        let staff = kind == BannerKind::Staff;
        let u = self.unicode();
        let art: &[&str; 8] = match (staff, u) {
            (false, true) => &HERO_UNICODE,
            (false, false) => &HERO_ASCII,
            (true, true) => &STAFF_UNICODE,
            (true, false) => &STAFF_ASCII,
        };
        let plate = if u { PLATE_UNICODE } else { PLATE_ASCII };
        let budget = self.caps.width.min(72).saturating_sub(INFO_COL).max(8);
        let mut free = infos.iter();
        let mut out = Vec::with_capacity(8);

        for (i, line) in art.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            let mark: String = chars.iter().take(MARK_W).collect();
            let rest: String = chars.iter().skip(INFO_COL).collect();
            let mark = mark.trim_end().to_string();

            let text = if i == 0 && staff {
                match rest.split_once(plate) {
                    Some((a, b)) => format!(
                        "{}{}{}",
                        self.paint(Sty::SRule, a),
                        self.paint(Sty::Plate, plate),
                        self.paint(Sty::SRule, b)
                    ),
                    None => rest,
                }
            } else if (1..=3).contains(&i) {
                self.bold(&rest)
            } else if i == 4 {
                let label = if staff { self.paint(Sty::STag, &rest) } else { rest };
                format!("{label}   {}", self.dim(version))
            } else if let Some(sentence) = rest.strip_prefix("[#]") {
                // A frase fixa de staff: nunca truncada.
                format!("{}{sentence}", self.paint(Sty::Plate, "[#]"))
            } else if i >= 5 {
                free.next().map(|l| self.render_info(l, budget)).unwrap_or_default()
            } else {
                String::new()
            };

            let grad = if staff { Sty::SGrad(i as u8) } else { Sty::Grad(i as u8) };
            let g = self.paint(grad, &mark);
            out.push(if text.is_empty() {
                g
            } else {
                format!("{g}{}{text}", " ".repeat(INFO_COL - width(&mark)))
            });
        }
        out
    }

    /// A linha da coroa compacta: `█▄█▄█ mjsv 0.3.0 (detalhe)`. `detail` é esmaecido.
    pub fn compact_line(&self, text: &str, detail: Option<&str>) -> String {
        let mut l = format!("{} {}", self.crown(), self.bold(text));
        if let Some(d) = detail {
            l.push(' ');
            l.push_str(&self.dim(d));
        }
        l
    }

    /// A coroa compacta com uma segunda linha (e terceira) de detalhe no recuo do texto.
    pub fn compact_block(&self, first: &str, details: &[String]) -> String {
        let mut lines = vec![format!("{} {first}", self.crown())];
        lines.extend(details.iter().take(2).map(|d| format!("{}{d}", self.crown_indent())));
        lines.join("\n")
    }

    /// O banner da tela inicial, com a linha em branco que o precede. `None` quando não há arte
    /// (fora de um terminal estilizado ou `MJSV_NO_BANNER`).
    ///
    /// Abaixo de 60 colunas vira a coroa compacta com as linhas de informação como detalhe; em
    /// `mjsvadm` a frase de ROOT vem numa linha própria.
    pub fn banner(&self, kind: BannerKind, version: &str, infos: &[InfoLine]) -> Option<String> {
        if !self.art() {
            return None;
        }
        if self.narrow() {
            return Some(self.narrow_banner(kind, version, infos));
        }
        let mut lines = vec![String::new()];
        lines.extend(self.banner_lines(kind, version, infos));
        Some(lines.join("\n"))
    }

    fn narrow_banner(&self, kind: BannerKind, version: &str, infos: &[InfoLine]) -> String {
        let budget = self.caps.width.saturating_sub(self.crown_indent().len() + 1).max(8);
        let mut details: Vec<String> = Vec::new();
        let mut first = self.bold(version);
        if kind == BannerKind::Staff {
            let plate = if self.unicode() { PLATE_UNICODE } else { PLATE_ASCII };
            let ch = if self.unicode() { "══" } else { "==" };
            first = format!(
                "{}  {}{}{}",
                self.bold(version),
                self.paint(Sty::SRule, ch),
                self.paint(Sty::Plate, plate),
                self.paint(Sty::SRule, ch)
            );
            details.push(format!(
                "{} comandos afetam toda a plataforma",
                self.paint(Sty::Plate, "[#]")
            ));
        }
        details.extend(infos.iter().map(|l| self.render_info(l, budget)));
        let mut lines = vec![format!("{} {first}", self.crown())];
        lines.extend(details.iter().take(3).map(|d| format!("{}{d}", self.crown_indent())));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::{Style, Tool};
    use super::*;

    #[test]
    fn hero_unicode_igual_ao_exemplo_da_especificacao() {
        let s = uni(Tool::Mjsv);
        let l = s.banner_lines(
            BannerKind::Hero,
            "mjsv 0.3.0",
            &[
                InfoLine::text("courtyard-api.mjsv.io"),
                InfoLine::kv("org", "acme (nUME7)"),
                InfoLine::text("3 VMs · 2 gameservers"),
            ],
        );
        let want = [
            "  ▄▄    ▄██▄    ▄▄",
            "  ▀▀████████████▀▀      ██▄ ▄██     ██  ▄█▀▀▀▀  ██   ██",
            "██▄              ▄██    ██▀█▀██  ▄  ██  ▀████▄   ██ ██",
            "███▄▄██▄    ▄██▄▄███    ██   ██  ▀███▀  ▄▄▄▄█▀    ▀█▀",
            "████████▄  ▄████████    H O S T I N G   mjsv 0.3.0",
            "████████████████████    courtyard-api.mjsv.io",
            "███████▀    ▀███████    org: acme (nUME7)",
            " ▀▀████      ████▀▀     3 VMs · 2 gameservers",
        ];
        assert_eq!(l, want);
    }

    #[test]
    fn staff_ascii_igual_ao_exemplo_da_especificacao() {
        let s = asc(Tool::Mjsvadm);
        let l = s.banner_lines(
            BannerKind::Staff,
            "mjsvadm 0.3.0",
            &[
                InfoLine::text("courtyard-api.mjsv.io"),
                InfoLine::kv("staff", "joao.silva@mjsv.io, 214 VMs, 58 orgs"),
            ],
        );
        let want = r#"         /\             == STAFF / ROOT =================
    |\__/  \__/|        ||\  /||     ||  //====  \\    //
|\_  \________/  _/|    || \/ ||     ||  \\==\\   \\  //
|  |  _      _  |  |    ||    ||  \__//   ===//    \\//
|  |_| |_  _| |_|  |    A D M   mjsvadm 0.3.0
|        \/        |    [#] comandos afetam toda a plataforma
'-._    .--.    _.-'    courtyard-api.mjsv.io
    '--|####|--'        staff: joao.silva@mjsv.io, 214 VMs, 58 orgs"#;
        assert_eq!(l.join("\n"), want);
    }

    #[test]
    fn banner_sem_arte_fora_de_terminal_e_sem_banner() {
        assert!(Style::plain(Tool::Mjsv).banner(BannerKind::Hero, "x", &[]).is_none());
        let mut s = uni(Tool::Mjsv);
        s.caps.no_banner = true;
        assert!(s.banner(BannerKind::Hero, "x", &[]).is_none());
        // glifos continuam: só o banner some
        assert!(s.fancy());
    }

    #[test]
    fn abaixo_de_60_colunas_vira_a_coroa() {
        let mut s = uni(Tool::Mjsv);
        s.caps.width = 50;
        let b = s
            .banner(
                BannerKind::Hero,
                "mjsv 0.3.0",
                &[InfoLine::kv("sessão", "nenhuma"), InfoLine::kv("próximo", "mjsv login")],
            )
            .unwrap();
        assert_eq!(b, "█▄█▄█ mjsv 0.3.0\n      sessão: nenhuma\n      próximo: mjsv login");
    }

    #[test]
    fn linhas_de_info_truncam_no_orcamento() {
        let mut s = uni(Tool::Mjsv);
        s.caps.width = 60;
        let l = s.banner_lines(BannerKind::Hero, "v", &[InfoLine::text("x".repeat(80))]);
        assert!(width(&l[5]) <= 60);
        assert!(l[5].ends_with('…'));
    }
}
