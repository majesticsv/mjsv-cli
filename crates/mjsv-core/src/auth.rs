//! Login, logout e identidade.

use serde_json::{json, Value};

use crate::client::{Client, Request, SERVICE_ID};
use crate::error::ApiError;
use crate::models::{Identity, OtpChallenge};
use crate::target::Target;

/// O que o `sign-in` devolve: ou a sessão pronta, ou um desafio de 2FA.
#[derive(Debug, Clone)]
pub enum LoginStep {
    Done(Box<Identity>),
    NeedsOtp { otp_login_token: String },
}

fn interpret_sign_in(value: Value) -> Result<LoginStep, ApiError> {
    let challenge: OtpChallenge = serde_json::from_value(value.clone()).map_err(|e| ApiError::Decode {
        path: "/auth/sign-in".into(),
        message: e.to_string(),
    })?;

    // O 2FA chega como 200 com `requiresOtp: true`, não como erro — olhar só o status faria o CLI
    // achar que logou e seguir sem token.
    if challenge.requires_otp {
        let otp_login_token = challenge.otp_login_token.ok_or_else(|| ApiError::Decode {
            path: "/auth/sign-in".into(),
            message: "requiresOtp veio sem otpLoginToken".into(),
        })?;

        return Ok(LoginStep::NeedsOtp { otp_login_token });
    }

    let identity: Identity = serde_json::from_value(value).map_err(|e| ApiError::Decode {
        path: "/auth/sign-in".into(),
        message: e.to_string(),
    })?;

    Ok(LoginStep::Done(Box::new(identity)))
}

/// Monta o `POST /auth/sign-in`.
///
/// `username` e `email` são ambos opcionais no DTO; mandamos o identificador no campo que casa com
/// o formato para não depender de o backend adivinhar.
pub fn sign_in_request(identifier: &str, password: &str) -> Request {
    let mut body = json!({ "password": password, "serviceId": SERVICE_ID });

    if identifier.contains('@') {
        body["email"] = json!(identifier);
    } else {
        body["username"] = json!(identifier);
    }

    Request::post("/auth/sign-in").body(body)
}

pub fn sign_in_otp_request(otp_login_token: &str, code: &str) -> Request {
    Request::post("/auth/sign-in/otp").body(json!({
        "otpLoginToken": otp_login_token,
        "otpCode": code,
        "serviceId": SERVICE_ID,
    }))
}

pub fn sign_in_recovery_request(otp_login_token: &str, recovery_code: &str) -> Request {
    Request::post("/auth/sign-in/recovery").body(json!({
        "otpLoginToken": otp_login_token,
        "recoveryCode": recovery_code,
        "serviceId": SERVICE_ID,
    }))
}

/// `GET /auth/verify` — valida o token guardado e devolve identidade + orgs.
///
/// É também o único jeito de um usuário comum descobrir suas organizações: `GET /org` é
/// administrativo.
pub fn verify_request() -> Request {
    Request::get("/auth/verify").target(Target::root())
}

pub async fn sign_in(client: &Client, identifier: &str, password: &str) -> Result<LoginStep, ApiError> {
    let value = client.send_value(&sign_in_request(identifier, password)).await?;
    interpret_sign_in(value)
}

pub async fn sign_in_otp(client: &Client, otp_login_token: &str, code: &str) -> Result<Identity, ApiError> {
    client.send(&sign_in_otp_request(otp_login_token, code)).await
}

pub async fn sign_in_recovery(client: &Client, otp_login_token: &str, code: &str) -> Result<Identity, ApiError> {
    client.send(&sign_in_recovery_request(otp_login_token, code)).await
}

pub async fn verify(client: &Client) -> Result<Identity, ApiError> {
    client.send(&verify_request()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identificador_com_arroba_vai_como_email() {
        let req = sign_in_request("ana@acme.com.br", "s");
        let body = req.body.unwrap();

        assert_eq!(body["email"], "ana@acme.com.br");
        assert!(body.get("username").is_none());
    }

    #[test]
    fn identificador_sem_arroba_vai_como_username() {
        let req = sign_in_request("gabriel", "s");
        let body = req.body.unwrap();

        assert_eq!(body["username"], "gabriel");
        assert!(body.get("email").is_none());
    }

    #[test]
    fn todo_login_se_identifica_como_mjsv_cli() {
        // O sign-in apaga só as sessões do mesmo serviceId; errar aqui derruba o navegador do
        // usuário toda vez que ele usar o CLI.
        for req in [
            sign_in_request("gabriel", "s"),
            sign_in_otp_request("t", "123456"),
            sign_in_recovery_request("t", "abc"),
        ] {
            assert_eq!(req.body.unwrap()["serviceId"], SERVICE_ID);
        }
    }

    #[test]
    fn resposta_com_requires_otp_vira_desafio() {
        let step = interpret_sign_in(json!({"requiresOtp": true, "otpLoginToken": "abc"})).unwrap();

        match step {
            LoginStep::NeedsOtp { otp_login_token } => assert_eq!(otp_login_token, "abc"),
            other => panic!("esperava desafio de OTP, veio {other:?}"),
        }
    }

    #[test]
    fn resposta_sem_otp_vira_sessao() {
        let step = interpret_sign_in(json!({
            "id": "Mjjp5",
            "username": "gabriel",
            "accessToken": "jwt",
            "orgs": [{"id": "K3xQ7", "name": "Majestic"}]
        }))
        .unwrap();

        match step {
            LoginStep::Done(id) => {
                assert_eq!(id.access_token.as_deref(), Some("jwt"));
                assert_eq!(id.orgs.len(), 1);
            }
            other => panic!("esperava sessão pronta, veio {other:?}"),
        }
    }

    #[test]
    fn desafio_sem_token_e_erro_e_nao_sessao_vazia() {
        let err = interpret_sign_in(json!({"requiresOtp": true})).unwrap_err();
        assert!(matches!(err, ApiError::Decode { .. }));
    }

    #[test]
    fn verify_vai_com_alvo_root() {
        let req = verify_request();
        let target = req.target.unwrap();

        assert_eq!(target.kind.as_str(), "ROOT");
        assert_eq!(target.id, "0000");
    }
}
