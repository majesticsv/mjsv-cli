//! Pedidos, tickets e chaves SSH.

use anyhow::{anyhow, Result};
use mjsv_core::client::Request;
use mjsv_core::models::{Listing, Order, SshKey, Ticket, TicketMessage};
use mjsv_core::output::{cell, cell_date, Table};
use mjsv_core::style::{sanitize, CardError, ConfirmSpec, Style, Tier, Tool};
use mjsv_core::target::Target;
use mjsv_core::Ctx;

use crate::cli::{OrderCommand, SshKeyCommand, TicketCommand};
use crate::cmd::commerce;
use crate::prompt;

// ─────────────────────────── pedidos ───────────────────────────

/// Todas as rotas de pedido são de escopo ORG, inclusive as que trazem o `orderId` no caminho:
/// o cabeçalho leva a organização e o caminho leva o pedido. O handler usa `req.targetId` como
/// filtro de dono, então o par não é decorativo — é o que impede ler o pedido de outra org.
/// Como uma lista de pedidos vazia se apresenta: `order list` sugere o primeiro passo (EM-hint),
/// `order pending` vazio é boa notícia (EM-clear).
#[derive(Clone, Copy)]
enum Vazio {
    Dica,
    TudoCerto,
}

async fn order_list(ctx: &Ctx, path: &str, titulo_vazio: &str, vazio: Vazio) -> Result<()> {
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
        let out = Style::stdout(ctx.format, Tool::Mjsv);
        match vazio {
            Vazio::Dica => println!(
                "{}",
                out.empty_hint(titulo_vazio, "crie um", "mjsv order create --product <id>")
            ),
            Vazio::TudoCerto => println!("{}", out.empty_clear(titulo_vazio)),
        }
        return Ok(());
    }

    // Vírgula só quando a tabela sai estilizada; um pipe segue vendo o ponto de hoje.
    let styled = Style::stdout(ctx.format, Tool::Mjsv).fancy();
    let mut table = Table::new(&["id", "status", "total", "vencimento", "criado"]);
    for o in &listing.data {
        table.push(vec![
            cell(o.id.as_deref()),
            cell(o.status.as_deref()),
            o.valor(styled),
            cell_date(o.due_date.as_deref()),
            cell_date(o.created_at.as_deref()),
        ]);
    }
    print!("{}", table.render_out());

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
        // Nível 1 (CF-1): cancelar interrompe o que aguardava pagamento, sem perda de dados.
        prompt::confirm_spec(
            &ConfirmSpec::new(Tier::T1, format!("Cancelar o pedido {id}?")),
            ctx.format,
        )?;
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

    println!(
        "{}",
        Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Pedido {id} cancelado."))
    );

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

    println!(
        "{}",
        Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Fatura salva em {caminho} ({} bytes).", bytes.len()))
    );

    Ok(())
}

pub async fn order(ctx: &Ctx, cmd: &OrderCommand) -> Result<()> {
    match cmd {
        OrderCommand::List => order_list(ctx, "/order", "Nenhum pedido nesta organização.", Vazio::Dica).await,
        OrderCommand::Pending => order_list(ctx, "/order/pending", "Nenhum pedido pendente.", Vazio::TudoCerto).await,
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
            print!("{}", table.render_out());

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

            println!(
                "{}",
                Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Ticket {id} reaberto."))
            );

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
            println!(
                "{}",
                Style::stdout(ctx.format, Tool::Mjsv)
                    .done(&format!("Ticket {} aberto.", sanitize(&cell(criado.id.as_deref()))))
            );

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

            println!(
                "{}",
                Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Resposta enviada ao ticket {id}."))
            );

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

            println!(
                "{}",
                Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Ticket {id} fechado."))
            );

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
                println!(
                    "{}",
                    Style::stdout(ctx.format, Tool::Mjsv).empty_hint(
                        "Nenhuma chave SSH nesta organização.",
                        "adicione uma",
                        "mjsv ssh-key add --name <nome> --file <chave.pub>"
                    )
                );
                return Ok(());
            }

            // Num terminal ASCII as reticências viram `...`; um pipe mantém o `…` de hoje.
            let estilo = Style::stdout(ctx.format, Tool::Mjsv);
            let reticencias = if estilo.fancy() { estilo.ellipsis() } else { "…" };
            let mut table = Table::new(&["id", "nome", "impressão", "criada"]);
            for k in &listing.data {
                table.push(vec![
                    cell(k.id.as_deref()),
                    cell(k.name.as_deref()),
                    // Resumo em vez da chave inteira: uma ed25519 tem ~70 caracteres de base64 e
                    // estoura a linha sem dizer nada a mais.
                    k.public_key
                        .as_deref()
                        .map(|k| resumo_da_chave(k, reticencias))
                        .unwrap_or_else(|| "-".into()),
                    cell_date(k.created_at.as_deref()),
                ]);
            }
            print!("{}", table.render_out());

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
                let plain = format!(
                    "{file} tem {} linhas — envie a chave PÚBLICA (.pub), que é uma linha só",
                    public_key.lines().count()
                );
                // Uma chave privada tem várias linhas e cai aqui, não no teste abaixo: o cartão
                // ER-ssh-key vale nos dois, e o texto sem terminal continua o de cada ramo.
                return Err(erro_chave_privada(file, &public_key, plain));
            }
            if public_key.starts_with("-----BEGIN") {
                let plain = format!("{file} é uma chave PRIVADA; envie o arquivo .pub correspondente");
                return Err(erro_chave_privada(file, &public_key, plain));
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

            println!(
                "{}",
                Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Chave `{name}` adicionada."))
            );

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

            println!(
                "{}",
                Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Chave {id} removida."))
            );

            Ok(())
        }
    }
}

/// ER-ssh-key: só quando o conteúdo é mesmo uma chave privada E existe `<arquivo>.pub` ao lado
/// (o conserto exato). Sem isso, o erro comum de hoje; `plain` é o texto sem terminal de cada ramo.
fn erro_chave_privada(file: &str, conteudo: &str, plain: String) -> anyhow::Error {
    let pub_file = format!("{file}.pub");
    if !conteudo.starts_with("-----BEGIN") || !std::path::Path::new(&pub_file).is_file() {
        return anyhow!(plain);
    }
    let nome = std::path::Path::new(file)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.to_string());

    CardError::new(
        plain,
        format!("{} é uma chave PRIVADA", mjsv_core::style::sanitize(&nome)),
    )
    .cause("Envie o arquivo .pub correspondente.")
    .hint("tente", format!("mjsv ssh-key add --name <nome> --file {pub_file}"))
    .into()
}

/// `reticencias` é `…` no texto de hoje (pipe) e o que o estilo pedir num terminal (`...` em ASCII).
fn resumo_da_chave(chave: &str, reticencias: &str) -> String {
    let mut partes = chave.split_whitespace();
    let tipo = partes.next().unwrap_or("");
    let corpo = partes.next().unwrap_or("");

    if corpo.len() > 16 {
        format!("{tipo} {reticencias}{}", &corpo[corpo.len() - 12..])
    } else {
        chave.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{erro_chave_privada, resumo_da_chave};

    #[test]
    fn resumo_mostra_o_tipo_e_o_fim_da_chave() {
        let k = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKcFmsr7z8QwEXEMPLO usuario@maquina";
        let r = resumo_da_chave(k, "…");

        assert!(r.starts_with("ssh-ed25519 …"), "{r}");
        assert!(r.ends_with("QwEXEMPLO"), "{r}");
    }

    #[test]
    fn chave_curta_demais_sai_inteira() {
        assert_eq!(resumo_da_chave("ssh-rsa AAAA", "…"), "ssh-rsa AAAA");
    }

    #[test]
    fn chave_privada_so_vira_cartao_com_o_pub_ao_lado() {
        let dir = std::env::temp_dir().join(format!("mjsv-sshkey-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let priv_path = dir.join("id_ed25519");
        let priv_s = priv_path.to_str().unwrap();
        let plain = format!("{priv_s} é uma chave PRIVADA; envie o arquivo .pub correspondente");

        // Sem `.pub` ao lado: erro comum, sem cartão.
        let e = erro_chave_privada(priv_s, "-----BEGIN OPENSSH PRIVATE KEY-----", plain.clone());
        assert!(e.downcast_ref::<mjsv_core::style::CardError>().is_none());

        std::fs::write(dir.join("id_ed25519.pub"), "ssh-ed25519 AAAA").unwrap();
        let e = erro_chave_privada(priv_s, "-----BEGIN OPENSSH PRIVATE KEY-----", plain.clone());
        let card = e.downcast_ref::<mjsv_core::style::CardError>().expect("cartão");
        assert_eq!(card.to_string(), plain);
        assert_eq!(card.title, "id_ed25519 é uma chave PRIVADA");

        // Conteúdo que não é chave privada nunca ganha o cartão.
        let e = erro_chave_privada(priv_s, "ssh-ed25519 AAAA", "x".into());
        assert!(e.downcast_ref::<mjsv_core::style::CardError>().is_none());

        std::fs::remove_dir_all(dir).ok();
    }
}
