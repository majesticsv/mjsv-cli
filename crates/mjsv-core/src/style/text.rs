//! Texto: largura de exibição, preenchimento, truncamento e saneamento.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Largura em células, ignorando sequências SGR (`ESC [ ... m`) já embutidas na string.
///
/// Medir por `chars().count()` desalinha qualquer coisa com CJK, emoji ou acento combinado; medir
/// com o SGR dentro contaria os bytes invisíveis da cor.
pub fn width(s: &str) -> usize {
    if !s.contains('\x1b') {
        return UnicodeWidthStr::width(s);
    }
    UnicodeWidthStr::width(strip_ansi(s).as_str())
}

/// Remove sequências CSI (`ESC [ ... letra`).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' && it.peek() == Some(&'[') {
            it.next();
            for n in it.by_ref() {
                if ('@'..='~').contains(&n) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Completa com espaços à direita até `w` células. Não corta.
pub fn pad(s: &str, w: usize) -> String {
    let n = width(s);
    format!("{s}{}", " ".repeat(w.saturating_sub(n)))
}

/// Corta em `max` células, terminando em `ell` quando algo foi removido. Só para texto puro (sem
/// SGR): quem pinta, pinta depois de cortar.
pub fn truncate(s: &str, max: usize, ell: &str) -> String {
    if width(s) <= max {
        return s.to_string();
    }
    let budget = max.saturating_sub(width(ell));
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str(ell);
    out
}

fn is_invisible(c: char) -> bool {
    matches!(c,
        // C0 (menos tab/nova linha, tratados à parte), DEL e C1.
        '\u{0}'..='\u{8}' | '\u{b}'..='\u{1f}' | '\u{7f}'..='\u{9f}'
        // bidi
        | '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        // largura zero
        | '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}' | '\u{180e}' | '\u{ad}')
}

/// Saneia texto vindo da API ou do usuário antes de imprimi-lo.
///
/// ESC e controles permitiriam a um nome de VM malicioso repintar o terminal de quem lista; bidi
/// reordena o que se lê e largura zero esconde caracteres. Tab e quebra de linha viram espaço, e
/// não somem, para as palavras não se colarem.
pub fn sanitize(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' => Some(' '),
            c if is_invisible(c) => None,
            c => Some(c),
        })
        .collect()
}

/// `true` se o texto passaria intacto por [`sanitize`] (usado para decidir entre nome e id).
pub fn is_clean(s: &str) -> bool {
    !s.chars().any(|c| matches!(c, '\t' | '\n' | '\r') || is_invisible(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largura_ignora_sgr_e_conta_cjk() {
        assert_eq!(width("abc"), 3);
        assert_eq!(width("\x1b[1mabc\x1b[0m"), 3);
        assert_eq!(width("web-ação"), 8);
        assert_eq!(width("日本"), 4);
    }

    #[test]
    fn preenche_por_celulas() {
        assert_eq!(pad("日", 4), "日  ");
        assert_eq!(pad("abcdef", 3), "abcdef");
    }

    #[test]
    fn trunca_com_reticencias() {
        assert_eq!(truncate("abcdefghij", 6, "…"), "abcde…");
        assert_eq!(truncate("abcdefghij", 6, "..."), "abc...");
        assert_eq!(truncate("abc", 6, "…"), "abc");
        assert_eq!(truncate("日本語です", 5, "…"), "日本…");
    }

    #[test]
    fn saneia_controle_bidi_e_largura_zero() {
        assert_eq!(sanitize("a\x1b[31mb"), "a[31mb");
        assert_eq!(sanitize("a\u{202e}b\u{200b}c"), "abc");
        assert_eq!(sanitize("a\nb\tc"), "a b c");
        assert_eq!(sanitize("normal ação"), "normal ação");
        assert!(is_clean("acme"));
        assert!(!is_clean("a\u{202e}"));
    }
}
