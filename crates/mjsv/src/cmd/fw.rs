//! Firewall e EIPs — as duas coisas são escopadas pela VM, não pela organização.

use anyhow::{anyhow, Result};
use mjsv_core::client::Request;
use mjsv_core::models::{EipView, Envelope, FirewallRule, FirewallView};
use mjsv_core::output::{cell, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;

use crate::cli::{EipCommand, FirewallCommand};
use crate::prompt;

/// Mostra o firewall do EIP da VM.
///
/// O que está no banco é a fonte da verdade, não o arquivo em disco no roteador: os `.filters` em
/// `/etc/xdpfw/rulesets/` são uma **projeção** reconciliada a partir daqui, e editá-los à mão é
/// revertido em poucos segundos. Por isso o CLI escreve pela API e não por SSH.
async fn list(ctx: &Ctx, vm: &str) -> Result<()> {
    let req = Request::get(format!("/vm/{vm}/firewall")).target(Target::vm(vm));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let view: FirewallView = serde_json::from_value(value)?;

    if view.rules.is_empty() {
        println!("Nenhuma regra de firewall nesta VM.");
    } else {
        let mut table = Table::new(&["id", "#", "ação", "proto", "portas", "origem", "descrição"]);
        for rule in &view.rules {
            table.push(vec![
                cell(rule.id.as_deref()),
                rule.position.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
                cell(rule.action.as_deref()),
                cell(rule.protocol.as_deref()),
                format_ports(rule),
                // Origem vazia quer dizer "qualquer" — e "qualquer" não é escrito como
                // `0.0.0.0/0`, que a API recusa.
                rule.source_cidr
                    .as_deref()
                    .filter(|c| !c.is_empty())
                    .unwrap_or("qualquer")
                    .to_string(),
                cell(rule.description.as_deref()),
            ]);
        }
        print!("{}", table.render_out());
    }

    if !view.gameguard.is_empty() {
        println!();
        let mut table = Table::new(&["id", "perfil", "portas"]);
        for range in &view.gameguard {
            table.push(vec![
                cell(range.id.as_deref()),
                cell(range.profile.as_deref()),
                format_ports(range),
            ]);
        }
        println!("gameguard:");
        print!("{}", table.render_out());
    }

    Ok(())
}

pub(crate) fn format_ports(rule: &FirewallRule) -> String {
    match (rule.port_start, rule.port_end) {
        (Some(a), Some(b)) if a == b => a.to_string(),
        (Some(a), Some(b)) => format!("{a}-{b}"),
        (Some(a), None) => a.to_string(),
        _ => "-".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn add(
    ctx: &Ctx,
    vm: &str,
    action: &str,
    protocol: &str,
    ports: &str,
    source: Option<&str>,
    description: Option<&str>,
    position: Option<u32>,
) -> Result<()> {
    let (start, end) = parse_ports(ports)?;

    let mut body = serde_json::json!({
        "action": action,
        "protocol": protocol,
        "portStart": start,
        "portEnd": end,
    });

    if let Some(src) = source {
        body["sourceCidr"] = serde_json::json!(src);
    }
    if let Some(desc) = description {
        body["description"] = serde_json::json!(desc);
    }
    if let Some(pos) = position {
        body["position"] = serde_json::json!(pos);
    }

    let req = Request::post(format!("/vm/{vm}/firewall/rules"))
        .target(Target::vm(vm))
        .body(body);

    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("Regra criada. Confira com `mjsv vm firewall list {vm}`.");

    Ok(())
}

/// Aceita `443` ou `27015-27020`.
pub(crate) fn parse_ports(spec: &str) -> Result<(u32, u32)> {
    let spec = spec.trim();

    let (start, end) = match spec.split_once('-') {
        Some((a, b)) => (a.trim().parse::<u32>()?, b.trim().parse::<u32>()?),
        None => {
            let p = spec.parse::<u32>()?;
            (p, p)
        }
    };

    if start == 0 || end == 0 || start > 65535 || end > 65535 {
        return Err(anyhow!("porta fora da faixa 1-65535: {spec}"));
    }
    if start > end {
        return Err(anyhow!("faixa invertida: {start} é maior que {end}"));
    }

    Ok((start, end))
}

async fn delete(ctx: &Ctx, vm: &str, rule_id: &str) -> Result<()> {
    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(
            &format!("Remover a regra {rule_id} do firewall de {vm}? Isso muda o tráfego que chega ao EIP na hora."),
            ctx.format,
        )?;
    }

    let req = Request::delete(format!("/vm/{vm}/firewall/rules/{rule_id}")).target(Target::vm(vm));

    let Some(_) = ctx.run(req).await? else {
        return Ok(());
    };

    println!("Regra {rule_id} removida.");

    Ok(())
}

/// EIPs da VM.
///
/// A rota usada é `/vm/:vmServiceId/eips`, a de escopo de cliente.
pub async fn eip(ctx: &Ctx, cmd: &EipCommand) -> Result<()> {
    match cmd {
        EipCommand::List { vm } => {
            let req = Request::get(format!("/vm/{vm}/eips")).target(Target::vm(vm));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let view: EipView = serde_json::from_value::<Envelope<EipView>>(value)?.data;

            if view.v4.is_empty() && view.v6.is_empty() {
                println!("Nenhum EIP associado a esta VM.");
                return Ok(());
            }

            if !view.v4.is_empty() {
                let mut table = Table::new(&["endereço", "estado", "ip privado"]);
                for e in &view.v4 {
                    table.push(vec![
                        cell(e.address.as_deref()),
                        cell(e.state.as_deref()),
                        cell(e.attached_private_ip.as_deref()),
                    ]);
                }
                print!("{}", table.render_out());
            }

            if !view.v6.is_empty() {
                println!();
                let mut table = Table::new(&["prefixo", "estado"]);
                for p in &view.v6 {
                    table.push(vec![cell(p.prefix.as_deref()), cell(p.state.as_deref())]);
                }
                print!("{}", table.render_out());

                // Prefixo delegado sem endereço aprendido = o convidado ainda não subiu o v6.
                // Dizer isso é mais útil do que uma seção vazia.
                if view.v6_addresses.is_empty() {
                    println!("(nenhum endereço v6 aprendido ainda dentro do prefixo)");
                } else {
                    println!("endereços v6 aprendidos: {}", view.v6_addresses.join(", "));
                }
            }

            Ok(())
        }
    }
}

pub async fn run(ctx: &Ctx, cmd: &FirewallCommand) -> Result<()> {
    match cmd {
        FirewallCommand::List { vm } => list(ctx, vm).await,
        FirewallCommand::Add {
            vm,
            action,
            protocol,
            ports,
            source,
            description,
            position,
        } => {
            add(
                ctx,
                vm,
                action,
                protocol,
                ports,
                source.as_deref(),
                description.as_deref(),
                *position,
            )
            .await
        }
        FirewallCommand::Delete { vm, rule_id } => delete(ctx, vm, rule_id).await,
        FirewallCommand::RuleUpdate {
            vm,
            rule_id,
            action,
            protocol,
            ports,
            source,
            description,
            position,
            enabled,
        } => {
            crate::cmd::vm_extra::rule_update(
                ctx,
                vm,
                rule_id,
                crate::cmd::vm_extra::RuleChanges {
                    action: action.as_deref(),
                    protocol: protocol.as_deref(),
                    ports: ports.as_deref(),
                    source: source.as_deref(),
                    description: description.as_deref(),
                    position: *position,
                    enabled: *enabled,
                },
            )
            .await
        }
        FirewallCommand::GameguardSet { vm, ports, profile } => {
            crate::cmd::vm_extra::gameguard_set(ctx, vm, ports, profile).await
        }
        FirewallCommand::GameguardRm { vm, range_id } => crate::cmd::vm_extra::gameguard_rm(ctx, vm, range_id).await,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_ports;

    #[test]
    fn porta_unica_vira_faixa_de_um() {
        assert_eq!(parse_ports("443").unwrap(), (443, 443));
    }

    #[test]
    fn faixa_com_hifen() {
        assert_eq!(parse_ports("27015-27020").unwrap(), (27015, 27020));
        assert_eq!(parse_ports(" 80 - 90 ").unwrap(), (80, 90));
    }

    #[test]
    fn recusa_faixa_invertida_e_fora_do_limite() {
        // A API também recusaria, mas com uma mensagem de validador; falhar aqui evita a ida à
        // rede e diz o que está errado.
        assert!(parse_ports("90-80").is_err());
        assert!(parse_ports("0").is_err());
        assert!(parse_ports("70000").is_err());
        assert!(parse_ports("abc").is_err());
    }
}
