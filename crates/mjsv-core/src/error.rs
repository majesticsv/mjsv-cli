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
    /// `ORG`/sqid para rota de cliente e `ROOT`/`0000` para rota administrativa, e a rota rejeita
    /// o par que não for o dela.
    #[error("acesso negado a {path} (enviado MJSV-TargetType: {}, MJSV-TargetId: {})\n\
             confira se o comando é de escopo de organização (--org) ou administrativo", target.kind, target.id)]
    Forbidden { path: String, target: Box<Target> },

    /// `checkAdminOtpRequired` no backend devolve 428 com `code: OTP_SETUP_REQUIRED` para
    /// qualquer rota administrativa quando a conta não tem 2FA. Não é erro de uso do CLI.
    #[error("esta conta precisa de 2FA para usar rotas administrativas — ative em https://mjsv.io/dashboard/me antes de continuar")]
    OtpSetupRequired,

    #[error("não encontrado: {0}")]
    NotFound(String),

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
