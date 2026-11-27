//! Entrada interativa do `mjsv`: um invólucro fino sobre `mjsv_core::prompt`.
//!
//! A lógica (onde a pergunta é escrita, os níveis de confirmação, a leitura do nome do nível 3)
//! mora no core e é a mesma do `mjsvadm`. Aqui só se fixa a ferramenta e se mantém a assinatura
//! que os comandos já usam.

use anyhow::Result;
use mjsv_core::output::Format;
use mjsv_core::style::{ConfirmSpec, Tool};

pub fn line(prompt: &str) -> Result<String> {
    mjsv_core::prompt::line(Tool::Mjsv, prompt)
}

pub fn password(prompt: &str) -> Result<String> {
    mjsv_core::prompt::password(Tool::Mjsv, prompt)
}

/// Confirmação de nível 0: a pergunta de hoje, com `[s/N]`.
///
/// Em saída de máquina `--yes` é obrigatório; sem terminal, é erro em vez de travar.
pub fn confirm(pergunta: &str, format: Format) -> Result<()> {
    confirm_spec(&ConfirmSpec::simple(pergunta), format)
}

/// Confirmação com nível (1 a 3) e linhas de contexto: o que os comandos usam para pedir o bloco
/// de CF-1..CF-3, inclusive o nome digitado do nível 3.
#[allow(dead_code)] // consumido pelos comandos que adotarem os níveis
pub fn confirm_spec(spec: &ConfirmSpec, format: Format) -> Result<()> {
    mjsv_core::prompt::confirm(Tool::Mjsv, spec, format)
}
