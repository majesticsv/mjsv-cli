//! Proteção anti-DDoS. Todas as rotas são `/protection/:serviceId/...` com alvo SERVICE — o id é o
//! do serviço de proteção, e o handler confere o cabeçalho contra o caminho (divergência é 400).

use anyhow::Result;
use mjsv_core::client::Request;
use mjsv_core::output::{cell, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::json;

use crate::cli::{ProtectionCommand, ProtectionGameguardCommand};
use crate::cmd::fw::parse_ports;
use crate::cmd::vm_extra::{cartao, confirmar, estilo_out, imprimir_gameguard};
use mjsv_core::style::sanitize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Servico {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    addresses: Vec<Endereco>,
    #[serde(default)]
    telemetry_available: Option<bool>,
    #[serde(default)]
    tunnel_configured: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Endereco {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    region_id: Option<String>,
}

fn sim_nao(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "sim",
        Some(false) => "não",
        None => "-",
    }
}

async fn show(ctx: &Ctx, service: &str) -> Result<()> {
    let req = Request::get(format!("/protection/{service}")).target(Target::service(service));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let s: Servico = serde_json::from_value(value)?;

    let out = estilo_out(ctx);
    let nome = cell(s.name.as_deref());
    let estado = cell(s.status.as_deref());
    cartao(
        &out,
        &format!("Proteção {nome}"),
        &[
            ("Nome", sanitize(&nome)),
            ("Serviço", sanitize(&cell(s.id.as_deref()))),
            ("Estado", out.state(&sanitize(&estado))),
            ("Telemetria", sim_nao(s.telemetry_available).to_string()),
            ("Túnel", sim_nao(s.tunnel_configured).to_string()),
        ],
        // O texto de hoje, com o alinhamento torto de `Telemetria:` que ele sempre teve.
        &[
            format!("Nome:      {nome}"),
            format!("Serviço:   {}", cell(s.id.as_deref())),
            format!("Estado:    {estado}"),
            format!("Telemetria: {}", sim_nao(s.telemetry_available)),
            format!("Túnel:     {}", sim_nao(s.tunnel_configured)),
        ],
    );

    if s.addresses.is_empty() {
        println!("{}", out.empty("Nenhum endereço protegido."));
        return Ok(());
    }

    println!();
    let mut table = Table::new(&["id", "endereço", "região"]);
    for a in &s.addresses {
        table.push(vec![
            cell(a.id.as_deref()),
            cell(a.address.as_deref()),
            cell(a.region_id.as_deref()),
        ]);
    }
    print!("{}", table.render_out());

    Ok(())
}

async fn telemetry(ctx: &Ctx, service: &str, from: u64, to: u64, ip: Option<&str>) -> Result<()> {
    let mut req = Request::get(format!("/protection/{service}/telemetry"))
        .target(Target::service(service))
        .query("from", from)
        .query("to", to);
    if let Some(ip) = ip {
        req = req.query("ip", ip);
    }

    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    // São séries temporais: não há tabela honesta para isso, então a saída é sempre estruturada
    // (a API recorta a janela em 90 dias).
    crate::render_value(ctx.format, &value);

    Ok(())
}

async fn firewall(ctx: &Ctx, service: &str, addr: &str) -> Result<()> {
    let req = Request::get(format!("/protection/{service}/addresses/{addr}/firewall")).target(Target::service(service));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    imprimir_gameguard(&value)
}

async fn gameguard_set(ctx: &Ctx, service: &str, addr: &str, ports: &str, profile: &str) -> Result<()> {
    let (start, end) = parse_ports(ports)?;

    let req = Request::put(format!("/protection/{service}/addresses/{addr}/firewall/gameguard"))
        .target(Target::service(service))
        .body(json!({ "portStart": start, "portEnd": end, "profile": profile }));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    imprimir_gameguard(&value)
}

async fn gameguard_rm(ctx: &Ctx, service: &str, addr: &str, range: &str) -> Result<()> {
    confirmar(
        ctx,
        &format!("Remover a faixa de gameguard {range} do endereço {addr}? A mitigação deixa de valer na hora."),
    )?;

    let req = Request::delete(format!(
        "/protection/{service}/addresses/{addr}/firewall/gameguard/{range}"
    ))
    .target(Target::service(service));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("{}", estilo_out(ctx).done(&format!("Faixa {range} removida.")));

    Ok(())
}

pub async fn run(ctx: &Ctx, cmd: &ProtectionCommand) -> Result<()> {
    match cmd {
        ProtectionCommand::Show { service } => show(ctx, service).await,
        ProtectionCommand::Telemetry { service, from, to, ip } => {
            telemetry(ctx, service, *from, *to, ip.as_deref()).await
        }
        ProtectionCommand::Incidents { service, filter } => {
            crate::cmd::incidents::list(
                ctx,
                &format!("/protection/{service}/incidents"),
                Target::service(service),
                filter,
            )
            .await
        }
        ProtectionCommand::Incident { service, incident } => {
            crate::cmd::incidents::detail(
                ctx,
                &format!("/protection/{service}/incidents/{incident}"),
                Target::service(service),
            )
            .await
        }
        ProtectionCommand::Firewall { service, addr } => firewall(ctx, service, addr).await,
        ProtectionCommand::Gameguard(ProtectionGameguardCommand::Set {
            service,
            addr,
            ports,
            profile,
        }) => gameguard_set(ctx, service, addr, ports, profile).await,
        ProtectionCommand::Gameguard(ProtectionGameguardCommand::Rm { service, addr, range }) => {
            gameguard_rm(ctx, service, addr, range).await
        }
    }
}
