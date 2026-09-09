//! O contexto que todo comando recebe: cliente, org corrente, formato e dry-run.

use crate::client::{Client, Request};
use crate::config::{Config, Credentials};
use crate::error::ApiError;
use crate::output::Format;
use crate::target::Target;

pub struct Ctx {
    pub client: Client,
    pub config: Config,
    pub credentials: Credentials,
    pub format: Format,
    pub print_request: bool,
    pub assume_yes: bool,
    /// Org corrente: `--org` > `config.toml` (definida por `mjsv org use`).
    pub org: Option<String>,
}

impl Ctx {
    /// O alvo de organização de um comando de cliente.
    ///
    /// Quase toda rota de cliente é `/org/:id/...`, e o id do cabeçalho tem de ser o mesmo do
    /// caminho. Passar por aqui é o que garante os dois saírem do mesmo lugar.
    pub fn org_target(&self) -> Result<(String, Target), ApiError> {
        let org = self.org.clone().ok_or_else(|| ApiError::Api {
            status: 0,
            message: "nenhuma organização selecionada — rode `mjsv org list` e depois `mjsv org use <id>`".into(),
        })?;

        Ok((org.clone(), Target::org(org)))
    }

    /// Executa, ou só descreve, conforme `--print-request`.
    ///
    /// O dry-run devolve `None` — quem chama decide o que imprimir. Nenhum comando pode montar a
    /// requisição por um caminho e imprimi-la por outro, senão o `--print-request` deixa de provar
    /// o que o comando manda de verdade.
    pub async fn run(&self, req: Request) -> Result<Option<serde_json::Value>, ApiError> {
        if self.print_request {
            println!("{}", req.describe(self.client.base_url()));
            return Ok(None);
        }

        self.client.send_value(&req).await.map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(org: Option<&str>) -> Ctx {
        Ctx {
            client: Client::new("http://localhost:5000", None).unwrap(),
            config: Config::default(),
            credentials: Credentials::default(),
            format: Format::Table,
            print_request: false,
            assume_yes: false,
            org: org.map(str::to_string),
        }
    }

    #[test]
    fn alvo_de_org_usa_o_mesmo_id_do_caminho() {
        let (path_id, target) = ctx(Some("Mjjp5")).org_target().unwrap();

        assert_eq!(path_id, "Mjjp5");
        assert!(target.matches_path_id(&path_id));
    }

    #[test]
    fn sem_org_a_mensagem_diz_o_que_fazer() {
        let err = ctx(None).org_target().unwrap_err();
        assert!(err.to_string().contains("org use"), "{err}");
    }
}
