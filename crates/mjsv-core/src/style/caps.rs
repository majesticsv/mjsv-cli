//! Detecção de capacidades de UM fluxo (stdout ou stderr).
//!
//! Tudo é injetável: `Env` abstrai as variáveis de ambiente e o resto entra por parâmetro, então
//! os testes montam qualquer terminal sem tocar em estado global do processo (que, em teste
//! paralelo, é uma fonte de falha intermitente).

use std::collections::HashMap;
use std::io::IsTerminal;

use crate::output::Format;

/// Leitura de variáveis de ambiente.
pub trait Env {
    fn var(&self, key: &str) -> Option<String>;
}

/// O ambiente real do processo.
pub struct SysEnv;

impl Env for SysEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Ambiente de mentira, para teste.
#[derive(Default, Clone)]
pub struct MapEnv(pub HashMap<String, String>);

impl MapEnv {
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        MapEnv(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }
}

impl Env for MapEnv {
    fn var(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

/// Qual fluxo está sendo decidido. O estilo de stdout depende de stdout ser TTY e o de stderr, de
/// stderr: um `mjsv vm list > arquivo` não pode perder a cor dos erros, nem o contrário.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    pub fn is_terminal(self) -> bool {
        match self {
            Stream::Stdout => std::io::stdout().is_terminal(),
            Stream::Stderr => std::io::stderr().is_terminal(),
        }
    }

    fn size(self) -> Option<usize> {
        use terminal_size::{terminal_size_of, Width};
        match self {
            Stream::Stdout => terminal_size_of(std::io::stdout()).map(|(Width(w), _)| w as usize),
            Stream::Stderr => terminal_size_of(std::io::stderr()).map(|(Width(w), _)| w as usize),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Depth {
    /// `NO_COLOR`: nenhum SGR, nem negrito nem esmaecido.
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    Unicode,
    Ascii,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caps {
    pub tty: bool,
    pub depth: Depth,
    pub charset: Charset,
    pub width: usize,
    /// `TERM=dumb`: tudo vira texto puro.
    pub dumb: bool,
    /// `MJSV_NO_BANNER`: sem banners nem ilustrações (glifos e cartões ficam).
    pub no_banner: bool,
    /// `--output json|yaml`: nada deste módulo aparece.
    pub machine: bool,
}

impl Caps {
    /// Fluxo que não é terminal: o texto de hoje, byte a byte.
    pub fn plain() -> Caps {
        Caps {
            tty: false,
            depth: Depth::None,
            charset: Charset::Ascii,
            width: 80,
            dumb: false,
            no_banner: false,
            machine: false,
        }
    }

    /// Terminal "de teste": TTY, tabela, sem `dumb`.
    pub fn terminal(charset: Charset, depth: Depth, width: usize) -> Caps {
        Caps {
            tty: true,
            depth,
            charset,
            width,
            dumb: false,
            no_banner: false,
            machine: false,
        }
    }

    /// A detecção de verdade. `tty` e `term_width` vêm de fora para o teste poder injetá-los.
    pub fn detect(env: &dyn Env, tty: bool, term_width: Option<usize>, machine: bool, windows: bool) -> Caps {
        let term = env.var("TERM").unwrap_or_default();
        let dumb = term == "dumb";

        let set = |k: &str| env.var(k).is_some_and(|v| !v.is_empty());

        let depth = if set("NO_COLOR") {
            Depth::None
        } else if matches!(env.var("COLORTERM").as_deref(), Some("truecolor") | Some("24bit")) {
            Depth::TrueColor
        } else if term.contains("256color") {
            Depth::Ansi256
        } else {
            Depth::Ansi16
        };

        let width = term_width
            .filter(|w| *w > 0)
            .or_else(|| {
                env.var("COLUMNS")
                    .and_then(|c| c.trim().parse().ok())
                    .filter(|w: &usize| *w > 0)
            })
            .unwrap_or(80);

        Caps {
            tty,
            depth,
            charset: detect_charset(env, &term, windows),
            width,
            dumb,
            no_banner: set("MJSV_NO_BANNER"),
            machine,
        }
    }

    pub fn from_stream(env: &dyn Env, stream: Stream, format: Format) -> Caps {
        let tty = stream.is_terminal();
        Caps::detect(
            env,
            tty,
            if tty { stream.size() } else { None },
            format.is_machine_readable(),
            cfg!(windows),
        )
    }
}

/// Unicode só quando TUDO é verdade: locale UTF-8, idioma que não seja CJK (os blocos são de
/// largura ambígua no leste asiático), `TERM` que não seja `linux`/`dumb` e, no Windows, o Windows
/// Terminal (o conhost antigo não tem os glifos). `MJSV_ASCII=1` força ASCII.
fn detect_charset(env: &dyn Env, term: &str, windows: bool) -> Charset {
    if env.var("MJSV_ASCII").is_some_and(|v| !v.is_empty() && v != "0") {
        return Charset::Ascii;
    }

    // Precedência do locale: LC_ALL > LC_CTYPE > LANG, a primeira não vazia.
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|k| env.var(k))
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    let lower = locale.to_lowercase();

    let utf8 = lower.contains("utf-8") || lower.contains("utf8");
    let lang = lower.split(['_', '.', '@']).next().unwrap_or("");
    let cjk = matches!(lang, "ja" | "ko" | "zh");
    let console = term == "linux" || term == "dumb";
    let windows_ok = !windows || env.var("WT_SESSION").is_some_and(|v| !v.is_empty());

    if utf8 && !cjk && !console && windows_ok {
        Charset::Unicode
    } else {
        Charset::Ascii
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(pairs: &[(&str, &str)]) -> Caps {
        Caps::detect(&MapEnv::new(pairs), true, Some(100), false, false)
    }

    #[test]
    fn profundidade_de_cor() {
        assert_eq!(caps(&[("COLORTERM", "truecolor")]).depth, Depth::TrueColor);
        assert_eq!(caps(&[("COLORTERM", "24bit")]).depth, Depth::TrueColor);
        assert_eq!(caps(&[("TERM", "xterm-256color")]).depth, Depth::Ansi256);
        assert_eq!(caps(&[("TERM", "xterm")]).depth, Depth::Ansi16);
        assert_eq!(caps(&[]).depth, Depth::Ansi16);
        // NO_COLOR vence qualquer outra indicação.
        assert_eq!(
            caps(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]).depth,
            Depth::None
        );
        // Variável vazia não conta (convenção de no-color.org).
        assert_eq!(
            caps(&[("NO_COLOR", ""), ("COLORTERM", "truecolor")]).depth,
            Depth::TrueColor
        );
    }

    #[test]
    fn conjunto_de_caracteres() {
        let u = |p: &[(&str, &str)]| caps(p).charset == Charset::Unicode;
        assert!(u(&[("LANG", "pt_BR.UTF-8")]));
        assert!(u(&[("LC_ALL", "en_US.utf8")]));
        // LC_ALL manda: LANG UTF-8 não salva um LC_ALL=C.
        assert!(!u(&[("LC_ALL", "C"), ("LANG", "pt_BR.UTF-8")]));
        assert!(!u(&[]));
        assert!(!u(&[("LANG", "ja_JP.UTF-8")]));
        assert!(!u(&[("LANG", "zh_CN.UTF-8")]));
        assert!(!u(&[("LANG", "pt_BR.UTF-8"), ("TERM", "linux")]));
        assert!(!u(&[("LANG", "pt_BR.UTF-8"), ("TERM", "dumb")]));
        assert!(!u(&[("LANG", "pt_BR.UTF-8"), ("MJSV_ASCII", "1")]));
    }

    #[test]
    fn windows_exige_windows_terminal() {
        let env = MapEnv::new(&[("LANG", "pt_BR.UTF-8")]);
        assert_eq!(Caps::detect(&env, true, None, false, true).charset, Charset::Ascii);
        let env = MapEnv::new(&[("LANG", "pt_BR.UTF-8"), ("WT_SESSION", "x")]);
        assert_eq!(Caps::detect(&env, true, None, false, true).charset, Charset::Unicode);
    }

    #[test]
    fn largura_cai_para_columns_e_depois_80() {
        let env = MapEnv::new(&[("COLUMNS", "132")]);
        assert_eq!(Caps::detect(&env, true, None, false, false).width, 132);
        assert_eq!(Caps::detect(&env, true, Some(100), false, false).width, 100);
        assert_eq!(Caps::detect(&MapEnv::default(), true, None, false, false).width, 80);
    }

    #[test]
    fn dumb_e_sem_banner() {
        let c = caps(&[("TERM", "dumb"), ("MJSV_NO_BANNER", "1")]);
        assert!(c.dumb && c.no_banner);
    }
}
