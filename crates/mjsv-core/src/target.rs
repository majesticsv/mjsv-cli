//! Os cabeçalhos `MJSV-TargetId` / `MJSV-TargetType`.
//!
//! Toda rota protegida da API é escopada por esse par (`src/guards/jwt-permissions.guard.ts` e
//! `src/guards/ensure-targetid.ts` no Courtyard-backend). O guard só confere a permissão contra o
//! alvo declarado no cabeçalho; **quem confere que o alvo é o mesmo da rota é cada handler**, com
//! um `req.targetId === id` seguido de `TargetType inválido`.
//!
//! Ou seja: o par não é uma escolha por comando, é uma função da própria rota. Por isso ele é
//! derivado aqui, num só lugar, e não montado à mão em cada subcomando — montar à mão é o jeito
//! mais fácil de escrever um CLI que toma 403 no primeiro comando.

use std::fmt;

/// Espelha `src/core/targetTypes.ts` do backend. Só os tipos que os binários usam hoje.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetType {
    Root,
    Org,
    User,
    Service,
    GameServer,
    VirtualMachine,
    Ticket,
}

impl TargetType {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetType::Root => "ROOT",
            TargetType::Org => "ORG",
            TargetType::User => "USER",
            TargetType::Service => "SERVICE",
            TargetType::GameServer => "GAMESERVER",
            TargetType::VirtualMachine => "VIRTUALMACHINE",
            TargetType::Ticket => "TICKET",
        }
    }
}

impl fmt::Display for TargetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// O id do contexto de conta (sem org) — literal, não um sqid.
///
/// É o mesmo valor que o painel manda (`Courtyard/src/core/adapters/http-client.ts`): quando não
/// há org, o par vira `0000` + `ROOT`.
pub const ROOT_ID: &str = "0000";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub kind: TargetType,
    pub id: String,
}

impl Target {
    pub fn root() -> Self {
        Target {
            kind: TargetType::Root,
            id: ROOT_ID.to_string(),
        }
    }

    pub fn org(id: impl Into<String>) -> Self {
        Target {
            kind: TargetType::Org,
            id: id.into(),
        }
    }

    pub fn vm(id: impl Into<String>) -> Self {
        Target {
            kind: TargetType::VirtualMachine,
            id: id.into(),
        }
    }

    pub fn gameserver(id: impl Into<String>) -> Self {
        Target {
            kind: TargetType::GameServer,
            id: id.into(),
        }
    }

    pub fn service(id: impl Into<String>) -> Self {
        Target {
            kind: TargetType::Service,
            id: id.into(),
        }
    }

    pub fn ticket(id: impl Into<String>) -> Self {
        Target {
            kind: TargetType::Ticket,
            id: id.into(),
        }
    }

    pub fn user(id: impl Into<String>) -> Self {
        Target {
            kind: TargetType::User,
            id: id.into(),
        }
    }

    /// O invariante da API: o id do cabeçalho tem de ser o mesmo que aparece no caminho.
    ///
    /// `ROOT` é a exceção — o id `0000` não corresponde a recurso nenhum, então rota de escopo
    /// root não tem id no caminho para comparar.
    pub fn matches_path_id(&self, path_id: &str) -> bool {
        match self.kind {
            TargetType::Root => self.id == ROOT_ID,
            _ => self.id == path_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_e_o_literal_0000() {
        let t = Target::root();
        assert_eq!(t.kind.as_str(), "ROOT");
        assert_eq!(t.id, "0000");
    }

    #[test]
    fn id_do_cabecalho_tem_de_bater_com_o_do_caminho() {
        // É exatamente o que cada handler do backend refaz antes de responder.
        let t = Target::org("Mjjp5");
        assert!(t.matches_path_id("Mjjp5"));
        assert!(!t.matches_path_id("outro"));
    }

    #[test]
    fn root_nao_compara_id_de_caminho() {
        assert!(Target::root().matches_path_id("qualquer-coisa"));
    }
}
