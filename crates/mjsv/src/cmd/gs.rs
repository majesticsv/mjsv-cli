use anyhow::Result;
use mjsv_core::client::Request;
use mjsv_core::models::{GameServer, Listing};
use mjsv_core::output::{cell, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;

use crate::cli::GsCommand;
use crate::cmd::gs_ext;
use crate::prompt;

/// Lista os gameservers da organização.
///
/// Ao contrário de `GET /vm`, que é ROOT-only, `GET /gameserver` aceita escopo `ORG` — o handler
/// ramifica em `targetType === ORG` e lista só os daquela org. A assimetria entre os dois módulos é
/// da API, não do CLI.
async fn list(ctx: &Ctx) -> Result<()> {
    let (_, target) = ctx.org_target()?;

    let Some(value) = ctx.run(Request::get("/gameserver").target(target)).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<GameServer> = serde_json::from_value(value)?;

    if listing.data.is_empty() {
        println!("Nenhum gameserver nesta organização.");
        return Ok(());
    }

    let mut table = Table::new(&["service", "nome", "estado"]);
    for gs in &listing.data {
        table.push(vec![
            cell(gs.service_id.as_deref()),
            cell(gs.name.as_deref()),
            cell(gs.status.as_deref().or(gs.state.as_deref())),
        ]);
    }
    print!("{}", table.render());

    Ok(())
}

async fn show(ctx: &Ctx, id: &str) -> Result<()> {
    let req = Request::get(format!("/gameserver/{id}")).target(Target::gameserver(id));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    crate::render_value(ctx.format, &value);

    Ok(())
}

/// Ações de energia do gameserver.
///
/// **O id NÃO vai no caminho**: `POST /gameserver/power` identifica o servidor exclusivamente pelo
/// cabeçalho `MJSV-TargetId` (o handler compara `req.targetNumberId` com `gameServer.serviceId`) e
/// leva só a ação no corpo. É uma forma diferente da usada em `/vm/:vmServiceId/start`, e a razão
/// de o alvo ser montado aqui mesmo: sem o cabeçalho certo a rota não tem como saber de quem é o
/// pedido.
async fn power(ctx: &Ctx, id: &str, action: &str) -> Result<()> {
    if action == "kill" && !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(
            &format!("`kill` mata o processo do gameserver {id} sem salvar. Continuar?"),
            ctx.format,
        )?;
    }

    let req = Request::post("/gameserver/power")
        .target(Target::gameserver(id))
        .body(serde_json::json!({ "action": action }));

    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("Pedido `{action}` enviado para {id}.");

    Ok(())
}

/// Reinstala o gameserver — mesma forma de `power`: id só no cabeçalho, corpo vazio.
async fn reinstall(ctx: &Ctx, id: &str) -> Result<()> {
    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(
            &format!(
                "Reinstalar {id} reexecuta o script de instalação e pode sobrescrever arquivos do servidor. Continuar?"
            ),
            ctx.format,
        )?;
    }

    let req = Request::post("/gameserver/reinstall").target(Target::gameserver(id));

    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("Reinstalação de {id} iniciada.");

    Ok(())
}

async fn simple_get(ctx: &Ctx, id: &str, suffix: &str) -> Result<()> {
    let req = Request::get(format!("/gameserver/{id}{suffix}")).target(Target::gameserver(id));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    crate::render_value(ctx.format, &value);

    Ok(())
}

pub async fn run(ctx: &Ctx, cmd: &GsCommand) -> Result<()> {
    match cmd {
        GsCommand::List => list(ctx).await,
        GsCommand::Show { id } => show(ctx, id).await,
        GsCommand::Start { id } => power(ctx, id, "start").await,
        GsCommand::Stop { id } => power(ctx, id, "stop").await,
        GsCommand::Restart { id } => power(ctx, id, "restart").await,
        GsCommand::Kill { id } => power(ctx, id, "kill").await,
        GsCommand::Reinstall { id } => reinstall(ctx, id).await,
        GsCommand::Metadata { id } => simple_get(ctx, id, "/metadata").await,
        GsCommand::Databases { id } => simple_get(ctx, id, "/databases").await,
        GsCommand::Schedules { id } => simple_get(ctx, id, "/schedules").await,
        GsCommand::Sftp { id } => simple_get(ctx, id, "/sftpuser").await,
        GsCommand::ByService { service } => gs_ext::by_service(ctx, service).await,
        GsCommand::Startup { id, env } => gs_ext::startup(ctx, id, env).await,
        GsCommand::File(c) => gs_ext::run_file(ctx, c).await,
        GsCommand::Db(c) => gs_ext::run_db(ctx, c).await,
        GsCommand::Schedule(c) => gs_ext::run_schedule(ctx, c).await,
        GsCommand::Addon(c) => gs_ext::run_addon(ctx, c).await,
        GsCommand::Modpack(c) => gs_ext::run_modpack(ctx, c).await,
        GsCommand::Setup { game } => gs_ext::setup(ctx, game).await,
        GsCommand::Fields { game } => gs_ext::fields(ctx, game).await,
        GsCommand::Options { game, field, param } => gs_ext::options(ctx, game, field, param).await,
    }
}
