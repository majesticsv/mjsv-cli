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
pub mod home;
pub mod models;
pub mod multipart;
pub mod output;
pub mod prompt;
pub mod routes;
pub mod style;
pub mod target;

pub use client::{Client, Method, Request};
pub use ctx::Ctx;
pub use error::ApiError;
pub use output::{Format, Table};
pub use target::{Target, TargetType};
