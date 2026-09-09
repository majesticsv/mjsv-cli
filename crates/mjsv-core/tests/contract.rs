//! Confere as rotas que o CLI chama contra o inventário do backend.
//!
//! A API não versiona nada e o spec não tem schema de resposta, então uma renomeação de rota só
//! apareceria como 404 na máquina de um cliente. Este teste antecipa isso para o CI.
//!
//! O inventário é lido de um arquivo vendorizado — o teste não vai à rede de propósito, senão o CI
//! passaria a depender da produção estar de pé.

use mjsv_core::routes::ROUTES;

const INVENTARIO: &str = include_str!("api-routes.txt");

fn rotas_do_backend() -> Vec<&'static str> {
    INVENTARIO
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

#[test]
fn toda_rota_chamada_pelo_cli_existe_no_backend() {
    let backend = rotas_do_backend();
    assert!(
        backend.len() > 100,
        "inventário parece truncado: {} linhas",
        backend.len()
    );

    let ausentes: Vec<String> = ROUTES
        .iter()
        .filter(|s| !s.pending_backend)
        .map(|s| format!("{} {}", s.method.as_str(), s.template))
        .filter(|r| !backend.contains(&r.as_str()))
        .collect();

    assert!(
        ausentes.is_empty(),
        "o CLI chama rota que não existe mais no backend:\n  {}\n\
         rode ./scripts/refresh-routes.sh e ajuste os comandos afetados",
        ausentes.join("\n  ")
    );
}

#[test]
fn rota_pendente_que_ja_existe_tem_de_perder_a_marca() {
    // Sem isto, `pending_backend` viraria um comentário morto: a rota entraria em produção e o
    // teste de cobertura continuaria pulando ela para sempre.
    let backend = rotas_do_backend();

    let ja_existem: Vec<String> = ROUTES
        .iter()
        .filter(|s| s.pending_backend)
        .map(|s| format!("{} {}", s.method.as_str(), s.template))
        .filter(|r| backend.contains(&r.as_str()))
        .collect();

    assert!(
        ja_existem.is_empty(),
        "estas rotas já estão em produção — tire o `pending` delas em routes.rs:\n  {}",
        ja_existem.join("\n  ")
    );
}
