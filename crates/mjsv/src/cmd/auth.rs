use anyhow::{anyhow, Result};
use mjsv_core::auth::{self, LoginStep};
use mjsv_core::client::{Client, Request};
use mjsv_core::config::{Config, Credentials, TOKEN_ENV};
use mjsv_core::error::{LoginRefused, LoginStage, OrgNotInAccount, TokenRefused};
use mjsv_core::models::{ApiToken, Identity, Listing};
use mjsv_core::output::{cell, cell_date, Table};
use mjsv_core::style::{sanitize, ConfirmSpec, LoginView, Marker, OrgSummary, SecretBox, Style, Tier, Tool};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use std::io::Write;

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

    // Um 401 aqui não é "sessão expirada": quem faz login ainda não tem sessão. É usuário, senha ou
    // código que não conferem, e o cartão precisa dizer qual dos dois.
    let step = auth::sign_in(&ctx.client, &identifier, &password)
        .await
        .map_err(|e| LoginRefused::from_unauthorized(e, LoginStage::Password))?;

    let identity = match step {
        LoginStep::Done(identity) => *identity,
        LoginStep::NeedsOtp { otp_login_token } => {
            // O 2FA vem como 200 com `requiresOtp`, então este ramo é o caminho normal de uma
            // conta protegida — não um erro.
            let code = prompt::line("Código 2FA (ou código de recuperação): ")?;

            if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
                auth::sign_in_otp(&ctx.client, &otp_login_token, &code)
                    .await
                    .map_err(|e| LoginRefused::from_unauthorized(e, LoginStage::OtpCode))?
            } else {
                auth::sign_in_recovery(&ctx.client, &otp_login_token, &code)
                    .await
                    .map_err(|e| LoginRefused::from_unauthorized(e, LoginStage::RecoveryCode))?
            }
        }
    };

    let token = identity
        .access_token
        .clone()
        .ok_or_else(|| anyhow!("login aceito mas sem accessToken na resposta"))?;

    let first = primeiro_login()?;
    save_session(&identity, token)?;
    report_login(ctx, &identity, first)
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
    // 401 com a chave digitada é chave recusada, não sessão expirada; os demais erros (rede, 5xx)
    // continuam com o contexto de antes.
    let identity = match auth::verify(&client).await {
        Ok(i) => i,
        Err(mjsv_core::ApiError::Unauthorized) => return Err(TokenRefused.into()),
        Err(e) => return Err(anyhow::Error::new(e).context("a chave informada não foi aceita pela API")),
    };

    let first = primeiro_login()?;
    save_session(&identity, token.trim().to_string())?;
    report_login(ctx, &identity, first)
}

/// Ainda não existe arquivo de credenciais: é o primeiro login desta máquina e merece o banner
/// completo. Tem de ser lido ANTES de `save_session`, que cria o arquivo.
fn primeiro_login() -> Result<bool> {
    Ok(!mjsv_core::config::credentials_path()?.exists())
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

fn report_login(ctx: &Ctx, identity: &Identity, first: bool) -> Result<()> {
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

    let out = Style::stdout(ctx.format, Tool::Mjsv);
    println!("{}", login_view_text(&out, identity, first, ctx.client.base_url()));

    Ok(())
}

/// O texto do resultado do login. Separado de `report_login` para os testes escolherem o terminal.
fn login_view_text(out: &Style, identity: &Identity, first: bool, base_url: &str) -> String {
    // Nome e id vêm da API e entram em arte: sem controle nem bidi. Fora de um terminal estilizado
    // o texto é o de hoje, byte a byte.
    let safe = |s: &str| out.safe(s);

    let orgs = match identity.orgs.as_slice() {
        [] => OrgSummary::None,
        [only] => OrgSummary::One {
            name: safe(only.name.as_deref().unwrap_or("?")),
            id: safe(&only.id),
        },
        many => OrgSummary::Many(many.len()),
    };

    out.login_result(&LoginView {
        username: safe(identity.username.as_deref().unwrap_or("?")),
        user_id: safe(identity.id.as_deref().unwrap_or("?")),
        orgs,
        first,
        host: mjsv_core::style::api_host(base_url),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

pub async fn logout(ctx: &Ctx) -> Result<()> {
    let out = Style::stdout(ctx.format, Tool::Mjsv);

    if !ctx.client.has_token() {
        println!("{}", out.noop("Nenhuma sessão local para encerrar."));
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
        // O aviso vai para stderr, então o glifo depende de stderr ser um terminal, não de stdout.
        let err = Style::stderr(ctx.format, Tool::Mjsv);
        let aviso = format!("o servidor não confirmou o sign-out ({e}); a credencial local foi apagada mesmo assim");
        if err.fancy() {
            eprintln!("{}", err.attention(&aviso));
        } else {
            eprintln!("aviso: {aviso}");
        }
    }

    Credentials::clear()?;
    println!("{}", out.logout_result());

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

    let out = Style::stdout(ctx.format, Tool::Mjsv);
    let safe = |s: &str| out.safe(s);
    let dash = |v: &Option<String>| safe(v.as_deref().unwrap_or("-"));

    for line in whoami_lines(&out, &identity, ctx.org.as_deref(), ctx.client.base_url(), &safe, &dash) {
        println!("{line}");
    }

    Ok(())
}

/// O cartão de `whoami` (DV-whoami / EM-whoami): régua de título e as seis linhas de hoje.
fn whoami_lines(
    out: &Style,
    identity: &Identity,
    org: Option<&str>,
    api: &str,
    safe: &dyn Fn(&str) -> String,
    dash: &dyn Fn(&Option<String>) -> String,
) -> Vec<String> {
    // Sem organização, o marcador `▲` mora dentro da célula do valor; sem terminal estilizado o
    // texto é o da especificação (a dica entra na própria linha, em vez de ficar só no erro
    // seguinte).
    let org_cell = match org {
        Some(o) => safe(o),
        None if out.fancy() => format!(
            "{} nenhuma selecionada (mjsv org use <id>)",
            out.marker(Marker::Attention)
        ),
        None => "(nenhuma selecionada — `mjsv org use <id>`)".to_string(),
    };

    let mut lines = Vec::new();
    if let Some(t) = out.title_rule(&format!("Conta {}", safe(identity.username.as_deref().unwrap_or("?")))) {
        lines.push(t);
    }
    lines.extend(card(
        out,
        14,
        &[
            ("Usuário", dash(&identity.username)),
            ("Nome", dash(&identity.name)),
            ("E-mail", dash(&identity.email)),
            ("Id", dash(&identity.id)),
            ("Organização", org_cell),
            ("API", api.to_string()),
        ],
    ));
    lines
}

/// Linhas `Rótulo:   valor` de um cartão de dados.
///
/// Num terminal estilizado o rótulo é esmaecido e o valor começa em `maior rótulo + 3`. Fora dele
/// o recuo é o de hoje (`plain_width` colunas), porque cada comando alinhava por conta própria e a
/// saída encanada não pode mudar um espaço.
pub(super) fn card(out: &Style, plain_width: usize, pairs: &[(&str, String)]) -> Vec<String> {
    if out.fancy() {
        return out.card_rows(pairs);
    }
    pairs
        .iter()
        .map(|(k, v)| format!("{}{v}", mjsv_core::style::pad(&format!("{k}:"), plain_width)))
        .collect()
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
                // A frase de hoje já embute a dica; num terminal ela vira a linha esmaecida de EM-hint.
                let out = Style::stdout(ctx.format, Tool::Mjsv);
                println!(
                    "{}",
                    out.empty_hint_plain(
                        "Nenhuma chave de API. Crie uma com `mjsv token create <nome>`.",
                        "Nenhuma chave de API.",
                        "crie uma",
                        "mjsv token create <nome>",
                    )
                );
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
            print!("{}", table.render_out());

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

            let out = Style::stdout(ctx.format, Tool::Mjsv);
            let nome = cell(created.name.as_deref());
            let quem = format!("{nome} ({})", created.id);

            // O segredo só aparece aqui; a caixa sai num único `write_all` com stdout travado para
            // nada se intercalar a ele. Sem arte (pipe, NO_BANNER) valem as linhas de hoje.
            let caixa = created.token.as_deref().and_then(|secret| {
                out.secret_box(
                    &SecretBox::new("COPIE AGORA", "não será mostrado de novo")
                        .secret(sanitize(secret))
                        .row("chave", sanitize(&quem))
                        .row("use com", format!("export {TOKEN_ENV}=<valor>")),
                )
            });

            if let Some(caixa) = caixa {
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(format!("{caixa}\n").as_bytes())?;
                stdout.flush()?;
                return Ok(());
            }

            println!("Chave criada: {quem}");
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
                // Nível 1: recuperável (cria-se outra chave), mas quem usa esta perde o acesso agora.
                prompt::confirm_spec(
                    &ConfirmSpec::new(
                        Tier::T1,
                        format!("Revogar a chave {id}? Quem a usa perde o acesso na hora."),
                    ),
                    ctx.format,
                )?;
            }

            let req = Request::delete(format!("/auth/token/{id}")).target(Target::root());
            let Some(_) = ctx.run(req).await? else { return Ok(()) };

            println!(
                "{}",
                Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Chave {id} revogada."))
            );

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
        let out = Style::stdout(ctx.format, Tool::Mjsv);
        println!(
            "{}",
            out.empty_hint(
                "Nenhuma organização vinculada a esta conta.",
                "comece por",
                "mjsv org create <nome>"
            )
        );
        return Ok(());
    }

    let mut table = Table::new(&["", "id", "nome"]);
    for o in &identity.orgs {
        let atual = if Some(&o.id) == ctx.org.as_ref() { "*" } else { " " };
        table.push(vec![atual.to_string(), o.id.clone(), cell(o.name.as_deref())]);
    }
    print!("{}", table.render_out());

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
        return Err(OrgNotInAccount {
            org: org.to_string(),
            available: conhecidas,
        }
        .into());
    }

    let mut config = Config::load()?;
    config.org = Some(org.to_string());
    config.save()?;

    println!(
        "{}",
        Style::stdout(ctx.format, Tool::Mjsv).done(&format!("Organização corrente: {org}"))
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mjsv_core::style::{Charset, Depth};

    fn identity(orgs: &[(&str, &str)]) -> Identity {
        let orgs: Vec<_> = orgs
            .iter()
            .map(|(id, name)| serde_json::json!({"id": id, "name": name}))
            .collect();
        serde_json::from_value(serde_json::json!({
            "id": "Qw3Rt", "username": "ana", "name": "Ana Souza", "email": "ana@acme.com.br", "orgs": orgs,
        }))
        .unwrap()
    }

    #[test]
    fn whoami_sem_org_usa_marcador_na_celula_e_texto_da_spec_no_pipe() {
        let id = identity(&[]);
        let safe = |s: &str| s.to_string();
        let dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());

        let tty = Style::terminal(Tool::Mjsv, Charset::Ascii, Depth::None, 100);
        let linhas = whoami_lines(&tty, &id, None, "https://api", &safe, &dash);
        assert!(linhas[0].starts_with("-- Conta ana "));
        assert!(linhas.contains(&"Organização:  ! nenhuma selecionada (mjsv org use <id>)".to_string()));

        let pipe = Style::plain(Tool::Mjsv);
        let linhas = whoami_lines(&pipe, &id, None, "https://api", &safe, &dash);
        assert_eq!(linhas.len(), 6);
        assert_eq!(linhas[0], "Usuário:      ana");
        assert_eq!(linhas[4], "Organização:  (nenhuma selecionada — `mjsv org use <id>`)");
        assert_eq!(linhas[5], "API:          https://api");
    }

    #[test]
    fn login_no_pipe_e_o_texto_de_hoje() {
        let pipe = Style::plain(Tool::Mjsv);
        let um = identity(&[("nUME7", "acme")]);
        assert_eq!(
            login_view_text(&pipe, &um, true, "https://api.mjsv.io"),
            "Autenticado como ana (Qw3Rt)\nOrganização: acme (nUME7)"
        );
        assert_eq!(
            login_view_text(&pipe, &identity(&[]), false, "https://api.mjsv.io"),
            "Autenticado como ana (Qw3Rt)\nNenhuma organização vinculada a esta conta."
        );
    }

    #[test]
    fn login_em_terminal_estreito_vira_coroa_compacta() {
        let tty = Style::terminal(Tool::Mjsv, Charset::Ascii, Depth::None, 50);
        let um = identity(&[("nUME7", "acme")]);
        let texto = login_view_text(&tty, &um, true, "https://api.mjsv.io");
        assert!(texto.contains("[+] Autenticado como ana (Qw3Rt)"));
        assert!(texto.contains("org: acme (nUME7)"));
    }

    #[test]
    fn cartao_no_pipe_mantem_o_recuo_de_hoje() {
        let pipe = Style::plain(Tool::Mjsv);
        let linhas = card(&pipe, 11, &[("Id", "x".to_string()), ("Criada em", "y".to_string())]);
        assert_eq!(linhas, vec!["Id:        x", "Criada em: y"]);
    }
}
