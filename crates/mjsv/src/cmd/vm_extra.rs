//! Operações de VM além do ciclo de energia: imagens, VPC, senhas, SSH e firewall avançado.
//!
//! Tudo é chaveado pelo id do **serviço** da VM, exceto `resolve`, que existe justamente para
//! descobrir o id da própria VM (as rotas de volume pedem esse, não o do serviço).

use anyhow::{anyhow, Result};
use mjsv_core::client::Request;
use mjsv_core::models::{FirewallView, Listing};
use mjsv_core::output::{cell, Table};
use mjsv_core::style::{sanitize, ConfirmSpec, SecretBox, Style, Tier, Tool};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cmd::fw::{format_ports, parse_ports};
use crate::prompt;

/// Estilo de stdout: decidido só por stdout ser um terminal (e pelo formato).
pub fn estilo_out(ctx: &Ctx) -> Style {
    Style::stdout(ctx.format, Tool::Mjsv)
}

/// Estilo de stderr, onde as perguntas aparecem.
pub fn estilo_err(ctx: &Ctx) -> Style {
    Style::stderr(ctx.format, Tool::Mjsv)
}

/// Pergunta antes de operação destrutiva. `--print-request` nunca pergunta: não há nada a
/// destruir, e o dry-run em script precisa passar direto.
pub fn confirmar_spec(ctx: &Ctx, spec: &ConfirmSpec) -> Result<()> {
    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm_spec(spec, ctx.format)?;
    }
    Ok(())
}

/// Nível 1 (CF-1): a frase de hoje atrás de um aviso, sem consulta extra.
pub fn confirmar(ctx: &Ctx, pergunta: &str) -> Result<()> {
    confirmar_spec(ctx, &ConfirmSpec::new(Tier::T1, pergunta))
}

/// Vale a pena buscar o recurso só para enfeitar a pergunta? Só se ela vai mesmo aparecer, num
/// terminal estilizado: em pipe, com `--yes`, `--print-request` ou json/yaml a consulta seria uma
/// ida à rede que ninguém vê.
pub fn vai_mostrar_bloco(ctx: &Ctx) -> bool {
    !ctx.assume_yes && !ctx.print_request && estilo_err(ctx).fancy()
}

/// Nome da VM para as linhas de um prompt de nível 2. Qualquer falha vira `None`: apagar nunca
/// trava numa consulta cosmética, o prompt só perde as linhas.
pub async fn nome_da_vm(ctx: &Ctx, vm: &str) -> Option<String> {
    if !vai_mostrar_bloco(ctx) {
        return None;
    }
    let req = Request::get(format!("/vm/{vm}")).target(Target::vm(vm));
    let value = ctx.run(req).await.ok().flatten()?;
    let nome = value.get("name").or_else(|| value.get("domainName"))?.as_str()?;
    Some(sanitize(nome)).filter(|n| !n.is_empty())
}

/// Cartão de dados (DV-vm-show): régua de título e `Rótulo:   valor` na coluna 0.
///
/// `linhas` com rótulo vazio são continuação do valor anterior (a mensagem de estado), recuada na
/// coluna do valor. `plain` é o texto de hoje, byte a byte, para quando stdout não é um terminal
/// estilizado: as larguras de rótulo de cada comando são diferentes e não podem mudar.
pub fn cartao(out: &Style, titulo: &str, linhas: &[(&str, String)], plain: &[String]) {
    if !out.fancy() {
        for l in plain {
            println!("{l}");
        }
        return;
    }
    for l in out.data_card(titulo, linhas, 0) {
        println!("{l}");
    }
}

/// Caixa de segredo num único `write_all` (ver `Style::print_secret_box`).
pub fn imprimir_caixa(out: &Style, caixa: &SecretBox) -> bool {
    out.print_secret_box(caixa)
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
    let out = estilo_out(ctx);
    let Some(vm) = listing.data.first() else {
        println!("{}", out.empty(&format!("Nenhuma VM para o serviço {service}.")));
        return Ok(());
    };
    let campo = |k: &str| vm.get(k).and_then(Value::as_str);
    let nome = cell(campo("name").or(campo("domainName")));
    let estado = cell(campo("state"));

    cartao(
        &out,
        &format!("VM {nome}"),
        &[
            ("Serviço", sanitize(service)),
            ("VM", sanitize(&cell(campo("id")))),
            ("Nome", sanitize(&nome)),
            ("Estado", out.state(&sanitize(&estado))),
        ],
        &[
            format!("Serviço:  {service}"),
            format!("VM:       {}", cell(campo("id"))),
            format!("Nome:     {nome}"),
            format!("Estado:   {estado}"),
        ],
    );

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
    print!("{}", table.render_out());
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
        println!(
            "{}",
            estilo_out(ctx).empty("Nenhuma imagem disponível na região desta VM.")
        );
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
        println!("{}", estilo_out(ctx).empty("Nenhuma imagem de nuvem disponível."));
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
        println!(
            "{}",
            estilo_out(ctx).empty("Nenhuma outra VPC disponível para esta VM.")
        );
        return Ok(());
    }

    let mut table = Table::new(&["id", "nome", "sub-rede"]);
    for v in &listing.data {
        let c = |k: &str| v.get(k).and_then(Value::as_str);
        table.push(vec![cell(c("id")), cell(c("name")), cell(c("subnetCidr"))]);
    }
    print!("{}", table.render_out());

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
        "{}",
        estilo_out(ctx).done(&format!(
            "VM movida para a VPC {}. Novo IP privado: {}.",
            cell(value.get("tenant").and_then(Value::as_str)),
            cell(value.get("privateIp").and_then(Value::as_str))
        ))
    );

    Ok(())
}

pub async fn rebuild(ctx: &Ctx, vm: &str, image: &str, pedir_senha: bool) -> Result<()> {
    // Nível 2: uma consulta da VM dá o nome para as linhas. A imagem só é conhecida pelo id, e a
    // organização pelo id também (o nome custaria outra consulta): as linhas degradam para eles.
    let nome = nome_da_vm(ctx, vm).await;
    let mut spec = ConfirmSpec::new(
        Tier::T2,
        format!("Reinstalar a VM {vm} com a imagem {image} APAGA o disco e não tem volta. Continuar?"),
    )
    .destructive()
    .head(match &nome {
        Some(n) => format!("Reinstalar a VM {n}?"),
        None => format!("Reinstalar a VM {vm}?"),
    })
    .note("APAGA o disco e não tem volta.");
    if let Some(n) = &nome {
        spec = spec
            .row("VM", format!("{n} ({vm})"))
            .row("imagem", sanitize(image))
            .row("org", ctx.org.as_deref().map(sanitize).unwrap_or_else(|| "-".into()));
    }
    confirmar_spec(ctx, &spec)?;

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
    let out = estilo_out(ctx);
    println!(
        "{}",
        out.accepted(&format!("Reinstalação iniciada. Acompanhe com `mjsv vm show {vm}`."))
    );
    if !pedir_senha {
        println!(
            "{}",
            out.noop(&format!(
                "A senha de root foi gerada: recupere com `mjsv vm root-password {vm}`."
            ))
        );
    }

    Ok(())
}

/// Imprime o segredo uma única vez em stdout. Nunca passa por `tracing`.
///
/// Num terminal com arte vai na caixa de segredo (OK-secret-console, título `SEGREDO`: a senha de
/// root pode ser buscada de novo, mas fica no histórico do terminal); senão, a linha de hoje.
fn imprimir_senha(ctx: &Ctx, vm: &str, value: &Value, rotulo: &str) {
    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, value);
        return;
    }

    let senha = value.get("password").and_then(Value::as_str);
    if let Some(senha) = senha {
        let caixa = SecretBox::new("SEGREDO", "fica no histórico do terminal")
            .secret(sanitize(senha))
            .row("VM", vm);
        if imprimir_caixa(&estilo_out(ctx), &caixa) {
            return;
        }
    }
    println!("{rotulo}: {}", cell(senha));
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

    imprimir_senha(ctx, vm, &value, "Nova senha de root");

    Ok(())
}

pub async fn root_password(ctx: &Ctx, vm: &str) -> Result<()> {
    let req = Request::post(format!("/vm/{vm}/root-password")).target(Target::vm(vm));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    imprimir_senha(ctx, vm, &value, "Senha de root");

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
    let out = estilo_out(ctx);
    if value.get("synced") == Some(&Value::Bool(true)) {
        println!(
            "{}",
            out.done(&format!(
                "IP sincronizado: {}.",
                cell(value.get("ip").and_then(Value::as_str))
            ))
        );
    } else {
        println!("{}", out.noop("Nada sincronizado: a VM não tem interface de rede."));
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
    let token = value.get("token").and_then(Value::as_str);
    let senha = value.get("vncPassword").and_then(Value::as_str);
    if let (Some(t), Some(p)) = (token, senha) {
        let caixa = SecretBox::new("USE AGORA", "o token expira em 60 segundos")
            .captioned("token:", sanitize(t))
            .captioned("senha:", sanitize(p))
            .row("console VNC", format!("VM {vm}"));
        if imprimir_caixa(&estilo_out(ctx), &caixa) {
            return Ok(());
        }
    }
    println!("Token:  {}", cell(token));
    println!("Senha:  {}", cell(senha));

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

    println!("{}", estilo_out(ctx).done(&format!("VM {vm} atualizada.")));

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

    println!(
        "{}",
        estilo_out(ctx).accepted(&format!(
            "Aplicação das chaves enviada (assíncrona). Confira o estado com `mjsv vm show {vm}`."
        ))
    );

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

    println!(
        "{}",
        estilo_out(ctx).done(&format!(
            "Regra {rule_id} atualizada. Confira com `mjsv vm firewall list {vm}`."
        ))
    );

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
    print!("{}", table.render_out());

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

    println!("{}", estilo_out(ctx).done(&format!("Faixa {range_id} removida.")));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mjsv_core::style::{strip_ansi, Charset, Depth};

    fn terminal() -> Style {
        Style::terminal(Tool::Mjsv, Charset::Unicode, Depth::TrueColor, 100)
    }

    #[test]
    fn cartao_alinha_a_mensagem_de_estado_na_coluna_do_valor() {
        let out = terminal();
        let linhas = [
            ("Nome", "db-01".to_string()),
            ("Serviço", "t7Vn3".to_string()),
            ("Estado", out.state("degraded")),
            ("", "disco com erros".to_string()),
            ("SO", "Debian 12".to_string()),
        ];
        let corpo: Vec<String> = out
            .data_card("VM", &linhas, 0)
            .iter()
            .skip(1)
            .map(|l| strip_ansi(l))
            .collect();
        assert_eq!(
            corpo,
            [
                "Nome:     db-01",
                "Serviço:  t7Vn3",
                "Estado:   ▲ degraded",
                "          disco com erros",
                "SO:       Debian 12",
            ]
        );
    }

    #[test]
    fn cartao_fora_do_terminal_nao_tem_regua_nem_marcador() {
        let p = Style::plain(Tool::Mjsv);
        assert!(p.title_rule("VM x").is_none());
        assert_eq!(p.state("running"), "running");
    }

    #[test]
    fn caixa_de_segredo_nao_aparece_sem_terminal() {
        let p = Style::plain(Tool::Mjsv);
        assert!(p.secret_box(&SecretBox::new("SEGREDO", "x").secret("abc")).is_none());
    }
}
