//! As linhas de informação da tela inicial (`mjsv` / `mjsvadm` sem subcomando).
//!
//! As contagens só são buscadas com sessão. Qualquer falha — rede, 401, tempo esgotado, formato
//! inesperado — degrada em silêncio para menos linhas: a tela inicial é um cumprimento, nunca um
//! motivo para o programa reclamar.

use std::time::Duration;

use serde_json::Value;

use crate::auth;
use crate::client::{Client, Request};
use crate::models::{Identity, Listing};
use crate::style::{api_host, sanitize, InfoLine, Style};
use crate::target::Target;

/// Os ajudantes abaixo são públicos para o `mjsvadm` montar a tela inicial dele no próprio repositório:
/// a rota de visão geral de infra é só da equipe e não deve morar nesta biblioteca pública.
///
/// Tempo máximo para TODAS as buscas da tela inicial juntas.
pub const BUDGET: Duration = Duration::from_secs(3);

pub async fn get(client: &Client, req: Request) -> Option<Value> {
    client.send_value(&req).await.ok()
}

pub fn count_of(value: Value) -> Option<i64> {
    Listing::<Value>::from_value(value)
        .ok()
        .map(|l| l.count.unwrap_or(l.data.len() as i64))
}

pub fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Linhas para a tela inicial do `mjsv` (linhas 5-7 do banner). `org` é a organização corrente.
pub async fn customer_infos(style: &Style, client: &Client, org: Option<&str>) -> Vec<InfoLine> {
    let host = InfoLine::text(api_host(client.base_url()));

    if !client.has_token() {
        return vec![
            host,
            InfoLine::kv("sessão", "nenhuma"),
            InfoLine::kv("próximo", "mjsv login"),
        ];
    }

    let work = async {
        let identity: Identity = match client.send(&auth::verify_request()).await {
            Ok(i) => i,
            Err(crate::error::ApiError::Unauthorized) => {
                return vec![
                    host.clone(),
                    InfoLine::kv("sessão", "expirada"),
                    InfoLine::kv("próximo", "mjsv login"),
                ];
            }
            Err(_) => return vec![host.clone()],
        };

        let Some(org_id) = org else {
            return vec![
                host.clone(),
                InfoLine::kv("org", "nenhuma selecionada"),
                InfoLine::kv(
                    "próximo",
                    if identity.orgs.is_empty() {
                        "mjsv org create <nome>"
                    } else {
                        "mjsv org use <id>"
                    },
                ),
            ];
        };

        let name = identity
            .orgs
            .iter()
            .find(|o| o.id == org_id)
            .and_then(|o| o.name.clone())
            .map(|n| format!("{n} ({org_id})"))
            .unwrap_or_else(|| org_id.to_string());
        // Nome de org vem da API: um NEL ou ESC racharia a linha do banner.
        let name = sanitize(&name);

        let (vms, gss) = tokio::join!(
            get(
                client,
                Request::get(format!("/org/{org_id}/virtualmachine")).target(Target::org(org_id))
            ),
            get(client, Request::get("/gameserver").target(Target::org(org_id))),
        );
        let (vms, gss) = (vms.and_then(count_of), gss.and_then(count_of));

        match (vms, gss) {
            (Some(0), Some(0)) => vec![
                InfoLine::kv("org", name),
                InfoLine::kv("serviços", "nenhum"),
                InfoLine::kv("próximo", "mjsv product list"),
            ],
            (Some(v), Some(g)) => vec![
                host.clone(),
                InfoLine::kv("org", name),
                InfoLine::text(format!(
                    "{}{}{}",
                    plural(v, "VM", "VMs"),
                    style.sep(),
                    plural(g, "gameserver", "gameservers")
                )),
            ],
            _ => vec![host.clone(), InfoLine::kv("org", name)],
        }
    };

    tokio::time::timeout(BUDGET, work)
        .await
        .unwrap_or_else(|_| vec![InfoLine::text(api_host(client.base_url()))])
}
