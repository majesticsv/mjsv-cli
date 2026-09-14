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
    r(Method::Get, "/vm/{vmServiceId}/eips", Scope::Vm),
    r(Method::Get, "/vm/{vmServiceId}/firewall", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/firewall/rules", Scope::Vm),
    r(Method::Delete, "/vm/{vmServiceId}/firewall/rules/{ruleId}", Scope::Vm),
    // Gameserver. `GET /gameserver` aceita escopo ORG — diferente de `GET /vm`, que é ROOT-only.
    r(Method::Get, "/gameserver", Scope::Org),
    r(Method::Get, "/gameserver/{gameServerServiceId}", Scope::GameServer),
    r(
        Method::Get,
        "/gameserver/{gameServerServiceId}/metadata",
        Scope::GameServer,
    ),
    r(
        Method::Get,
        "/gameserver/{gameServerServiceId}/databases",
        Scope::GameServer,
    ),
    r(
        Method::Get,
        "/gameserver/{gameServerServiceId}/schedules",
        Scope::GameServer,
    ),
    r(
        Method::Get,
        "/gameserver/{gameServerServiceId}/sftpuser",
        Scope::GameServer,
    ),
    // Sem id no caminho DE PROPÓSITO: estas identificam o servidor só pelo cabeçalho de alvo.
    r(Method::Post, "/gameserver/power", Scope::GameServer),
    r(Method::Post, "/gameserver/reinstall", Scope::GameServer),
    // Pedidos: escopo ORG mesmo quando o `orderId` está no caminho — o cabeçalho leva a
    // organização e o handler a usa como filtro de dono.
    r(Method::Get, "/order", Scope::Org),
    r(Method::Get, "/order/pending", Scope::Org),
    r(Method::Get, "/order/{orderId}", Scope::Org),
    r(Method::Post, "/order/{orderId}/cancel", Scope::Org),
    r(Method::Get, "/order/{orderId}/invoice-pdf", Scope::Org),
    // Tickets: a ÚNICA família que mistura dois escopos. Listar/abrir/ver é ORG; ler mensagens,
    // responder e fechar é TICKET, com o id do próprio ticket no cabeçalho.
    r(Method::Get, "/ticket", Scope::Org),
    r(Method::Get, "/ticket/category", Scope::Org),
    r(Method::Get, "/ticket/{ticketId}", Scope::Org),
    r(Method::Post, "/ticket", Scope::Org),
    r(Method::Get, "/ticket/{ticketId}/messages", Scope::Ticket),
    r(Method::Post, "/ticket/{ticketId}/message", Scope::Ticket),
    r(Method::Put, "/ticket/{ticketId}/close", Scope::Ticket),
    r(Method::Get, "/ssh-keys/org/{orgId}", Scope::Org),
    r(Method::Post, "/ssh-keys/org/{orgId}", Scope::Org),
    r(Method::Delete, "/ssh-keys/org/{orgId}/{keyId}", Scope::Org),
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
    fn caminho_de_org_implica_escopo_de_org() {
        // O invariante vale na direção que importa: se o id da org está no caminho, o cabeçalho
        // tem de ser ORG com esse mesmo valor, senão o handler responde `TargetType inválido`.
        //
        // A recíproca NÃO vale: `GET /gameserver` é de escopo ORG e não tem id nenhum no caminho —
        // a org vem só do cabeçalho. Exigir `{id}` aqui rejeitaria uma rota legítima.
        for spec in ROUTES.iter().filter(|s| s.template.contains("/org/{id}")) {
            assert_eq!(
                spec.scope,
                Scope::Org,
                "{} {} tem id de org no caminho mas não está no escopo ORG",
                spec.method.as_str(),
                spec.template
            );
        }
    }

    #[test]
    fn rota_de_gameserver_ou_traz_o_id_no_caminho_ou_so_no_cabecalho() {
        // `POST /gameserver/power` e `/reinstall` não têm id no caminho: o handler compara
        // `req.targetNumberId` com o `serviceId` do gameserver, ou seja, o alvo é a ÚNICA fonte.
        // Mandá-las sem cabeçalho de alvo não dá 404, dá acesso ao servidor errado ou 400.
        const SEM_ID_NO_CAMINHO: &[&str] = &["/gameserver/power", "/gameserver/reinstall"];

        for spec in ROUTES.iter().filter(|s| s.scope == Scope::GameServer) {
            assert!(
                spec.template.contains("{gameServerServiceId}") || SEM_ID_NO_CAMINHO.contains(&spec.template),
                "{} {} é de escopo GAMESERVER mas não declara de onde vem o id",
                spec.method.as_str(),
                spec.template
            );
        }
    }

    #[test]
    fn rota_de_ticket_com_escopo_ticket_traz_o_id_no_caminho() {
        // Os tickets são a única família que mistura escopos: ORG para listar/abrir/ver, TICKET
        // para mensagens/resposta/fechamento. Mandar o alvo errado NÃO dá 403 — o handler faz
        // `return new BadRequestException(...)` em vez de `throw`, o que vira 200 com corpo de
        // erro, e o comando parece ter funcionado sem fazer nada.
        for spec in ROUTES.iter().filter(|s| s.scope == Scope::Ticket) {
            assert!(
                spec.template.contains("/ticket/{ticketId}"),
                "{} {} é de escopo TICKET mas não traz o ticketId no caminho",
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
