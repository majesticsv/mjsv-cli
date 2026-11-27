//! Volumes de armazenamento extra.
//!
//! As rotas de volume pedem o id da **VM** (o sqid do recurso), não o id do serviço que o resto da
//! CLI usa. Os comandos recebem o id do serviço e o traduzem com `GET /vm/service/:serviceId`.

use anyhow::Result;
use mjsv_core::client::Request;
use mjsv_core::models::Listing;
use mjsv_core::output::{cell, Table};
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::json;

use crate::cli::VolumeCommand;
use crate::cmd::vm_extra::{confirmar, estilo_out, reais, resolver_vm_id};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Volume {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    size_gb: Option<i64>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    target_dev: Option<String>,
    #[serde(default)]
    vm: Option<VmRef>,
}

#[derive(Debug, Deserialize)]
struct VmRef {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Produto {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    base_price: Option<i64>,
    #[serde(default)]
    size_gb: Option<i64>,
}

fn tabela_volumes(volumes: &[Volume]) {
    let mut table = Table::new(&["id", "nome", "tamanho", "estado", "vm", "dispositivo"]);
    for v in volumes {
        table.push(vec![
            cell(v.id.as_deref()),
            cell(v.name.as_deref()),
            v.size_gb.map(|s| format!("{s} GiB")).unwrap_or_else(|| "-".into()),
            cell(v.state.as_deref()),
            cell(v.vm.as_ref().and_then(|m| m.name.as_deref())),
            cell(v.target_dev.as_deref()),
        ]);
    }
    print!("{}", table.render_out());
}

async fn list(ctx: &Ctx) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;

    let req = Request::get(format!("/storage-volumes/org/{org_id}")).target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Volume> = Listing::from_value(value)?;
    if listing.data.is_empty() {
        println!(
            "{}",
            estilo_out(ctx).empty_hint(
                "Nenhum volume nesta organização.",
                "veja os planos",
                "mjsv volume products --vm <serviço>"
            )
        );
        return Ok(());
    }
    tabela_volumes(&listing.data);

    Ok(())
}

async fn available(ctx: &Ctx, vm_service: &str) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;
    let vm_id = resolver_vm_id(ctx, vm_service).await?;

    let req = Request::get(format!("/storage-volumes/org/{org_id}/available/vm/{vm_id}")).target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Volume> = Listing::from_value(value)?;
    if listing.data.is_empty() {
        println!("{}", estilo_out(ctx).empty("Nenhum volume desanexado no nó desta VM."));
        return Ok(());
    }
    tabela_volumes(&listing.data);

    Ok(())
}

async fn products(ctx: &Ctx, vm_service: &str) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;
    let vm_id = resolver_vm_id(ctx, vm_service).await?;

    let req = Request::get(format!("/storage-products/available/org/{org_id}/vm/{vm_id}")).target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    // Lista vazia também é o que a API devolve quando a VPC da VM não tem região.
    let listing: Listing<Produto> = Listing::from_value(value)?;
    if listing.data.is_empty() {
        println!(
            "{}",
            estilo_out(ctx).empty("Nenhum plano de volume disponível para a região desta VM.")
        );
        return Ok(());
    }

    let mut table = Table::new(&["id", "nome", "tamanho", "preço"]);
    for p in &listing.data {
        table.push(vec![
            cell(p.id.as_deref()),
            cell(p.name.as_deref()),
            p.size_gb.map(|s| format!("{s} GiB")).unwrap_or_else(|| "-".into()),
            reais(p.base_price),
        ]);
    }
    print!("{}", table.render_out());

    Ok(())
}

async fn attach(ctx: &Ctx, volume: &str, vm_service: &str) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;
    let vm_id = resolver_vm_id(ctx, vm_service).await?;

    let req = Request::post(format!("/storage-volumes/org/{org_id}/{volume}/attach"))
        .target(target)
        .body(json!({ "vmId": vm_id }));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!(
        "{}",
        estilo_out(ctx).done(&format!(
            "Volume {volume} anexado a {vm_service} como {}. Monte-o no sistema convidado.",
            cell(value.get("targetDev").and_then(|v| v.as_str()))
        ))
    );

    Ok(())
}

async fn detach(ctx: &Ctx, volume: &str) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;

    confirmar(
        ctx,
        &format!(
            "Desanexar o volume {volume}? Desmonte-o no sistema convidado antes, ou os dados em uso podem corromper."
        ),
    )?;

    let req = Request::post(format!("/storage-volumes/org/{org_id}/{volume}/detach")).target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("{}", estilo_out(ctx).done(&format!("Volume {volume} desanexado.")));

    Ok(())
}

pub async fn run(ctx: &Ctx, cmd: &VolumeCommand) -> Result<()> {
    match cmd {
        VolumeCommand::List => list(ctx).await,
        VolumeCommand::Available { vm } => available(ctx, vm).await,
        VolumeCommand::Products { vm } => products(ctx, vm).await,
        VolumeCommand::Attach { volume, vm } => attach(ctx, volume, vm).await,
        VolumeCommand::Detach { volume } => detach(ctx, volume).await,
    }
}
