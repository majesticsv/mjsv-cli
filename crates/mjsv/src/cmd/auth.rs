use anyhow::{anyhow, Context, Result};
use mjsv_core::auth::{self, LoginStep};
use mjsv_core::client::{Client, Request};
use mjsv_core::config::{Config, Credentials, TOKEN_ENV};
use mjsv_core::models::{ApiToken, Identity, Listing};
use mjsv_core::output::{cell, cell_date, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;

use crate::cli::{LoginArgs, TokenCommand};
use crate::prompt;

pub async fn login(ctx: &Ctx, args: &LoginArgs) -> Result<()> {
    if args.token {
        return login_with_token(ctx).await;
    }

    let identifier = match &args.username {
        Some(u) => u.clone(),
        None => prompt::line("Usuário ou e-mail: ")?,
    };

    let password = prompt::password("Senha: ")?;

    let identity = match auth::sign_in(&ctx.client, &identifier, &password).await? {
        LoginStep::Done(identity) => *identity,
        LoginStep::NeedsOtp { otp_login_token } => {
            // O 2FA vem como 200 com `requiresOtp`, então este ramo é o caminho normal de uma
            // conta protegida — não um erro.
            let code = prompt::line("Código 2FA (ou código de recuperação): ")?;

            if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
                auth::sign_in_otp(&ctx.client, &otp_login_token, &code).await?
            } else {
                auth::sign_in_recovery(&ctx.client, &otp_login_token, &code).await?
            }
        }
    };

    let token = identity
        .access_token
        .clone()
        .ok_or_else(|| anyhow!("login aceito mas sem accessToken na resposta"))?;

    save_session(&identity, token)?;
    report_login(ctx, &identity)
}

/// `mjsv login --token`: valida a chave antes de gravar.
///
/// Gravar sem validar deixaria o usuário com um arquivo que só falha no próximo comando, com um
/// 401 que parece problema de sessão em vez de chave errada.
async fn login_with_token(ctx: &Ctx) -> Result<()> {
    let token = match std::env::var(TOKEN_ENV).ok().filter(|v| !v.trim().is_empty()) {
        Some(t) => t,
        None => prompt::password("Chave de API: ")?,
    };

    if token.trim().is_empty() {
        return Err(anyhow!("nenhuma chave informada"));
    }

    let client = Client::new(ctx.client.base_url(), Some(token.trim().to_string()))?;
    let identity = auth::verify(&client)
        .await
        .context("a chave informada não foi aceita pela API")?;

    save_session(&identity, token.trim().to_string())?;
    report_login(ctx, &identity)
}

fn save_session(identity: &Identity, token: String) -> Result<()> {
    let creds = Credentials {
        token: Some(token),
        username: identity.username.clone(),
        user_id: identity.id.clone(),
    };
    creds.save()?;

    // A org é gravada só se ainda não houver uma escolhida, para o login não desfazer um
    // `org use` anterior. Conta com uma única org não precisa escolher nada.
    let mut config = Config::load()?;
    if config.org.is_none() {
        if let [only] = identity.orgs.as_slice() {
            config.org = Some(only.id.clone());
            config.save()?;
        }
    }

    Ok(())
}

fn report_login(ctx: &Ctx, identity: &Identity) -> Result<()> {
    if ctx.format.is_machine_readable() {
        // O token nunca sai na saída de máquina: `mjsv login --output json` acabaria em log de CI.
        let value = serde_json::json!({
            "id": identity.id,
            "username": identity.username,
            "orgs": identity.orgs.iter().map(|o| serde_json::json!({"id": o.id, "name": o.name})).collect::<Vec<_>>(),
        });
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!(
        "Autenticado como {} ({})",
        identity.username.as_deref().unwrap_or("?"),
        identity.id.as_deref().unwrap_or("?")
    );

    match identity.orgs.len() {
        0 => println!("Nenhuma organização vinculada a esta conta."),
        1 => println!(
            "Organização: {} ({})",
            identity.orgs[0].name.as_deref().unwrap_or("?"),
            identity.orgs[0].id
        ),
        n => println!("{n} organizações — escolha com `mjsv org use <id>` (veja `mjsv org list`)."),
    }

    Ok(())
}

pub async fn logout(ctx: &Ctx) -> Result<()> {
    if !ctx.client.has_token() {
        println!("Nenhuma sessão local para encerrar.");
        return Ok(());
    }

    // O sign-out do servidor pode falhar (rede caída, token já revogado) e ainda assim a
    // credencial local tem de sumir — deixá-la para trás é pior do que uma linha órfã no banco.
    let req = Request::post("/auth/sign-out").body(serde_json::json!({}));

    if ctx.print_request {
        println!("{}", req.describe(ctx.client.base_url()));
        return Ok(());
    }

    if let Err(e) = ctx.client.send_value(&req).await {
        eprintln!("aviso: o servidor não confirmou o sign-out ({e}); a credencial local foi apagada mesmo assim");
    }

    Credentials::clear()?;
    println!("Sessão encerrada.");

    Ok(())
}

pub async fn whoami(ctx: &Ctx) -> Result<()> {
    let Some(value) = ctx.run(auth::verify_request()).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let identity: Identity = serde_json::from_value(value)?;

    println!("Usuário:      {}", identity.username.as_deref().unwrap_or("-"));
    println!("Nome:         {}", identity.name.as_deref().unwrap_or("-"));
    println!("E-mail:       {}", identity.email.as_deref().unwrap_or("-"));
    println!("Id:           {}", identity.id.as_deref().unwrap_or("-"));
    println!(
        "Organização:  {}",
        ctx.org.as_deref().unwrap_or("(nenhuma selecionada)")
    );
    println!("API:          {}", ctx.client.base_url());

    Ok(())
}

pub async fn token(ctx: &Ctx, cmd: &TokenCommand) -> Result<()> {
    match cmd {
        TokenCommand::List => {
            let req = Request::get("/auth/token").target(Target::root());
            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let listing: Listing<ApiToken> = serde_json::from_value(value)?;

            if listing.data.is_empty() {
                println!("Nenhuma chave de API. Crie uma com `mjsv token create <nome>`.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "nome", "criada em"]);
            for t in &listing.data {
                table.push(vec![
                    t.id.clone(),
                    cell(t.name.as_deref()),
                    cell_date(t.created_at.as_deref()),
                ]);
            }
            print!("{}", table.render());

            Ok(())
        }

        TokenCommand::Create { name } => {
            let req = Request::post("/auth/token")
                .target(Target::root())
                .body(serde_json::json!({ "name": name }));

            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let created: ApiToken = serde_json::from_value(value)?;

            println!("Chave criada: {} ({})", cell(created.name.as_deref()), created.id);
            println!();
            match created.token.as_deref() {
                // O aviso é literal porque é verdade: `GET /auth/token` não devolve mais o
                // segredo, então esta é a única vez que ele aparece.
                Some(secret) => {
                    println!("{secret}");
                    println!();
                    println!("Guarde agora — este valor não é exibido novamente.");
                    println!("Use com: export {TOKEN_ENV}=<valor>");
                }
                None => println!("(a API não devolveu o segredo — verifique a versão do backend)"),
            }

            Ok(())
        }

        TokenCommand::Revoke { id } => {
            if !ctx.assume_yes && !ctx.print_request {
                prompt::confirm(
                    &format!("Revogar a chave {id}? Quem a usa perde o acesso na hora."),
                    ctx.format,
                )?;
            }

            let req = Request::delete(format!("/auth/token/{id}")).target(Target::root());
            let Some(_) = ctx.run(req).await? else { return Ok(()) };

            println!("Chave {id} revogada.");

            Ok(())
        }
    }
}

pub async fn org_list(ctx: &Ctx) -> Result<()> {
    // As orgs do usuário vêm de `/auth/verify`.
    let Some(value) = ctx.run(auth::verify_request()).await? else {
        return Ok(());
    };

    let identity: Identity = serde_json::from_value(value)?;

    if ctx.format.is_machine_readable() {
        let orgs: Vec<_> = identity
            .orgs
            .iter()
            .map(|o| serde_json::json!({"id": o.id, "name": o.name, "current": Some(&o.id) == ctx.org.as_ref()}))
            .collect();
        crate::render_value(ctx.format, &serde_json::Value::Array(orgs));
        return Ok(());
    }

    if identity.orgs.is_empty() {
        println!("Nenhuma organização vinculada a esta conta.");
        return Ok(());
    }

    let mut table = Table::new(&["", "id", "nome"]);
    for o in &identity.orgs {
        let atual = if Some(&o.id) == ctx.org.as_ref() { "*" } else { " " };
        table.push(vec![atual.to_string(), o.id.clone(), cell(o.name.as_deref())]);
    }
    print!("{}", table.render());

    Ok(())
}

pub async fn org_use(ctx: &Ctx, org: &str) -> Result<()> {
    // Confere que a org existe na conta antes de gravar: um id errado no config produziria 403 em
    // todo comando seguinte, com o alvo certo do ponto de vista do CLI.
    let Some(value) = ctx.run(auth::verify_request()).await? else {
        return Ok(());
    };

    let identity: Identity = serde_json::from_value(value)?;

    if !identity.orgs.iter().any(|o| o.id == org) {
        let conhecidas: Vec<String> = identity.orgs.iter().map(|o| o.id.clone()).collect();
        return Err(anyhow!(
            "organização {org} não pertence a esta conta (disponíveis: {})",
            if conhecidas.is_empty() {
                "nenhuma".to_string()
            } else {
                conhecidas.join(", ")
            }
        ));
    }

    let mut config = Config::load()?;
    config.org = Some(org.to_string());
    config.save()?;

    println!("Organização corrente: {org}");

    Ok(())
}
