//! VPCs (`virt-tenants`) da organização.

use anyhow::Result;
use mjsv_core::client::Request;
use mjsv_core::models::Listing;
use mjsv_core::output::{cell, Table};
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::VpcCommand;
use crate::cmd::vm_extra::{confirmar_spec, estilo_out, vai_mostrar_bloco};
use mjsv_core::style::{sanitize, ConfirmSpec, Tier};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vpc {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    subnet_cidr: Option<String>,
    #[serde(default)]
    region: Option<Region>,
    #[serde(default, rename = "_count")]
    count: Option<Contagem>,
}

#[derive(Debug, Deserialize)]
struct Region {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Contagem {
    #[serde(default)]
    vms: Option<i64>,
    #[serde(default)]
    eips: Option<i64>,
}

fn num(v: Option<i64>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "-".into())
}

async fn list(ctx: &Ctx) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;

    let req = Request::get(format!("/virt-tenants/org/{org_id}")).target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Vpc> = Listing::from_value(value)?;
    if listing.data.is_empty() {
        println!(
            "{}",
            estilo_out(ctx).empty_hint(
                "Nenhuma VPC nesta organização.",
                "crie uma",
                "mjsv vpc create --region <id> --name <nome>"
            )
        );
        return Ok(());
    }

    let mut table = Table::new(&["id", "nome", "região", "sub-rede", "vms", "eips"]);
    for v in &listing.data {
        table.push(vec![
            cell(v.id.as_deref()),
            cell(v.name.as_deref()),
            cell(v.region.as_ref().and_then(|r| r.name.as_deref())),
            cell(v.subnet_cidr.as_deref()),
            num(v.count.as_ref().and_then(|c| c.vms)),
            num(v.count.as_ref().and_then(|c| c.eips)),
        ]);
    }
    print!("{}", table.render_out());

    Ok(())
}

/// Regiões que o cliente pode escolher ao criar uma VPC.
///
/// A rota devolve a linha inteira da região (VNIs incluídos) — a tabela mostra só o que serve para
/// escolher; o JSON sai intacto.
async fn regions(ctx: &Ctx) -> Result<()> {
    let (_org_id, target) = ctx.org_target()?;

    let req = Request::get("/virt-regions").target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Value> = Listing::from_value(value)?;
    if listing.data.is_empty() {
        println!("{}", estilo_out(ctx).empty("Nenhuma região disponível."));
        return Ok(());
    }

    let mut table = Table::new(&["id", "nome"]);
    for r in &listing.data {
        let c = |k: &str| r.get(k).and_then(Value::as_str);
        table.push(vec![cell(c("id")), cell(c("name"))]);
    }
    print!("{}", table.render_out());

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create(
    ctx: &Ctx,
    region: &str,
    name: &str,
    subnet: Option<&str>,
    auto_ipv6: Option<bool>,
    dhcp: Option<bool>,
    dhcp_start: Option<&str>,
    dhcp_end: Option<&str>,
) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;

    let mut body = json!({ "regionId": region, "name": name });
    if let Some(s) = subnet {
        body["subnetCidr"] = json!(s);
    }
    if let Some(v) = auto_ipv6 {
        body["autoIpv6"] = json!(v);
    }
    if let Some(v) = dhcp {
        body["dhcpV4Enabled"] = json!(v);
    }
    if let Some(v) = dhcp_start {
        body["dhcpV4Start"] = json!(v);
    }
    if let Some(v) = dhcp_end {
        body["dhcpV4End"] = json!(v);
    }

    let req = Request::post(format!("/virt-tenants/org/{org_id}"))
        .target(target)
        .body(body);
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
            "VPC criada: {}. Veja com `mjsv vpc list`.",
            cell(value.get("id").and_then(Value::as_str))
        ))
    );

    Ok(())
}

/// A VPC `id` dentro da listagem da organização. Qualquer falha é `None`: o prompt só perde as
/// linhas, apagar nunca trava numa consulta cosmética.
async fn buscar(ctx: &Ctx, org_id: &str, id: &str) -> Option<Vpc> {
    let (_, target) = ctx.org_target().ok()?;
    let req = Request::get(format!("/virt-tenants/org/{org_id}")).target(target);
    let value = ctx.run(req).await.ok().flatten()?;
    let listing: Listing<Vpc> = Listing::from_value(value).ok()?;
    listing.data.into_iter().find(|v| v.id.as_deref() == Some(id))
}

async fn delete(ctx: &Ctx, id: &str) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;

    // A API responde 409 se ainda há VM ou EIP na rede; o aviso é sobre o que some se passar.
    // Nível 2: não há rota de detalhe da VPC, mas a listagem da organização traz nome, sub-rede e
    // quantas VMs/EIPs ainda estão nela: uma consulta só, e só se o bloco vai aparecer.
    let mut spec = ConfirmSpec::new(Tier::T2, format!("Apagar a VPC {id}? Não tem volta."))
        .destructive()
        .head(format!("Apagar a VPC {id}?"))
        .note("Não tem volta.");
    if vai_mostrar_bloco(ctx) {
        if let Some(v) = buscar(ctx, &org_id, id).await {
            spec = spec
                .head(format!("Apagar a VPC {}?", sanitize(v.name.as_deref().unwrap_or(id))))
                .row("VPC", format!("{} ({id})", sanitize(v.name.as_deref().unwrap_or("-"))))
                .row("sub-rede", sanitize(v.subnet_cidr.as_deref().unwrap_or("-")))
                .row("org", sanitize(&org_id));
        }
    }
    confirmar_spec(ctx, &spec)?;

    let req = Request::delete(format!("/virt-tenants/org/{org_id}/{id}")).target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("{}", estilo_out(ctx).done(&format!("VPC {id} apagada.")));

    Ok(())
}

pub async fn run(ctx: &Ctx, cmd: &VpcCommand) -> Result<()> {
    match cmd {
        VpcCommand::List => list(ctx).await,
        VpcCommand::Regions => regions(ctx).await,
        VpcCommand::Create {
            region,
            name,
            subnet,
            auto_ipv6,
            dhcp,
            dhcp_start,
            dhcp_end,
        } => {
            create(
                ctx,
                region,
                name,
                subnet.as_deref(),
                *auto_ipv6,
                *dhcp,
                dhcp_start.as_deref(),
                dhcp_end.as_deref(),
            )
            .await
        }
        VpcCommand::Delete { id } => delete(ctx, id).await,
    }
}
