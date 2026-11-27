//! Incidentes de ataque — a mesma API serve a VM (`/vm/:vmServiceId/incidents`) e o serviço de
//! proteção (`/protection/:serviceId/incidents`), então o código também é um só.

use anyhow::Result;
use mjsv_core::client::Request;
use mjsv_core::output::{cell, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::Value;

use crate::cli::IncidentFilter;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    available: Option<bool>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    active_count: Option<i64>,
    #[serde(default)]
    addresses: Vec<String>,
    #[serde(default)]
    items: Vec<Item>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    started_at: Option<i64>,
    #[serde(default)]
    duration_seconds: Option<i64>,
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    port: Option<i64>,
    // `layer` e `vector` mudaram de formato entre versões; aceitar qualquer JSON evita que um
    // campo novo derrube a listagem inteira.
    #[serde(default)]
    layer: Option<Value>,
    #[serde(default)]
    vector: Option<Value>,
}

/// Segundos Unix em `AAAA-MM-DD HH:MM:SS` (UTC), sem puxar dependência de datas só para isso.
pub fn format_unix(secs: i64) -> String {
    let dias = secs.div_euclid(86_400);
    let resto = secs.rem_euclid(86_400);

    // Algoritmo de "civil from days" (Howard Hinnant), válido para o calendário gregoriano.
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + i64::from(mes <= 2);

    format!(
        "{ano:04}-{mes:02}-{dia:02} {:02}:{:02}:{:02}",
        resto / 3600,
        (resto % 3600) / 60,
        resto % 60
    )
}

fn texto(v: &Option<Value>) -> String {
    match v {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::Null) | None => "-".into(),
        Some(Value::Object(o)) => o
            .get("label")
            .or_else(|| o.get("key"))
            .and_then(Value::as_str)
            .map_or_else(|| Value::Object(o.clone()).to_string(), str::to_string),
        Some(other) => other.to_string(),
    }
}

pub async fn list(ctx: &Ctx, path: &str, target: Target, f: &IncidentFilter) -> Result<()> {
    let mut req = Request::get(path).target(target);
    if let Some(v) = &f.status {
        req = req.query("status", v);
    }
    if let Some(v) = &f.ip {
        req = req.query("ip", v);
    }
    if let Some(v) = f.from {
        req = req.query("from", v);
    }
    if let Some(v) = f.to {
        req = req.query("to", v);
    }
    if let Some(v) = f.limit {
        req = req.query("limit", v);
    }
    if let Some(v) = &f.cursor {
        req = req.query("cursor", v);
    }

    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let page: Page = serde_json::from_value(value)?;

    // A API responde 200 com `available:false` quando o backend de telemetria não está
    // configurado ou caiu — tratar como lista vazia esconderia o motivo.
    if page.available == Some(false) {
        println!(
            "Incidentes indisponíveis: {}",
            page.reason.as_deref().unwrap_or("sem motivo informado")
        );
        return Ok(());
    }

    if page.items.is_empty() {
        println!("Nenhum incidente no período.");
        return Ok(());
    }

    if !page.addresses.is_empty() {
        println!("Endereços: {}", page.addresses.join(", "));
    }
    if let Some(n) = page.active_count {
        println!("Ativos:    {n}");
    }

    let mut table = Table::new(&[
        "id",
        "estado",
        "início (UTC)",
        "duração",
        "ip",
        "porta",
        "camada",
        "vetor",
    ]);
    for i in &page.items {
        table.push(vec![
            cell(i.id.as_deref()),
            cell(i.status.as_deref()),
            i.started_at.map(format_unix).unwrap_or_else(|| "-".into()),
            i.duration_seconds
                .map(|s| format!("{s}s"))
                .unwrap_or_else(|| "-".into()),
            cell(i.ip.as_deref()),
            i.port.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            texto(&i.layer),
            texto(&i.vector),
        ]);
    }
    print!("{}", table.render());

    if let Some(c) = page.next_cursor.as_deref().filter(|c| !c.is_empty()) {
        println!("Mais resultados: repita com --cursor {c}");
    }

    Ok(())
}

pub async fn detail(ctx: &Ctx, path: &str, target: Target) -> Result<()> {
    let req = Request::get(path).target(target);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    // O detalhe traz séries de tráfego e análise; em tabela só caberia um resumo, e o resumo já
    // está na listagem. JSON/YAML é o formato útil aqui.
    if ctx.format.is_machine_readable() || value.get("available") == Some(&Value::Bool(false)) {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let i: Item = serde_json::from_value(value.clone())?;
    println!("Incidente: {}", cell(i.id.as_deref()));
    println!("Estado:    {}", cell(i.status.as_deref()));
    println!(
        "Início:    {}",
        i.started_at.map(format_unix).unwrap_or_else(|| "-".into())
    );
    println!("IP:        {}", cell(i.ip.as_deref()));
    println!("Camada:    {}", texto(&i.layer));
    println!("Vetor:     {}", texto(&i.vector));
    println!("(séries de tráfego e análise: use --output json)");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::format_unix;

    #[test]
    fn data_unix_conhecida() {
        assert_eq!(format_unix(0), "1970-01-01 00:00:00");
        assert_eq!(format_unix(1_700_000_000), "2023-11-14 22:13:20");
        assert_eq!(format_unix(951_782_400), "2000-02-29 00:00:00");
    }
}
