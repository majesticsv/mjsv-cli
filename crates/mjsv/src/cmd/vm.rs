use anyhow::Result;
use mjsv_core::client::Request;
use mjsv_core::models::{Listing, Vm};
use mjsv_core::output::{cell, cell_date, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;

use crate::cli::VmCommand;
use crate::prompt;

/// Lista as VMs da organização.
///
/// A rota é `/org/:id/virtualmachine`, e NÃO `GET /vm` — essa é ROOT-only
/// (`@SystemFlag(VM_MANAGE_NODES)` mais um `targetId === '0000'`), então um usuário comum toma 403.
async fn list(ctx: &Ctx) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;

    let req = Request::get(format!("/org/{org_id}/virtualmachine")).target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Vm> = serde_json::from_value(value)?;

    if listing.data.is_empty() {
        println!("Nenhuma VM nesta organização.");
        return Ok(());
    }

    let mut table = Table::new(&["service", "nome", "estado", "vcpu", "memória", "so"]);
    for vm in &listing.data {
        table.push(vec![
            // O id de serviço é o que todos os outros comandos recebem, então é a primeira coluna:
            // as rotas `/vm/:vmServiceId` são chaveadas por ele, não pelo id da VM.
            cell(vm.service_id.as_deref()),
            cell(vm.name.as_deref()),
            cell(vm.state.as_deref()),
            vm.vcpu.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            vm.memory_mb.map(format_memoria).unwrap_or_else(|| "-".into()),
            cell(vm.os_name.as_deref()),
        ]);
    }
    print!("{}", table.render_out());

    Ok(())
}

fn format_memoria(mb: i64) -> String {
    if mb >= 1024 && mb % 1024 == 0 {
        format!("{} GiB", mb / 1024)
    } else {
        format!("{mb} MiB")
    }
}

async fn show(ctx: &Ctx, id: &str) -> Result<()> {
    let req = Request::get(format!("/vm/{id}")).target(Target::vm(id));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let vm: Vm = serde_json::from_value(value)?;

    println!("Nome:     {}", cell(vm.name.as_deref()));
    println!("Serviço:  {}", cell(vm.service_id.as_deref()));
    println!("Estado:   {}", cell(vm.state.as_deref()));
    if let Some(msg) = vm.state_msg.as_deref().filter(|m| !m.is_empty()) {
        println!("          {msg}");
    }
    println!("SO:       {}", cell(vm.os_name.as_deref()));
    println!(
        "Recursos: {} vCPU, {}",
        vm.vcpu.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
        vm.memory_mb.map(format_memoria).unwrap_or_else(|| "-".into())
    );
    println!("Criada:   {}", cell_date(vm.created_at.as_deref()));

    Ok(())
}

/// `start`, `stop`, `reboot` e `kill`.
///
/// `stop` é desligamento por ACPI e `kill` é corte de energia — por isso só o segundo pede
/// confirmação: ele pode corromper o sistema de arquivos do convidado, o primeiro não.
async fn power(ctx: &Ctx, action: &str, id: &str, destrutivo: bool) -> Result<()> {
    if destrutivo && !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(
            &format!("`{action}` corta a energia da VM {id} sem avisar o sistema convidado. Continuar?"),
            ctx.format,
        )?;
    }

    let req = Request::post(format!("/vm/{id}/{action}")).target(Target::vm(id));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    // A operação é assíncrona: o control plane reconcilia depois. Dizer "ligada" aqui seria
    // mentira — o que aconteceu foi o pedido ser aceito.
    println!("Pedido `{action}` enviado para {id}. Acompanhe com `mjsv vm power {id}`.");

    Ok(())
}

async fn power_state(ctx: &Ctx, id: &str) -> Result<()> {
    let req = Request::get(format!("/vm/{id}/power")).target(Target::vm(id));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}

async fn metrics(ctx: &Ctx, id: &str) -> Result<()> {
    let req = Request::get(format!("/vm/{id}/metrics")).target(Target::vm(id));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    crate::render_value(ctx.format, &value);

    Ok(())
}

/// Estado do console VNC.
///
/// Não abre nada, e desde 2026-09-15 também não devolve endereço: o `vncIp` saiu da resposta da
/// API porque era o endereço do hipervisor na rede de gerência e não habilitava nada de fora — o
/// VNC não é roteável pela internet. O acesso de verdade é pelo painel, que pega um token de uso
/// único e abre um WebSocket contra a própria API. Sobra aqui o que dá para responder honestamente
/// da linha de comando: se a VM tem console atribuído e em que porta, no nó.
async fn console(ctx: &Ctx, id: &str) -> Result<()> {
    let req = Request::get(format!("/vm/{id}/vnc")).target(Target::vm(id));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    crate::render_value(ctx.format, &value);

    Ok(())
}

pub async fn run(ctx: &Ctx, cmd: &VmCommand) -> Result<()> {
    match cmd {
        VmCommand::List => list(ctx).await,
        VmCommand::Show { id } => show(ctx, id).await,
        VmCommand::Start { id } => power(ctx, "start", id, false).await,
        VmCommand::Stop { id } => power(ctx, "stop", id, false).await,
        VmCommand::Reboot { id } => power(ctx, "reboot", id, false).await,
        VmCommand::Kill { id } => power(ctx, "kill", id, true).await,
        VmCommand::Power { id } => power_state(ctx, id).await,
        VmCommand::Metrics { id } => metrics(ctx, id).await,
        VmCommand::Console { id } => console(ctx, id).await,
        VmCommand::Firewall(sub) => crate::cmd::fw::run(ctx, sub).await,
        VmCommand::Resolve { id } => crate::cmd::vm_extra::resolve(ctx, id).await,
        VmCommand::Incidents { id, filter } => {
            crate::cmd::incidents::list(ctx, &format!("/vm/{id}/incidents"), Target::vm(id), filter).await
        }
        VmCommand::Incident { id, incident } => {
            crate::cmd::incidents::detail(ctx, &format!("/vm/{id}/incidents/{incident}"), Target::vm(id)).await
        }
        VmCommand::Images { id } => crate::cmd::vm_extra::images(ctx, id).await,
        VmCommand::CloudImages => crate::cmd::vm_extra::cloud_images(ctx).await,
        VmCommand::Vpcs { id } => crate::cmd::vm_extra::vpcs(ctx, id).await,
        VmCommand::Move { id, vpc } => crate::cmd::vm_extra::move_vpc(ctx, id, vpc).await,
        VmCommand::Rebuild { id, image, password } => crate::cmd::vm_extra::rebuild(ctx, id, image, *password).await,
        VmCommand::ResetPassword { id, password } => crate::cmd::vm_extra::reset_password(ctx, id, *password).await,
        VmCommand::RootPassword { id } => crate::cmd::vm_extra::root_password(ctx, id).await,
        VmCommand::SyncDhcpIp { id, ip } => crate::cmd::vm_extra::sync_dhcp_ip(ctx, id, ip.as_deref()).await,
        VmCommand::VncToken { id } => crate::cmd::vm_extra::vnc_token(ctx, id).await,
        VmCommand::Update { id, vnc_password } => crate::cmd::vm_extra::update(ctx, id, *vnc_password).await,
        VmCommand::SshKeys { id, keys, clear } => crate::cmd::vm_extra::ssh_keys(ctx, id, keys, *clear).await,
    }
}
