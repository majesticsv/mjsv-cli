//! Conta do usuário: perfil, sessões e notificações da organização.

use anyhow::{anyhow, Result};
use mjsv_core::client::Request;
use mjsv_core::models::Listing;
use mjsv_core::output::{cell, cell_date, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::{NotificationCommand, ProfileCommand, SessionCommand};
use crate::prompt;

use super::org::with_page;

fn raw(ctx: &Ctx, value: &Value) -> bool {
    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, value);
        return true;
    }
    false
}

pub async fn whoami(ctx: &Ctx, full: bool) -> Result<()> {
    if full {
        return profile(ctx, &ProfileCommand::Show).await;
    }

    super::auth::whoami(ctx).await
}

// ── perfil ──────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Me {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    email: Option<String>,
    /// Centavos.
    #[serde(default)]
    balance: Option<i64>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    otp_enabled: Option<bool>,
    #[serde(default)]
    tax_id_type: Option<String>,
    #[serde(default)]
    legal_name: Option<String>,
    /// O backend só devolve se há documento cadastrado, não o número.
    #[serde(default)]
    has_tax_id: Option<bool>,
    #[serde(default)]
    orgs: Vec<MeOrg>,
}

#[derive(Debug, Deserialize)]
struct MeOrg {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

fn sim_nao(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "sim",
        Some(false) => "não",
        None => "-",
    }
}

fn reais(cents: Option<i64>) -> String {
    match cents {
        Some(c) => {
            let sinal = if c < 0 { "-" } else { "" };
            format!("{sinal}R$ {},{:02}", c.abs() / 100, c.abs() % 100)
        }
        None => "-".into(),
    }
}

fn print_me(ctx: &Ctx, me: &Me) {
    println!("Usuário:         {}", cell(me.username.as_deref()));
    println!("Nome:            {}", cell(me.name.as_deref()));
    println!(
        "E-mail:          {} (verificado: {})",
        cell(me.email.as_deref()),
        sim_nao(me.email_verified)
    );
    println!("Id:              {}", cell(me.id.as_deref()));
    println!("Saldo:           {}", reais(me.balance));
    println!("2FA:             {}", sim_nao(me.otp_enabled));
    println!(
        "Documento:       {} ({})",
        cell(me.tax_id_type.as_deref()),
        if me.has_tax_id == Some(true) {
            "cadastrado"
        } else {
            "não cadastrado"
        }
    );
    println!("Razão social:    {}", cell(me.legal_name.as_deref()));
    println!(
        "Organização:     {}",
        ctx.org.as_deref().unwrap_or("(nenhuma selecionada)")
    );
    println!("API:             {}", ctx.client.base_url());

    if !me.orgs.is_empty() {
        println!();
        let mut table = Table::new(&["org", "nome"]);
        for o in &me.orgs {
            table.push(vec![cell(o.id.as_deref()), cell(o.name.as_deref())]);
        }
        print!("{}", table.render_out());
    }
}

pub async fn profile(ctx: &Ctx, cmd: &ProfileCommand) -> Result<()> {
    match cmd {
        ProfileCommand::Show => {
            let req = Request::get("/user/me").target(Target::root());
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            let me: Me = serde_json::from_value(value)?;
            print_me(ctx, &me);

            Ok(())
        }

        ProfileCommand::Update {
            name,
            tax_id_type,
            tax_id,
            legal_name,
            billing_zip_code,
            billing_street,
            billing_number,
            billing_complement,
            billing_district,
            billing_city_code,
            billing_city_name,
            billing_state,
            billing_phone,
        } => {
            // Só entra no corpo o que foi informado: `""` é valor válido e LIMPA o campo de
            // cobrança, então "ausente" e "vazio" não podem se confundir.
            let campos: [(&str, &Option<String>); 13] = [
                ("name", name),
                ("taxIdType", tax_id_type),
                ("taxId", tax_id),
                ("legalName", legal_name),
                ("billingZipCode", billing_zip_code),
                ("billingStreet", billing_street),
                ("billingNumber", billing_number),
                ("billingComplement", billing_complement),
                ("billingDistrict", billing_district),
                ("billingCityCode", billing_city_code),
                ("billingCityName", billing_city_name),
                ("billingState", billing_state),
                ("billingPhone", billing_phone),
            ];

            let mut body = serde_json::Map::new();
            for (chave, valor) in campos {
                if let Some(v) = valor {
                    body.insert(chave.to_string(), json!(v));
                }
            }

            if body.is_empty() {
                return Err(anyhow!(
                    "nada para atualizar: informe ao menos um campo (veja `mjsv profile update --help`)"
                ));
            }

            let req = Request::put("/user/me")
                .target(Target::root())
                .body(Value::Object(body));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Perfil atualizado.");

            Ok(())
        }
    }
}

// ── sessões ─────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRow {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    ip: Option<String>,
    /// Cliente que fez o login (painel, CLI...).
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

pub async fn session(ctx: &Ctx, cmd: &SessionCommand) -> Result<()> {
    match cmd {
        SessionCommand::List => {
            let req = Request::get("/auth/session").target(Target::root());
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            let listing: Listing<SessionRow> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhuma sessão.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "cliente", "ip", "criada em", "último uso"]);
            for s in &listing.data {
                table.push(vec![
                    cell(s.id.as_deref()),
                    cell(s.service.as_deref()),
                    cell(s.ip.as_deref()),
                    cell_date(s.created_at.as_deref()),
                    cell_date(s.updated_at.as_deref()),
                ]);
            }
            print!("{}", table.render_out());

            Ok(())
        }
    }
}

// ── notificações ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotificationRow {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    is_read: Option<bool>,
    #[serde(default)]
    created_at: Option<String>,
}

pub async fn notification(ctx: &Ctx, cmd: &NotificationCommand) -> Result<()> {
    // Os handlers conferem que o cabeçalho é ORG com o mesmo id do caminho; sem org corrente
    // não há o que mandar.
    let (org, target) = ctx.org_target()?;

    match cmd {
        NotificationCommand::List { kind, unread, page } => {
            let path = match kind {
                Some(k) => format!("/notifications/org/{org}/type/{k}"),
                None => format!("/notifications/org/{org}"),
            };

            let mut req = with_page(Request::get(path).target(target), page)?;
            if *unread {
                req = req.query("filter[isRead]", "false");
            }

            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            let listing: Listing<NotificationRow> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhuma notificação.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "tipo", "lida", "título", "criada em"]);
            for x in &listing.data {
                table.push(vec![
                    cell(x.id.as_deref()),
                    cell(x.kind.as_deref()),
                    sim_nao(x.is_read).to_string(),
                    cell(x.title.as_deref()),
                    cell_date(x.created_at.as_deref()),
                ]);
            }
            print!("{}", table.render_out());

            Ok(())
        }

        NotificationCommand::Count { by_type } => {
            let path = if *by_type {
                format!("/notifications/org/{org}/count-by-type")
            } else {
                format!("/notifications/org/{org}/count")
            };

            let Some(value) = ctx.run(Request::get(path).target(target)).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            match value.as_object() {
                Some(obj) if !obj.is_empty() => {
                    for (k, v) in obj {
                        println!(
                            "{k}: {}",
                            v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())
                        );
                    }
                }
                _ => println!("{value}"),
            }

            Ok(())
        }

        NotificationCommand::Read { id } => {
            let req = Request::patch(format!("/notifications/org/{org}/{id}/read")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Notificação {id} marcada como lida.");

            Ok(())
        }

        NotificationCommand::ReadAll => {
            // Sem corpo.
            let req = Request::patch(format!("/notifications/org/{org}/read-all")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Todas as notificações marcadas como lidas.");

            Ok(())
        }

        NotificationCommand::Delete { id } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(&format!("Apagar a notificação {id}?"), ctx.format)?;
            }

            let req = Request::delete(format!("/notifications/org/{org}/{id}")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Notificação {id} apagada.");

            Ok(())
        }

        NotificationCommand::ClearRead => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm("Apagar todas as notificações já lidas?", ctx.format)?;
            }

            // No backend, `DELETE org/:orgId/:notificationId` é declarada ANTES desta rota e
            // captura `read` como id — o decode do sqid falha e a resposta é 404. Enquanto o
            // handler não for reordenado, esta chamada não chega ao código certo.
            let req = Request::delete(format!("/notifications/org/{org}/read")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Notificações lidas apagadas.");

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saldo_em_centavos_vira_reais() {
        assert_eq!(reais(Some(12345)), "R$ 123,45");
        assert_eq!(reais(Some(5)), "R$ 0,05");
        assert_eq!(reais(Some(-250)), "-R$ 2,50");
        assert_eq!(reais(None), "-");
    }
}
