//! Configuração e credenciais em disco.
//!
//! São dois arquivos de propósito: `config.toml` é inofensivo (URL da API, org atual, formato) e
//! pode ser versionado ou copiado entre máquinas; `credentials.toml` guarda um JWT de 365 dias e
//! nasce com modo 0600. Misturar os dois convida a copiar o segredo junto com a preferência.
//!
//! Não há keyring do sistema por decisão: quebra em máquina headless, que é o caso de uso
//! principal de um CLI, e custaria três backends de plataforma.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::client::DEFAULT_API_URL;

/// Vence qualquer arquivo. É o que faz o CI nunca precisar escrever credencial em disco.
pub const TOKEN_ENV: &str = "MJSV_TOKEN";
pub const API_URL_ENV: &str = "MJSV_API_URL";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Org corrente (sqid), definida por `mjsv org use`.
    pub org: Option<String>,
    pub api_url: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Credentials {
    pub token: Option<String>,
    /// Só para o `whoami` conseguir dizer de quem é a sessão sem ir à rede.
    pub username: Option<String>,
    pub user_id: Option<String>,
}

pub fn config_dir() -> anyhow::Result<PathBuf> {
    let base = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("não foi possível descobrir o diretório de configuração do usuário"))?;

    Ok(base.join("mjsv"))
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn credentials_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("credentials.toml"))
}

fn load<T: Default + serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(toml::from_str(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}

/// Escreve com 0600 desde a criação.
///
/// O modo é aplicado no `OpenOptions`, e não com um `chmod` depois: entre criar com 0644 e
/// corrigir existe uma janela em que o token é legível por qualquer usuário da máquina.
fn write_private(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;

    Ok(())
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        load(&config_path()?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        write_private(&config_path()?, &toml::to_string_pretty(self)?)
    }

    /// Precedência: `--api-url` (tratado por quem chama) > env > arquivo > produção.
    pub fn resolve_api_url(&self, flag: Option<&str>) -> String {
        flag.map(str::to_string)
            .or_else(|| std::env::var(API_URL_ENV).ok().filter(|v| !v.trim().is_empty()))
            .or_else(|| self.api_url.clone())
            .unwrap_or_else(|| DEFAULT_API_URL.to_string())
    }
}

impl Credentials {
    pub fn load() -> anyhow::Result<Self> {
        load(&credentials_path()?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        write_private(&credentials_path()?, &toml::to_string_pretty(self)?)
    }

    pub fn clear() -> anyhow::Result<()> {
        let path = credentials_path()?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// O env sempre vence o arquivo.
    pub fn resolve_token(&self) -> Option<String> {
        std::env::var(TOKEN_ENV)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| self.token.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_vence_o_arquivo_para_o_token() {
        let creds = Credentials {
            token: Some("do-arquivo".into()),
            ..Default::default()
        };

        std::env::set_var(TOKEN_ENV, "do-ambiente");
        assert_eq!(creds.resolve_token().as_deref(), Some("do-ambiente"));

        // Env vazia não pode mascarar o arquivo — `MJSV_TOKEN=` em um .env acontece.
        std::env::set_var(TOKEN_ENV, "");
        assert_eq!(creds.resolve_token().as_deref(), Some("do-arquivo"));

        std::env::remove_var(TOKEN_ENV);
        assert_eq!(creds.resolve_token().as_deref(), Some("do-arquivo"));
    }

    #[test]
    fn url_cai_para_producao_quando_nada_e_dito() {
        std::env::remove_var(API_URL_ENV);
        let cfg = Config::default();

        assert_eq!(cfg.resolve_api_url(None), DEFAULT_API_URL);
        assert_eq!(
            cfg.resolve_api_url(Some("http://localhost:5000")),
            "http://localhost:5000"
        );
    }

    #[cfg(unix)]
    #[test]
    fn credencial_nasce_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("mjsv-cli-teste-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("credentials.toml");

        write_private(&path, "token = \"segredo\"\n").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "credencial não pode nascer legível por outros");

        std::fs::remove_dir_all(&dir).ok();
    }
}
