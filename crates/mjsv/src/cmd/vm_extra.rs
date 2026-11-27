//! Operações de VM além do ciclo de energia: imagens, VPC, senhas, SSH e firewall avançado.
//!
//! Tudo é chaveado pelo id do **serviço** da VM, exceto `resolve`, que existe justamente para
//! descobrir o id da própria VM (as rotas de volume pedem esse, não o do serviço).

use anyhow::{anyhow, Result};
use mjsv_core::client::Request;
use mjsv_core::models::{FirewallView, Listing};
use mjsv_core::output::{cell, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cmd::fw::{format_ports, parse_ports};
use crate::prompt;

/// Pergunta antes de operação destrutiva. `--print-request` nunca pergunta: não há nada a
/// destruir, e o dry-run em script precisa passar direto.
pub fn confirmar(ctx: &Ctx, pergunta: &str) -> Result<()> {
    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(pergunta, ctx.format)?;
    }
    Ok(())
}

/// `1990` centavos em `R$ 19,90`.
pub fn reais(centavos: Option<i64>) -> String {
    match centavos {
        Some(c) => format!("R$ {},{:02}", c / 100, c % 100),
        None => "-".into(),
    }
}

/// Resolve o id da VM (não o do serviço) via `GET /vm/service/:serviceId`.
///
/// É a única rota de VM com alvo SERVICE, e a resposta é `{data:[vm]}`. Em `--print-request` não
/// há resposta, então devolve um marcador para a requisição seguinte ainda poder ser descrita.
pub async fn resolver_vm_id(ctx: &Ctx, service: &str) -> Result<String> {
    let req = Request::get(format!("/vm/service/{service}")).target(Target::service(service));
    let Some(value) = ctx.run(req).await? else {
        return Ok("<id-da-vm>".into());
    };

    let listing: Listing<Value> = Listing::from_value(value)?;
    listing
        .data
        .first()
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("a API não devolveu a VM do serviço {service}"))
}

pub async fn resolve(ctx: &Ctx, service: &str) -> Result<()> {
    let req = Request::get(format!("/vm/service/{service}")).target(Target::service(service));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Value> = Listing::from_value(value)?;
    let Some(vm) = listing.data.first() else {
        println!("Nenhuma VM para o serviço {service}.");
        return Ok(());
    };
    let campo = |k: &str| vm.get(k).and_then(Value::as_str);

    println!("Serviço:  {service}");
    println!("VM:       {}", cell(campo("id")));
    println!("Nome:     {}", cell(campo("name").or(campo("domainName"))));
    println!("Estado:   {}", cell(campo("state")));

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Imagem {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    fancy_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    renderer: Option<String>,
    #[serde(default)]
    price_monthly: Option<i64>,
    // Sem `path` de propósito: é o caminho interno da imagem no armazenamento, não serve ao
    // cliente e não deve aparecer em tabela.
}

fn tabela_imagens(listing: &Listing<Imagem>, com_slug: bool) {
    let mut cols = vec!["id"];
    if com_slug {
        cols.push("slug");
    }
    cols.extend(["nome", "descrição", "tipo", "preço/mês"]);

    let mut table = Table::new(&cols);
    for i in &listing.data {
        let mut row = vec![cell(i.id.as_deref())];
        if com_slug {
            row.push(cell(i.slug.as_deref()));
        }
        row.extend([
            cell(i.fancy_name.as_deref()),
            cell(i.description.as_deref()),
            cell(i.renderer.as_deref()),
            reais(i.price_monthly),
        ]);
        table.push(row);
    }
    print!("{}", table.render());
}

pub async fn images(ctx: &Ctx, vm: &str) -> Result<()> {
    let req = Request::get(format!("/vm/{vm}/isos")).target(Target::vm(vm));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Imagem> = Listing::from_value(value)?;
    if listing.data.is_empty() {
        println!("Nenhuma imagem disponível na região desta VM.");
        return Ok(());
    }
    tabela_imagens(&listing, false);

    Ok(())
}

/// Catálogo de imagens de nuvem.
///
/// A rota fica sob `/vm/admin/...` por razão histórica de organização do backend, mas NÃO é
/// administrativa: o guard aceita a flag de usuário `VM_LIST` com alvo de organização, e é o que o
/// painel usa para montar o seletor de imagens. Por isso é a única rota `/admin` do CLI público.
pub async fn cloud_images(ctx: &Ctx) -> Result<()> {
    let (_org_id, target) = ctx.org_target()?;

    let req = Request::get("/vm/admin/cloud-images").target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Imagem> = Listing::from_value(value)?;
    if listing.data.is_empty() {
        println!("Nenhuma imagem de nuvem disponível.");
        return Ok(());
    }
    tabela_imagens(&listing, true);

    Ok(())
}

pub async fn vpcs(ctx: &Ctx, vm: &str) -> Result<()> {
    let req = Request::get(format!("/vm/{vm}/vpcs")).target(Target::vm(vm));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Value> = Listing::from_value(value)?;
    if listing.data.is_empty() {
        println!("Nenhuma outra VPC disponível para esta VM.");
        return Ok(());
    }

    let mut table = Table::new(&["id", "nome", "sub-rede"]);
    for v in &listing.data {
        let c = |k: &str| v.get(k).and_then(Value::as_str);
        table.push(vec![cell(c("id")), cell(c("name")), cell(c("subnetCidr"))]);
    }
    print!("{}", table.render());

    Ok(())
}

pub async fn move_vpc(ctx: &Ctx, vm: &str, vpc: &str) -> Result<()> {
    confirmar(
        ctx,
        &format!("Mover a VM {vm} para a VPC {vpc}? O IP privado pode mudar (a API recusa se houver EIP anexado)."),
    )?;

    let req = Request::post(format!("/vm/{vm}/move"))
        .target(Target::vm(vm))
        .body(json!({ "tenantId": vpc }));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!(
        "VM movida para a VPC {}. Novo IP privado: {}.",
        cell(value.get("tenant").and_then(Value::as_str)),
        cell(value.get("privateIp").and_then(Value::as_str))
    );

    Ok(())
}

pub async fn rebuild(ctx: &Ctx, vm: &str, image: &str, pedir_senha: bool) -> Result<()> {
    confirmar(
        ctx,
        &format!("Reinstalar a VM {vm} com a imagem {image} APAGA o disco e não tem volta. Continuar?"),
    )?;

    let mut body = json!({ "isoId": image });
    if pedir_senha && !ctx.print_request {
        body["password"] = json!(prompt::password("Nova senha de root: ")?);
    } else if pedir_senha {
        body["password"] = json!("<senha>");
    }

    let req = Request::post(format!("/vm/{vm}/rebuild"))
        .target(Target::vm(vm))
        .body(body);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    // 202: a reinstalação é assíncrona, o estado da VM é o que mostra o andamento.
    println!("Reinstalação iniciada. Acompanhe com `mjsv vm show {vm}`.");
    if !pedir_senha {
        println!("A senha de root foi gerada: recupere com `mjsv vm root-password {vm}`.");
    }

    Ok(())
}

/// Imprime o segredo uma única vez em stdout. Nunca passa por `tracing`.
fn imprimir_senha(ctx: &Ctx, value: &Value, rotulo: &str) {
    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, value);
        return;
    }

    println!("{rotulo}: {}", cell(value.get("password").and_then(Value::as_str)));
}

pub async fn reset_password(ctx: &Ctx, vm: &str, pedir_senha: bool) -> Result<()> {
    confirmar(
        ctx,
        &format!("Trocar a senha de root da VM {vm}? A senha atual deixa de valer."),
    )?;

    let body = if pedir_senha && !ctx.print_request {
        json!({ "password": prompt::password("Nova senha de root: ")? })
    } else if pedir_senha {
        json!({ "password": "<senha>" })
    } else {
        json!({})
    };

    let req = Request::post(format!("/vm/{vm}/reset-password"))
        .target(Target::vm(vm))
        .body(body);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    imprimir_senha(ctx, &value, "Nova senha de root");

    Ok(())
}

pub async fn root_password(ctx: &Ctx, vm: &str) -> Result<()> {
    let req = Request::post(format!("/vm/{vm}/root-password")).target(Target::vm(vm));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    imprimir_senha(ctx, &value, "Senha de root");

    Ok(())
}

pub async fn sync_dhcp_ip(ctx: &Ctx, vm: &str, ip: Option<&str>) -> Result<()> {
    let mut req = Request::post(format!("/vm/{vm}/sync-dhcp-ip")).target(Target::vm(vm));
    if let Some(ip) = ip {
        req = req.body(json!({ "ip": ip }));
    }

    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    // `synced:false` com `ip:null` é uma VM sem vnic, não uma falha da chamada.
    if value.get("synced") == Some(&Value::Bool(true)) {
        println!("IP sincronizado: {}.", cell(value.get("ip").and_then(Value::as_str)));
    } else {
        println!("Nada sincronizado: a VM não tem interface de rede.");
    }

    Ok(())
}

pub async fn vnc_token(ctx: &Ctx, vm: &str) -> Result<()> {
    let req = Request::post(format!("/vm/{vm}/vnc-token")).target(Target::vm(vm));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    // O token vale para um WebSocket no gateway da API e expira rápido; a senha é a do VNC em
    // texto claro. Os dois são segredo e saem só aqui.
    println!("Token:  {}", cell(value.get("token").and_then(Value::as_str)));
    println!("Senha:  {}", cell(value.get("vncPassword").and_then(Value::as_str)));

    Ok(())
}

pub async fn update(ctx: &Ctx, vm: &str, pedir_vnc: bool) -> Result<()> {
    if !pedir_vnc {
        return Err(anyhow!("nada a alterar: informe --vnc-password"));
    }

    let body = json!({
        "vncPassword": if ctx.print_request {
            json!("<senha>")
        } else {
            json!(prompt::password("Nova senha do VNC: ")?)
        }
    });

    let req = Request::put(format!("/vm/{vm}")).target(Target::vm(vm)).body(body);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("VM {vm} atualizada.");

    Ok(())
}

pub async fn ssh_keys(ctx: &Ctx, vm: &str, keys: &[String], clear: bool) -> Result<()> {
    if keys.is_empty() && !clear {
        return Err(anyhow!("informe ao menos uma --key, ou --clear para remover todas"));
    }

    // A API substitui o conjunto inteiro e ignora em silêncio chaves que não são da organização
    // da VM — daí o aviso de que `[]` apaga tudo.
    confirmar(
        ctx,
        &format!(
            "As chaves SSH da VM {vm} serão SUBSTITUÍDAS por {} chave(s). Continuar?",
            keys.len()
        ),
    )?;

    let req = Request::put(format!("/vm/{vm}/ssh-keys"))
        .target(Target::vm(vm))
        .body(json!({ "sshKeyIds": keys }));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("Aplicação das chaves enviada (assíncrona). Confira o estado com `mjsv vm show {vm}`.");

    Ok(())
}

/// Campos opcionais de `PUT /vm/:id/firewall/rules/:ruleId` — atualização parcial.
pub struct RuleChanges<'a> {
    pub action: Option<&'a str>,
    pub protocol: Option<&'a str>,
    pub ports: Option<&'a str>,
    pub source: Option<&'a str>,
    pub description: Option<&'a str>,
    pub position: Option<u32>,
    pub enabled: Option<bool>,
}

pub async fn rule_update(ctx: &Ctx, vm: &str, rule_id: &str, c: RuleChanges<'_>) -> Result<()> {
    let mut body = json!({});
    if let Some(a) = c.action {
        body["action"] = json!(a);
    }
    if let Some(p) = c.protocol {
        body["protocol"] = json!(p);
    }
    if let Some(ports) = c.ports {
        let (start, end) = parse_ports(ports)?;
        body["portStart"] = json!(start);
        body["portEnd"] = json!(end);
    }
    if let Some(s) = c.source {
        // `null` é "qualquer origem"; `0.0.0.0/0` a API recusa, então a palavra `any` existe
        // para não empurrar o usuário para o valor errado.
        body["sourceCidr"] = if s == "any" || s.is_empty() {
            Value::Null
        } else {
            json!(s)
        };
    }
    if let Some(d) = c.description {
        body["description"] = json!(d);
    }
    if let Some(p) = c.position {
        body["position"] = json!(p);
    }
    if let Some(e) = c.enabled {
        body["enabled"] = json!(e);
    }

    if body.as_object().is_some_and(|o| o.is_empty()) {
        return Err(anyhow!("nada a alterar: informe ao menos um campo da regra"));
    }

    // Uma regra errada pode cortar o acesso à própria VM, e a mudança vale na hora.
    confirmar(
        ctx,
        &format!("Alterar a regra {rule_id} do firewall de {vm}? Isso muda o tráfego que chega ao EIP na hora."),
    )?;

    let req = Request::put(format!("/vm/{vm}/firewall/rules/{rule_id}"))
        .target(Target::vm(vm))
        .body(body);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("Regra {rule_id} atualizada. Confira com `mjsv vm firewall list {vm}`.");

    Ok(())
}

/// Tabela das faixas de gameguard de uma resposta de estado de firewall.
pub fn imprimir_gameguard(value: &Value) -> Result<()> {
    let view: FirewallView = serde_json::from_value(value.clone())?;

    if view.gameguard.is_empty() {
        println!("Nenhuma faixa de gameguard.");
        return Ok(());
    }

    let mut table = Table::new(&["id", "perfil", "portas"]);
    for r in &view.gameguard {
        table.push(vec![cell(r.id.as_deref()), cell(r.profile.as_deref()), format_ports(r)]);
    }
    print!("{}", table.render());

    Ok(())
}

pub async fn gameguard_set(ctx: &Ctx, vm: &str, ports: &str, profile: &str) -> Result<()> {
    let (start, end) = parse_ports(ports)?;

    let req = Request::put(format!("/vm/{vm}/firewall/gameguard"))
        .target(Target::vm(vm))
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

pub async fn gameguard_rm(ctx: &Ctx, vm: &str, range_id: &str) -> Result<()> {
    confirmar(
        ctx,
        &format!("Remover a faixa de gameguard {range_id} de {vm}? A mitigação deixa de valer na hora."),
    )?;

    let req = Request::delete(format!("/vm/{vm}/firewall/gameguard/{range_id}")).target(Target::vm(vm));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("Faixa {range_id} removida.");

    Ok(())
}
