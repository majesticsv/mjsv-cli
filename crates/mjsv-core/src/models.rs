//! Structs escritas à mão para as respostas que os comandos realmente consomem.
//!
//! O spec OpenAPI da API (`/doc-json`) tem os corpos de requisição, mas **nenhum schema de
//! resposta** — 0 de 342 operações. Gerar cliente a partir dele daria tipos vazios, então o que
//! vale aqui é a fonte do backend. O spec continua útil como inventário de rotas, para detectar
//! renomeação, não como fonte de tipos.
//!
//! Todo campo é `Option` ou tem `default`: um campo novo no backend não pode quebrar o CLI, e um
//! campo removido tem de virar coluna vazia, não erro de parse.

use serde::{Deserialize, Serialize};

/// Ids são sqid (texto), nunca número — `prisma.extended` os codifica na saída.
pub type Sqid = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Org {
    pub id: Sqid,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgRole {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub org_id: Option<Sqid>,
    #[serde(default)]
    pub name: Option<String>,
}

/// O que `/auth/sign-in`, `/auth/sign-in/otp` e `/auth/verify` devolvem.
///
/// As orgs vêm daqui, e não de `GET /org` — essa rota é administrativa. É a única fonte de
/// descoberta de organização que um usuário comum tem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub orgs: Vec<Org>,
    #[serde(default)]
    pub org_roles: Vec<OrgRole>,
    /// bigint como texto: o backend troca o JSON.stringify global por json-bigint, então isto
    /// pode passar do alcance de f64.
    #[serde(default)]
    pub system_permissions: Option<serde_json::Value>,
    #[serde(default)]
    pub access_token: Option<String>,
}

/// A resposta parcial do `/auth/sign-in` quando a conta tem 2FA.
///
/// Vem com status 200, não com erro — quem trata o login precisa olhar o campo, não o status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtpChallenge {
    #[serde(default)]
    pub requires_otp: bool,
    #[serde(default)]
    pub otp_login_token: Option<String>,
}

/// Chave de API. **Sem o segredo**: `GET /auth/token` não o devolve mais; ele aparece uma única
/// vez, na criação.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiToken {
    pub id: Sqid,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Só preenchido na resposta do POST.
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: Sqid,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Uma VM.
///
/// A API tem **dois** modelos de VM e a listagem devolve os dois no mesmo array: o legado
/// (`VirtualMachine`, com `domainName`/`memory`/`OS`) e o atual sobre VPC (`VirtTenantVm`, com
/// `name`/`memoryMb`/`osName`). O `getOrgVirtualMachines` faz `virtTenantVm ?? virtualMachine` e
/// não marca qual é qual, então os campos equivalentes são unificados com `alias` — sem isso, uma
/// VM legada apareceria com todas as colunas vazias em vez de dar erro.
///
/// `serviceId` é o identificador que importa: as rotas são `/vm/:vmServiceId`, chaveadas pelo
/// serviço, não pelo id da VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vm {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub service_id: Option<Sqid>,
    #[serde(default, alias = "domainName")]
    pub name: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default, alias = "stateMessage")]
    pub state_msg: Option<String>,
    #[serde(default)]
    pub vcpu: Option<i64>,
    #[serde(default, alias = "memory")]
    pub memory_mb: Option<i64>,
    #[serde(default, alias = "OS")]
    pub os_name: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameServer {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub service_id: Option<Sqid>,
    #[serde(default)]
    pub name: Option<String>,
    /// A API usa `status` em alguns caminhos e `state` em outros; os dois são lidos.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

/// Uma regra de firewall — serve também para as faixas do gameguard, que compartilham os campos de
/// porta.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallRule {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub position: Option<i64>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub port_start: Option<i64>,
    #[serde(default)]
    pub port_end: Option<i64>,
    #[serde(default)]
    pub source_cidr: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Só nas faixas de gameguard.
    #[serde(default)]
    pub profile: Option<String>,
}

/// O corpo de `GET /vm/:id/firewall`.
///
/// Os nomes das coleções mudaram de lugar entre versões da API, então os dois aparecem como alias;
/// o `default` é o que impede uma VM sem gameguard de virar erro de parse.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallView {
    #[serde(default, alias = "firewallRules")]
    pub rules: Vec<FirewallRule>,
    #[serde(default, alias = "gameguardRanges")]
    pub gameguard: Vec<FirewallRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip {
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    /// O IP privado da VM para onde este EIP faz NAT.
    #[serde(default)]
    pub attached_private_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EipV6 {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

/// O corpo de `GET /vm/:id/eips`.
///
/// Não é uma lista: é `{ data: { v4, v6, v6Addresses } }`. O IPv6 aparece como **prefixo delegado**
/// (`/64`) e, separado, os endereços que o roteador de fato aprendeu dentro dele — um prefixo
/// atribuído sem endereço aprendido significa que o convidado ainda não configurou o v6.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EipView {
    #[serde(default)]
    pub v4: Vec<Eip>,
    #[serde(default)]
    pub v6: Vec<EipV6>,
    #[serde(default)]
    pub v6_addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub status: Option<String>,
    /// Valor **em centavos**, inteiro. `totalAmount` é o que o cliente paga; `rawAmount` é o preço
    /// antes de cupom/crédito, e serve de reserva porque nem toda rota devolve os dois.
    ///
    /// `i64` e não `f64`: dinheiro não sobrevive a uma ida e volta por ponto flutuante, que é a
    /// mesma razão de o crate usar `arbitrary_precision`.
    #[serde(default)]
    pub total_amount: Option<i64>,
    #[serde(default)]
    pub raw_amount: Option<i64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

impl Order {
    /// Centavos para texto, com a divisão feita em inteiros — nada de `/ 100.0`.
    pub fn valor_formatado(&self) -> String {
        let Some(centavos) = self.total_amount.or(self.raw_amount) else {
            return "-".to_string();
        };

        let sinal = if centavos < 0 { "-" } else { "" };
        let abs = centavos.unsigned_abs();
        let valor = format!("{sinal}{}.{:02}", abs / 100, abs % 100);

        match self.currency.as_deref() {
            Some("BRL") | None => format!("R$ {valor}"),
            Some(outra) => format!("{valor} {outra}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub category: Option<TicketCategory>,
    /// `GET /ticket` NÃO devolve `createdAt` — a data só aparece nas mensagens. Fica como `Option`
    /// porque outras rotas trazem, mas a listagem não pode mostrar uma coluna que é sempre vazia.
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketCategory {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketAuthor {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketMessage {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub content: Option<String>,
    /// `GET /ticket/:id/messages` devolve só `authorId`; o objeto `author` (com nome e e-mail) vem
    /// apenas pelo `GET /ticket/:id`, que usa outro serviço. Os dois são lidos para a exibição não
    /// depender de qual rota trouxe a mensagem.
    #[serde(default)]
    pub author: Option<TicketAuthor>,
    #[serde(default)]
    pub author_id: Option<Sqid>,
    #[serde(default)]
    pub is_staff: Option<bool>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    #[serde(default)]
    pub id: Option<Sqid>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Envelope de item único (`{ data }`), distinto do `Listing`, que traz `{ data, count }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub data: T,
}

/// Envelope de listagem usado pela API (`{ data, count }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing<T> {
    #[serde(default = "Vec::new")]
    pub data: Vec<T>,
    #[serde(default)]
    pub count: Option<i64>,
}

impl<T: serde::de::DeserializeOwned> Listing<T> {
    /// Aceita o envelope **ou** um array cru.
    ///
    /// A API não é consistente nisto: `GET /order` devolve `{data, count}` e `GET /order/pending`
    /// devolve `[...]` direto. Sem tratar os dois, o segundo falha com
    /// `invalid type: map, expected a sequence` — um erro que parece bug do CLI e é diferença de
    /// contrato entre duas rotas do mesmo controller.
    pub fn from_value(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        if value.is_array() {
            let data: Vec<T> = serde_json::from_value(value)?;
            let count = data.len() as i64;
            return Ok(Listing {
                data,
                count: Some(count),
            });
        }

        serde_json::from_value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_aceita_campo_novo_do_backend() {
        // Um campo que o CLI não conhece não pode virar erro de parse — senão qualquer deploy do
        // backend quebra todos os binários já instalados.
        let json = serde_json::json!({
            "id": "Mjjp5",
            "username": "gabriel",
            "orgs": [{"id": "K3xQ7", "name": "Majestic"}],
            "campoQueAindaNaoExiste": 42
        });

        let id: Identity = serde_json::from_value(json).unwrap();
        assert_eq!(id.username.as_deref(), Some("gabriel"));
        assert_eq!(id.orgs.len(), 1);
        // Ausentes viram None, não erro.
        assert!(id.email.is_none());
    }

    #[test]
    fn o_desafio_de_otp_vem_com_200_e_nao_com_erro() {
        let json = serde_json::json!({"requiresOtp": true, "otpLoginToken": "abc"});
        let c: OtpChallenge = serde_json::from_value(json).unwrap();

        assert!(c.requires_otp);
        assert_eq!(c.otp_login_token.as_deref(), Some("abc"));
    }

    #[test]
    fn login_sem_otp_nao_e_desafio() {
        let json = serde_json::json!({"id": "Mjjp5", "accessToken": "jwt"});
        let c: OtpChallenge = serde_json::from_value(json.clone()).unwrap();
        assert!(!c.requires_otp);

        let id: Identity = serde_json::from_value(json).unwrap();
        assert_eq!(id.access_token.as_deref(), Some("jwt"));
    }

    #[test]
    fn vm_do_modelo_atual_virt_tenant() {
        let vm: Vm = serde_json::from_value(serde_json::json!({
            "id": "K3xQ7", "serviceId": "y3zqv", "name": "ascp-vm",
            "state": "active", "vcpu": 2, "memoryMb": 4096, "osName": "ubuntu-24.04"
        }))
        .unwrap();

        assert_eq!(vm.name.as_deref(), Some("ascp-vm"));
        assert_eq!(vm.memory_mb, Some(4096));
        assert_eq!(vm.service_id.as_deref(), Some("y3zqv"));
    }

    #[test]
    fn vm_do_modelo_legado_cai_nos_mesmos_campos() {
        // A listagem devolve os dois modelos no mesmo array, sem marcar qual é qual. Sem os
        // aliases, uma VM legada renderizaria com todas as colunas vazias em vez de dar erro —
        // pior que falhar, porque parece que a VM está quebrada.
        let vm: Vm = serde_json::from_value(serde_json::json!({
            "id": "K3xQ7", "serviceId": "y3zqv", "domainName": "vm-antiga",
            "state": "running", "vcpu": 1, "memory": 2048, "OS": "debian-12"
        }))
        .unwrap();

        assert_eq!(vm.name.as_deref(), Some("vm-antiga"));
        assert_eq!(vm.memory_mb, Some(2048));
        assert_eq!(vm.os_name.as_deref(), Some("debian-12"));
    }

    #[test]
    fn listagem_aceita_envelope_e_array_cru() {
        // `GET /order` devolve {data,count}; `GET /order/pending` devolve um array. As duas rotas
        // são do mesmo controller e discordam do envelope.
        let envelope = serde_json::json!({"data": [{"id": "njLME"}], "count": 1});
        let cru = serde_json::json!([{"id": "njLME"}]);

        for v in [envelope, cru] {
            let l: Listing<Order> = Listing::from_value(v).unwrap();
            assert_eq!(l.data.len(), 1);
            assert_eq!(l.count, Some(1));
        }
    }

    #[test]
    fn valor_sai_de_centavos_sem_passar_por_float() {
        let com = |total: Option<i64>, raw: Option<i64>, moeda: &str| {
            Order {
                id: None,
                status: None,
                total_amount: total,
                raw_amount: raw,
                currency: Some(moeda.into()),
                due_date: None,
                created_at: None,
            }
            .valor_formatado()
        };

        assert_eq!(com(Some(5500), None, "BRL"), "R$ 55.00");
        assert_eq!(com(Some(11600), None, "BRL"), "R$ 116.00");
        // O centavo sobrevive: 5 centavos não vira 0.05000000000000001.
        assert_eq!(com(Some(5), None, "BRL"), "R$ 0.05");
        assert_eq!(com(Some(-1250), None, "BRL"), "R$ -12.50");
        // `rawAmount` só é usado quando não há `totalAmount`.
        assert_eq!(com(None, Some(999), "BRL"), "R$ 9.99");
        assert_eq!(com(Some(100), Some(999), "BRL"), "R$ 1.00");
        assert_eq!(com(Some(5500), None, "USD"), "55.00 USD");
        assert_eq!(com(None, None, "BRL"), "-");
    }

    #[test]
    fn permissao_grande_nao_e_arredondada() {
        // json-bigint no backend: o valor pode passar do alcance exato de f64. Guardamos como
        // Value com arbitrary_precision para o dígito não se perder na volta.
        let json = serde_json::json!({"id": "Mjjp5", "systemPermissions": 9007199254740993i64});
        let id: Identity = serde_json::from_value(json).unwrap();

        assert_eq!(id.system_permissions.unwrap().to_string(), "9007199254740993");
    }
}
