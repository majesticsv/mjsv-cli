//! O contrato duro do módulo `style`: o que NUNCA pode mudar, qualquer que seja o terminal.

use mjsv_core::output::{Format, Table};
use mjsv_core::style::{
    width, BannerKind, Caps, Charset, ConfirmSpec, Depth, InfoLine, MapEnv, ReportCtx, RuleSpec, SecretBox, Style,
    Tier, Tool,
};

fn with(env: &[(&str, &str)], tty: bool, format: Format, tool: Tool) -> Style {
    Style::with_env(&MapEnv::new(env), tty, Some(100), format, tool)
}

const UTF8: (&str, &str) = ("LANG", "pt_BR.UTF-8");

#[test]
fn json_e_yaml_nao_recebem_nada() {
    for f in [Format::Json, Format::Yaml] {
        let s = with(&[UTF8, ("COLORTERM", "truecolor")], true, f, Tool::Mjsv);
        assert!(!s.fancy() && !s.art() && !s.color());
        assert_eq!(s.done("feito."), "feito.");
        assert_eq!(s.error_line("x"), "erro: x");
        assert!(s.banner(BannerKind::Hero, "v", &[]).is_none());
        assert!(s.title_rule("t").is_none());
        assert!(s.secret_box(&SecretBox::new("a", "b").secret("s")).is_none());
        assert_eq!(s.meter(50.0, false), "");
    }
}

#[test]
fn fluxo_que_nao_e_terminal_recebe_o_texto_de_hoje() {
    let s = with(&[UTF8, ("COLORTERM", "truecolor")], false, Format::Table, Tool::Mjsv);
    assert!(!s.fancy());
    assert_eq!(s.done("Regra criada."), "Regra criada.");
    assert_eq!(s.accepted("Pedido enviado."), "Pedido enviado.");
    assert_eq!(s.state("running"), "running");
    assert_eq!(
        s.order_created("njLME", "PENDING", "R$ 94,32"),
        "Pedido njLME criado (PENDING).\nTotal:   R$ 94,32\nPague com: mjsv order pay njLME"
    );
    assert_eq!(s.checkout("njLME", "https://x/y"), "Pague em: https://x/y");
    assert_eq!(s.logout_result(), "Sessão encerrada.");
    assert_eq!(
        s.no_system_flag(),
        "Esta conta não tem flag de sistema — as rotas administrativas vão responder 403."
    );
    assert!(s.root_step("x").is_none());
    assert!(s.version_line("0.3.0").is_none());
    assert_eq!(
        ConfirmSpec::new(Tier::T2, "Apagar? Continuar?")
            .destructive()
            .row("a", "b")
            .render(&s),
        "Apagar? Continuar? [s/N] "
    );
    let mut t = Table::new(&["id", "estado"]);
    t.push(vec!["a".into(), "running".into()]);
    assert_eq!(t.render_styled(&s), t.render());
    assert_eq!(t.render(), "ID  ESTADO\na   running\n");
}

#[test]
fn no_color_mantem_os_caracteres_sem_nenhum_sgr() {
    let s = with(
        &[UTF8, ("COLORTERM", "truecolor"), ("NO_COLOR", "1")],
        true,
        Format::Table,
        Tool::Mjsvadm,
    );
    assert!(s.fancy() && s.art() && !s.color());
    let rc = ReportCtx::default();
    let mut all = vec![
        s.done("x"),
        s.rule(&RuleSpec::new().staff().plate().tag("SEM VOLTA").crown()),
        s.meter(95.0, false),
        s.spark(&[0.1, 0.9]),
        s.progress_line(1, "Enviando", Some(50.0), "x"),
        s.secret_box(&SecretBox::new("A", "b").secret("s")).unwrap(),
        s.banner(BannerKind::Staff, "mjsvadm 0.3.0", &[InfoLine::text("h")])
            .unwrap(),
        s.render_error(&anyhow::anyhow!("falhou"), &rc),
        ConfirmSpec::new(Tier::T3, "x").typed("n", "i").render(&s),
    ];
    all.push(s.state("running"));
    for a in all {
        assert!(!a.contains('\x1b'), "SGR sob NO_COLOR: {a:?}");
    }
    // Os caracteres de arte continuam.
    assert!(s.banner(BannerKind::Staff, "v", &[]).unwrap().contains('█'));
}

#[test]
fn sem_banner_tira_a_arte_e_mantem_glifos_e_cartoes() {
    let s = with(
        &[UTF8, ("MJSV_NO_BANNER", "1"), ("NO_COLOR", "1")],
        true,
        Format::Table,
        Tool::Mjsv,
    );
    assert!(s.fancy() && !s.art());
    assert!(s.banner(BannerKind::Hero, "v", &[]).is_none());
    assert!(s.secret_box(&SecretBox::new("a", "b").secret("s")).is_none());
    assert!(s.version_line("1").is_none());
    assert_eq!(s.done("ok"), "✓ ok");
    assert!(s.title_rule("VM").is_some());
    // Sem arte o "nunca entrou" é o 401 comum, sem coroa.
    let out = s.render_error(&mjsv_core::ApiError::Unauthorized.into(), &ReportCtx::default());
    assert!(out.starts_with("✗ Sessão expirada"), "{out}");
}

#[test]
fn term_dumb_e_texto_puro() {
    let s = with(&[UTF8, ("TERM", "dumb")], true, Format::Table, Tool::Mjsv);
    assert!(!s.fancy());
    assert_eq!(s.done("x"), "x");
    assert_eq!(s.erase_line(), "");
    assert!(!s.can_redraw());
}

#[test]
fn larguras_nunca_passam_do_que_a_especificacao_manda() {
    for cs in [Charset::Unicode, Charset::Ascii] {
        for tool in [Tool::Mjsv, Tool::Mjsvadm] {
            let s = Style::terminal(tool, cs, Depth::TrueColor, 200);
            let kind = if tool == Tool::Mjsvadm {
                BannerKind::Staff
            } else {
                BannerKind::Hero
            };
            let infos = [
                InfoLine::text("x".repeat(100)),
                InfoLine::kv("org", "y".repeat(100)),
                InfoLine::text("z".repeat(100)),
            ];
            for l in s.banner_lines(kind, "mjsv 0.3.0-beta.1+a1b2c3d", &infos).iter().skip(5) {
                assert!(width(l) <= 72, "banner passa de 72: {}", width(l));
            }
            for c in [Some(Tier::T2), Some(Tier::T3)] {
                let spec = ConfirmSpec::new(c.unwrap(), "x").typed("n", "i").row("a", "b");
                for l in spec.render(&s).lines() {
                    assert!(width(l) <= 60 + 8, "régua/linha larga: {l}");
                }
            }
            for r in [
                RuleSpec::new(),
                RuleSpec::new().title("VM web-01"),
                RuleSpec::new().staff().plate().tag("SEM VOLTA").crown(),
            ] {
                assert_eq!(width(&s.rule(&r)), 60);
            }
            // Meter: 20 células em qualquer percentual.
            for p in 0..=100 {
                assert_eq!(width(&s.meter(p as f64, false)), 20);
            }
            // Em terminal de 40 colunas a régua encolhe para colunas - 1.
            let narrow = Style::terminal(tool, cs, Depth::None, 40);
            assert_eq!(width(&narrow.rule(&RuleSpec::new())), 39);
        }
    }
}

#[test]
fn deteccao_de_capacidades_por_fluxo() {
    let env = MapEnv::new(&[UTF8, ("TERM", "xterm-256color")]);
    let out = Caps::detect(&env, true, Some(120), false, false);
    let err = Caps::detect(&env, false, None, false, false);
    assert!(out.tty && !err.tty);
    assert_eq!(out.depth, Depth::Ansi256);
    assert_eq!(out.width, 120);
}
