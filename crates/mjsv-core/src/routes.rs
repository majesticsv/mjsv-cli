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
    /// `ROOT` + `0000` — o que o painel manda em rota de conta.
    Root,
    /// `ORG` + o sqid da org, que também é o `:id` do caminho.
    Org,
    Vm,
    GameServer,
    Ticket,
    /// `SERVICE` + o id do serviço — rotas que recebem o serviço sem saber o tipo do recurso
    /// (`/vm/service/:serviceId`, `/gameserver/service/:serviceId`, `/protection/:serviceId`).
    Service,
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
    // `/vm/admin/cloud-images` não é rota administrativa: aceita a flag de usuário VM_LIST com alvo
    // ORG e é o que alimenta o seletor de imagens do painel. `/vm/service/{serviceId}` é a única
    // rota de VM com alvo SERVICE (as demais são VIRTUALMACHINE).
    r(Method::Get, "/storage-volumes/org/{orgId}", Scope::Org),
    r(
        Method::Get,
        "/storage-volumes/org/{orgId}/available/vm/{vmId}",
        Scope::Org,
    ),
    r(Method::Post, "/storage-volumes/org/{orgId}/{volId}/detach", Scope::Org),
    r(Method::Post, "/storage-volumes/org/{orgId}/{volId}/attach", Scope::Org),
    r(
        Method::Get,
        "/storage-products/available/org/{orgId}/vm/{vmId}",
        Scope::Org,
    ),
    r(Method::Get, "/protection/{serviceId}", Scope::Service),
    r(Method::Get, "/protection/{serviceId}/telemetry", Scope::Service),
    r(Method::Get, "/protection/{serviceId}/incidents", Scope::Service),
    r(
        Method::Get,
        "/protection/{serviceId}/incidents/{incidentId}",
        Scope::Service,
    ),
    r(
        Method::Get,
        "/protection/{serviceId}/addresses/{addrId}/firewall",
        Scope::Service,
    ),
    r(
        Method::Put,
        "/protection/{serviceId}/addresses/{addrId}/firewall/gameguard",
        Scope::Service,
    ),
    r(
        Method::Delete,
        "/protection/{serviceId}/addresses/{addrId}/firewall/gameguard/{rangeId}",
        Scope::Service,
    ),
    r(Method::Delete, "/virt-tenants/org/{orgId}/{tenantId}", Scope::Org),
    r(Method::Get, "/virt-regions", Scope::Org),
    r(Method::Get, "/virt-tenants/org/{orgId}", Scope::Org),
    r(Method::Post, "/virt-tenants/org/{orgId}", Scope::Org),
    r(Method::Get, "/vm/admin/cloud-images", Scope::Org),
    r(
        Method::Delete,
        "/vm/{vmServiceId}/firewall/gameguard/{rangeId}",
        Scope::Vm,
    ),
    r(Method::Get, "/vm/service/{serviceId}", Scope::Service),
    r(Method::Get, "/vm/{vmServiceId}/incidents", Scope::Vm),
    r(Method::Get, "/vm/{vmServiceId}/incidents/{incidentId}", Scope::Vm),
    r(Method::Get, "/vm/{vmServiceId}/isos", Scope::Vm),
    r(Method::Get, "/vm/{vmServiceId}/vpcs", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/move", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/rebuild", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/reset-password", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/root-password", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/sync-dhcp-ip", Scope::Vm),
    r(Method::Post, "/vm/{vmServiceId}/vnc-token", Scope::Vm),
    r(Method::Put, "/vm/{vmServiceId}", Scope::Vm),
    r(Method::Put, "/vm/{vmServiceId}/firewall/gameguard", Scope::Vm),
    r(Method::Put, "/vm/{vmServiceId}/firewall/rules/{ruleId}", Scope::Vm),
    r(Method::Put, "/vm/{vmServiceId}/ssh-keys", Scope::Vm),
    r(Method::Get, "/gameserver/service/{serviceId}", Scope::Service),
    r(
        Method::Put,
        "/gameserver/{gameServerServiceId}/startup",
        Scope::GameServer,
    ),
    r(
        Method::Get,
        "/gameserver/{gameServerServiceId}/directories",
        Scope::GameServer,
    ),
    r(
        Method::Get,
        "/gameserver/{gameServerServiceId}/download",
        Scope::GameServer,
    ),
    r(Method::Get, "/gameserver/{gameServerServiceId}/file", Scope::GameServer),
    r(
        Method::Get,
        "/gameserver/{gameServerServiceId}/manager-lock",
        Scope::GameServer,
    ),
    r(Method::Put, "/gameserver/{gameServerServiceId}/file", Scope::GameServer),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/directory",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/files/compress",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/files/decompress",
        Scope::GameServer,
    ),
    r(
        Method::Delete,
        "/gameserver/{gameServerServiceId}/file",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/file",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/addons",
        Scope::GameServer,
    ),
    r(
        Method::Delete,
        "/gameserver/{gameServerServiceId}/addons",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/addons/updates",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/database/{databaseName}/password",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/database",
        Scope::GameServer,
    ),
    r(
        Method::Delete,
        "/gameserver/{gameServerServiceId}/database/{databaseId}",
        Scope::GameServer,
    ),
    r(
        Method::Get,
        "/gameserver/{gameServerServiceId}/schedules/{scheduleId}",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/schedules",
        Scope::GameServer,
    ),
    r(
        Method::Put,
        "/gameserver/{gameServerServiceId}/schedules/{scheduleId}",
        Scope::GameServer,
    ),
    r(
        Method::Delete,
        "/gameserver/{gameServerServiceId}/schedules/{scheduleId}",
        Scope::GameServer,
    ),
    r(
        Method::Post,
        "/gameserver/{gameServerServiceId}/schedules/{scheduleId}/tasks",
        Scope::GameServer,
    ),
    r(
        Method::Put,
        "/gameserver/{gameServerServiceId}/schedules/{scheduleId}/tasks/{taskId}",
        Scope::GameServer,
    ),
    r(
        Method::Delete,
        "/gameserver/{gameServerServiceId}/schedules/{scheduleId}/tasks/{taskId}",
        Scope::GameServer,
    ),
    // Catálogo: não é de ninguém, então o guard só exige sessão + permissão de gameserver. O painel
    // manda ORG com a org ativa, e é isso que o CLI faz — não há id de org no caminho.
    r(Method::Get, "/larder/{game}/setup", Scope::Org),
    r(Method::Get, "/larder/{game}/fields", Scope::Org),
    r(Method::Get, "/larder/{game}/options/{field}", Scope::Org),
    r(Method::Get, "/larder/{game}/search/modpacks", Scope::Org),
    r(Method::Get, "/larder/{game}/modpack/{provider}/{id}", Scope::Org),
    r(Method::Get, "/larder/{game}/search/addons", Scope::Org),
    r(Method::Get, "/larder/{game}/addons/categories", Scope::Org),
    r(Method::Get, "/larder/{game}/addon/{provider}/{id}", Scope::Org),
    r(Method::Get, "/larder/{game}/addon/{provider}/{id}/versions", Scope::Org),
    r(Method::Post, "/larder/{game}/addons/resolve", Scope::Org),
    r(Method::Post, "/larder/{game}/addons/lookup", Scope::Org),
    r(Method::Post, "/larder/{game}/addons/updates", Scope::Org),
    r(Method::Post, "/larder/{game}/lookup", Scope::Org),
    r(Method::Post, "/larder/{game}/resolve", Scope::Org),
    r(Method::Get, "/notifications/org/{orgId}", Scope::Org),
    r(Method::Get, "/notifications/org/{orgId}/count", Scope::Org),
    r(Method::Get, "/notifications/org/{orgId}/count-by-type", Scope::Org),
    r(Method::Get, "/notifications/org/{orgId}/type/{type}", Scope::Org),
    r(
        Method::Patch,
        "/notifications/org/{orgId}/{notificationId}/read",
        Scope::Org,
    ),
    r(Method::Patch, "/notifications/org/{orgId}/read-all", Scope::Org),
    r(
        Method::Delete,
        "/notifications/org/{orgId}/{notificationId}",
        Scope::Org,
    ),
    r(Method::Delete, "/notifications/org/{orgId}/read", Scope::Org),
    r(Method::Get, "/auth/session", Scope::Root),
    r(Method::Get, "/user/me", Scope::Root),
    r(Method::Put, "/user/me", Scope::Root),
    r(Method::Post, "/org", Scope::Root),
    r(Method::Get, "/org/{id}", Scope::Org),
    r(Method::Delete, "/org/{id}", Scope::Org),
    r(Method::Put, "/org/{id}", Scope::Org),
    r(Method::Delete, "/org/{id}/leave", Scope::Org),
    r(Method::Get, "/org/{id}/member", Scope::Org),
    r(Method::Post, "/org/{id}/member", Scope::Org),
    r(Method::Get, "/org/{id}/invite", Scope::Org),
    r(Method::Delete, "/org/{id}/invite/{inviteId}", Scope::Org),
    r(Method::Delete, "/org/{id}/member/{userId}", Scope::Org),
    r(Method::Delete, "/org/{id}/member/{userId}/role", Scope::Org),
    r(Method::Post, "/org/{id}/member/{userId}/role", Scope::Org),
    r(Method::Get, "/org/{id}/role", Scope::Org),
    r(Method::Put, "/org/{id}/positionswap", Scope::Org),
    r(Method::Post, "/org/{id}/role", Scope::Org),
    r(Method::Delete, "/org/{id}/role/{roleId}", Scope::Org),
    r(Method::Put, "/org/{id}/role/{roleId}", Scope::Org),
    r(Method::Get, "/org/{id}/service", Scope::Org),
    r(Method::Get, "/org/{id}/service/{serviceId}", Scope::Org),
    r(Method::Post, "/org/{id}/service/{serviceId}/rename", Scope::Org),
    r(Method::Post, "/org/{id}/service/{serviceId}/cancel", Scope::Org),
    r(Method::Post, "/org/{id}/service/{serviceId}/cancel/undo", Scope::Org),
    r(Method::Get, "/auditlogs/org/{orgId}", Scope::Org),
    // Catálogo, cupom e metadados são públicos (sem alvo); Root é só o rótulo mais próximo.
    r(Method::Get, "/coupon/code/{code}", Scope::Root),
    r(Method::Get, "/product", Scope::Root),
    r(Method::Get, "/product/metadata", Scope::Root),
    r(Method::Get, "/product/{id}", Scope::Root),
    r(Method::Get, "/product/{id}/isos", Scope::Root),
    r(Method::Get, "/order/metadata", Scope::Root),
    r(Method::Get, "/payment/methods", Scope::Root),
    // Checkout não passa pelo guard de permissões: a ordem vem no corpo e a posse é conferida
    // pela participação na org, então o painel manda o alvo de conta.
    r(Method::Post, "/payment/checkout", Scope::Root),
    r(Method::Delete, "/order/{orderId}", Scope::Org),
    r(Method::Post, "/order", Scope::Org),
    r(Method::Post, "/order/group", Scope::Org),
    r(Method::Post, "/order/quote", Scope::Org),
    r(Method::Post, "/order/{orderId}/generate-pdf", Scope::Org),
    // Listagens de ticket por categoria/serviço/mensagem são ORG; só reabrir é TICKET.
    r(Method::Get, "/ticket/category/{categoryId}", Scope::Org),
    r(Method::Get, "/ticket/message/{messageId}", Scope::Org),
    r(Method::Get, "/ticket/service/{serviceId}", Scope::Org),
    r(Method::Put, "/ticket/{ticketId}/open", Scope::Ticket),
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
    fn rota_de_ticket_so_reabrir_e_ticket_e_as_listagens_sao_org() {
        // `/ticket/category/{categoryId}`, `/ticket/service/{serviceId}` e `/ticket/message/{id}`
        // parecem filhas de `/ticket/{ticketId}`, mas o guard delas resolve a org: mandar TICKET
        // aí devolve lista vazia em vez de erro.
        for t in [
            "/ticket/category/{categoryId}",
            "/ticket/service/{serviceId}",
            "/ticket/message/{messageId}",
        ] {
            let spec = ROUTES.iter().find(|s| s.template == t).expect(t);
            assert_eq!(spec.scope, Scope::Org, "{t}");
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
    fn rota_de_notificacao_e_auditoria_traz_o_orgid_no_caminho() {
        // Estas famílias nomeiam o parâmetro `{orgId}` (e não `{id}`) e o handler recusa com 400
        // quando o cabeçalho de alvo não é ORG com o mesmo valor — por isso o escopo é ORG.
        for spec in ROUTES
            .iter()
            .filter(|s| s.template.starts_with("/notifications/org/") || s.template.starts_with("/auditlogs/org/"))
        {
            assert_eq!(spec.scope, Scope::Org, "{} {}", spec.method.as_str(), spec.template);
            assert!(spec.template.contains("/org/{orgId}"), "{}", spec.template);
        }
    }

    #[test]
    fn rota_de_conta_do_usuario_nao_leva_alvo_de_org() {
        // `/user/me`, `/auth/session` e `POST /org` só pedem login: o painel manda ROOT/0000 e o
        // guard não confere nada. Mandar ORG aqui não quebra, mas mente sobre o escopo.
        for (m, t) in [
            ("GET", "/user/me"),
            ("PUT", "/user/me"),
            ("GET", "/auth/session"),
            ("POST", "/org"),
        ] {
            let spec = ROUTES
                .iter()
                .find(|s| s.method.as_str() == m && s.template == t)
                .unwrap_or_else(|| panic!("{m} {t} não está na tabela"));
            assert_eq!(spec.scope, Scope::Root, "{m} {t}");
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

    #[test]
    fn rota_de_escopo_service_traz_o_id_do_servico_no_caminho() {
        // Com alvo SERVICE o handler compara o cabeçalho com `:serviceId` do caminho; um caminho
        // sem ele não teria o que conferir e o 400 de divergência viraria acesso errado.
        for spec in ROUTES.iter().filter(|s| s.scope == Scope::Service) {
            assert!(
                spec.template.contains("{serviceId}"),
                "{} {} é de escopo SERVICE mas não traz serviceId no caminho",
                spec.method.as_str(),
                spec.template
            );
        }
    }

    #[test]
    fn rota_de_volume_e_vpc_leva_a_org_no_caminho_e_no_cabecalho() {
        for spec in ROUTES.iter().filter(|s| s.template.contains("/org/{orgId}")) {
            assert_eq!(
                spec.scope,
                Scope::Org,
                "{} {} tem orgId no caminho mas não está no escopo ORG",
                spec.method.as_str(),
                spec.template
            );
        }
    }

    #[test]
    fn catalogo_e_org_sem_id_de_org_e_gameserver_por_servico_e_service() {
        // O catálogo (`/larder/{game}/...`) não tem recurso dono: o caminho só traz o jogo, e o alvo
        // é a org ativa. Mandar GAMESERVER aqui também passaria no guard, mas exigiria um servidor
        // que o usuário não tem de ter só para pesquisar mods.
        for spec in ROUTES.iter().filter(|s| s.template.starts_with("/larder/")) {
            assert_eq!(spec.scope, Scope::Org, "{} deveria ser ORG", spec.template);
            assert!(spec.template.starts_with("/larder/{game}/"), "{}", spec.template);
        }

        // `GET /gameserver/service/{serviceId}` não sabe o tipo do recurso: SERVICE, não GAMESERVER.
        let por_servico = ROUTES
            .iter()
            .find(|s| s.template == "/gameserver/service/{serviceId}")
            .expect("rota ausente");
        assert_eq!(por_servico.scope, Scope::Service);
    }
}
