//! Inventário das rotas que os comandos chamam.
//!
//! Declarar as rotas em um só lugar serve a duas coisas que valem mais do que a economia de
//! linhas:
//!
//! 1. **Escopo é propriedade da rota, não escolha do comando.** O par
//!    `MJSV-TargetId`/`MJSV-TargetType` é conferido contra o id do caminho por cada handler do
//!    backend, então ele é derivável da rota. Com a tabela aqui, um teste percorre tudo de uma vez
//!    em vez de depender de alguém lembrar em cada subcomando.
//! 2. **Renomeação no backend vira teste vermelho.** `tests/contract.rs` confere esta lista contra
//!    o inventário de rotas do `/doc-json` vendorizado.

use crate::client::Method;

/// De onde sai o par de cabeçalhos de uma rota.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `ROOT` + `0000` — contexto administrativo, e também o que o painel manda em rota de conta.
    Root,
    /// `ORG` + o sqid da org, que também é o `:id` do caminho.
    Org,
    Vm,
    GameServer,
    Ticket,
}

#[derive(Debug, Clone, Copy)]
pub struct RouteSpec {
    pub method: Method,
    /// Com o parâmetro no formato do OpenAPI (`{id}`), para casar com o inventário.
    pub template: &'static str,
    pub scope: Scope,
    /// Rota que só existe depois de um deploy do backend ainda não feito.
    ///
    /// Serve de lembrete executável: o teste de contrato cobra a remoção da marca assim que a rota
    /// aparecer no inventário atualizado.
    pub pending_backend: bool,
}

const fn r(method: Method, template: &'static str, scope: Scope) -> RouteSpec {
    RouteSpec {
        method,
        template,
        scope,
        pending_backend: false,
    }
}

/// Sem uso no momento — a última rota pendente entrou em produção em 2026-09-08.
///
/// Fica porque o mecanismo é o jeito de declarar uma rota que depende de deploy do backend sem
/// deixar o teste de contrato vermelho enquanto isso, e as fases seguintes do CLI vão precisar dele
/// de novo. O teste `rota_pendente_que_ja_existe_tem_de_perder_a_marca` garante que a marca não
/// sobreviva ao deploy.
#[allow(dead_code)]
const fn pending(method: Method, template: &'static str, scope: Scope) -> RouteSpec {
    RouteSpec {
        method,
        template,
        scope,
        pending_backend: true,
    }
}

pub const ROUTES: &[RouteSpec] = &[
    r(Method::Post, "/auth/sign-in", Scope::Root),
    r(Method::Post, "/auth/sign-in/otp", Scope::Root),
    r(Method::Post, "/auth/sign-in/recovery", Scope::Root),
    r(Method::Post, "/auth/sign-out", Scope::Root),
    r(Method::Get, "/auth/verify", Scope::Root),
    r(Method::Get, "/auth/token", Scope::Root),
    r(Method::Post, "/auth/token", Scope::Root),
    // Substituiu `DELETE /auth/token`, que revogava o token do próprio header — uma chave nomeada
    // não era revogável a partir de uma sessão. Em produção desde 2026-09-08.
    r(Method::Delete, "/auth/token/{tokenId}", Scope::Root),
    r(Method::Get, "/org/{id}/virtualmachine", Scope::Org),
    r(Method::Get, "/org/{id}/gameserver", Scope::Org),
    // `{vmServiceId}` é o id do SERVIÇO, não o da VM, e o cabeçalho de alvo é `VIRTUALMACHINE` com
    // esse mesmo valor — é o que todo handler de `/vm/:vmServiceId` re-confere.
    r(Method::Get, "/vm/{vmServiceId}", Scope::Vm),
    r(Method::Get, "/vm/{vmServiceId}/power", Scope::Vm),
    r(Method::Get, "/vm/{vmServiceId}/metrics", Scope::Vm),
    r(Method::Get, "/vm/{vmServiceId}/vnc", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/start", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/stop", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/reboot", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/kill", Scope::Vm),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nenhuma_rota_duplicada() {
        let mut vistas: Vec<(&str, &str)> = ROUTES.iter().map(|r| (r.method.as_str(), r.template)).collect();
        let antes = vistas.len();
        vistas.sort_unstable();
        vistas.dedup();

        assert_eq!(vistas.len(), antes, "há rota declarada duas vezes");
    }

    #[test]
    fn rota_de_org_tem_o_id_no_caminho() {
        // O invariante da API: o id do cabeçalho é o mesmo do caminho. Uma rota de escopo ORG sem
        // `{id}` no template significa que o escopo está errado, e o resultado seria 403.
        for spec in ROUTES.iter().filter(|s| s.scope == Scope::Org) {
            assert!(
                spec.template.contains("/org/{id}"),
                "{} {} está marcada como ORG mas não tem o id no caminho",
                spec.method.as_str(),
                spec.template
            );
        }
    }

    #[test]
    fn rota_de_vm_e_chaveada_pelo_servico() {
        // O parâmetro se chama `vmServiceId` de propósito: passar o id da VM aí devolve 403,
        // porque o handler compara o cabeçalho com o valor do caminho e ambos têm de ser o
        // serviço.
        for spec in ROUTES.iter().filter(|s| s.scope == Scope::Vm) {
            assert!(
                spec.template.contains("/vm/{vmServiceId}"),
                "{} {} é de escopo VM mas não usa vmServiceId",
                spec.method.as_str(),
                spec.template
            );
        }
    }

    #[test]
    fn rota_de_escopo_root_nao_tem_parametro_de_dono() {
        for spec in ROUTES.iter().filter(|s| s.scope == Scope::Root) {
            assert!(
                !spec.template.contains("/org/{"),
                "{} {} é ROOT mas o caminho é de organização",
                spec.method.as_str(),
                spec.template
            );
        }
    }
}
