//! Organização: membros, cargos, convites e serviços cobrados.
//!
//! O `{id}` do caminho é sempre a org, e o cabeçalho de alvo tem de ser `ORG` com esse mesmo valor
//! — por isso tudo que age sobre "a organização corrente" passa por `ctx.org_target()`, que devolve
//! os dois a partir de uma única origem.

use anyhow::{anyhow, Result};
use mjsv_core::client::Request;
use mjsv_core::models::Listing;
use mjsv_core::output::{cell, cell_date, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::{
    OrgCommand, OrgInviteCommand, OrgMemberCommand, OrgRoleCommand, OrgServiceCommand, PageArgs, PermArgs,
};
use crate::prompt;

pub async fn run(ctx: &Ctx, cmd: &OrgCommand) -> Result<()> {
    match cmd {
        // As duas moram em auth.rs: as orgs do usuário vêm de `/auth/verify`.
        OrgCommand::List => super::auth::org_list(ctx).await,
        OrgCommand::Use { org } => super::auth::org_use(ctx, org).await,
        OrgCommand::Create { name } => create(ctx, name).await,
        OrgCommand::Show { org } => show(ctx, org.as_deref()).await,
        OrgCommand::Rename { org, name } => rename(ctx, org, name).await,
        OrgCommand::Delete { org } => delete(ctx, org).await,
        OrgCommand::Leave { org } => leave(ctx, org).await,
        OrgCommand::AuditLog { page } => audit_log(ctx, page).await,
        OrgCommand::Member(sub) => member(ctx, sub).await,
        OrgCommand::Role(sub) => role(ctx, sub).await,
        OrgCommand::Invite(sub) => invite(ctx, sub).await,
        OrgCommand::Service(sub) => service(ctx, sub).await,
    }
}

/// Paginação e filtros viram query só quando informados: a API tem defaults (página 1, 10 itens).
pub(crate) fn with_page(mut req: Request, page: &PageArgs) -> Result<Request> {
    if let Some(p) = page.page {
        req = req.query("page", p);
    }
    if let Some(c) = page.count {
        req = req.query("count", c);
    }
    if let Some(s) = &page.search {
        req = req.query("search", s);
    }
    if let Some(o) = &page.order_by {
        req = req.query("orderBy", o);
    }
    if let Some(o) = &page.order {
        req = req.query("order", o);
    }
    for f in &page.filters {
        let (k, v) = parse_filter(f)?;
        req = req.query(k, v);
    }
    Ok(req)
}

/// `campo=valor` -> `filter[campo]`; `campo[op]=valor` -> `filter[campo][op]`.
fn parse_filter(spec: &str) -> Result<(String, String)> {
    let (key, value) = spec
        .split_once('=')
        .filter(|(k, _)| !k.trim().is_empty())
        .ok_or_else(|| anyhow!("filtro inválido: {spec} (use campo=valor ou campo[op]=valor)"))?;

    let key = key.trim();
    let query_key = match key.split_once('[') {
        Some((campo, op)) => format!("filter[{campo}][{op}"),
        None => format!("filter[{key}]"),
    };

    Ok((query_key, value.to_string()))
}

/// Imprime o valor cru em saída de máquina. Devolve `true` quando já terminou.
fn raw(ctx: &Ctx, value: &Value) -> bool {
    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, value);
        return true;
    }
    false
}

fn n(v: Option<i64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "-".into())
}

// ── organização ─────────────────────────────────────────────────────────────────────────────

async fn create(ctx: &Ctx, name: &str) -> Result<()> {
    // Rota de conta: não pede org corrente, e o painel manda ROOT.
    let req = Request::post("/org")
        .target(Target::root())
        .body(json!({ "name": name }));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if raw(ctx, &value) {
        return Ok(());
    }

    let id = value.get("id").and_then(Value::as_str).unwrap_or("?");
    println!("Organização criada: {name} ({id}). Use com `mjsv org use {id}`.");

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrgView {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    users: Vec<Value>,
}

async fn show(ctx: &Ctx, org: Option<&str>) -> Result<()> {
    let id = match org {
        Some(o) => o.to_string(),
        None => ctx.org_target()?.0,
    };

    let req = Request::get(format!("/org/{id}")).target(Target::org(&id));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if raw(ctx, &value) {
        return Ok(());
    }

    // Org inexistente volta 200 com corpo nulo, não 404.
    if value.is_null() {
        return Err(anyhow!("organização {id} não encontrada"));
    }

    let view: OrgView = serde_json::from_value(value)?;

    println!("Id:        {}", cell(view.id.as_deref()));
    println!("Nome:      {}", cell(view.name.as_deref()));
    println!("Criada em: {}", cell_date(view.created_at.as_deref()));
    println!("Membros:   {}", view.users.len());

    Ok(())
}

async fn rename(ctx: &Ctx, org: &str, name: &str) -> Result<()> {
    let req = Request::put(format!("/org/{org}"))
        .target(Target::org(org))
        .body(json!({ "name": name }));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if raw(ctx, &value) {
        return Ok(());
    }

    println!("Organização {org} renomeada para {name}.");

    Ok(())
}

async fn delete(ctx: &Ctx, org: &str) -> Result<()> {
    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(
            &format!("Apagar a organização {org}? Só o dono consegue e os serviços ativos ficam sem dono."),
            ctx.format,
        )?;
    }

    let req = Request::delete(format!("/org/{org}")).target(Target::org(org));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if raw(ctx, &value) {
        return Ok(());
    }

    println!("Organização {org} apagada.");

    Ok(())
}

async fn leave(ctx: &Ctx, org: &str) -> Result<()> {
    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(
            &format!("Sair da organização {org}? Você perde o acesso até ser convidado de novo."),
            ctx.format,
        )?;
    }

    let req = Request::delete(format!("/org/{org}/leave")).target(Target::org(org));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if raw(ctx, &value) {
        return Ok(());
    }

    println!("Você saiu da organização {org}.");

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuditRow {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

async fn audit_log(ctx: &Ctx, page: &PageArgs) -> Result<()> {
    let (org, target) = ctx.org_target()?;
    let req = with_page(Request::get(format!("/auditlogs/org/{org}")).target(target), page)?;
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if raw(ctx, &value) {
        return Ok(());
    }

    let listing: Listing<AuditRow> = Listing::from_value(value)?;

    if listing.data.is_empty() {
        println!("Nenhum registro de auditoria.");
        return Ok(());
    }

    let mut table = Table::new(&["quando", "ação", "id"]);
    for l in &listing.data {
        table.push(vec![
            cell_date(l.created_at.as_deref()),
            cell(l.action.as_deref()),
            cell(l.id.as_deref()),
        ]);
    }
    print!("{}", table.render());

    Ok(())
}

// ── membros ─────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MemberRow {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

async fn member(ctx: &Ctx, cmd: &OrgMemberCommand) -> Result<()> {
    let (org, target) = ctx.org_target()?;

    match cmd {
        OrgMemberCommand::List { page } => {
            let req = with_page(Request::get(format!("/org/{org}/member")).target(target), page)?;
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            let listing: Listing<MemberRow> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhum membro.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "nome", "usuário", "e-mail"]);
            for m in &listing.data {
                table.push(vec![
                    cell(m.id.as_deref()),
                    cell(m.name.as_deref()),
                    cell(m.username.as_deref()),
                    cell(m.email.as_deref()),
                ]);
            }
            print!("{}", table.render());

            Ok(())
        }

        OrgMemberCommand::Add { email } => {
            // Se o e-mail não tem conta, o backend manda um convite (com limite de envios) em vez
            // de adicionar — por isso a mensagem não afirma que a pessoa já é membro.
            let req = Request::post(format!("/org/{org}/member"))
                .target(target)
                .body(json!({ "userEmail": email }));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Pedido enviado para {email}. Confira com `mjsv org member list` e `mjsv org invite list`.");

            Ok(())
        }

        OrgMemberCommand::Remove { user } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(
                    &format!("Remover o membro {user} da organização {org}? Ele perde o acesso na hora."),
                    ctx.format,
                )?;
            }

            let req = Request::delete(format!("/org/{org}/member/{user}")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Membro {user} removido.");

            Ok(())
        }

        OrgMemberCommand::RoleAdd { user, role } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(
                    &format!("Dar o cargo {role} ao membro {user}? As permissões dele mudam na hora."),
                    ctx.format,
                )?;
            }

            let req = Request::post(format!("/org/{org}/member/{user}/role"))
                .target(target)
                .body(json!({ "roleId": role }));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Cargo {role} atribuído a {user}.");

            Ok(())
        }

        OrgMemberCommand::RoleRemove { user, role } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(
                    &format!("Tirar o cargo {role} do membro {user}? As permissões dele mudam na hora."),
                    ctx.format,
                )?;
            }

            // É um DELETE com corpo JSON: o `roleId` não vai no caminho.
            let req = Request::delete(format!("/org/{org}/member/{user}/role"))
                .target(target)
                .body(json!({ "roleId": role }));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Cargo {role} removido de {user}.");

            Ok(())
        }
    }
}

// ── cargos ──────────────────────────────────────────────────────────────────────────────────

/// Nome -> número da permissão (`UserPermissions`). O bit na máscara é `número - 1`.
///
/// Só as permissões de organização/cliente: as demais existem no backend para outros contextos e
/// não fazem sentido num cargo de cliente.
const PERMISSOES: &[(&str, u32)] = &[
    ("all", 1),
    ("members-list", 2),
    ("members-invite", 3),
    ("data-update", 4),
    ("org-delete", 5),
    ("org-transfer", 6),
    ("org-admin", 7),
    ("ticket-create", 8),
    ("ticket-view", 9),
    ("ticket-close", 10),
    ("order-list", 11),
    ("gs-list", 12),
    ("gs-details", 13),
    ("gs-power", 14),
    ("gs-console", 15),
    ("gs-file", 16),
    ("gs-database", 17),
    ("logs-view", 18),
    ("vm-list", 19),
    ("vm-console", 20),
    ("vm-power", 22),
    ("vm-vpc-manage", 23),
    ("vm-update", 24),
    ("vm-firewall-manage", 25),
];

/// Máscara decimal (string, porque o backend a trata como bigint) a partir de nomes ou da máscara
/// já pronta. `None` quando nenhuma das duas foi informada.
fn permission_mask(perms: &PermArgs) -> Result<Option<String>> {
    if let Some(mask) = &perms.mask {
        if mask.is_empty() || !mask.chars().all(|c| c.is_ascii_digit()) {
            return Err(anyhow!("--mask deve ser um número decimal: {mask}"));
        }
        return Ok(Some(mask.clone()));
    }

    if perms.flags.is_empty() {
        return Ok(None);
    }

    let mut mask: u64 = 0;
    for flag in &perms.flags {
        let normal = flag.trim().to_lowercase().replace('_', "-");
        let (_, num) = PERMISSOES
            .iter()
            .find(|(nome, _)| *nome == normal)
            .ok_or_else(|| anyhow!("permissão desconhecida: {flag} (veja `mjsv org role flags`)"))?;
        mask |= 1u64 << (num - 1);
    }

    Ok(Some(mask.to_string()))
}

#[derive(Debug, Deserialize)]
struct RoleRow {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    position: Option<i64>,
    /// bigint: pode chegar como texto ou número.
    #[serde(default)]
    permissions: Option<Value>,
}

fn value_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => "-".into(),
        Some(other) => other.to_string(),
    }
}

async fn role(ctx: &Ctx, cmd: &OrgRoleCommand) -> Result<()> {
    if let OrgRoleCommand::Flags = cmd {
        let mut table = Table::new(&["nome", "número"]);
        for (nome, num) in PERMISSOES {
            table.push(vec![(*nome).to_string(), num.to_string()]);
        }
        print!("{}", table.render());
        return Ok(());
    }

    let (org, target) = ctx.org_target()?;

    match cmd {
        OrgRoleCommand::Flags => Ok(()),

        OrgRoleCommand::List { page } => {
            let req = with_page(Request::get(format!("/org/{org}/role")).target(target), page)?;
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            let listing: Listing<RoleRow> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhum cargo.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "posição", "nome", "permissões (máscara)"]);
            for r in &listing.data {
                table.push(vec![
                    cell(r.id.as_deref()),
                    n(r.position),
                    cell(r.name.as_deref()),
                    value_text(r.permissions.as_ref()),
                ]);
            }
            print!("{}", table.render());

            Ok(())
        }

        OrgRoleCommand::Create { name, position, perms } => {
            let mask = permission_mask(perms)?.ok_or_else(|| anyhow!("informe as permissões com --flags ou --mask"))?;

            let req = Request::post(format!("/org/{org}/role"))
                .target(target)
                .body(json!({ "name": name, "permissions": mask, "position": position }));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Cargo {name} criado (posição {position}, máscara {mask}).");

            Ok(())
        }

        OrgRoleCommand::Update {
            role,
            name,
            position,
            perms,
        } => {
            let mut body = json!({});
            if let Some(v) = name {
                body["name"] = json!(v);
            }
            if let Some(v) = position {
                body["position"] = json!(v);
            }
            if permission_mask(perms)?.is_some() {
                // O DTO do backend recusa `permissions` neste PUT (conflito entre a conversão para
                // BigInt e a validação de string): melhor avisar aqui do que devolver um 400 opaco.
                return Err(anyhow!(
                    "a API ainda não aceita alterar as permissões de um cargo; apague e recrie o cargo com `org role create`"
                ));
            }

            if body.as_object().is_some_and(|o| o.is_empty()) {
                return Err(anyhow!("nada para atualizar: use --name ou --position"));
            }

            let req = Request::put(format!("/org/{org}/role/{role}"))
                .target(target)
                .body(body);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Cargo {role} atualizado.");

            Ok(())
        }

        OrgRoleCommand::Delete { role } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(
                    &format!("Apagar o cargo {role}? Os membros que o têm perdem essas permissões."),
                    ctx.format,
                )?;
            }

            let req = Request::delete(format!("/org/{org}/role/{role}")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Cargo {role} apagado.");

            Ok(())
        }

        OrgRoleCommand::Swap { role_a, role_b } => {
            let req = Request::put(format!("/org/{org}/positionswap"))
                .target(target)
                .body(json!({ "roleOneId": role_a, "roleTwoId": role_b }));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Posições de {role_a} e {role_b} trocadas.");

            Ok(())
        }
    }
}

// ── convites ────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InviteRow {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    sent_at: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    invited_by: Option<MemberRow>,
}

async fn invite(ctx: &Ctx, cmd: &OrgInviteCommand) -> Result<()> {
    let (org, target) = ctx.org_target()?;

    match cmd {
        OrgInviteCommand::List => {
            let req = Request::get(format!("/org/{org}/invite")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            let listing: Listing<InviteRow> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhum convite pendente.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "e-mail", "enviado em", "expira em", "convidado por"]);
            for i in &listing.data {
                table.push(vec![
                    cell(i.id.as_deref()),
                    cell(i.email.as_deref()),
                    cell_date(i.sent_at.as_deref()),
                    cell_date(i.expires_at.as_deref()),
                    cell(i.invited_by.as_ref().and_then(|u| u.name.as_deref())),
                ]);
            }
            print!("{}", table.render());

            Ok(())
        }

        OrgInviteCommand::Revoke { invite } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(
                    &format!("Revogar o convite {invite}? O link enviado deixa de valer."),
                    ctx.format,
                )?;
            }

            let req = Request::delete(format!("/org/{org}/invite/{invite}")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Convite {invite} revogado.");

            Ok(())
        }
    }
}

// ── serviços ────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceRow {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    cancel_scheduled_at: Option<String>,
}

async fn service(ctx: &Ctx, cmd: &OrgServiceCommand) -> Result<()> {
    let (org, target) = ctx.org_target()?;

    match cmd {
        OrgServiceCommand::List { page } => {
            let req = with_page(Request::get(format!("/org/{org}/service")).target(target), page)?;
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            let listing: Listing<ServiceRow> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhum serviço.");
                return Ok(());
            }

            let mut table = Table::new(&["serviço", "tipo", "status", "nome", "cancela em"]);
            for s in &listing.data {
                table.push(vec![
                    cell(s.id.as_deref()),
                    cell(s.kind.as_deref()),
                    cell(s.status.as_deref()),
                    cell(s.name.as_deref()),
                    cell_date(s.cancel_scheduled_at.as_deref()),
                ]);
            }
            print!("{}", table.render());

            Ok(())
        }

        OrgServiceCommand::Show { service } => {
            let req = Request::get(format!("/org/{org}/service/{service}")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            let s: ServiceRow = serde_json::from_value(value)?;

            println!("Serviço:    {}", cell(s.id.as_deref()));
            println!("Nome:       {}", cell(s.name.as_deref()));
            println!("Tipo:       {}", cell(s.kind.as_deref()));
            println!("Status:     {}", cell(s.status.as_deref()));
            println!("Cancela em: {}", cell_date(s.cancel_scheduled_at.as_deref()));
            println!("(configuração completa: --output json)");

            Ok(())
        }

        OrgServiceCommand::Rename { service, name } => {
            let req = Request::post(format!("/org/{org}/service/{service}/rename"))
                .target(target)
                .body(json!({ "name": name }));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Serviço {service} renomeado para {name}.");

            Ok(())
        }

        OrgServiceCommand::Cancel {
            service, now, reason, ..
        } => {
            if !ctx.assume_yes && !ctx.print_request {
                let pergunta = if *now {
                    format!("Cancelar o serviço {service} AGORA? Os recursos são apagados e não há como desfazer.")
                } else {
                    format!("Cancelar o serviço {service} ao fim do período pago?")
                };
                prompt::confirm(&pergunta, ctx.format)?;
            }

            let mut body = json!({ "mode": if *now { "now" } else { "end_of_period" } });
            if let Some(r) = reason {
                body["reason"] = json!(r);
            }

            let req = Request::post(format!("/org/{org}/service/{service}/cancel"))
                .target(target)
                .body(body);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            if *now {
                println!("Serviço {service} cancelado.");
            } else {
                println!("Cancelamento agendado. Desfaça com `mjsv org service cancel-undo {service}`.");
            }

            Ok(())
        }

        OrgServiceCommand::CancelUndo { service } => {
            let req = Request::post(format!("/org/{org}/service/{service}/cancel/undo")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if raw(ctx, &value) {
                return Ok(());
            }

            println!("Cancelamento do serviço {service} desfeito.");

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(f: &[&str]) -> PermArgs {
        PermArgs {
            flags: f.iter().map(|s| s.to_string()).collect(),
            mask: None,
        }
    }

    #[test]
    fn mascara_usa_o_numero_da_permissao_menos_um() {
        // ALL = 1 -> bit 0; ORG_ADMIN = 7 -> bit 6; VM_FIREWALL_MANAGE = 25 -> bit 24.
        assert_eq!(permission_mask(&flags(&["all"])).unwrap().unwrap(), "1");
        assert_eq!(permission_mask(&flags(&["org-admin"])).unwrap().unwrap(), "64");
        assert_eq!(
            permission_mask(&flags(&["ALL", "VM_FIREWALL_MANAGE"]))
                .unwrap()
                .unwrap(),
            (1u64 + (1 << 24)).to_string()
        );
    }

    #[test]
    fn permissao_desconhecida_e_mascara_invalida_sao_recusadas() {
        assert!(permission_mask(&flags(&["inexistente"])).is_err());
        let ruim = PermArgs {
            flags: vec![],
            mask: Some("0x10".into()),
        };
        assert!(permission_mask(&ruim).is_err());
        assert!(permission_mask(&PermArgs::default()).unwrap().is_none());
    }

    #[test]
    fn filtro_vira_chave_de_query() {
        assert_eq!(parse_filter("type=VPS").unwrap(), ("filter[type]".into(), "VPS".into()));
        assert_eq!(
            parse_filter("createdAt[gte]=2024-01-01").unwrap(),
            ("filter[createdAt][gte]".into(), "2024-01-01".into())
        );
        assert!(parse_filter("semigual").is_err());
    }
}
