//! Núcleo do `mjsv`.
//!
//! Aqui fica tudo o que não é comando: cliente HTTP, cabeçalhos de alvo, login, configuração e
//! renderização. Autenticação e tradução de erro
//! ficam escritas uma vez só.

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
