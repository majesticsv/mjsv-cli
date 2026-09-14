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

/// Envelope de listagem usado pela API (`{ data, count }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing<T> {
    #[serde(default = "Vec::new")]
    pub data: Vec<T>,
    #[serde(default)]
    pub count: Option<i64>,
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
    fn permissao_grande_nao_e_arredondada() {
        // json-bigint no backend: o valor pode passar do alcance exato de f64. Guardamos como
        // Value com arbitrary_precision para o dígito não se perder na volta.
        let json = serde_json::json!({"id": "Mjjp5", "systemPermissions": 9007199254740993i64});
        let id: Identity = serde_json::from_value(json).unwrap();

        assert_eq!(id.system_permissions.unwrap().to_string(), "9007199254740993");
    }
}
