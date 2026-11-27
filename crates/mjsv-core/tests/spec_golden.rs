//! Confere o módulo `style` contra a especificação aprovada.
//!
//! `tests/golden/spec.json` é gerado do catálogo
//! `/projects/MJSV/temp/ascii/tools/outputs_assets.py` por `tools/gen_golden.py` (cada asset, nos
//! dois conjuntos de caracteres, sem cor e em truecolor). Aqui cada asset é remontado pela API
//! pública e comparado linha a linha, SGR inclusive: se alguém mexer num glifo, numa largura ou
//! numa cor, o teste aponta qual asset da especificação deixou de bater.

use mjsv_core::error::{ApiError, LoginRefused, LoginStage, OrgNotInAccount, TokenRefused};
use mjsv_core::prompt::{NoTerminalHint, PromptError};
use mjsv_core::style::{
    BannerKind, CardError, Charset, ConfirmSpec, Depth, Glyph, InfoLine, Marker, Receipt, ReceiptItem, ReportCtx,
    RuleSpec, SecretBox, Semantics, Sty, Style, Tier, Tool,
};
use mjsv_core::target::Target;
use serde_json::Value;

const HOST: &str = "courtyard-api.mjsv.io";

fn spec() -> Value {
    serde_json::from_str(include_str!("golden/spec.json")).unwrap()
}

fn lines(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_string())
        .collect()
}

/// Remonta o asset `id` com `build` em unicode/ascii x sem cor/truecolor e compara.
fn check(id: &str, tool: Tool, build: impl Fn(&Style) -> Vec<String>) {
    let all = spec();
    let a = &all[id];
    assert!(!a.is_null(), "asset {id} não está no catálogo");
    for (cs_name, cs) in [("unicode", Charset::Unicode), ("ascii", Charset::Ascii)] {
        for (mode, depth) in [("none", Depth::None), ("true", Depth::TrueColor)] {
            let s = Style::terminal(tool, cs, depth, 100);
            let got: Vec<String> = build(&s)
                .iter()
                .flat_map(|b| {
                    b.split('\n')
                        .map(|l| l.trim_end_matches(' ').to_string())
                        .collect::<Vec<_>>()
                })
                .collect();
            let want: Vec<String> = lines(&a[cs_name][mode])
                .iter()
                .map(|l| l.trim_end_matches(' ').to_string())
                .collect();
            assert_eq!(got, want, "asset {id} ({cs_name}, {mode})");
        }
    }
}

fn rc() -> ReportCtx {
    ReportCtx {
        api_url: format!("https://{HOST}"),
        stored_token: true,
        ..Default::default()
    }
}

fn err(s: &Style, e: impl Into<anyhow::Error>, rc: &ReportCtx) -> Vec<String> {
    vec![s.render_error(&e.into(), rc)]
}

// ------------------------------------------------------------------------------- vazios

#[test]
fn em_neutral_hint_clear_unknown() {
    check("EM-neutral", Tool::Mjsv, |s| {
        [
            "Nenhuma sessão.",
            "Nenhum hypervisor encontrado.",
            "Nenhuma VM encontrada.",
        ]
        .iter()
        .map(|t| s.empty(t))
        .collect()
    });
    check("EM-hint", Tool::Mjsv, |s| {
        vec![
            s.empty_hint(
                "Nenhuma VM nesta organização.",
                "comece pelo catálogo",
                "mjsv product list",
            ),
            s.empty_hint(
                "Nenhuma chave SSH nesta organização.",
                "adicione uma",
                "mjsv ssh-key add --name <nome> --file <chave.pub>",
            ),
        ]
    });
    check("EM-clear", Tool::Mjsv, |s| {
        [
            "Nenhum incidente no período.",
            "Nenhum pedido pendente.",
            "Nenhum aviso encontrado.",
            "Nenhum comando na fila.",
        ]
        .iter()
        .map(|t| s.empty_clear(t))
        .collect()
    });
    check("EM-unknown", Tool::Mjsv, |s| {
        vec![s.empty_unknown("Incidentes indisponíveis: telemetria desativada neste serviço")]
    });
}

#[test]
fn em_courtyard_e_whoami() {
    check("EM-courtyard-example", Tool::Mjsv, |s| vec![s.empty_courtyard()]);
    check("EM-whoami", Tool::Mjsv, |s| {
        let mut v = vec![s.title_rule("Conta ana").unwrap()];
        v.extend(s.card_rows(&[
            ("Usuário", "ana".to_string()),
            ("Nome", "Ana Souza".into()),
            ("E-mail", "ana@acme.com.br".into()),
            ("Id", "Qw3Rt".into()),
            (
                "Organização",
                format!(
                    "{} nenhuma selecionada (mjsv org use <id>)",
                    s.marker(Marker::Attention)
                ),
            ),
            ("API", format!("https://{HOST}")),
        ]));
        v
    });
}

#[test]
fn em_bare_banners() {
    check("EM-bare-no-session", Tool::Mjsv, |s| {
        s.banner_lines(
            BannerKind::Hero,
            "mjsv 0.3.0",
            &[
                InfoLine::text(HOST),
                InfoLine::kv("sessão", "nenhuma"),
                InfoLine::kv("próximo", "mjsv login"),
            ],
        )
    });
    check("EM-bare-no-org", Tool::Mjsv, |s| {
        s.banner_lines(
            BannerKind::Hero,
            "mjsv 0.3.0",
            &[
                InfoLine::text(HOST),
                InfoLine::kv("org", "nenhuma selecionada"),
                InfoLine::kv("próximo", "mjsv org use <id>"),
            ],
        )
    });
    check("EM-bare-empty-org", Tool::Mjsv, |s| {
        s.banner_lines(
            BannerKind::Hero,
            "mjsv 0.3.0",
            &[
                InfoLine::kv("org", "acme (nUME7)"),
                InfoLine::kv("serviços", "nenhum"),
                InfoLine::kv("próximo", "mjsv product list"),
            ],
        )
    });
    check("EM-bare-staff-no-session", Tool::Mjsvadm, |s| {
        s.banner_lines(
            BannerKind::Staff,
            "mjsvadm 0.3.0",
            &[InfoLine::kv("sessão", "nenhuma"), InfoLine::kv("próximo", "mjsv login")],
        )
    });
}

// ------------------------------------------------------------------------------- erros

#[test]
fn er_linhas_e_sessao() {
    check("ER-line", Tool::Mjsv, |s| {
        [
            "porta fora da faixa 1-65535: 70000",
            "nada a alterar: informe --vnc-password",
            "falha ao ler ./world.zip",
            "409: erro sem mensagem",
        ]
        .iter()
        .map(|t| s.error_line(t))
        .collect()
    });
    check("ER-401", Tool::Mjsv, |s| err(s, ApiError::Unauthorized, &rc()));
    check("ER-401-env", Tool::Mjsv, |s| {
        err(
            s,
            ApiError::Unauthorized,
            &ReportCtx {
                token_from_env: true,
                ..rc()
            },
        )
    });
    check("ER-login", Tool::Mjsv, |s| {
        err(s, LoginRefused { stage: LoginStage::Any }, &rc())
    });
    check("ER-token", Tool::Mjsv, |s| err(s, TokenRefused, &rc()));
    check("ER-never", Tool::Mjsv, |s| {
        err(
            s,
            ApiError::Unauthorized,
            &ReportCtx {
                stored_token: false,
                ..rc()
            },
        )
    });
}

#[test]
fn er_organizacao_e_403() {
    check("ER-no-org", Tool::Mjsv, |s| err(s, ApiError::NoOrg, &rc()));
    check("ER-org-use", Tool::Mjsv, |s| {
        err(
            s,
            OrgNotInAccount {
                org: "xYz12".into(),
                available: vec!["nUME7".into(), "k3PqD".into()],
            },
            &rc(),
        )
    });
    check("ER-403", Tool::Mjsv, |s| {
        err(
            s,
            ApiError::Forbidden {
                path: "/vm/k3TqZ/power".into(),
                target: Box::new(Target::org("nUME7")),
            },
            &rc(),
        )
    });
    check("ER-403-staff", Tool::Mjsvadm, |s| {
        err(
            s,
            ApiError::Forbidden {
                path: "/vm/k3TqZ/power".into(),
                target: Box::new(Target::root()),
            },
            &rc(),
        )
    });
    check("ER-flag", Tool::Mjsvadm, |s| vec![s.no_system_flag()]);
}

#[test]
fn er_404_2fa_400_5xx_decode() {
    // O asset empilha dois erros, de ferramentas diferentes: confere cada metade.
    let all = spec();
    for (cs_name, cs) in [("unicode", Charset::Unicode), ("ascii", Charset::Ascii)] {
        for (mode, depth) in [("none", Depth::None), ("true", Depth::TrueColor)] {
            let c = Style::terminal(Tool::Mjsv, cs, depth, 100);
            let a = Style::terminal(Tool::Mjsvadm, cs, depth, 100);
            let r1 = ReportCtx {
                noun: Some("vm".into()),
                ..rc()
            };
            let r2 = ReportCtx {
                noun: Some("org".into()),
                ..rc()
            };
            let mut got = vec![c.render_error(&ApiError::NotFound("/vm/k3TqX".into()).into(), &r1)];
            got.push(a.render_error(&anyhow::anyhow!("organização k3PqD não encontrada"), &r2));
            let got: Vec<String> = got
                .iter()
                .flat_map(|b| b.split('\n').map(str::to_string).collect::<Vec<_>>())
                .collect();
            let want = lines(&all["ER-404"][cs_name][mode]);
            assert_eq!(got, want, "ER-404 ({cs_name}, {mode})");
        }
    }

    check("ER-2fa", Tool::Mjsv, |s| err(s, ApiError::OtpSetupRequired, &rc()));
    check("ER-400-list", Tool::Mjsv, |s| {
        err(
            s,
            ApiError::Api {
                status: 400,
                message: "E-mail inválido; Senha não pode estar vazia".into(),
            },
            &rc(),
        )
    });
    check("ER-5xx", Tool::Mjsv, |s| {
        err(
            s,
            ApiError::Api {
                status: 502,
                message: "erro sem mensagem".into(),
            },
            &rc(),
        )
    });
    check("ER-decode", Tool::Mjsv, |s| {
        err(
            s,
            ApiError::Decode {
                path: "/vm/k3TqZ/metrics".into(),
                message: "missing field `cpu` at line 1 column 212".into(),
            },
            &rc(),
        )
    });
}

#[test]
fn er_rede_prompt_cartoes_proprios_e_verbose() {
    check("ER-net", Tool::Mjsv, |s| {
        vec![s.render_net_card_for_tests(false, "/vm", "dns error: failed to lookup address information", &rc())]
    });
    check("ER-timeout", Tool::Mjsv, |s| {
        vec![s.render_net_card_for_tests(true, "/vm", "operation timed out", &rc())]
    });
    check("ER-no-terminal", Tool::Mjsv, |s| {
        let mut v = err(
            s,
            PromptError::NoTerminal {
                what: "esta pergunta",
                hint: NoTerminalHint::Token,
            },
            &rc(),
        );
        v.extend(err(
            s,
            PromptError::NoTerminal {
                what: "esta pergunta",
                hint: NoTerminalHint::Yes,
            },
            &rc(),
        ));
        v
    });
    check("ER-org-flag", Tool::Mjsvadm, |s| {
        let a = CardError::new("x", "--org não se aplica a este comando")
            .cause("No mjsvadm, o alvo é sempre ROOT/0000; use o filtro do comando.")
            .hint("tente", "mjsvadm vm list --help");
        let b = CardError::new("x", "gs create precisa de --org <id>")
            .cause("O game server nasce na organização informada.")
            .hint("tente", "mjsvadm org list");
        let mut v = err(s, a, &rc());
        v.extend(err(s, b, &rc()));
        v
    });
    check("ER-ssh-key", Tool::Mjsv, |s| {
        let e = CardError::new("x", "id_ed25519 é uma chave PRIVADA")
            .cause("Envie o arquivo .pub correspondente.")
            .hint("tente", "mjsv ssh-key add --name <nome> --file id_ed25519.pub");
        err(s, e, &rc())
    });
    check("ER-verbose", Tool::Mjsv, |s| {
        err(
            s,
            ApiError::NotFound("/vm/k3TqX".into()),
            &ReportCtx {
                noun: Some("vm".into()),
                verbose: 1,
                ..rc()
            },
        )
    });
}

// ------------------------------------------------------------------------------- sucessos

#[test]
fn ok_linhas_e_cartoes() {
    check("OK-done", Tool::Mjsv, |s| {
        vec![
            s.done("Organização corrente: nUME7"),
            s.done("Regra criada. Confira com `mjsv vm firewall list k3TqZ`."),
            s.done("Volume Mjjp5 anexado a k3TqZ como vdb. Monte-o no sistema convidado."),
            s.done("Fatura salva em fatura-njLME.pdf (48213 bytes)."),
            s.noop("Nada sincronizado: a VM não tem interface de rede."),
            s.noop("Nenhuma sessão local para encerrar."),
            s.attention("dynmap.jar removido — reinicie o servidor para aplicar."),
        ]
    });
    check("OK-accepted", Tool::Mjsv, |s| {
        vec![
            s.accepted("Pedido `start` enviado para k3TqZ. Acompanhe com `mjsv vm power k3TqZ`."),
            s.accepted("Reinstalação iniciada. Acompanhe com `mjsv vm show k3TqZ`."),
            s.noop("A senha de root foi gerada: recupere com `mjsv vm root-password k3TqZ`."),
            s.accepted("Ação `reboot` aceita para a VM k3TqZ."),
        ]
    });
    check("OK-order-created", Tool::Mjsv, |s| {
        vec![s.order_created("njLME", "PENDING", "R$ 94,32")]
    });
    check("OK-checkout", Tool::Mjsv, |s| {
        vec![s.checkout("njLME", "https://checkout.mjsv.io/c/8Jk2Lq0PzX4vN7cR1tY6")]
    });
}

#[test]
fn ok_login_logout() {
    use mjsv_core::style::{LoginView, OrgSummary};
    let view = |first: bool, orgs: OrgSummary| LoginView {
        username: "ana".into(),
        user_id: "Qw3Rt".into(),
        orgs,
        first,
        host: HOST.into(),
        version: "0.3.0".into(),
    };
    let one = || OrgSummary::One {
        name: "acme".into(),
        id: "nUME7".into(),
    };
    check("OK-login", Tool::Mjsv, |s| vec![s.login_result(&view(false, one()))]);
    check("OK-login-first", Tool::Mjsv, |s| {
        vec![s.login_result(&view(true, one())).trim_start_matches('\n').to_string()]
    });
    check("OK-login-first-no-org", Tool::Mjsv, |s| {
        vec![s
            .login_result(&view(true, OrgSummary::None))
            .trim_start_matches('\n')
            .to_string()]
    });
    check("OK-logout", Tool::Mjsv, |s| vec![s.logout_result()]);
}

#[test]
fn ok_segredos_e_staff() {
    check("OK-secret", Tool::Mjsv, |s| {
        let b = SecretBox::new("COPIE AGORA", "não será mostrado de novo")
            .secret("mjsv_pat_8Jk2Lq0PzX4vN7cR1tY6uE3wB9sD5fGh")
            .row("chave", "ci (njLME)")
            .row("use com", "export MJSV_TOKEN=<valor>");
        vec![s.secret_box(&b).unwrap()]
    });
    check("OK-secret-console", Tool::Mjsv, |s| {
        let b = SecretBox::new("USE AGORA", "o token expira em 60 segundos")
            .captioned(
                "token:",
                "eyJhbGciOiJIUzI1NiJ9.eyJ2bSI6ImszVHFaIiwiZXhwIjoxNzk1Nzk1NzI2fQ.q7Vd",
            )
            .captioned("senha:", "hT4kW9pZ")
            .row("console VNC", "VM k3TqZ");
        vec![s.secret_box(&b).unwrap()]
    });
    check("OK-staff-vm-create", Tool::Mjsvadm, |s| {
        let mut v = vec![
            s.rule(&RuleSpec::new().staff().plate().tag("ORG nUME7")),
            s.title_line(Glyph::Done, "VM criada: web-01 (k3TqZ)"),
        ];
        v.extend(s.rows(
            &[
                ("IP", "10.20.0.14"),
                ("EIP", "203.0.113.27"),
                ("próximo", "mjsvadm machine show k3TqZ"),
            ],
            s.indent(),
            false,
        ));
        let b = SecretBox::new("COPIE AGORA", "não será mostrado de novo")
            .captioned("senha de root:", "q7Vd-M2xp-Lr9K-tB4s");
        v.push(s.secret_box(&b).unwrap());
        v
    });
    check("OK-staff-otp-setup", Tool::Mjsvadm, |s| {
        vec![
            s.rule(&RuleSpec::new().staff().plate()),
            s.title_line(Glyph::Accepted, "OTP: cadastro iniciado"),
            format!(
                "{}{} otp-qr.png (0600){}leia no autenticador e apague",
                s.indent(),
                s.dim("QR:"),
                s.sep()
            ),
            format!("{}{}", s.indent(), s.dim("token de cadastro:")),
            s.bold("st_9LwQ2mVb7KpZc1RyH8sJ"),
            s.hint_default("conclua", "mjsvadm otp verify <código> --setup-token <token>"),
        ]
    });
    check("OK-staff-json", Tool::Mjsvadm, |s| {
        [
            "E-mail de redefinição de senha enviado para ana@acme.com.br",
            "Carteira de Qw3Rt ajustada: +R$ 50,00",
            "Pedido njLME: status alterado para COMPLETED",
        ]
        .iter()
        .map(|t| s.staff_done_line(t).unwrap())
        .collect()
    });
}

// ------------------------------------------------------------------------------- confirmações

fn prompt(s: &Style, c: ConfirmSpec) -> String {
    c.render(s)
}

#[test]
fn cf_cliente() {
    check("CF-0", Tool::Mjsv, |s| {
        vec![
            prompt(s, ConfirmSpec::simple("Fechar o ticket 4821?")),
            prompt(
                s,
                ConfirmSpec::simple("Enviar map.zip para /map.zip sobrescreve o arquivo se ele já existir. Continuar?"),
            ),
        ]
    });
    check("CF-1", Tool::Mjsv, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(
                Tier::T1,
                "`kill` corta a energia da VM k3TqZ sem avisar o sistema convidado. Continuar?",
            ),
        )]
    });
    check("CF-1-pay", Tool::Mjsv, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(Tier::T1, "Pagar o pedido njLME com o saldo da carteira?")
                .ask("Pagar?")
                .row("total", "R$ 94,32")
                .row("saldo", format!("R$ 152,40{}resta R$ 58,08", s.sep())),
        )]
    });
    check("CF-2", Tool::Mjsv, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(
                Tier::T2,
                "Reinstalar a VM k3TqZ com a imagem ubuntu-24.04 APAGA o disco e não tem volta. Continuar?",
            )
            .destructive()
            .head("Reinstalar a VM web-01?")
            .row("VM", "web-01 (k3TqZ)")
            .row("imagem", "ubuntu-24.04")
            .row("org", "acme (nUME7)")
            .note("APAGA o disco e não tem volta."),
        )]
    });
    check("CF-2-files", Tool::Mjsv, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(Tier::T2, "Apagar 2 arquivo(s) de /plugins? Não há lixeira. Continuar?")
                .destructive()
                .head("Apagar 2 arquivos de /plugins?")
                .row("arquivos", format!("old.jar{}backup.zip", s.sep()))
                .row("gameserver", "mc-02 (p9WxA)")
                .row("org", "acme (nUME7)")
                .note("Não há lixeira."),
        )]
    });
    check("CF-3", Tool::Mjsv, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(
                Tier::T3,
                "Apagar a organização acme? Só o dono consegue e os serviços ativos ficam sem dono.",
            )
            .typed("acme", "nUME7")
            .head("Apagar a organização acme?")
            .row("org", "acme (nUME7)")
            .row("dono", "ana@acme.com.br")
            .row("serviços", format!("3 VMs{0}2 gameservers{0}1 volume", s.sep()))
            .note("Só o dono consegue e os serviços ativos ficam sem dono."),
        )]
    });
}

#[test]
fn cf_staff() {
    check("CF-A0", Tool::Mjsvadm, |s| {
        vec![prompt(s, ConfirmSpec::simple("Remover a imagem debian-11?"))]
    });
    check("CF-A1", Tool::Mjsvadm, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(Tier::T1, "Executar `reboot` na VM k3TqZ? Continuar?")
                .head("Executar `reboot` na VM k3TqZ?")
                .row("org", "acme (nUME7)"),
        )]
    });
    check("CF-A2", Tool::Mjsvadm, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(
                Tier::T2,
                "Reinstalar a VM k3TqZ com a imagem ubuntu-24.04? O disco será APAGADO.",
            )
            .destructive()
            .head("Reinstalar a VM web-01?")
            .row("VM", "web-01 (k3TqZ)")
            .row("imagem", "ubuntu-24.04")
            .row("org", "acme (nUME7)")
            .row("nó", "gru-03")
            .note("O disco será APAGADO."),
        )]
    });
    check("CF-A2-money", Tool::Mjsvadm, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(Tier::T2, "Ajustar a carteira do usuário Qw3Rt em -5000 centavos?")
                .ask("Ajustar?")
                .head("Ajustar a carteira do usuário Qw3Rt?")
                .row("usuário", "ana@acme.com.br (Qw3Rt)")
                .row("ajuste", "-R$ 50,00")
                .row("saldo", format!("R$ 152,40{}resta R$ 102,40", s.sep()))
                .row("motivo", "estorno do pedido njLME"),
        )]
    });
    check("CF-A3", Tool::Mjsvadm, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(
                Tier::T3,
                "Apagar DEFINITIVAMENTE o usuário Qw3Rt? É hard delete e não há como desfazer.",
            )
            .typed("ana@acme.com.br", "Qw3Rt")
            .head("Apagar DEFINITIVAMENTE o usuário Qw3Rt?")
            .row("usuário", "ana@acme.com.br (Qw3Rt)")
            .row("orgs", format!("acme (nUME7){}dono", s.sep()))
            .row("afeta", format!("3 VMs{0}2 gameservers{0}1 volume", s.sep()))
            .note("É hard delete e não há como desfazer."),
        )]
    });
    check("CF-A3-region", Tool::Mjsvadm, |s| {
        vec![prompt(
            s,
            ConfirmSpec::new(Tier::T3, "Remover a região gru?")
                .typed("gru", "gru")
                .head("Remover a região gru?")
                .row("região", "gru (São Paulo)")
                .row("afeta", format!("6 nós{0}412 VMs{0}58 orgs", s.sep())),
        )]
    });
    check("CF-step", Tool::Mjsvadm, |s| {
        vec![s.root_step("Apagando o usuário ana@acme.com.br (Qw3Rt)").unwrap()]
    });
    check("CF-cancel", Tool::Mjsv, |s| {
        let mut v = err(s, PromptError::Cancelled, &rc());
        v.extend(err(s, PromptError::Mismatch, &rc()));
        v
    });
}

// ------------------------------------------------------------------------------- dados

fn rows_state(s: &Style, state: &str) -> String {
    s.state(state)
}

#[test]
fn dv_cartoes() {
    let vm = |s: &Style, state: &str| {
        let mut v = vec![s.title_rule("VM web-01").unwrap()];
        v.extend(s.card_rows(&[
            ("Nome", "web-01".to_string()),
            ("Serviço", "s4Lm8".into()),
            ("Estado", rows_state(s, state)),
            ("SO", "Ubuntu 24.04 LTS".into()),
            ("Recursos", "2 vCPU, 4 GiB".into()),
            ("Criada", "2026-10-03 14:22:10".into()),
        ]));
        v
    };
    check("DV-vm-show", Tool::Mjsv, |s| vm(s, "running"));

    check("DV-vm-show-degraded", Tool::Mjsv, |s| {
        let r = s.card_rows(&[
            ("Nome", "db-01".to_string()),
            ("Serviço", "t7Vn3".into()),
            ("Estado", s.state("degraded")),
            ("SO", "Debian 12".into()),
        ]);
        let mut v = vec![s.title_rule("VM db-01").unwrap()];
        v.extend(r[..3].iter().cloned());
        v.push("          disco virtual com erros de E/S no nó gru-03".into());
        v.extend(r[3..].iter().cloned());
        v
    });
    check("DV-adm-vm-show", Tool::Mjsvadm, |s| {
        let mut v = vec![s.title_rule("VM web-01").unwrap()];
        v.extend(s.card_rows(&[
            ("VM", "k3TqZ".to_string()),
            ("Nome", "web-01".into()),
            ("Estado", s.state("reinstalling")),
            ("Mensagem", "baixando imagem".into()),
            ("Desejado", s.state("running")),
            ("Observado", s.state("installing")),
            ("Nó", "gru-03".into()),
        ]));
        v.push(s.dim("(use --output json para IPs, discos e operações)"));
        v
    });
    check("DV-adm-whoami", Tool::Mjsvadm, |s| {
        let mut v = vec![s.title_rule("Staff joao.silva").unwrap()];
        v.extend(s.card_rows(&[
            ("Usuário", "joao.silva".to_string()),
            ("E-mail", "joao.silva@mjsv.io".into()),
            ("Id", "Aa1Bc".into()),
            ("Permissões", "0x7FFFFFFF".into()),
            ("Contexto", s.paint(Sty::Plate, "ROOT / 0000")),
        ]));
        v
    });
    check("DV-probe", Tool::Mjsvadm, |s| {
        s.card_rows(&[
            ("Alcançável", format!("{} NÃO", s.marker(Marker::Failed))),
            ("Mensagem", "timeout após 5000 ms".into()),
            ("Latência", s.dim("-")),
        ])
    });
    check("DV-vm-power", Tool::Mjsv, |s| {
        vec![
            format!("web-01  {}", s.state("starting")),
            format!("web-01  {}", s.state("running")),
        ]
    });
}

fn tbl(s: &Style, headers: &[&str], rows: &[&[&str]], state: &[usize], sem: Semantics) -> Vec<String> {
    let h: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    let r: Vec<Vec<String>> = rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect();
    s.table(&h, &r, state, sem).lines().map(str::to_string).collect()
}

#[test]
fn dv_tabelas() {
    check("DV-vm-list", Tool::Mjsv, |s| {
        tbl(
            s,
            &["SERVICE", "NOME", "ESTADO", "VCPU", "MEMÓRIA", "SO"],
            &[
                &["s4Lm8", "web-01", "running", "2", "4 GiB", "Ubuntu 24.04 LTS"],
                &["t7Vn3", "db-01", "degraded", "4", "8 GiB", "Debian 12"],
                &["h2Bc6", "staging", "stopped", "1", "2 GiB", "Ubuntu 22.04 LTS"],
                &["r9Yd1", "ci-runner", "installing", "2", "4 GiB", "AlmaLinux 9"],
            ],
            &[2],
            Semantics::Resource,
        )
    });
    check("DV-order-list", Tool::Mjsv, |s| {
        tbl(
            s,
            &["ID", "STATUS", "TOTAL", "VENCIMENTO", "CRIADO"],
            &[
                &[
                    "njLME",
                    "PENDING",
                    "R$ 94,32",
                    "2026-11-30 23:59:59",
                    "2026-11-27 10:02:11",
                ],
                &[
                    "gT5wQ",
                    "COMPLETED",
                    "R$ 59,90",
                    "2026-10-30 23:59:59",
                    "2026-10-27 08:40:02",
                ],
                &[
                    "bN8cJ",
                    "CANCELLED",
                    "R$ 119,80",
                    "2026-09-30 23:59:59",
                    "2026-09-27 16:21:45",
                ],
            ],
            &[1],
            Semantics::Resource,
        )
    });
    check("DV-incidents", Tool::Mjsv, |s| {
        let mut v = vec![
            "Endereços: 200.9.155.14, 200.9.155.15".to_string(),
            format!("Ativos: 1{}", s.trailing(Marker::Attention)),
            String::new(),
        ];
        v.extend(tbl(
            s,
            &["ID", "ESTADO", "INÍCIO (UTC)", "DURAÇÃO", "VETOR"],
            &[
                &["c4Kd2", "mitigated", "2026-11-26 03:14:07", "00:12:40", "UDP flood"],
                &["c4Kf7", "ongoing", "2026-11-27 13:55:21", "00:03:02", "SYN flood"],
            ],
            &[1],
            Semantics::Resource,
        ));
        v.push(String::new());
        v.push(s.dim("Mais resultados: repita com --cursor eyJvIjoyMH0"));
        v
    });
    check("DV-adm-node-list", Tool::Mjsvadm, |s| {
        tbl(
            s,
            &[
                "ID",
                "NOME",
                "ENDEREÇO",
                "REGIÃO",
                "LOCAL",
                "CLASSE",
                "SAÚDE",
                "VISTO EM",
            ],
            &[
                &[
                    "1",
                    "gru-01",
                    "10.20.0.11",
                    "gru",
                    "gru-a",
                    "epyc",
                    "healthy",
                    "2026-11-27 14:31:02",
                ],
                &[
                    "3",
                    "gru-03",
                    "10.20.0.13",
                    "gru",
                    "gru-a",
                    "epyc",
                    "degraded",
                    "2026-11-27 14:31:00",
                ],
                &[
                    "7",
                    "gru-07",
                    "10.20.0.17",
                    "gru",
                    "gru-b",
                    "xeon",
                    "offline",
                    "2026-11-27 11:02:44",
                ],
            ],
            &[6],
            Semantics::Resource,
        )
    });
    check("DV-adm-operation-list", Tool::Mjsvadm, |s| {
        tbl(
            s,
            &["ID", "TIPO", "ESTADO", "ALVO", "CRIADA", "ATUALIZADA"],
            &[
                &[
                    "8841",
                    "rebuild",
                    "running",
                    "k3TqZ",
                    "2026-11-27 14:28:10",
                    "2026-11-27 14:28:58",
                ],
                &[
                    "8840",
                    "power",
                    "completed",
                    "k3TqZ",
                    "2026-11-27 14:27:51",
                    "2026-11-27 14:27:55",
                ],
                &[
                    "8839",
                    "resize",
                    "failed",
                    "m4CdX",
                    "2026-11-27 14:20:03",
                    "2026-11-27 14:21:17",
                ],
            ],
            &[2],
            Semantics::Operation,
        )
    });
    check("DV-adm-network-check", Tool::Mjsvadm, |s| {
        tbl(
            s,
            &["STATUS", "CHECAGEM", "DETALHE"],
            &[
                &["ok", "vxlan gru-01 <-> gru-03", "mtu 1450, 0% perda"],
                &["warn", "dhcp vpc-acme-01", "pool 92% usado"],
                &["fail", "bgp peer 200.9.152.1", "sessão caída há 00:04:12"],
            ],
            &[0],
            Semantics::Resource,
        )
    });
    check("DV-adm-region-list", Tool::Mjsvadm, |s| {
        let warn = format!("2{}", s.trailing(Marker::Attention));
        let err = format!("0{}", s.trailing(Marker::Failed));
        tbl(
            s,
            &["ID", "NOME", "NÓS", "NÓS OK", "VMS"],
            &[
                &["gru", "São Paulo", "8", "8", "171"],
                &["poa", "Porto Alegre", "3", &warn, "39"],
                &["for", "Fortaleza", "1", &err, "4"],
            ],
            &[],
            Semantics::Resource,
        )
    });
}

#[test]
fn dv_medidores_recibo_progresso() {
    check("DV-meter", Tool::Mjsv, |s| {
        [0.0, 1.0, 25.0, 62.0, 75.0, 91.0, 100.0]
            .iter()
            .map(|p| {
                let mark = if *p >= 90.0 {
                    format!(
                        " {}",
                        s.paint(Sty::M90, &mjsv_core::style::strip_ansi(&s.marker(Marker::Attention)))
                    )
                } else {
                    String::new()
                };
                format!("{}  {:>3}%{mark}", s.meter(*p, false), *p as i64)
            })
            .collect()
    });

    check("DV-upload", Tool::Mjsv, |s| {
        vec![
            s.progress_line(0, "Enviando world.zip", Some(4.0), "2,5 de 61,4 MiB"),
            s.progress_line(1, "Enviando world.zip", Some(62.0), "38,1 de 61,4 MiB"),
            s.progress_line(2, "Enviando world.zip", Some(100.0), "61,4 de 61,4 MiB"),
            s.done("world.zip enviado para /world.zip (64382217 bytes)."),
        ]
    });

    check("DV-receipt", Tool::Mjsv, |s| {
        let r = Receipt {
            order_id: "njLME".into(),
            items: vec![
                ReceiptItem {
                    description: "VM Cloud 2 vCPU 4 GiB, 1 mês".into(),
                    price: "R$ 59,90".into(),
                },
                ReceiptItem {
                    description: "Gameserver Minecraft 8 GiB, 1 mês".into(),
                    price: "R$ 44,90".into(),
                },
                ReceiptItem {
                    description: "Cupom BEMVINDO10".into(),
                    price: "-R$ 10,48".into(),
                },
            ],
            total: "R$ 94,32".into(),
            balance_before: "R$ 152,40".into(),
            balance_after: "R$ 58,08".into(),
        };
        vec![s.receipt(&r).unwrap()]
    });
}

// ------------------------------------------------------------------------------- coluna "plain"

/// Fora de um terminal, cada asset é exatamente o texto da coluna "plain" da especificação.
fn plain_lines(id: &str) -> Vec<String> {
    let all = spec();
    let p = &all[id]["plain"];
    assert!(p.is_array(), "asset {id} não tem coluna plain em lista");
    lines(p)
}

fn split(v: Vec<String>) -> Vec<String> {
    v.iter()
        .flat_map(|b| {
            b.split('\n')
                .map(|l| l.trim_end_matches(' ').to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn fora_de_terminal_cada_asset_e_a_coluna_plain() {
    let p = Style::plain(Tool::Mjsv);
    let a = Style::plain(Tool::Mjsvadm);
    let r = rc();
    let e = |s: &Style, x: anyhow::Error, rc: &ReportCtx| s.render_error(&x, rc);

    let cases: Vec<(&str, Vec<String>)> = vec![
        ("ER-401", vec![e(&p, ApiError::Unauthorized.into(), &r)]),
        (
            "ER-never",
            vec![e(
                &p,
                ApiError::Unauthorized.into(),
                &ReportCtx {
                    stored_token: false,
                    ..rc()
                },
            )],
        ),
        (
            "ER-login",
            vec![e(&p, LoginRefused { stage: LoginStage::Any }.into(), &r)],
        ),
        ("ER-token", vec![e(&p, TokenRefused.into(), &r)]),
        ("ER-no-org", vec![e(&p, ApiError::NoOrg.into(), &r)]),
        (
            "ER-org-use",
            vec![e(
                &p,
                OrgNotInAccount {
                    org: "xYz12".into(),
                    available: vec!["nUME7".into(), "k3PqD".into()],
                }
                .into(),
                &r,
            )],
        ),
        (
            "ER-403",
            vec![e(
                &p,
                ApiError::Forbidden {
                    path: "/vm/k3TqZ/power".into(),
                    target: Box::new(Target::org("nUME7")),
                }
                .into(),
                &r,
            )],
        ),
        ("ER-2fa", vec![e(&p, ApiError::OtpSetupRequired.into(), &r)]),
        (
            "ER-400-list",
            vec![e(
                &p,
                ApiError::Api {
                    status: 400,
                    message: "E-mail inválido; Senha não pode estar vazia".into(),
                }
                .into(),
                &r,
            )],
        ),
        (
            "ER-5xx",
            vec![e(
                &p,
                ApiError::Api {
                    status: 502,
                    message: "erro sem mensagem".into(),
                }
                .into(),
                &r,
            )],
        ),
        (
            "ER-decode",
            vec![e(
                &p,
                ApiError::Decode {
                    path: "/vm/k3TqZ/metrics".into(),
                    message: "missing field `cpu` at line 1 column 212".into(),
                }
                .into(),
                &r,
            )],
        ),
        (
            "ER-no-terminal",
            vec![
                e(
                    &p,
                    PromptError::NoTerminal {
                        what: "esta pergunta",
                        hint: NoTerminalHint::Token,
                    }
                    .into(),
                    &r,
                ),
                e(
                    &p,
                    PromptError::NoTerminal {
                        what: "esta pergunta",
                        hint: NoTerminalHint::Yes,
                    }
                    .into(),
                    &r,
                ),
            ],
        ),
        (
            "ER-404",
            vec![
                e(&p, ApiError::NotFound("/vm/k3TqX".into()).into(), &r),
                e(&a, anyhow::anyhow!("organização k3PqD não encontrada"), &r),
            ],
        ),
        (
            "ER-verbose",
            vec![e(
                &p,
                ApiError::NotFound("/vm/k3TqX".into()).into(),
                &ReportCtx { verbose: 1, ..rc() },
            )],
        ),
        (
            "ER-line",
            [
                "porta fora da faixa 1-65535: 70000",
                "nada a alterar: informe --vnc-password",
                "falha ao ler ./world.zip",
                "409: erro sem mensagem",
            ]
            .iter()
            .map(|t| e(&p, anyhow::anyhow!(t.to_string()), &r))
            .collect(),
        ),
        (
            "OK-order-created",
            vec![p.order_created("njLME", "PENDING", "R$ 94,32")],
        ),
        (
            "OK-checkout",
            vec![p.checkout("njLME", "https://checkout.mjsv.io/c/8Jk2Lq0PzX4vN7cR1tY6")],
        ),
        ("OK-logout", vec![p.logout_result()]),
        ("ER-flag", vec![a.no_system_flag()]),
        (
            "EM-neutral",
            vec![
                p.empty("Nenhuma sessão."),
                p.empty("Nenhum hypervisor encontrado."),
                p.empty("Nenhuma VM encontrada."),
            ],
        ),
        (
            "CF-0",
            vec![
                ConfirmSpec::simple("Fechar o ticket 4821?").render(&p),
                ConfirmSpec::simple("Enviar map.zip para /map.zip sobrescreve o arquivo se ele já existir. Continuar?")
                    .render(&p),
            ],
        ),
        (
            "CF-A0",
            vec![ConfirmSpec::simple("Remover a imagem debian-11?").render(&a)],
        ),
        (
            "CF-3",
            vec![ConfirmSpec::new(
                Tier::T3,
                "Apagar a organização acme? Só o dono consegue e os serviços ativos ficam sem dono.",
            )
            .typed("acme", "nUME7")
            .render(&p)],
        ),
        (
            "CF-A3-region",
            vec![ConfirmSpec::new(Tier::T3, "Remover a região gru?")
                .typed("gru", "gru")
                .render(&a)],
        ),
        (
            "CF-cancel",
            vec![
                e(&p, PromptError::Cancelled.into(), &r),
                e(&p, PromptError::Mismatch.into(), &r),
            ],
        ),
    ];

    for (id, got) in cases {
        assert_eq!(
            split(got),
            plain_lines(id)
                .iter()
                .map(|l| l.trim_end_matches(' ').to_string())
                .collect::<Vec<_>>(),
            "plain de {id}"
        );
    }
}
