//! Renderização da saída: tabela para gente, JSON para script, YAML para leitura.
//!
//! O YAML é escrito à mão de propósito. Com `arbitrary_precision` ligado, um número vira
//! internamente `{"$serde_json::private::Number": "..."}` e **qualquer outro serializer** — o
//! serde_yaml inclusive — emite esse mapa em vez do número. Ou seja: adicionar a dependência de
//! YAML produziria saída silenciosamente errada justamente nos ids, que é o motivo de
//! `arbitrary_precision` existir aqui. Escrever a partir do `Value` evita o problema inteiro.

use std::fmt::Write as _;

use serde_json::Value;

use crate::style::{sanitize, Semantics, Style, Tool};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Table,
    Json,
    Yaml,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        match s.to_ascii_lowercase().as_str() {
            "table" => Some(Format::Table),
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            _ => None,
        }
    }

    /// O formato pedido na linha de comando, lido do `argv` cru. Serve aos caminhos em que o clap
    /// ainda não devolveu um `Cli` (`--version`, ajuda, erro de uso): ali `--output json` precisa
    /// valer do mesmo jeito, senão a saída de máquina ganharia arte.
    pub fn from_args(args: &[String]) -> Option<Format> {
        let mut it = args.iter();
        while let Some(a) = it.next() {
            if let Some(v) = a.strip_prefix("--output=") {
                return Format::parse(v);
            }
            if a == "--output" {
                return it.next().and_then(|v| Format::parse(v));
            }
        }
        None
    }

    /// Formato de máquina não pode fazer pergunta em prompt — não há quem responda.
    pub fn is_machine_readable(self) -> bool {
        matches!(self, Format::Json | Format::Yaml)
    }
}

/// Uma tabela pronta para imprimir: colunas declaradas, linhas na ordem em que devem sair.
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Como ler `running` numa coluna de estado: recurso saudável ou operação em andamento.
    pub semantics: Semantics,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        Table {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
            semantics: Semantics::Resource,
        }
    }

    /// Tabela de operações: `running` é "em andamento", não "ok".
    pub fn operations(mut self) -> Self {
        self.semantics = Semantics::Operation;
        self
    }

    pub fn push(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    /// A tabela para o stdout: `render()` byte a byte quando o stdout não é um terminal estilizado
    /// (pipe, arquivo, `NO_COLOR` não muda estrutura, `TERM=dumb`); num terminal, cabeçalho em
    /// negrito e marcador de estado nas colunas `ESTADO`/`STATUS`/`SAÚDE`.
    ///
    /// É o que todo comando chama no lugar de `render()`: o estilo é decidido aqui, uma vez, e não
    /// em cada um dos comandos.
    pub fn render_out(&self) -> String {
        self.render_styled(&Style::stdout(Format::Table, Tool::Mjsv))
    }

    /// Como [`Table::render_out`] com o estilo dado (testável sem terminal).
    pub fn render_styled(&self, style: &Style) -> String {
        if self.rows.is_empty() || !style.fancy() {
            return self.render();
        }
        let state_cols: Vec<usize> = self
            .headers
            .iter()
            .enumerate()
            .filter(|(_, h)| matches!(h.to_uppercase().as_str(), "ESTADO" | "STATUS" | "SAÚDE"))
            .map(|(i, _)| i)
            .collect();
        // Texto da API dentro de uma tabela também é saneado: ESC num nome de VM repintaria o
        // terminal de quem lista.
        let rows: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|r| r.iter().map(|c| sanitize(c)).collect())
            .collect();
        style.table(&self.headers, &rows, &state_cols, self.semantics)
    }

    pub fn render(&self) -> String {
        if self.rows.is_empty() {
            return String::new();
        }

        // Largura em caracteres, não em bytes: nome de VM com acento contaria dobrado por byte e
        // desalinharia a coluna.
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.chars().count()).collect();

        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.chars().count());
                }
            }
        }

        let mut out = String::new();

        // A última coluna nunca é preenchida à direita — nem no cabeçalho: espaço no fim da linha
        // suja o diff de quem guarda a saída e atrapalha copiar.
        for (i, h) in self.headers.iter().enumerate() {
            if i + 1 == self.headers.len() {
                out.push_str(&h.to_uppercase());
            } else {
                let _ = write!(out, "{:<width$}  ", h.to_uppercase(), width = widths[i]);
            }
        }
        out.push('\n');

        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i >= widths.len() {
                    continue;
                }
                // A última coluna não é preenchida à direita: espaço no fim atrapalha copiar.
                if i + 1 == widths.len() || i + 1 == row.len() {
                    out.push_str(cell);
                } else {
                    let _ = write!(out, "{:<width$}  ", cell, width = widths[i]);
                }
            }
            out.push('\n');
        }

        out
    }
}

/// Célula de tabela a partir de um campo que pode faltar.
pub fn cell(value: Option<&str>) -> String {
    value.filter(|v| !v.is_empty()).unwrap_or("-").to_string()
}

/// Data ISO da API em algo curto e legível, sem virar dependência de fuso.
pub fn cell_date(value: Option<&str>) -> String {
    match value {
        Some(v) if v.len() >= 19 => v[..19].replace('T', " "),
        Some(v) if !v.is_empty() => v.to_string(),
        _ => "-".to_string(),
    }
}

pub fn to_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub fn to_yaml(value: &Value) -> String {
    let mut out = String::new();
    write_yaml(&mut out, value, 0, false);
    out
}

fn yaml_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        // `to_string` de um Number com arbitrary_precision devolve os dígitos originais.
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            let precisa_aspas = s.is_empty()
                || s.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit() || "-?:,[]{}#&*!|>'\"%@`".contains(c))
                || s.contains(": ")
                || s.contains(" #")
                || s.contains('\n')
                || matches!(s.as_str(), "true" | "false" | "null" | "yes" | "no" | "~");

            if precisa_aspas {
                format!(
                    "\"{}\"",
                    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
                )
            } else {
                s.clone()
            }
        }
        _ => unreachable!("escalar"),
    }
}

fn write_yaml(out: &mut String, value: &Value, indent: usize, inline: bool) {
    let pad = "  ".repeat(indent);

    match value {
        Value::Object(map) if map.is_empty() => {
            out.push_str("{}\n");
        }
        Value::Object(map) => {
            if inline {
                out.push('\n');
            }
            for (k, v) in map {
                match v {
                    Value::Object(inner) if !inner.is_empty() => {
                        let _ = writeln!(out, "{pad}{k}:");
                        write_yaml(out, v, indent + 1, false);
                    }
                    Value::Array(items) if !items.is_empty() => {
                        let _ = writeln!(out, "{pad}{k}:");
                        write_yaml(out, v, indent, false);
                    }
                    _ => {
                        let _ = write!(out, "{pad}{k}: ");
                        write_yaml(out, v, indent + 1, true);
                    }
                }
            }
        }
        Value::Array(items) if items.is_empty() => {
            out.push_str("[]\n");
        }
        Value::Array(items) => {
            if inline {
                out.push('\n');
            }
            for item in items {
                let _ = write!(out, "{pad}- ");
                match item {
                    Value::Object(_) | Value::Array(_) => {
                        // Sub-estrutura desce um nível; o "- " já ocupa a coluna do primeiro campo.
                        let mut buf = String::new();
                        write_yaml(&mut buf, item, indent + 1, false);
                        let trimmed = buf.trim_start_matches(' ');
                        out.push_str(trimmed);
                    }
                    _ => write_yaml(out, item, indent, true),
                }
            }
        }
        scalar => {
            let _ = writeln!(out, "{}", yaml_scalar(scalar));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tabela_alinha_pela_maior_celula() {
        let mut t = Table::new(&["id", "nome"]);
        t.push(vec!["Mjjp5".into(), "web-01".into()]);
        t.push(vec!["K3xQ7abc".into(), "db".into()]);

        let render = t.render();
        let linhas: Vec<&str> = render.lines().collect();

        assert_eq!(linhas[0], "ID        NOME");
        assert_eq!(linhas[1], "Mjjp5     web-01");
        assert_eq!(linhas[2], "K3xQ7abc  db");
    }

    #[test]
    fn largura_conta_caracteres_e_nao_bytes() {
        let mut t = Table::new(&["nome", "x"]);
        t.push(vec!["ação".into(), "1".into()]);
        t.push(vec!["abcd".into(), "2".into()]);

        // "ação" tem 4 caracteres e 5 bytes; se a largura fosse em bytes, as colunas
        // desalinhariam.
        let linhas: Vec<String> = t.render().lines().map(str::to_string).collect();
        assert_eq!(linhas[1], "ação  1");
        assert_eq!(linhas[2], "abcd  2");
    }

    #[test]
    fn tabela_vazia_nao_imprime_cabecalho_solto() {
        assert_eq!(Table::new(&["id"]).render(), "");
    }

    #[test]
    fn celula_ausente_vira_traco() {
        assert_eq!(cell(None), "-");
        assert_eq!(cell(Some("")), "-");
        assert_eq!(cell(Some("web-01")), "web-01");
    }

    #[test]
    fn data_iso_vira_legivel() {
        assert_eq!(cell_date(Some("2026-09-07T04:27:48.420Z")), "2026-09-07 04:27:48");
        assert_eq!(cell_date(None), "-");
    }

    #[test]
    fn yaml_nao_estraga_numero_grande() {
        // O motivo de o YAML ser escrito à mão: com arbitrary_precision, passar por outro
        // serializer emitiria `$serde_json::private::Number` no lugar do número.
        let v: Value = serde_json::from_str(r#"{"id": 9007199254740993}"#).unwrap();
        let yaml = to_yaml(&v);

        assert_eq!(yaml.trim(), "id: 9007199254740993");
        assert!(!yaml.contains("serde_json"), "{yaml}");
    }

    #[test]
    fn yaml_de_lista_de_objetos() {
        let v = json!({"data": [{"id": "Mjjp5", "name": "web-01"}, {"id": "K3xQ7", "name": "db"}]});

        assert_eq!(
            to_yaml(&v),
            "data:\n- id: Mjjp5\n  name: web-01\n- id: K3xQ7\n  name: db\n"
        );
    }

    #[test]
    fn yaml_poe_aspas_no_que_seria_relido_errado() {
        let v = json!({"a": "true", "b": "10.0.0.5", "c": "sim", "d": ""});
        let yaml = to_yaml(&v);

        assert!(yaml.contains("a: \"true\""), "{yaml}");
        // Começa com dígito: sem aspas o YAML releria como número.
        assert!(yaml.contains("b: \"10.0.0.5\""), "{yaml}");
        assert!(yaml.contains("c: sim"), "{yaml}");
        assert!(yaml.contains("d: \"\""), "{yaml}");
    }

    #[test]
    fn json_e_yaml_sao_leituras_de_maquina() {
        assert!(Format::Json.is_machine_readable());
        assert!(Format::Yaml.is_machine_readable());
        assert!(!Format::Table.is_machine_readable());
        assert_eq!(Format::parse("YAML"), Some(Format::Yaml));
        assert_eq!(Format::parse("csv"), None);
    }
}
