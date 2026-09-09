//! Entrada interativa.
//!
//! Todo prompt daqui falha em vez de travar quando não há terminal: um CLI que espera resposta
//! dentro de um pipeline de CI fica pendurado até o timeout do job, e o motivo não aparece em
//! lugar nenhum.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{anyhow, Result};
use mjsv_core::output::Format;

fn exige_terminal(o_que: &str) -> Result<()> {
    if std::io::stdin().is_terminal() {
        return Ok(());
    }

    Err(anyhow!(
        "{o_que} exige um terminal — em script, autentique com a variável MJSV_TOKEN"
    ))
}

pub fn line(prompt: &str) -> Result<String> {
    exige_terminal("esta pergunta")?;

    print!("{prompt}");
    std::io::stdout().flush()?;

    let mut buf = String::new();
    std::io::stdin().lock().read_line(&mut buf)?;

    Ok(buf.trim().to_string())
}

pub fn password(prompt: &str) -> Result<String> {
    exige_terminal("a leitura de senha")?;

    Ok(rpassword::prompt_password(prompt)?)
}

/// Confirmação de operação destrutiva.
///
/// Em saída de máquina não há como perguntar, então `--yes` passa a ser obrigatório: o contrário
/// seria um script que apaga recurso sem ninguém ter confirmado nada.
pub fn confirm(pergunta: &str, format: Format) -> Result<()> {
    if format.is_machine_readable() {
        return Err(anyhow!("{pergunta}\nem saída não interativa, confirme com --yes"));
    }

    let resposta = line(&format!("{pergunta} [s/N] "))?;

    if matches!(resposta.to_lowercase().as_str(), "s" | "sim" | "y" | "yes") {
        return Ok(());
    }

    Err(anyhow!("cancelado"))
}
