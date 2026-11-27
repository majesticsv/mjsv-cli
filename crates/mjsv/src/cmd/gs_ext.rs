//! Gameserver: arquivos, bancos, agendamentos, add-ons e o catálogo.
//!
//! Todas as rotas `/gameserver/:gameServerServiceId/...` daqui conferem `MJSV-TargetId` contra o
//! id do caminho com `TargetType=GAMESERVER`, então o mesmo id vai nos dois lugares. As rotas do
//! catálogo (`/larder/:game/...`) não têm dono: o alvo é a org ativa.

use anyhow::{anyhow, bail, Context, Result};
use mjsv_core::client::Request;
use mjsv_core::output::{cell, Table};
use mjsv_core::style::{sanitize, ConfirmSpec, Style, Tier, Tool};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::{GsAddonCommand, GsDbCommand, GsFileCommand, GsModpackCommand, GsScheduleCommand, GsTaskCommand};
use crate::prompt;

/// Pede confirmação, a menos que `--yes` ou `--print-request` (dry-run não executa nada).
fn confirmar(ctx: &Ctx, pergunta: &str) -> Result<()> {
    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(pergunta, ctx.format)?;
    }

    Ok(())
}

/// Confirmação de nível 1 (CF-1): impacto no serviço, recuperável. Só dados em mãos, sem consulta.
/// `sentence` é a pergunta de hoje, que é o que um pipe vê.
fn confirmar_t1(ctx: &Ctx, pergunta: &str) -> Result<()> {
    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm_spec(&ConfirmSpec::new(Tier::T1, pergunta), ctx.format)?;
    }

    Ok(())
}

fn gs_req(req: Request, id: &str) -> Request {
    req.target(Target::gameserver(id))
}

/// Como a linha de sucesso de `executar` é desenhada (OK-done).
#[derive(Clone, Copy)]
enum Aviso {
    /// `✓` — feito.
    Feito,
    /// `▲` — feito, mas falta uma ação do usuário (reiniciar o servidor).
    Atencao,
}

/// Executa e imprime o corpo cru, ou `msg` quando a saída é de tabela.
async fn executar(ctx: &Ctx, req: Request, msg: &str) -> Result<()> {
    executar_com(ctx, req, msg, Aviso::Feito).await
}

async fn executar_com(ctx: &Ctx, req: Request, msg: &str, aviso: Aviso) -> Result<()> {
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
    } else {
        let out = Style::stdout(ctx.format, Tool::Mjsv);
        match aviso {
            Aviso::Feito => println!("{}", out.done(msg)),
            Aviso::Atencao => println!("{}", out.attention(msg)),
        }
    }

    Ok(())
}

/// Nome do gameserver para o bloco de uma confirmação de nível 2 (uma consulta, spec CF-2).
///
/// Só consulta quando o prompt vai de fato desenhar o bloco (stderr é terminal estilizado): em um
/// pipe a pergunta é a frase de hoje e a consulta seria tráfego à toa. Qualquer falha devolve
/// `None` e o prompt degrada para o nível 1 sob a mesma régua — apagar nunca trava numa consulta
/// cosmética. A rota de leitura (`GET /gameserver/:id`) não é a do `--print-request`: este nem
/// chega aqui, o prompt é pulado.
pub(crate) async fn nome_do_gameserver(ctx: &Ctx, id: &str) -> Option<String> {
    if !Style::stderr(ctx.format, Tool::Mjsv).fancy() {
        return None;
    }

    let req = gs_req(Request::get(format!("/gameserver/{id}")), id);
    let value = ctx.client.send_value(&req).await.ok()?;
    nome_na_resposta(&value)
}

/// O corpo pode vir cru ou embrulhado em `data`; o nome é texto da API, então é higienizado antes
/// de entrar na arte.
fn nome_na_resposta(value: &Value) -> Option<String> {
    let nome = value
        .get("name")
        .or_else(|| value.get("data").and_then(|d| d.get("name")))?
        .as_str()?;
    let nome = sanitize(nome);

    (!nome.trim().is_empty()).then_some(nome)
}

/// Linhas de contexto `gameserver` e `org` (a org só pelo id: o nome custaria outra consulta).
pub(crate) fn linhas_do_gameserver(ctx: &Ctx, mut spec: ConfirmSpec, nome: &str, id: &str) -> ConfirmSpec {
    spec = spec.row("gameserver", format!("{nome} ({id})"));
    if let Some(org) = &ctx.org {
        spec = spec.row("org", org.clone());
    }

    spec
}

/// Texto curto dos nomes de uma lista: os 2 primeiros e `· +N` (CF-2-files).
fn resumo_de_nomes(out: &Style, nomes: &[String]) -> String {
    let mut partes: Vec<String> = nomes.iter().take(2).map(|n| sanitize(n)).collect();
    if nomes.len() > 2 {
        partes.push(format!("+{}", nomes.len() - 2));
    }

    partes.join(out.sep())
}

/// Executa e imprime o corpo cru em qualquer formato.
async fn mostrar(ctx: &Ctx, req: Request) -> Result<()> {
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    crate::render_value(ctx.format, &value);

    Ok(())
}

fn pares(params: &[String]) -> Result<Vec<(String, String)>> {
    params
        .iter()
        .map(|p| {
            p.split_once('=')
                .filter(|(k, _)| !k.is_empty())
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| anyhow!("parâmetro `{p}` inválido — use CHAVE=VALOR"))
        })
        .collect()
}

fn com_query(mut req: Request, params: &[String]) -> Result<Request> {
    for (k, v) in pares(params)? {
        req = req.query(k, v);
    }

    Ok(req)
}

/// Lê o corpo JSON de um arquivo (ou stdin com `-`).
///
/// O catálogo valida o corpo, não o backend (é um `Record<string, unknown>` opaco), então o CLI só
/// garante que é JSON válido antes de enviar.
fn ler_json(file: &str) -> Result<Value> {
    let texto = if file == "-" {
        std::io::read_to_string(std::io::stdin()).context("falha ao ler o stdin")?
    } else {
        std::fs::read_to_string(file).with_context(|| format!("falha ao ler {file}"))?
    };

    serde_json::from_str(&texto).with_context(|| format!("{file} não contém JSON válido"))
}

/// Divide `m h dom mon dow` nos cinco campos que a API guarda separados.
fn cron_campos(cron: &str) -> Result<[String; 5]> {
    let campos: Vec<&str> = cron.split_whitespace().collect();

    let [m, h, dom, mon, dow] = campos[..] else {
        bail!("cron `{cron}` inválido — esperados 5 campos (minuto hora dia-do-mês mês dia-da-semana)");
    };

    Ok([m, h, dom, mon, dow].map(str::to_string))
}

fn inserir_cron(corpo: &mut Value, cron: &str) -> Result<()> {
    let [minute, hour, dom, month, dow] = cron_campos(cron)?;
    corpo["minute"] = json!(minute);
    corpo["hour"] = json!(hour);
    corpo["dayOfMonth"] = json!(dom);
    corpo["month"] = json!(month);
    corpo["dayOfWeek"] = json!(dow);

    Ok(())
}

// ─────────────────────────── gameserver ───────────────────────────

/// `GET /gameserver/service/:serviceId` é a única rota de gameserver de escopo SERVICE: ela existe
/// para quem só tem o id do serviço e não sabe o tipo do recurso.
pub async fn by_service(ctx: &Ctx, service: &str) -> Result<()> {
    mostrar(
        ctx,
        Request::get(format!("/gameserver/service/{service}")).target(Target::service(service)),
    )
    .await
}

/// Só variáveis de ambiente.
pub async fn startup(ctx: &Ctx, id: &str, env: &[String]) -> Result<()> {
    let mut environment = serde_json::Map::new();
    for (k, v) in pares(env)? {
        environment.insert(k, Value::String(v));
    }

    let req = gs_req(Request::put(format!("/gameserver/{id}/startup")), id).body(json!({ "environment": environment }));

    executar(ctx, req, &format!("Variáveis de {id} atualizadas.")).await
}

// ─────────────────────────── arquivos ───────────────────────────

/// Forma tolerante da listagem do painel de arquivos: `data[].attributes`.
#[derive(Debug, Deserialize)]
struct Listagem {
    #[serde(default)]
    data: Vec<Entrada>,
}

#[derive(Debug, Deserialize)]
struct Entrada {
    #[serde(default)]
    attributes: Option<Atributos>,
}

#[derive(Debug, Deserialize)]
struct Atributos {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    is_file: Option<bool>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    modified_at: Option<String>,
}

async fn file_ls(ctx: &Ctx, id: &str, dir: &str) -> Result<()> {
    let req = gs_req(Request::get(format!("/gameserver/{id}/directories")), id).query("directory", dir);

    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    // Se o formato não for o esperado, o JSON cru é melhor que uma tabela vazia enganosa.
    let Ok(listagem) = serde_json::from_value::<Listagem>(value.clone()) else {
        crate::render_value(ctx.format, &value);
        return Ok(());
    };

    if listagem.data.is_empty() {
        println!("Nenhum item em {dir}.");
        return Ok(());
    }

    let mut table = Table::new(&["tipo", "nome", "tamanho", "modificado"]);
    for e in listagem.data.iter().filter_map(|e| e.attributes.as_ref()) {
        table.push(vec![
            if e.is_file == Some(false) { "dir" } else { "arquivo" }.to_string(),
            cell(e.name.as_deref()),
            e.size.map(|s| s.to_string()).unwrap_or_else(|| "-".into()),
            cell(e.modified_at.as_deref()),
        ]);
    }
    print!("{}", table.render_out());

    Ok(())
}

/// O conteúdo vem como texto dentro do JSON; no formato de tabela é impresso sem moldura para
/// poder redirecionar para um arquivo.
async fn file_cat(ctx: &Ctx, id: &str, path: &str) -> Result<()> {
    let req = gs_req(Request::get(format!("/gameserver/{id}/file")), id).query("filePath", path);

    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    match value {
        Value::String(texto) if !ctx.format.is_machine_readable() => print!("{texto}"),
        outro => crate::render_value(ctx.format, &outro),
    }

    Ok(())
}

/// A rota NÃO devolve os bytes: devolve um link temporário assinado do painel do nó, que é o que o
/// navegador abre. Por isso aqui só se imprime o link — baixar de fato é com curl/wget nele.
async fn file_download(ctx: &Ctx, id: &str, path: &str) -> Result<()> {
    // `intent` fica de fora: ele só muda o texto do log de auditoria, e o padrão ("baixou") é o
    // verdadeiro para um comando de download.
    let req = gs_req(Request::get(format!("/gameserver/{id}/download")), id).query("path", path);

    mostrar(ctx, req).await
}

/// A escrita é multipart: o conteúdo vai no campo `file`, o diretório sai de `filePath` e o nome
/// final é o `filename` da parte — por isso o nome remoto é separado do caminho aqui.
async fn file_upload(ctx: &Ctx, id: &str, local: &str, remote: &str) -> Result<()> {
    let nome = remote
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| anyhow!("o destino `{remote}` precisa terminar no nome do arquivo"))?;

    let req = gs_req(Request::post(format!("/gameserver/{id}/file")), id);

    if ctx.print_request {
        println!("{}", req.describe(ctx.client.base_url()));
        println!("(multipart: file={local} como `{nome}`, filePath={remote})");
        return Ok(());
    }

    let conteudo = std::fs::read(local).with_context(|| format!("falha ao ler {local}"))?;
    let bytes = conteudo.len();

    let campos = [("filePath", remote)];
    let envio = ctx.client.send_multipart(&req, nome, conteudo, &campos);
    let value = if ctx.format.is_machine_readable() {
        envio.await?
    } else {
        com_progresso(nome, bytes, envio).await?
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
    } else {
        let out = Style::stdout(ctx.format, Tool::Mjsv);
        println!(
            "{}",
            out.done(&format!("{local} enviado para {remote} ({bytes} bytes)."))
        );
    }

    Ok(())
}

/// Espera o envio desenhando UMA linha de progresso em stderr (DV-upload).
///
/// O cliente HTTP manda o corpo de uma vez e não avisa quantos bytes já saíram, então o
/// percentual é desconhecido e a linha cai na forma do spec para tamanho incerto: só o quadro do
/// spinner e o tamanho total (`▀ Enviando world.zip  61,4 MiB`). Sem redesenho possível (pipe,
/// `TERM=dumb`) não se imprime nada; a linha é apagada e o stderr esvaziado antes de o chamador
/// escrever a frase final no stdout, e também quando o envio falha, para o cartão de erro não
/// ficar colado nos restos do spinner.
async fn com_progresso<T>(nome: &str, bytes: usize, envio: impl std::future::Future<Output = T>) -> T {
    use std::io::Write;

    let err = Style::stderr(mjsv_core::output::Format::Table, Tool::Mjsv);
    if !err.can_redraw() {
        return envio.await;
    }

    let rotulo = format!("Enviando {}", sanitize(nome));
    let detalhe = tamanho_legivel(bytes as u64);
    let mut tique = tokio::time::interval(std::time::Duration::from_millis(mjsv_core::style::SPINNER_INTERVAL_MS));
    let mut quadro = 0usize;
    tokio::pin!(envio);

    let resultado = loop {
        tokio::select! {
            r = &mut envio => break r,
            _ = tique.tick() => {
                let linha = err.progress_line(quadro, &rotulo, None, &detalhe);
                eprint!("{}{}", err.erase_line(), linha);
                let _ = std::io::stderr().flush();
                quadro += 1;
            }
        }
    };

    eprint!("{}", err.erase_line());
    let _ = std::io::stderr().flush();

    resultado
}

/// `61,4 MiB` (vírgula decimal, como o resto da saída em português). Abaixo de 1 KiB, bytes.
fn tamanho_legivel(bytes: u64) -> String {
    const UNIDADES: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut valor = bytes as f64 / 1024.0;
    let mut i = 0;
    while valor >= 1024.0 && i + 1 < UNIDADES.len() {
        valor /= 1024.0;
        i += 1;
    }

    format!("{valor:.1} {}", UNIDADES[i]).replace('.', ",")
}

/// CF-2-files: lista os 2 primeiros nomes e `· +N`. A frase de hoje (que um pipe vê) continua
/// listando todos os nomes.
async fn confirmar_rm_arquivos(ctx: &Ctx, id: &str, dir: &str, names: &[String]) -> Result<()> {
    if ctx.assume_yes || ctx.print_request {
        return Ok(());
    }

    let frase = format!(
        "Apagar {} de {dir} no gameserver {id}? Não há lixeira.",
        names.join(", ")
    );
    let out = Style::stderr(ctx.format, Tool::Mjsv);
    let plural = if names.len() == 1 { "arquivo" } else { "arquivos" };
    let mut spec = ConfirmSpec::new(Tier::T2, frase)
        .destructive()
        .head(format!("Apagar {} {plural} de {}?", names.len(), sanitize(dir)))
        .note("Não há lixeira.");

    if let Some(nome) = nome_do_gameserver(ctx, id).await {
        spec = spec.row("arquivos", resumo_de_nomes(&out, names));
        spec = linhas_do_gameserver(ctx, spec, &nome, id);
    }

    prompt::confirm_spec(&spec, ctx.format)?;

    Ok(())
}

/// CF-2 do `db rm`: os dados do banco somem.
async fn confirmar_db_rm(ctx: &Ctx, id: &str, name: &str) -> Result<()> {
    if ctx.assume_yes || ctx.print_request {
        return Ok(());
    }

    let frase = format!("Apagar o banco {name} do gameserver {id}? Os dados são perdidos.");
    let mut spec = ConfirmSpec::new(Tier::T2, frase)
        .destructive()
        .head(format!("Apagar o banco {}?", sanitize(name)))
        .note("Os dados são perdidos.");

    if let Some(nome) = nome_do_gameserver(ctx, id).await {
        spec = spec.row("banco", sanitize(name));
        spec = linhas_do_gameserver(ctx, spec, &nome, id);
    }

    prompt::confirm_spec(&spec, ctx.format)?;

    Ok(())
}

async fn file(ctx: &Ctx, cmd: &GsFileCommand) -> Result<()> {
    match cmd {
        GsFileCommand::Ls { id, dir } => file_ls(ctx, id, dir).await,
        GsFileCommand::Cat { id, path } => file_cat(ctx, id, path).await,
        GsFileCommand::Download { id, path } => file_download(ctx, id, path).await,
        GsFileCommand::Upload { id, local, remote } => {
            confirmar(
                ctx,
                &format!("Enviar {local} para {remote} sobrescreve o arquivo se ele já existir. Continuar?"),
            )?;
            file_upload(ctx, id, local, remote).await
        }
        GsFileCommand::Mv { id, dir, old, new } => {
            confirmar(
                ctx,
                &format!("Mover {old} para {new} em {dir} sobrescreve o destino se ele já existir. Continuar?"),
            )?;
            let req = gs_req(Request::put(format!("/gameserver/{id}/file")), id)
                .body(json!({ "filePath": dir, "oldFileName": old, "newFileName": new }));

            executar(ctx, req, &format!("{old} renomeado para {new} em {dir}.")).await
        }
        GsFileCommand::Mkdir { id, dir, name } => {
            let req = gs_req(Request::post(format!("/gameserver/{id}/directory")), id)
                .body(json!({ "path": dir, "name": name }));

            executar(ctx, req, &format!("Pasta {name} criada em {dir}.")).await
        }
        GsFileCommand::Compress { id, dir, names } => {
            let req = gs_req(Request::post(format!("/gameserver/{id}/files/compress")), id)
                .body(json!({ "filePath": dir, "filesToCompress": names }));

            executar(
                ctx,
                req,
                &format!("Compactação de {} item(ns) de {dir} concluída.", names.len()),
            )
            .await
        }
        GsFileCommand::Decompress { id, dir, archive } => {
            confirmar(
                ctx,
                &format!("Descompactar {archive} em {dir} sobrescreve arquivos existentes. Continuar?"),
            )?;
            let req = gs_req(Request::post(format!("/gameserver/{id}/files/decompress")), id)
                .body(json!({ "filePath": dir, "fileToDecompress": archive }));

            executar(ctx, req, &format!("{archive} descompactado em {dir}.")).await
        }
        GsFileCommand::Rm { id, dir, names } => {
            confirmar_rm_arquivos(ctx, id, dir, names).await?;

            // DELETE com corpo JSON: é assim que o backend recebe a lista de nomes.
            let req = gs_req(Request::delete(format!("/gameserver/{id}/file")), id)
                .body(json!({ "filePath": dir, "filesToDelete": names }));

            executar(ctx, req, &format!("{} item(ns) apagado(s) de {dir}.", names.len())).await
        }
    }
}

// ─────────────────────────── bancos ───────────────────────────

/// A senha é impressa porque é o propósito do comando; o backend registra a revelação na
/// auditoria. Nada aqui passa por `tracing`.
async fn db(ctx: &Ctx, cmd: &GsDbCommand) -> Result<()> {
    match cmd {
        GsDbCommand::Create { id, name } => {
            let req = gs_req(Request::post(format!("/gameserver/{id}/database")), id).body(json!({ "name": name }));

            mostrar(ctx, req).await
        }
        GsDbCommand::Password { id, name } => {
            let req = gs_req(Request::post(format!("/gameserver/{id}/database/{name}/password")), id);

            mostrar(ctx, req).await
        }
        GsDbCommand::Delete { id, name } => {
            confirmar_db_rm(ctx, id, name).await?;

            // O parâmetro da rota se chama `databaseId`, mas o handler procura pelo NOME.
            let req = gs_req(Request::delete(format!("/gameserver/{id}/database/{name}")), id);

            executar(ctx, req, &format!("Banco {name} apagado.")).await
        }
    }
}

// ─────────────────────────── agendamentos ───────────────────────────

async fn schedule(ctx: &Ctx, cmd: &GsScheduleCommand) -> Result<()> {
    match cmd {
        GsScheduleCommand::Show { id, schedule } => {
            mostrar(
                ctx,
                gs_req(Request::get(format!("/gameserver/{id}/schedules/{schedule}")), id),
            )
            .await
        }
        GsScheduleCommand::Create {
            id,
            name,
            cron,
            only_online,
            disabled,
        } => {
            let mut corpo = json!({ "name": name, "enabled": !disabled, "onlyWhenOnline": only_online });
            inserir_cron(&mut corpo, cron)?;

            let req = gs_req(Request::post(format!("/gameserver/{id}/schedules")), id).body(corpo);

            executar(ctx, req, &format!("Agendamento {name} criado.")).await
        }
        GsScheduleCommand::Edit {
            id,
            schedule,
            name,
            cron,
            enabled,
            only_online,
        } => {
            let mut corpo = json!({});
            if let Some(n) = name {
                corpo["name"] = json!(n);
            }
            if let Some(c) = cron {
                inserir_cron(&mut corpo, c)?;
            }
            if let Some(e) = enabled {
                corpo["enabled"] = json!(e);
            }
            if let Some(o) = only_online {
                corpo["onlyWhenOnline"] = json!(o);
            }

            if corpo.as_object().is_some_and(|o| o.is_empty()) {
                bail!("nada para editar — informe --name, --cron, --enabled ou --only-online");
            }

            let req = gs_req(Request::put(format!("/gameserver/{id}/schedules/{schedule}")), id).body(corpo);

            executar(ctx, req, &format!("Agendamento {schedule} atualizado.")).await
        }
        GsScheduleCommand::Delete { id, schedule } => {
            confirmar_t1(
                ctx,
                &format!("Apagar o agendamento {schedule} do gameserver {id} e suas tarefas?"),
            )?;

            let req = gs_req(Request::delete(format!("/gameserver/{id}/schedules/{schedule}")), id);

            executar(ctx, req, &format!("Agendamento {schedule} apagado.")).await
        }
        GsScheduleCommand::Task(t) => task(ctx, t).await,
    }
}

async fn task(ctx: &Ctx, cmd: &GsTaskCommand) -> Result<()> {
    match cmd {
        GsTaskCommand::Add {
            id,
            schedule,
            action,
            payload,
            offset,
            continue_on_failure,
        } => {
            let mut corpo = json!({ "action": action, "payload": payload, "continueOnFailure": continue_on_failure });
            if let Some(o) = offset {
                corpo["timeOffset"] = json!(o);
            }

            let req = gs_req(
                Request::post(format!("/gameserver/{id}/schedules/{schedule}/tasks")),
                id,
            )
            .body(corpo);

            executar(ctx, req, &format!("Tarefa adicionada ao agendamento {schedule}.")).await
        }
        GsTaskCommand::Edit {
            id,
            schedule,
            task,
            action,
            payload,
            offset,
            continue_on_failure,
        } => {
            let mut corpo = json!({});
            if let Some(a) = action {
                corpo["action"] = json!(a);
            }
            if let Some(p) = payload {
                corpo["payload"] = json!(p);
            }
            if let Some(o) = offset {
                corpo["timeOffset"] = json!(o);
            }
            if let Some(c) = continue_on_failure {
                corpo["continueOnFailure"] = json!(c);
            }

            if corpo.as_object().is_some_and(|o| o.is_empty()) {
                bail!("nada para editar — informe --action, --payload, --offset ou --continue-on-failure");
            }

            let req = gs_req(
                Request::put(format!("/gameserver/{id}/schedules/{schedule}/tasks/{task}")),
                id,
            )
            .body(corpo);

            executar(ctx, req, &format!("Tarefa {task} atualizada.")).await
        }
        GsTaskCommand::Delete { id, schedule, task } => {
            confirmar_t1(ctx, &format!("Apagar a tarefa {task} do agendamento {schedule}?"))?;

            let req = gs_req(
                Request::delete(format!("/gameserver/{id}/schedules/{schedule}/tasks/{task}")),
                id,
            );

            executar(ctx, req, &format!("Tarefa {task} apagada.")).await
        }
    }
}

// ─────────────────────────── catálogo ───────────────────────────

/// Catálogo não é de ninguém: o alvo é a org ativa (é o que o painel manda), e o caminho não leva
/// id de org.
async fn catalogo_get(ctx: &Ctx, path: String, params: &[String], q: Option<&str>) -> Result<()> {
    let (_, target) = ctx.org_target()?;

    let mut req = com_query(Request::get(path).target(target), params)?;
    if let Some(q) = q {
        req = req.query("q", q);
    }

    mostrar(ctx, req).await
}

async fn catalogo_post(ctx: &Ctx, path: String, file: &str) -> Result<()> {
    let corpo = ler_json(file)?;
    let (_, target) = ctx.org_target()?;

    mostrar(ctx, Request::post(path).target(target).body(corpo)).await
}

pub async fn setup(ctx: &Ctx, game: &str) -> Result<()> {
    catalogo_get(ctx, format!("/larder/{game}/setup"), &[], None).await
}

pub async fn fields(ctx: &Ctx, game: &str) -> Result<()> {
    catalogo_get(ctx, format!("/larder/{game}/fields"), &[], None).await
}

pub async fn options(ctx: &Ctx, game: &str, field: &str, param: &[String]) -> Result<()> {
    catalogo_get(ctx, format!("/larder/{game}/options/{field}"), param, None).await
}

async fn modpack(ctx: &Ctx, cmd: &GsModpackCommand) -> Result<()> {
    match cmd {
        GsModpackCommand::Search { game, query, param } => {
            catalogo_get(ctx, format!("/larder/{game}/search/modpacks"), param, Some(query)).await
        }
        GsModpackCommand::Show {
            game,
            provider,
            modpack,
            param,
        } => catalogo_get(ctx, format!("/larder/{game}/modpack/{provider}/{modpack}"), param, None).await,
        GsModpackCommand::Resolve { game, file } => catalogo_post(ctx, format!("/larder/{game}/resolve"), file).await,
        GsModpackCommand::Lookup { game, file } => catalogo_post(ctx, format!("/larder/{game}/lookup"), file).await,
    }
}

// ─────────────────────────── add-ons ───────────────────────────

/// Instalar e remover só edita o lock do servidor; quem baixa e apaga o jar é o servidor no
/// próximo boot. Por isso as respostas são rápidas e a pasta não muda até reiniciar.
async fn addon(ctx: &Ctx, cmd: &GsAddonCommand) -> Result<()> {
    match cmd {
        GsAddonCommand::Search { game, query, param } => {
            catalogo_get(ctx, format!("/larder/{game}/search/addons"), param, Some(query)).await
        }
        GsAddonCommand::Categories { game, param } => {
            catalogo_get(ctx, format!("/larder/{game}/addons/categories"), param, None).await
        }
        GsAddonCommand::Show {
            game,
            provider,
            project,
            param,
        } => catalogo_get(ctx, format!("/larder/{game}/addon/{provider}/{project}"), param, None).await,
        GsAddonCommand::Versions {
            game,
            provider,
            project,
            param,
        } => {
            catalogo_get(
                ctx,
                format!("/larder/{game}/addon/{provider}/{project}/versions"),
                param,
                None,
            )
            .await
        }
        GsAddonCommand::Resolve { game, file } => {
            catalogo_post(ctx, format!("/larder/{game}/addons/resolve"), file).await
        }
        GsAddonCommand::Lookup { game, file } => {
            catalogo_post(ctx, format!("/larder/{game}/addons/lookup"), file).await
        }
        GsAddonCommand::Updates { game, file } => {
            catalogo_post(ctx, format!("/larder/{game}/addons/updates"), file).await
        }
        GsAddonCommand::Installed { id } => {
            mostrar(ctx, gs_req(Request::get(format!("/gameserver/{id}/manager-lock")), id)).await
        }
        GsAddonCommand::Install {
            id,
            provider,
            project,
            version,
        } => {
            let mut corpo = json!({ "provider": provider, "project": project });
            if let Some(v) = version {
                corpo["version"] = json!(v);
            }

            let req = gs_req(Request::post(format!("/gameserver/{id}/addons")), id).body(corpo);

            // Só o lock muda; o jar chega no próximo boot, então a linha pede uma ação (`▲`).
            executar_com(
                ctx,
                req,
                &format!("{project} ({provider}) adicionado — reinicie o servidor para instalar."),
                Aviso::Atencao,
            )
            .await
        }
        GsAddonCommand::Remove { id, file } => {
            confirmar(ctx, &format!("Remover o add-on {file} do gameserver {id}?"))?;

            let req = gs_req(Request::delete(format!("/gameserver/{id}/addons")), id).body(json!({ "file": file }));

            executar_com(
                ctx,
                req,
                &format!("{file} removido — reinicie o servidor para aplicar."),
                Aviso::Atencao,
            )
            .await
        }
        GsAddonCommand::CheckUpdates { id } => {
            mostrar(
                ctx,
                gs_req(Request::post(format!("/gameserver/{id}/addons/updates")), id),
            )
            .await
        }
    }
}

pub async fn run_file(ctx: &Ctx, cmd: &GsFileCommand) -> Result<()> {
    file(ctx, cmd).await
}

pub async fn run_db(ctx: &Ctx, cmd: &GsDbCommand) -> Result<()> {
    db(ctx, cmd).await
}

pub async fn run_schedule(ctx: &Ctx, cmd: &GsScheduleCommand) -> Result<()> {
    schedule(ctx, cmd).await
}

pub async fn run_addon(ctx: &Ctx, cmd: &GsAddonCommand) -> Result<()> {
    addon(ctx, cmd).await
}

pub async fn run_modpack(ctx: &Ctx, cmd: &GsModpackCommand) -> Result<()> {
    modpack(ctx, cmd).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_vira_cinco_campos() {
        let mut corpo = json!({});
        inserir_cron(&mut corpo, "*/5 3 * * 1").unwrap();

        assert_eq!(corpo["minute"], "*/5");
        assert_eq!(corpo["hour"], "3");
        assert_eq!(corpo["dayOfWeek"], "1");
        assert!(cron_campos("* * *").is_err());
    }

    #[test]
    fn nome_da_resposta_cru_ou_em_data() {
        assert_eq!(nome_na_resposta(&json!({"name": "mc-02"})).as_deref(), Some("mc-02"));
        assert_eq!(
            nome_na_resposta(&json!({"data": {"name": "mc-02"}})).as_deref(),
            Some("mc-02")
        );
        assert_eq!(nome_na_resposta(&json!({"name": "  "})), None);
        assert_eq!(nome_na_resposta(&json!({"name": 3})), None);
        // Controle e ESC não entram na arte.
        assert_eq!(
            nome_na_resposta(&json!({"name": "a\u{1b}[31mb"})).as_deref(),
            Some("a[31mb")
        );
    }

    #[test]
    fn resumo_mostra_dois_e_o_resto() {
        use mjsv_core::style::Charset;
        let uni = Style::terminal(Tool::Mjsv, Charset::Unicode, mjsv_core::style::Depth::None, 100);
        let asc = Style::terminal(Tool::Mjsv, Charset::Ascii, mjsv_core::style::Depth::None, 100);
        let n: Vec<String> = ["old.jar", "backup.zip", "x", "y", "z"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert_eq!(resumo_de_nomes(&uni, &n[..1]), "old.jar");
        assert_eq!(resumo_de_nomes(&uni, &n[..2]), "old.jar · backup.zip");
        assert_eq!(resumo_de_nomes(&uni, &n), "old.jar · backup.zip · +3");
        assert_eq!(resumo_de_nomes(&asc, &n), "old.jar, backup.zip, +3");
    }

    #[test]
    fn tamanho_legivel_usa_virgula() {
        assert_eq!(tamanho_legivel(512), "512 B");
        assert_eq!(tamanho_legivel(64_382_217), "61,4 MiB");
        assert_eq!(tamanho_legivel(2048), "2,0 KiB");
    }

    #[test]
    fn parametro_sem_igual_e_recusado() {
        assert!(pares(&["version".into()]).is_err());
        assert_eq!(pares(&["a=b=c".into()]).unwrap(), vec![("a".into(), "b=c".into())]);
    }
}
