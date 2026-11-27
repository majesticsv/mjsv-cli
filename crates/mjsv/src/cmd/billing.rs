//! Pedidos, tickets e chaves SSH.

use anyhow::{anyhow, Result};
use mjsv_core::client::Request;
use mjsv_core::models::{Listing, Order, SshKey, Ticket, TicketMessage};
use mjsv_core::output::{cell, cell_date, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;

use crate::cli::{OrderCommand, SshKeyCommand, TicketCommand};
use crate::cmd::commerce;
use crate::prompt;

// ─────────────────────────── pedidos ───────────────────────────

/// Todas as rotas de pedido são de escopo ORG, inclusive as que trazem o `orderId` no caminho:
/// o cabeçalho leva a organização e o caminho leva o pedido. O handler usa `req.targetId` como
/// filtro de dono, então o par não é decorativo — é o que impede ler o pedido de outra org.
async fn order_list(ctx: &Ctx, path: &str, titulo_vazio: &str) -> Result<()> {
    let (_, target) = ctx.org_target()?;

    let Some(value) = ctx.run(Request::get(path).target(target)).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let listing: Listing<Order> = Listing::from_value(value)?;

    if listing.data.is_empty() {
        println!("{titulo_vazio}");
        return Ok(());
    }

    let mut table = Table::new(&["id", "status", "total", "vencimento", "criado"]);
    for o in &listing.data {
        table.push(vec![
            cell(o.id.as_deref()),
            cell(o.status.as_deref()),
            o.valor_formatado(),
            cell_date(o.due_date.as_deref()),
            cell_date(o.created_at.as_deref()),
        ]);
    }
    print!("{}", table.render());

    Ok(())
}

async fn order_show(ctx: &Ctx, id: &str) -> Result<()> {
    let (_, target) = ctx.org_target()?;

    let Some(value) = ctx.run(Request::get(format!("/order/{id}")).target(target)).await? else {
        return Ok(());
    };

    crate::render_value(ctx.format, &value);

    Ok(())
}

async fn order_cancel(ctx: &Ctx, id: &str) -> Result<()> {
    let (_, target) = ctx.org_target()?;

    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(&format!("Cancelar o pedido {id}?"), ctx.format)?;
    }

    let Some(value) = ctx
        .run(Request::post(format!("/order/{id}/cancel")).target(target))
        .await?
    else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("Pedido {id} cancelado.");

    Ok(())
}

/// Baixa o PDF da fatura.
///
/// Vai pelo `client.download` e não pelo caminho normal: o `send_value` lê a resposta como texto e
/// tenta parsear JSON, o que transformaria o PDF em uma string reinterpretada como UTF-8 — o
/// arquivo sairia gravado e não abriria.
///
/// `via_post` escolhe a rota: `GET invoice-pdf` (normal) ou `POST generate-pdf`. Os dois handlers
/// chamam o mesmo gerador e devolvem o mesmo PDF; a única diferença é o verbo (o POST responde 201).
async fn order_invoice(ctx: &Ctx, id: &str, destino: Option<&str>, via_post: bool) -> Result<()> {
    let (_, target) = ctx.org_target()?;
    let req = if via_post {
        Request::post(format!("/order/{id}/generate-pdf"))
    } else {
        Request::get(format!("/order/{id}/invoice-pdf"))
    }
    .target(target);

    if ctx.print_request {
        println!("{}", req.describe(ctx.client.base_url()));
        return Ok(());
    }

    let bytes = ctx.client.download(&req).await?;

    if bytes.is_empty() {
        return Err(anyhow!("a API devolveu um PDF vazio para o pedido {id}"));
    }

    let caminho = destino
        .map(str::to_string)
        .unwrap_or_else(|| format!("fatura-{id}.pdf"));
    std::fs::write(&caminho, &bytes)?;

    println!("Fatura salva em {caminho} ({} bytes).", bytes.len());

    Ok(())
}

pub async fn order(ctx: &Ctx, cmd: &OrderCommand) -> Result<()> {
    match cmd {
        OrderCommand::List => order_list(ctx, "/order", "Nenhum pedido nesta organização.").await,
        OrderCommand::Pending => order_list(ctx, "/order/pending", "Nenhum pedido pendente.").await,
        OrderCommand::Show { id } => order_show(ctx, id).await,
        OrderCommand::Cancel { id } => order_cancel(ctx, id).await,
        OrderCommand::Invoice { id, output } => order_invoice(ctx, id, output.as_deref(), false).await,
        OrderCommand::GeneratePdf { id, output } => order_invoice(ctx, id, output.as_deref(), true).await,
        OrderCommand::Delete { id } => commerce::order_delete(ctx, id).await,
        OrderCommand::Create(args) => commerce::order_create(ctx, args, false).await,
        OrderCommand::Quote(args) => commerce::order_create(ctx, args, true).await,
        OrderCommand::Group { orders } => commerce::order_group(ctx, orders).await,
        OrderCommand::Pay {
            id,
            method,
            use_balance,
        } => commerce::order_pay(ctx, id, method, *use_balance).await,
        OrderCommand::Coupon { code } => commerce::coupon(ctx, code).await,
        OrderCommand::Metadata => commerce::metadata(ctx, "/order/metadata").await,
    }
}

// ─────────────────────────── tickets ───────────────────────────

/// Os tickets misturam DOIS escopos, e é a única família da API que faz isso:
///
/// - listar, abrir e ver um ticket usam **ORG** (o ticket pertence à organização);
/// - ler mensagens, responder e fechar usam **TICKET**, com o id do próprio ticket no cabeçalho.
///
/// Mandar ORG onde se espera TICKET não dá 403: o handler cai no `return new
/// BadRequestException(...)` — devolvido, não lançado —, o que vira um 200 com corpo de erro. Ou
/// seja, o comando "funcionaria" e não faria nada. Por isso cada subcomando aqui declara o alvo
/// explicitamente.
pub async fn ticket(ctx: &Ctx, cmd: &TicketCommand) -> Result<()> {
    match cmd {
        TicketCommand::List {
            category,
            service,
            page,
        } => {
            // As três listagens são ORG: o guard delas resolve a org pelo cabeçalho e filtra por
            // ela, então um id de categoria/serviço de outra org volta como lista vazia, não 403.
            let (_, target) = ctx.org_target()?;
            let path = match (category, service) {
                (Some(c), _) => format!("/ticket/category/{c}"),
                (None, Some(s)) => format!("/ticket/service/{s}"),
                (None, None) => "/ticket".to_string(),
            };
            let req = Request::get(path).target(target);
            let req = crate::cmd::org::with_page(req, page)?;

            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let listing: Listing<Ticket> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhum ticket encontrado.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "título", "status", "categoria"]);
            for t in &listing.data {
                table.push(vec![
                    cell(t.id.as_deref()),
                    cell(t.title.as_deref()),
                    cell(t.status.as_deref()),
                    cell(t.category.as_ref().and_then(|c| c.name.as_deref())),
                ]);
            }
            print!("{}", table.render());

            Ok(())
        }

        TicketCommand::Message { id } => {
            // Escopo ORG, ao contrário de `ticket messages`: aqui o id é da mensagem, e o guard
            // só consegue resolver a org (a mensagem de outra org volta 404).
            let (_, target) = ctx.org_target()?;
            let req = Request::get(format!("/ticket/message/{id}")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let listing: Listing<TicketMessage> = Listing::from_value(value)?;

            let Some(m) = listing.data.first() else {
                println!("Mensagem não encontrada.");
                return Ok(());
            };

            let autor = m
                .author
                .as_ref()
                .and_then(|a| a.name.as_deref())
                .or(m.author_id.as_deref())
                .unwrap_or("desconhecido");
            println!("Autor:   {autor}");
            println!("Data:    {}", cell_date(m.created_at.as_deref()));
            println!("\n{}", m.content.as_deref().unwrap_or("").trim());

            Ok(())
        }

        TicketCommand::Reopen { id } => {
            // Escopo TICKET com o próprio id; o guard resolve a org do ticket e confere o papel lá.
            let req = Request::put(format!("/ticket/{id}/open")).target(Target::ticket(id));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            println!("Ticket {id} reaberto.");

            Ok(())
        }

        TicketCommand::Categories => {
            let (_, target) = ctx.org_target()?;
            let Some(value) = ctx.run(Request::get("/ticket/category").target(target)).await? else {
                return Ok(());
            };

            crate::render_value(ctx.format, &value);

            Ok(())
        }

        TicketCommand::Show { id } => {
            let (_, target) = ctx.org_target()?;
            let Some(value) = ctx.run(Request::get(format!("/ticket/{id}")).target(target)).await? else {
                return Ok(());
            };

            crate::render_value(ctx.format, &value);

            Ok(())
        }

        TicketCommand::Messages { id } => {
            // Escopo TICKET, não ORG.
            let req = Request::get(format!("/ticket/{id}/messages")).target(Target::ticket(id));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let listing: Listing<TicketMessage> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhuma mensagem neste ticket.");
                return Ok(());
            }

            for m in &listing.data {
                let autor = m
                    .author
                    .as_ref()
                    .and_then(|a| a.name.as_deref())
                    .or(m.author_id.as_deref())
                    .unwrap_or("desconhecido");
                let quem = if m.is_staff.unwrap_or(false) {
                    "suporte"
                } else {
                    "cliente"
                };

                println!("── {} ({quem}) · {}", autor, cell_date(m.created_at.as_deref()));
                println!("{}\n", m.content.as_deref().unwrap_or("").trim());
            }

            Ok(())
        }

        TicketCommand::Open {
            title,
            message,
            category,
            service,
        } => {
            let (_, target) = ctx.org_target()?;

            let mut body = serde_json::json!({
                "title": title,
                "initialMessage": message,
                "categoryId": category,
            });
            if let Some(s) = service {
                body["serviceId"] = serde_json::json!(s);
            }

            let Some(value) = ctx.run(Request::post("/ticket").target(target).body(body)).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let criado: Ticket = serde_json::from_value(value)?;
            println!("Ticket {} aberto.", cell(criado.id.as_deref()));

            Ok(())
        }

        TicketCommand::Reply { id, message } => {
            let req = Request::post(format!("/ticket/{id}/message"))
                .target(Target::ticket(id))
                .body(serde_json::json!({ "content": message }));

            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            println!("Resposta enviada ao ticket {id}.");

            Ok(())
        }

        TicketCommand::Close { id } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(&format!("Fechar o ticket {id}?"), ctx.format)?;
            }

            let req = Request::put(format!("/ticket/{id}/close")).target(Target::ticket(id));
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            println!("Ticket {id} fechado.");

            Ok(())
        }
    }
}

// ─────────────────────────── chaves SSH ───────────────────────────

/// As chaves são da organização, não do usuário — quem tem `VM_UPDATE` na org adiciona e remove.
pub async fn ssh_key(ctx: &Ctx, cmd: &SshKeyCommand) -> Result<()> {
    let (org_id, target) = ctx.org_target()?;

    match cmd {
        SshKeyCommand::List => {
            let req = Request::get(format!("/ssh-keys/org/{org_id}")).target(target);
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let listing: Listing<SshKey> = Listing::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhuma chave SSH nesta organização.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "nome", "impressão", "criada"]);
            for k in &listing.data {
                table.push(vec![
                    cell(k.id.as_deref()),
                    cell(k.name.as_deref()),
                    // Resumo em vez da chave inteira: uma ed25519 tem ~70 caracteres de base64 e
                    // estoura a linha sem dizer nada a mais.
                    k.public_key
                        .as_deref()
                        .map(resumo_da_chave)
                        .unwrap_or_else(|| "-".into()),
                    cell_date(k.created_at.as_deref()),
                ]);
            }
            print!("{}", table.render());

            Ok(())
        }

        SshKeyCommand::Add { name, file } => {
            let public_key = std::fs::read_to_string(file)
                .map_err(|e| anyhow!("não foi possível ler {file}: {e}"))?
                .trim()
                .to_string();

            // A API recusa qualquer coisa com quebra de linha de propósito: a chave é escrita
            // literalmente no cloud-init da VM, e um `\n` no comentário injetaria YAML. Barrar aqui
            // evita a ida à rede e diz o motivo.
            if public_key.lines().count() != 1 {
                return Err(anyhow!(
                    "{file} tem {} linhas — envie a chave PÚBLICA (.pub), que é uma linha só",
                    public_key.lines().count()
                ));
            }
            if public_key.starts_with("-----BEGIN") {
                return Err(anyhow!(
                    "{file} é uma chave PRIVADA; envie o arquivo .pub correspondente"
                ));
            }

            let req = Request::post(format!("/ssh-keys/org/{org_id}"))
                .target(target)
                .body(serde_json::json!({ "name": name, "publicKey": public_key }));

            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            println!("Chave `{name}` adicionada.");

            Ok(())
        }

        SshKeyCommand::Delete { id } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(
                    &format!("Remover a chave {id}? VMs já provisionadas mantêm a chave que receberam."),
                    ctx.format,
                )?;
            }

            let req = Request::delete(format!("/ssh-keys/org/{org_id}/{id}")).target(target);
            let Some(_) = ctx.run(req).await? else {
                return Ok(());
            };

            println!("Chave {id} removida.");

            Ok(())
        }
    }
}

fn resumo_da_chave(chave: &str) -> String {
    let mut partes = chave.split_whitespace();
    let tipo = partes.next().unwrap_or("");
    let corpo = partes.next().unwrap_or("");

    if corpo.len() > 16 {
        format!("{tipo} …{}", &corpo[corpo.len() - 12..])
    } else {
        chave.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::resumo_da_chave;

    #[test]
    fn resumo_mostra_o_tipo_e_o_fim_da_chave() {
        let k = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKcFmsr7z8QwEXEMPLO usuario@maquina";
        let r = resumo_da_chave(k);

        assert!(r.starts_with("ssh-ed25519 …"), "{r}");
        assert!(r.ends_with("QwEXEMPLO"), "{r}");
    }

    #[test]
    fn chave_curta_demais_sai_inteira() {
        assert_eq!(resumo_da_chave("ssh-rsa AAAA"), "ssh-rsa AAAA");
    }
}
