//! Núcleo compartilhado pelo `mjsv` (público) e pelo `mjsvadm` (privado).
//!
//! Aqui fica tudo o que não é comando: cliente HTTP, cabeçalhos de alvo, login, configuração e
//! renderização. Os dois binários falam com a mesma API, então autenticação e tradução de erro
//! precisam ser escritas uma vez só — e nada específico de rota administrativa entra aqui, porque
//! este crate é público.

pub mod auth;
pub mod client;
pub mod config;
pub mod ctx;
pub mod error;
pub mod models;
pub mod output;
pub mod routes;
pub mod target;

pub use client::{Client, Method, Request};
pub use ctx::Ctx;
pub use error::ApiError;
pub use output::{Format, Table};
pub use target::{Target, TargetType};
