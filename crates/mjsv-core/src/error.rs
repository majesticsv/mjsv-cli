//! Tradução de erro da API para mensagem acionável.
//!
//! O CLI é usado em terminal e em script, então o custo de um erro cru é alto: um `403` sem
//! contexto manda o usuário procurar permissão quando o problema quase sempre é outro — o par de
//! cabeçalhos enviado. Por isso o erro carrega o alvo junto.

use crate::target::Target;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("sessão expirada ou token revogado — rode `mjsv login`")]
    Unauthorized,

    /// O 403 quase nunca é falta de permissão de verdade: é o alvo errado. O painel manda
    /// `ORG`/sqid para rota de organização, e a rota rejeita o par que não for o dela.
    #[error("acesso negado a {path} (enviado MJSV-TargetType: {}, MJSV-TargetId: {})\n\
             confira se o comando é de escopo de organização e se --org aponta para a org certa", target.kind, target.id)]
    Forbidden { path: String, target: Box<Target> },

    /// A API devolve 428 com `code: OTP_SETUP_REQUIRED` quando a conta não tem 2FA.
    /// Não é erro de uso do CLI.
    #[error("esta conta precisa de 2FA para esta operação — ative em https://mjsv.io/dashboard/me antes de continuar")]
    OtpSetupRequired,

    #[error("não encontrado: {0}")]
    NotFound(String),

    /// Comando de escopo de organização sem organização selecionada.
    ///
    /// Era um `Api { status: 0, .. }`, e o `Display` desse variante vazava o `0:` na frente da
    /// mensagem. Variante própria também deixa o renderizador desenhar o cartão certo.
    #[error("nenhuma organização selecionada — rode `mjsv org list` e depois `mjsv org use <id>`")]
    NoOrg,

    #[error("{status}: {message}")]
    Api { status: u16, message: String },

    #[error("falha de rede ao falar com {url}: {source}")]
    Transport {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("resposta inesperada da API em {path}: {message}")]
    Decode { path: String, message: String },
}

/// Em que etapa do login a API recusou. Um 401 de quem nunca teve sessão não é "sessão expirada".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStage {
    /// Não se sabe em que etapa foi recusado: o texto único da especificação.
    Any,
    Password,
    OtpCode,
    RecoveryCode,
}

/// O `sign-in` foi recusado (usuário, senha ou código 2FA).
#[derive(Debug)]
pub struct LoginRefused {
    pub stage: LoginStage,
}

impl std::fmt::Display for LoginRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.stage {
            LoginStage::Any => "login recusado: usuário, senha ou código 2FA não conferem",
            LoginStage::Password => "login recusado: usuário ou senha não conferem",
            LoginStage::OtpCode => "login recusado: código 2FA não confere",
            LoginStage::RecoveryCode => "login recusado: código de recuperação não confere",
        })
    }
}

impl std::error::Error for LoginRefused {}

impl LoginRefused {
    /// Converte um 401 do sign-in em recusa de login; qualquer outro erro passa como veio.
    pub fn from_unauthorized(e: ApiError, stage: LoginStage) -> anyhow::Error {
        match e {
            ApiError::Unauthorized => anyhow::Error::new(LoginRefused { stage }),
            other => anyhow::Error::new(other),
        }
    }
}

/// `mjsv login --token` com uma chave que a API não aceitou.
#[derive(Debug, thiserror::Error)]
#[error("a chave informada não foi aceita pela API: sessão expirada ou token revogado — rode `mjsv login`")]
pub struct TokenRefused;

/// `mjsv org use` com uma organização que não é da conta.
#[derive(Debug)]
pub struct OrgNotInAccount {
    pub org: String,
    pub available: Vec<String>,
}

impl std::fmt::Display for OrgNotInAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lista = if self.available.is_empty() {
            "nenhuma".to_string()
        } else {
            self.available.join(", ")
        };
        write!(
            f,
            "organização {} não pertence a esta conta (disponíveis: {lista})",
            self.org
        )
    }
}

impl std::error::Error for OrgNotInAccount {}

/// Extrai a mensagem de um corpo de erro do Nest.
///
/// O Nest responde `{ "message": "..." }` ou `{ "message": ["...", "..."] }` — a segunda forma vem
/// do class-validator, com um item por campo inválido. As duas precisam virar texto legível.
pub fn message_from_body(body: &serde_json::Value, fallback: &str) -> String {
    match body.get("message") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(items)) => items.iter().filter_map(|i| i.as_str()).collect::<Vec<_>>().join("; "),
        _ => fallback.to_string(),
    }
}

/// O `code` que distingue o 428 de setup de OTP de qualquer outra precondição.
pub fn is_otp_setup_required(body: &serde_json::Value) -> bool {
    body.get("code").and_then(|c| c.as_str()) == Some("OTP_SETUP_REQUIRED")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mensagem_de_string_simples() {
        assert_eq!(
            message_from_body(&json!({"message": "Token inválido."}), "x"),
            "Token inválido."
        )
    }

    #[test]
    fn mensagem_de_array_do_class_validator() {
        let body = json!({"message": ["E-mail inválido", "Senha não pode estar vazia"]});
        assert_eq!(
            message_from_body(&body, "x"),
            "E-mail inválido; Senha não pode estar vazia"
        );
    }

    #[test]
    fn sem_message_usa_o_fallback() {
        assert_eq!(
            message_from_body(&json!({"foo": 1}), "erro desconhecido"),
            "erro desconhecido"
        );
    }

    #[test]
    fn reconhece_o_428_de_otp() {
        assert!(is_otp_setup_required(&json!({"code": "OTP_SETUP_REQUIRED"})));
        assert!(!is_otp_setup_required(&json!({"code": "OUTRA_COISA"})));
        assert!(!is_otp_setup_required(&json!({})));
    }

    #[test]
    fn o_403_mostra_o_alvo_enviado() {
        let e = ApiError::Forbidden {
            path: "/vm".into(),
            target: Box::new(crate::target::Target::org("Mjjp5")),
        };
        let texto = e.to_string();
        assert!(texto.contains("ORG"), "{texto}");
        assert!(texto.contains("Mjjp5"), "{texto}");
    }
}
