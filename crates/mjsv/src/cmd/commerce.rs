//! Catálogo, cupons, pagamento e as operações de pedido que vão além de listar/cancelar.

use anyhow::{anyhow, Result};
use mjsv_core::client::Request;
use mjsv_core::models::{Listing, Order};
use mjsv_core::output::{cell, Table};
use mjsv_core::target::Target;
use mjsv_core::Ctx;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::{OrderBodyArgs, PaymentCommand, ProductCommand};
use crate::prompt;

// ─────────────────────────── pedidos ───────────────────────────

/// Lê o JSON de `--file`; `-` é a entrada padrão.
fn ler_json(origem: &str) -> Result<Value> {
    let texto = if origem == "-" {
        std::io::read_to_string(std::io::stdin())?
    } else {
        std::fs::read_to_string(origem).map_err(|e| anyhow!("não foi possível ler {origem}: {e}"))?
    };

    serde_json::from_str(&texto).map_err(|e| anyhow!("JSON inválido em {origem}: {e}"))
}

/// `chave=valor` com o corte no PRIMEIRO `=`: o valor pode carregar `=` (ids base64, por exemplo).
fn par_chave_valor(texto: &str, flag: &str) -> Result<(String, String)> {
    match texto.split_once('=') {
        Some((k, v)) if !k.trim().is_empty() && !v.is_empty() => Ok((k.trim().to_string(), v.to_string())),
        _ => Err(anyhow!("{flag} espera `chave=valor`, recebi `{texto}`")),
    }
}

fn corpo_do_pedido(args: &OrderBodyArgs) -> Result<Value> {
    if let Some(f) = &args.file {
        return ler_json(f);
    }

    let produto = args
        .product
        .as_deref()
        .ok_or_else(|| anyhow!("informe --product ou --file"))?;

    let mut opcoes = Vec::new();
    for o in &args.options {
        let (id, valor) = par_chave_valor(o, "--option")?;
        opcoes.push(json!({ "optionId": id, "value": valor }));
    }

    let mut body = json!({
        "currency": args.currency,
        "orders": [{ "productId": produto, "selectedOptions": opcoes }],
    });
    if let Some(s) = &args.service {
        body["serviceId"] = json!(s);
    }
    if let Some(c) = &args.coupon {
        body["couponCode"] = json!(c);
    }
    if let Some(w) = args.wallet {
        body["walletUsage"] = json!(w);
    }

    Ok(body)
}

/// `quote` e `create` compartilham o corpo; o quote responde 201 mas não persiste nada.
pub async fn order_create(ctx: &Ctx, args: &OrderBodyArgs, simular: bool) -> Result<()> {
    let (_, target) = ctx.org_target()?;
    let body = corpo_do_pedido(args)?;

    // Saldo reservado só é devolvido se o pedido for apagado/cancelado: vale perguntar.
    let usa_carteira = body.get("walletUsage").and_then(Value::as_u64).unwrap_or(0) > 0;
    if !simular && usa_carteira && !ctx.assume_yes && !ctx.print_request {
        prompt::confirm("Reservar saldo da carteira para este pedido?", ctx.format)?;
    }

    let path = if simular { "/order/quote" } else { "/order" };
    let Some(value) = ctx.run(Request::post(path).target(target).body(body)).await? else {
        return Ok(());
    };

    if simular || ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let o: Order = serde_json::from_value(value)?;
    println!(
        "Pedido {} criado ({}).",
        cell(o.id.as_deref()),
        cell(o.status.as_deref())
    );
    println!("Total:   {}", o.valor_formatado());
    println!("Pague com: mjsv order pay {}", cell(o.id.as_deref()));

    Ok(())
}

pub async fn order_delete(ctx: &Ctx, id: &str) -> Result<()> {
    let (_, target) = ctx.org_target()?;

    if !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(
            &format!("Apagar o pedido {id}? Os serviços aguardando pagamento somem e o saldo reservado volta."),
            ctx.format,
        )?;
    }

    let Some(value) = ctx.run(Request::delete(format!("/order/{id}")).target(target)).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    println!("Pedido {id} apagado.");

    Ok(())
}

pub async fn order_group(ctx: &Ctx, orders: &[String]) -> Result<()> {
    let (_, target) = ctx.org_target()?;

    let req = Request::post("/order/group")
        .target(target)
        .body(json!({ "orders": orders }));
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let o: Order = serde_json::from_value(value)?;
    println!("Pedidos agrupados no pedido {}.", cell(o.id.as_deref()));
    println!("Total:   {}", o.valor_formatado());

    Ok(())
}

/// O checkout NÃO passa pelo guard de permissões: a posse é conferida pela participação na org do
/// pedido (que vem no corpo), então não há alvo de org — o painel manda o alvo de conta.
pub async fn order_pay(ctx: &Ctx, id: &str, metodo: &str, usar_saldo: bool) -> Result<()> {
    if (usar_saldo || metodo == "Balance") && !ctx.assume_yes && !ctx.print_request {
        prompt::confirm(&format!("Pagar o pedido {id} com o saldo da carteira?"), ctx.format)?;
    }

    let mut body = json!({ "orderId": id, "paymentMethod": metodo });
    if usar_saldo {
        body["useBalanceUser"] = json!(true);
    }

    let req = Request::post("/payment/checkout").target(Target::root()).body(body);
    let Some(value) = ctx.run(req).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    // Com `Balance` não há gateway: o pedido conclui na hora e a URL pode vir vazia.
    match value.get("urlCheckoutRedirect").and_then(Value::as_str) {
        Some(url) if !url.is_empty() => println!("Pague em: {url}"),
        _ => println!("Pedido {id} pago."),
    }

    Ok(())
}

pub async fn coupon(ctx: &Ctx, code: &str) -> Result<()> {
    // Qualquer usuário logado consulta; sem alvo, e o servidor normaliza (trim/maiúsculas).
    let Some(value) = ctx.run(Request::get(format!("/coupon/code/{}", code.trim()))).await? else {
        return Ok(());
    };

    if ctx.format.is_machine_readable() {
        crate::render_value(ctx.format, &value);
        return Ok(());
    }

    let c: Coupon = serde_json::from_value(value)?;
    let desconto = match (c.discount_type.as_deref(), c.percentage, c.amount) {
        (Some("PERCENT"), Some(p), _) => format!("{p}%"),
        (_, _, Some(a)) => format!("{}.{:02}", a / 100, a % 100),
        _ => "-".to_string(),
    };

    println!("Código:     {}", cell(c.code.as_deref()));
    println!("Desconto:   {desconto}");
    println!("Duração:    {}", cell(c.duration.as_deref()));
    println!(
        "1ª compra:  {}",
        if c.first_purchase_only.unwrap_or(false) {
            "sim"
        } else {
            "não"
        }
    );
    println!("Expira em:  {}", cell(c.expires_at.as_deref()));

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Coupon {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    discount_type: Option<String>,
    #[serde(default)]
    percentage: Option<i64>,
    /// Centavos, como o resto dos valores monetários de pedido.
    #[serde(default)]
    amount: Option<i64>,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    first_purchase_only: Option<bool>,
    #[serde(default)]
    expires_at: Option<String>,
}

/// Metadados de filtro: públicos e sem alvo, devolvidos como estão.
pub async fn metadata(ctx: &Ctx, path: &str) -> Result<()> {
    let Some(value) = ctx.run(Request::get(path)).await? else {
        return Ok(());
    };

    crate::render_value(ctx.format, &value);

    Ok(())
}

// ─────────────────────────── pagamento ───────────────────────────

#[derive(Debug, Deserialize)]
struct PaymentMethod {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    types: Vec<String>,
}

pub async fn payment(ctx: &Ctx, cmd: &PaymentCommand) -> Result<()> {
    match cmd {
        PaymentCommand::Methods => {
            let Some(value) = ctx.run(Request::get("/payment/methods").target(Target::root())).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let metodos: Vec<PaymentMethod> = serde_json::from_value(value)?;
            if metodos.is_empty() {
                println!("Nenhuma forma de pagamento disponível.");
                return Ok(());
            }

            let mut table = Table::new(&["método", "tipos"]);
            for m in &metodos {
                table.push(vec![cell(m.name.as_deref()), m.types.join(", ")]);
            }
            print!("{}", table.render_out());

            Ok(())
        }
    }
}

// ─────────────────────────── produtos ───────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    service_type: Option<String>,
    /// Centavos (o mesmo `basePrice` que o cálculo de pedido soma direto ao total em centavos).
    #[serde(default)]
    base_price: Option<Value>,
}

/// `base_price` em texto, com a divisão feita em inteiros — nada de ponto flutuante em dinheiro.
fn preco_formatado(v: Option<&Value>) -> String {
    let Some(n) = v.and_then(Value::as_i64) else {
        return "-".to_string();
    };
    let sinal = if n < 0 { "-" } else { "" };
    let abs = n.unsigned_abs();

    format!("{sinal}{}.{:02}", abs / 100, abs % 100)
}

pub async fn product(ctx: &Ctx, cmd: &ProductCommand) -> Result<()> {
    // Catálogo é público: nenhuma rota daqui manda cabeçalho de alvo.
    match cmd {
        ProductCommand::List { page } => {
            let req = crate::cmd::org::with_page(Request::get("/product"), page)?;

            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let listing: Listing<Product> = Listing::from_value(value)?;
            if listing.data.is_empty() {
                println!("Nenhum produto encontrado.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "nome", "tipo", "preço base"]);
            for p in &listing.data {
                table.push(vec![
                    cell(p.id.as_deref()),
                    cell(p.name.as_deref()),
                    cell(p.service_type.as_deref()),
                    preco_formatado(p.base_price.as_ref()),
                ]);
            }
            print!("{}", table.render_out());

            Ok(())
        }

        ProductCommand::Get { id } => {
            let Some(value) = ctx.run(Request::get(format!("/product/{id}"))).await? else {
                return Ok(());
            };

            // Produto inexistente volta 200 com corpo vazio (null), não 404.
            if value.is_null() && !ctx.format.is_machine_readable() {
                return Err(anyhow!("produto {id} não encontrado"));
            }

            crate::render_value(ctx.format, &value);

            Ok(())
        }

        ProductCommand::Isos {
            id,
            region,
            region_option,
        } => {
            let mut req = Request::get(format!("/product/{id}/isos"));
            if let Some(r) = region {
                req = req.query("regionId", r);
            }
            if let Some(r) = region_option {
                req = req.query("regionOptionId", r);
            }

            let Some(value) = ctx.run(req).await? else {
                return Ok(());
            };

            if ctx.format.is_machine_readable() {
                crate::render_value(ctx.format, &value);
                return Ok(());
            }

            let listing: Listing<Iso> = Listing::from_value(value)?;
            if listing.data.is_empty() {
                println!("Nenhuma ISO disponível para este produto/região.");
                return Ok(());
            }

            let mut table = Table::new(&["id", "nome", "padrão"]);
            for i in &listing.data {
                table.push(vec![
                    cell(i.id.as_deref()),
                    cell(i.name.as_deref()),
                    if i.is_default.unwrap_or(false) { "sim" } else { "" }.to_string(),
                ]);
            }
            print!("{}", table.render_out());

            Ok(())
        }

        ProductCommand::Metadata => metadata(ctx, "/product/metadata").await,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Iso {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    is_default: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn par_corta_no_primeiro_igual() {
        assert_eq!(
            par_chave_valor("abc=x==", "--option").unwrap(),
            ("abc".into(), "x==".into())
        );
        assert!(par_chave_valor("semigual", "--option").is_err());
        assert!(par_chave_valor("=v", "--option").is_err());
    }

    #[test]
    fn preco_em_centavos() {
        assert_eq!(preco_formatado(Some(&json!(12_345))), "123.45");
        assert_eq!(preco_formatado(None), "-");
    }

    #[test]
    fn corpo_monta_opcoes_e_extras() {
        let args = OrderBodyArgs {
            product: Some("P1".into()),
            options: vec!["c1=v1".into()],
            currency: "BRL".into(),
            service: Some("S1".into()),
            coupon: None,
            wallet: Some(500),
            file: None,
        };
        let b = corpo_do_pedido(&args).unwrap();

        assert_eq!(b["orders"][0]["selectedOptions"][0]["optionId"], "c1");
        assert_eq!(b["serviceId"], "S1");
        assert_eq!(b["walletUsage"], 500);
        assert!(b.get("couponCode").is_none());
    }
}
