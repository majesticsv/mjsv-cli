//! O cliente HTTP: URL base, bearer, cabeçalhos de alvo e tradução de erro.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{is_otp_setup_required, message_from_body, ApiError};
use crate::target::Target;

pub const DEFAULT_API_URL: &str = "https://courtyard-api.mjsv.io";

/// O `serviceId` com que o CLI se identifica no login.
///
/// O campo é texto livre, sem lista branca (`auth.dto.ts`), e o `signIn` apaga apenas as sessões
/// **do mesmo serviceId** antes de criar a nova (`auth.service.ts`). Logo, um valor próprio para o
/// CLI garante que logar aqui não derruba a sessão do navegador — e vice-versa.
pub const SERVICE_ID: &str = "mjsv-cli";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// Uma requisição já montada, ainda não enviada.
///
/// Existir como valor é o que torna o `--print-request` possível: o mesmo objeto que iria para a
/// rede é impresso. Se o dry-run fosse montado em outro caminho, ele deixaria de provar qualquer
/// coisa sobre o que o comando realmente manda.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub target: Option<Target>,
    pub query: Vec<(String, String)>,
    pub body: Option<Value>,
}

impl Request {
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Request {
            method,
            path: path.into(),
            target: None,
            query: Vec::new(),
            body: None,
        }
    }

    pub fn get(path: impl Into<String>) -> Self {
        Request::new(Method::Get, path)
    }

    pub fn post(path: impl Into<String>) -> Self {
        Request::new(Method::Post, path)
    }

    pub fn patch(path: impl Into<String>) -> Self {
        Request::new(Method::Patch, path)
    }

    pub fn put(path: impl Into<String>) -> Self {
        Request::new(Method::Put, path)
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Request::new(Method::Delete, path)
    }

    pub fn target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    pub fn query(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.query.push((key.into(), value.to_string()));
        self
    }

    pub fn body(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    /// O que o `--print-request` imprime. Nunca inclui o bearer — o dry-run existe para revisar
    /// alvo e corpo, e um segredo impresso acaba em log de CI e em cópia de terminal.
    pub fn describe(&self, base_url: &str) -> String {
        let mut out = format!("{} {}{}", self.method.as_str(), base_url, self.path);

        if !self.query.is_empty() {
            let qs: Vec<String> = self.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
            out.push('?');
            out.push_str(&qs.join("&"));
        }

        out.push('\n');
        out.push_str("Authorization: Bearer <omitido>\n");

        if let Some(t) = &self.target {
            out.push_str(&format!("MJSV-TargetId: {}\nMJSV-TargetType: {}\n", t.id, t.kind));
        } else {
            out.push_str("(sem cabeçalhos de alvo — rota não escopada)\n");
        }

        if let Some(b) = &self.body {
            out.push('\n');
            out.push_str(&serde_json::to_string_pretty(b).unwrap_or_else(|_| b.to_string()));
            out.push('\n');
        }

        out
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl Client {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("mjsv-cli/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .build()?;

        let base_url = base_url.into().trim_end_matches('/').to_string();

        Ok(Client { http, base_url, token })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    pub async fn send<T: DeserializeOwned>(&self, req: &Request) -> Result<T, ApiError> {
        let value = self.send_value(req).await?;

        serde_json::from_value(value).map_err(|e| ApiError::Decode {
            path: req.path.clone(),
            message: e.to_string(),
        })
    }

    /// Envia e devolve o JSON cru.
    ///
    /// A saída `--output json` repassa exatamente isto, então nada aqui pode reordenar ou
    /// reinterpretar o corpo: o contrato de script é o corpo da API.
    pub async fn send_value(&self, req: &Request) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base_url, req.path);

        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        };

        let mut builder = self.http.request(method, &url);

        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }

        if let Some(t) = &req.target {
            builder = builder
                .header("MJSV-TargetId", &t.id)
                .header("MJSV-TargetType", t.kind.as_str());
        }

        if !req.query.is_empty() {
            builder = builder.query(&req.query);
        }

        if let Some(body) = &req.body {
            builder = builder.json(body);
        }

        self.finish(builder, url, req).await
    }

    /// Envia um arquivo como `multipart/form-data` (hoje, só a escrita de arquivo do gameserver).
    ///
    /// O backend lê o conteúdo do campo `file` e o diretório de destino de `filePath`; o nome que o
    /// arquivo terá no servidor é o `filename` da parte, não algo do corpo — por isso ele é um
    /// parâmetro próprio aqui.
    pub async fn send_multipart(
        &self,
        req: &Request,
        filename: &str,
        content: Vec<u8>,
        fields: &[(&str, &str)],
    ) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base_url, req.path);

        let mut form = reqwest::multipart::Form::new();
        for (k, v) in fields {
            form = form.text((*k).to_string(), (*v).to_string());
        }
        form = form.part(
            "file",
            reqwest::multipart::Part::bytes(content).file_name(filename.to_string()),
        );

        let mut builder = self.http.post(&url).multipart(form);

        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }

        if let Some(t) = &req.target {
            builder = builder
                .header("MJSV-TargetId", &t.id)
                .header("MJSV-TargetType", t.kind.as_str());
        }

        self.finish(builder, url, req).await
    }

    async fn finish(&self, builder: reqwest::RequestBuilder, url: String, req: &Request) -> Result<Value, ApiError> {
        let response = builder.send().await.map_err(|source| ApiError::Transport {
            url: url.clone(),
            source,
        })?;

        let status = response.status();
        let text = response.text().await.map_err(|source| ApiError::Transport {
            url: url.clone(),
            source,
        })?;

        // Corpo vazio é resposta legítima (204, e alguns handlers devolvem só o status).
        let parsed: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text.clone()))
        };

        if status.is_success() {
            return Ok(parsed);
        }

        Err(self.map_error(status.as_u16(), &parsed, req))
    }

    /// Baixa uma resposta binária (hoje, o PDF da fatura).
    ///
    /// Existe separado do `send_value` porque aquele faz `.text()` e depois tenta parsear JSON: um
    /// PDF passaria por ali como uma `Value::String` com os bytes reinterpretados como UTF-8, que é
    /// corrupção silenciosa — o arquivo sai gravado e não abre.
    ///
    /// O erro continua saindo pelo mesmo `map_error`: quando a resposta NÃO é de sucesso o corpo é
    /// JSON de verdade, então vale a pena decodificá-lo em vez de escrever o erro no arquivo.
    pub async fn download(&self, req: &Request) -> Result<Vec<u8>, ApiError> {
        let url = format!("{}{}", self.base_url, req.path);

        // Respeita o método da requisição: `POST /order/{id}/generate-pdf` também devolve binário.
        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut builder = self.http.request(method, &url);

        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }

        if let Some(t) = &req.target {
            builder = builder
                .header("MJSV-TargetId", &t.id)
                .header("MJSV-TargetType", t.kind.as_str());
        }

        let response = builder.send().await.map_err(|source| ApiError::Transport {
            url: url.clone(),
            source,
        })?;

        let status = response.status();
        let bytes = response.bytes().await.map_err(|source| ApiError::Transport {
            url: url.clone(),
            source,
        })?;

        if status.is_success() {
            return Ok(bytes.to_vec());
        }

        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

        Err(self.map_error(status.as_u16(), &body, req))
    }

    fn map_error(&self, status: u16, body: &Value, req: &Request) -> ApiError {
        match status {
            401 => ApiError::Unauthorized,
            403 => ApiError::Forbidden {
                path: req.path.clone(),
                target: Box::new(req.target.clone().unwrap_or_else(Target::root)),
            },
            404 => ApiError::NotFound(message_from_body(body, &req.path)),
            428 if is_otp_setup_required(body) => ApiError::OtpSetupRequired,
            _ => ApiError::Api {
                status,
                message: message_from_body(body, "erro sem mensagem"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn describe_mostra_metodo_url_e_alvo() {
        let req = Request::get("/org/Mjjp5/virtualmachine")
            .target(Target::org("Mjjp5"))
            .query("take", 25);

        let texto = req.describe(DEFAULT_API_URL);

        assert!(
            texto.contains("GET https://courtyard-api.mjsv.io/org/Mjjp5/virtualmachine?take=25"),
            "{texto}"
        );
        assert!(texto.contains("MJSV-TargetId: Mjjp5"), "{texto}");
        assert!(texto.contains("MJSV-TargetType: ORG"), "{texto}");
    }

    #[test]
    fn describe_nunca_imprime_o_bearer() {
        let texto = Request::get("/auth/verify")
            .target(Target::root())
            .describe(DEFAULT_API_URL);

        assert!(texto.contains("Bearer <omitido>"), "{texto}");
    }

    #[test]
    fn describe_inclui_o_corpo() {
        let texto = Request::post("/auth/token")
            .body(json!({"name": "laptop"}))
            .describe(DEFAULT_API_URL);

        assert!(texto.contains("\"name\": \"laptop\""), "{texto}");
    }

    #[test]
    fn o_serviceid_do_cli_e_proprio_para_nao_derrubar_o_navegador() {
        // O sign-in apaga só as sessões do mesmo serviceId. Se isto virasse "mjsv-panel", logar
        // pelo CLI deslogaria o usuário do painel.
        assert_eq!(SERVICE_ID, "mjsv-cli");
    }
}
