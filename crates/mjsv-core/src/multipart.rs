//! Corpo `multipart/form-data` montado por quem chama.
//!
//! O backend usa `nestjs-form-data`, e cada rota escolhe os próprios nomes de parte (`file` e
//! `filePath` na escrita de arquivo do gameserver, por exemplo). Por isso o cliente não conhece nenhum nome: quem chama diz o
//! nome, o nome do arquivo e o tipo de cada parte.
//!
//! O formulário é um valor (`Clone`, `Debug`) que viaja dentro do [`Request`](crate::Request): o
//! mesmo objeto que vai para a rede é o que o `--print-request` descreve.

use anyhow::{anyhow, Result};

use crate::style::sanitize;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    Text(String),
    File {
        filename: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Part {
    name: String,
    content: Content,
}

/// Um formulário multipart: campos de texto e partes de arquivo, na ordem em que foram
/// adicionados (a ordem importa para quem lê o corpo em streaming no servidor).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Form {
    parts: Vec<Part>,
}

impl Form {
    pub fn new() -> Self {
        Form::default()
    }

    /// Campo de texto (sem `filename`, como um `<input>` comum).
    pub fn text(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.parts.push(Part {
            name: name.into(),
            content: Content::Text(value.into()),
        });
        self
    }

    /// Parte de arquivo. `content_type` é o `Content-Type` da parte; `None` deixa a parte sem o
    /// cabeçalho. Falha com um tipo malformado em vez de mandar para a rede e levar um 400 do
    /// servidor.
    pub fn file(
        mut self,
        name: impl Into<String>,
        filename: impl Into<String>,
        content_type: Option<&str>,
        bytes: Vec<u8>,
    ) -> Result<Self> {
        if let Some(ct) = content_type {
            reqwest::multipart::Part::bytes(Vec::new())
                .mime_str(ct)
                .map_err(|_| anyhow!("content-type inválido: `{ct}`"))?;
        }

        self.parts.push(Part {
            name: name.into(),
            content: Content::File {
                filename: filename.into(),
                content_type: content_type.map(str::to_string),
                bytes,
            },
        });
        Ok(self)
    }

    #[doc(hidden)]
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    #[doc(hidden)]
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Nomes das partes, na ordem de envio.
    #[doc(hidden)]
    pub fn names(&self) -> Vec<&str> {
        self.parts.iter().map(|p| p.name.as_str()).collect()
    }

    /// Converte para o formulário do `reqwest`. Consome uma cópia dos bytes: o `Request` continua
    /// descritível depois do envio.
    pub(crate) fn to_reqwest(&self) -> Result<reqwest::multipart::Form> {
        let mut form = reqwest::multipart::Form::new();

        for p in &self.parts {
            form = match &p.content {
                Content::Text(v) => form.text(p.name.clone(), v.clone()),
                Content::File {
                    filename,
                    content_type,
                    bytes,
                } => {
                    let mut part = reqwest::multipart::Part::bytes(bytes.clone()).file_name(filename.clone());
                    if let Some(ct) = content_type {
                        part = part
                            .mime_str(ct)
                            .map_err(|_| anyhow!("content-type inválido: `{ct}`"))?;
                    }
                    form.part(p.name.clone(), part)
                }
            };
        }

        Ok(form)
    }

    /// Uma linha por parte para o `--print-request`: nome, nome do arquivo, tamanho e tipo.
    /// Os bytes de arquivo nunca entram aqui; o texto dos campos entra, porque é ele que se quer
    /// revisar (o JSON de `data`, o `filePath`).
    pub fn describe(&self) -> String {
        let mut out = format!("multipart/form-data, {} parte(s):\n", self.parts.len());

        for (i, p) in self.parts.iter().enumerate() {
            let n = i + 1;
            match &p.content {
                Content::Text(v) => {
                    out.push_str(&format!(
                        "  [{n}] {} — campo de texto, {} bytes\n      valor: {}\n",
                        sanitize(&p.name),
                        v.len(),
                        sanitize(v)
                    ));
                }
                Content::File {
                    filename,
                    content_type,
                    bytes,
                } => {
                    out.push_str(&format!(
                        "  [{n}] {} — arquivo `{}`, {} bytes, {}\n",
                        sanitize(&p.name),
                        sanitize(filename),
                        bytes.len(),
                        content_type.as_deref().unwrap_or("sem content-type"),
                    ));
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mantem_a_ordem_e_os_nomes_escolhidos() {
        let f = Form::new()
            .text("data", "{}")
            .file("icon", "a.png", Some("image/png"), vec![1, 2, 3])
            .unwrap()
            .text("meta", "x");

        assert_eq!(f.names(), ["data", "icon", "meta"]);
        assert_eq!(f.len(), 3);
        assert!(!f.is_empty());
        assert!(Form::new().is_empty());
    }

    #[test]
    fn content_type_malformado_e_recusado_na_montagem() {
        let err = Form::new()
            .file("icon", "a.png", Some("não é mime"), vec![])
            .unwrap_err();
        assert!(err.to_string().contains("content-type inválido"), "{err}");
    }

    #[test]
    fn converte_para_o_formulario_do_reqwest() {
        let f = Form::new()
            .text("data", "{}")
            .file("icon", "a.png", Some("image/png"), vec![0; 4])
            .unwrap()
            .file("raw", "b.bin", None, vec![9])
            .unwrap();

        assert!(f.to_reqwest().is_ok());
    }

    #[test]
    fn descricao_traz_nome_arquivo_tamanho_e_tipo_sem_os_bytes() {
        let f = Form::new()
            .text("data", r#"{"name":"vps"}"#)
            .file("icon", "logo.png", Some("image/png"), b"SEGREDO-DOS-BYTES".to_vec())
            .unwrap();

        let d = f.describe();

        assert!(d.contains("2 parte(s)"), "{d}");
        assert!(d.contains("[1] data — campo de texto, 14 bytes"), "{d}");
        assert!(d.contains(r#"valor: {"name":"vps"}"#), "{d}");
        assert!(d.contains("[2] icon — arquivo `logo.png`, 17 bytes, image/png"), "{d}");
        assert!(!d.contains("SEGREDO"), "{d}");
    }

    #[test]
    fn descricao_nao_deixa_escapes_chegarem_ao_terminal() {
        let d = Form::new()
            .text("a\x1b[31m", "v\x1b[0m\u{202e}x")
            .file("f", "n\x1b[31m\u{202e}.png", None, vec![1])
            .unwrap()
            .describe();
        assert!(!d.contains('\x1b') && !d.contains('\u{202e}'), "{d:?}");
    }

    #[test]
    fn descricao_de_arquivo_sem_content_type() {
        let d = Form::new().file("file", "x.txt", None, vec![1]).unwrap().describe();
        assert!(d.contains("sem content-type"), "{d}");
    }
}
