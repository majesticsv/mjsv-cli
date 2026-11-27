//! Entrada interativa, compartilhada pelos dois binários.
//!
//! Todo prompt falha em vez de travar quando não há terminal: um CLI que espera resposta dentro
//! de um pipeline de CI fica pendurado até o timeout do job, e o motivo não aparece em lugar
//! nenhum.
//!
//! As perguntas vão para o STDERR se ele for um terminal, senão para o stdout se for, senão para
//! `/dev/tty`; sem nenhum dos três é o erro "exige um terminal". Uma pergunta nunca é escrita num
//! fluxo redirecionado: `mjsv vm rebuild > log` não pode engolir a pergunta nem sujar o log.

use std::io::{BufRead, IsTerminal, Write};

use crate::output::Format;
use crate::style::{Caps, ConfirmSpec, Style, SysEnv, Tool};

/// O que o usuário deveria fazer em script, para o erro de "sem terminal".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoTerminalHint {
    /// Pergunta de login: autenticar pela variável `MJSV_TOKEN`.
    Token,
    /// Confirmação: passar `--yes`.
    Yes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptError {
    /// "esta pergunta exige um terminal — em script, ..."
    NoTerminal { what: &'static str, hint: NoTerminalHint },
    /// Confirmação em saída de máquina sem `--yes`.
    NeedsYes { question: String },
    /// O usuário respondeu que não (ou deixou em branco).
    Cancelled,
    /// O nome digitado no nível 3 não confere.
    Mismatch,
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptError::NoTerminal { what, hint } => {
                let tail = match hint {
                    NoTerminalHint::Token => "autentique com a variável MJSV_TOKEN",
                    NoTerminalHint::Yes => "passe --yes",
                };
                write!(f, "{what} exige um terminal — em script, {tail}")
            }
            PromptError::NeedsYes { question } => write!(f, "{question}\nem saída não interativa, confirme com --yes"),
            PromptError::Cancelled => f.write_str("cancelado"),
            PromptError::Mismatch => f.write_str("cancelado: o nome digitado não confere"),
        }
    }
}

impl std::error::Error for PromptError {}

/// Onde a pergunta é escrita.
enum Sink {
    Stderr,
    Stdout,
    #[cfg(unix)]
    Tty(std::fs::File),
}

impl Sink {
    fn pick() -> Option<Sink> {
        if std::io::stderr().is_terminal() {
            return Some(Sink::Stderr);
        }
        if std::io::stdout().is_terminal() {
            return Some(Sink::Stdout);
        }
        #[cfg(unix)]
        {
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open("/dev/tty") {
                return Some(Sink::Tty(f));
            }
        }
        None
    }

    fn write(&mut self, text: &str) -> std::io::Result<()> {
        match self {
            Sink::Stderr => {
                let mut e = std::io::stderr().lock();
                e.write_all(text.as_bytes())?;
                e.flush()
            }
            Sink::Stdout => {
                let mut o = std::io::stdout().lock();
                o.write_all(text.as_bytes())?;
                o.flush()
            }
            #[cfg(unix)]
            Sink::Tty(f) => {
                f.write_all(text.as_bytes())?;
                f.flush()
            }
        }
    }

    /// O estilo do destino escolhido: o de stderr/stdout conforme o fluxo, e o do `/dev/tty` como
    /// terminal comum.
    fn style(&self, tool: Tool) -> Style {
        match self {
            Sink::Stderr => Style::detect(crate::style::Stream::Stderr, Format::Table, tool),
            Sink::Stdout => Style::detect(crate::style::Stream::Stdout, Format::Table, tool),
            #[cfg(unix)]
            Sink::Tty(_) => Style::new(Caps::detect(&SysEnv, true, None, false, false), tool),
        }
    }
}

fn need_terminal(what: &'static str, hint: NoTerminalHint) -> Result<Sink, PromptError> {
    if !std::io::stdin().is_terminal() {
        return Err(PromptError::NoTerminal { what, hint });
    }
    Sink::pick().ok_or(PromptError::NoTerminal { what, hint })
}

/// Há como perguntar agora? Quem alimenta o cartão com um GET só deve pagá-lo se a pergunta vai
/// mesmo acontecer; sem terminal o erro é o de hoje (`exige um terminal`), sem requisição extra.
pub fn can_ask() -> bool {
    std::io::stdin().is_terminal() && Sink::pick().is_some()
}

fn default_hint(tool: Tool) -> NoTerminalHint {
    match tool {
        Tool::Mjsv => NoTerminalHint::Token,
        Tool::Mjsvadm => NoTerminalHint::Yes,
    }
}

fn read_line() -> anyhow::Result<String> {
    let mut buf = String::new();
    std::io::stdin().lock().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

/// Uma linha de texto (usuário, código 2FA). No `mjsv` o conselho de script é o `MJSV_TOKEN`; no
/// `mjsvadm`, `--yes`.
pub fn line(tool: Tool, label: &str) -> anyhow::Result<String> {
    let mut sink = need_terminal("esta pergunta", default_hint(tool))?;
    sink.write(label)?;
    read_line()
}

/// Uma senha, sem eco.
pub fn password(tool: Tool, label: &str) -> anyhow::Result<String> {
    let mut sink = need_terminal("a leitura de senha", default_hint(tool))?;
    sink.write(label)?;
    Ok(rpassword::read_password()?)
}

/// Confirmação. Em saída de máquina não há como perguntar, então `--yes` passa a ser obrigatório:
/// o contrário seria um script que apaga recurso sem ninguém ter confirmado nada.
pub fn confirm(tool: Tool, spec: &ConfirmSpec, format: Format) -> anyhow::Result<()> {
    if format.is_machine_readable() {
        return Err(PromptError::NeedsYes {
            question: spec.sentence.clone(),
        }
        .into());
    }

    let mut sink = need_terminal("esta pergunta", NoTerminalHint::Yes)?;
    let style = sink.style(tool);
    // SGR 0 antes de bloquear: nada pintado pode vazar para o que o usuário digita.
    let text = format!("{}{}", spec.render(&style), style.reset());
    sink.write(&text)?;

    let answer = read_line()?;
    spec.check(&answer).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mensagens_de_hoje() {
        let e = PromptError::NoTerminal {
            what: "esta pergunta",
            hint: NoTerminalHint::Token,
        };
        assert_eq!(
            e.to_string(),
            "esta pergunta exige um terminal — em script, autentique com a variável MJSV_TOKEN"
        );
        let e = PromptError::NoTerminal {
            what: "esta pergunta",
            hint: NoTerminalHint::Yes,
        };
        assert_eq!(
            e.to_string(),
            "esta pergunta exige um terminal — em script, passe --yes"
        );
        assert_eq!(PromptError::Cancelled.to_string(), "cancelado");
        assert_eq!(
            PromptError::NeedsYes {
                question: "Apagar?".into()
            }
            .to_string(),
            "Apagar?\nem saída não interativa, confirme com --yes"
        );
    }

    #[test]
    fn saida_de_maquina_exige_yes() {
        let spec = ConfirmSpec::simple("Apagar?");
        let err = confirm(Tool::Mjsv, &spec, Format::Json).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<PromptError>(),
            Some(PromptError::NeedsYes { .. })
        ));
    }
}
