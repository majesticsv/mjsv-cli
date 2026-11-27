//! Arte estática aprovada (`/projects/MJSV/temp/ascii/FINAL.md`): banners, coroa compacta, placa.
//!
//! As linhas são copiadas do documento, não redesenhadas. Todas têm a marca nas colunas 0-19 e o
//! texto a partir da coluna 24, nos dois conjuntos de caracteres, para o renderizador ter um único
//! layout. Linhas não têm espaço à direita: só se preenche quando há texto depois.

/// Colunas ocupadas pela marca.
pub const MARK_W: usize = 20;
/// Coluna (base 0) onde começam o logotipo, o rótulo e as linhas de informação.
pub const INFO_COL: usize = 24;

/// A placa `STAFF / ROOT`: 14 células, com os dois espaços.
pub const PLATE_ASCII: &str = " STAFF / ROOT ";
pub const PLATE_UNICODE: &str = " STAFF · ROOT ";

/// Frase fixa do banner de staff (linha 5), nunca truncada.
pub const STAFF_SENTENCE: &str = "[#] comandos afetam toda a plataforma";

/// Gradiente da marca, uma cor por linha (truecolor). Quente na coroa, claro no portão.
pub const GRADIENT_CUSTOMER: [(u8, u8, u8); 8] = [
    (255, 92, 57),
    (255, 99, 59),
    (254, 107, 62),
    (254, 114, 64),
    (253, 122, 66),
    (253, 129, 68),
    (252, 137, 71),
    (252, 144, 73),
];

pub const GRADIENT_STAFF: [(u8, u8, u8); 8] = [
    (255, 59, 48),
    (249, 55, 47),
    (243, 51, 45),
    (237, 47, 44),
    (232, 43, 42),
    (226, 39, 41),
    (220, 35, 39),
    (214, 31, 38),
];

/// 256 cores: linhas 0-3 e 4-7.
pub const GRADIENT_CUSTOMER_256: [u8; 2] = [203, 209];
pub const GRADIENT_STAFF_256: [u8; 2] = [196, 160];

/// O pátio vazio (EM-courtyard), 20x4. Coluna de texto 24, como o banner.
pub const COURTYARD_UNICODE: [&str; 4] = [
    "▄ ▄ ▄          ▄ ▄ ▄",
    "█▀▀▀█          █▀▀▀█",
    "█ ▄ █  ·    ·  █ ▄ █",
    "█▄█▄█▄▄▄▄▄▄▄▄▄▄█▄█▄█",
];
pub const COURTYARD_ASCII: [&str; 4] = [
    r"|\_/|          |\_/|",
    r"|   |          |   |",
    r"| _ |  .    .  | _ |",
    r"||_||__________||_||",
];

/// Banner hero (ascii): FINAL.md A/C, 8 linhas, marca nas colunas 0-19.
pub const HERO_ASCII: [&str; 8] = [
    r#"         /\"#,
    r#"    |\__/  \__/|        ||\  /||     ||  //====  \\    //"#,
    r#"|\_  \________/  _/|    || \/ ||     ||  \\==\\   \\  //"#,
    r#"|  |  _      _  |  |    ||    ||  \__//   ===//    \\//"#,
    r#"|  |_| |_  _| |_|  |    H O S T I N G"#,
    r#"|        \/        |"#,
    r#"'-._    .--.    _.-'"#,
    r#"    '--|    |--'"#,
];

/// Banner hero (unicode): FINAL.md A/C, 8 linhas, marca nas colunas 0-19.
pub const HERO_UNICODE: [&str; 8] = [
    r#"  ▄▄    ▄██▄    ▄▄"#,
    r#"  ▀▀████████████▀▀      ██▄ ▄██     ██  ▄█▀▀▀▀  ██   ██"#,
    r#"██▄              ▄██    ██▀█▀██  ▄  ██  ▀████▄   ██ ██"#,
    r#"███▄▄██▄    ▄██▄▄███    ██   ██  ▀███▀  ▄▄▄▄█▀    ▀█▀"#,
    r#"████████▄  ▄████████    H O S T I N G"#,
    r#"████████████████████"#,
    r#"███████▀    ▀███████"#,
    r#" ▀▀████      ████▀▀"#,
];

/// Banner staff (ascii): FINAL.md A/C, 8 linhas, marca nas colunas 0-19.
pub const STAFF_ASCII: [&str; 8] = [
    r#"         /\             == STAFF / ROOT ================="#,
    r#"    |\__/  \__/|        ||\  /||     ||  //====  \\    //"#,
    r#"|\_  \________/  _/|    || \/ ||     ||  \\==\\   \\  //"#,
    r#"|  |  _      _  |  |    ||    ||  \__//   ===//    \\//"#,
    r#"|  |_| |_  _| |_|  |    A D M"#,
    r#"|        \/        |    [#] comandos afetam toda a plataforma"#,
    r#"'-._    .--.    _.-'"#,
    r#"    '--|####|--'"#,
];

/// Banner staff (unicode): FINAL.md A/C, 8 linhas, marca nas colunas 0-19.
pub const STAFF_UNICODE: [&str; 8] = [
    r#"  ▄▄    ▄██▄    ▄▄      ══ STAFF · ROOT ═══════════════"#,
    r#"  ▀▀████████████▀▀      ██▄ ▄██     ██  ▄█▀▀▀▀  ██   ██"#,
    r#"██▄              ▄██    ██▀█▀██  ▄  ██  ▀████▄   ██ ██"#,
    r#"███▄▄██▄    ▄██▄▄███    ██   ██  ▀███▀  ▄▄▄▄█▀    ▀█▀"#,
    r#"████████▄  ▄████████    A D M"#,
    r#"████████████████████    [#] comandos afetam toda a plataforma"#,
    r#"███████▀▌▐▌▐▀███████"#,
    r#" ▀▀████▐▌▐▌▐▌████▀▀"#,
];

/// Coroa compacta (ascii), FINAL.md B.
pub const COMPACT_ASCII: &str = r#"\_/^\_/"#;

/// Coroa compacta (unicode), FINAL.md B.
pub const COMPACT_UNICODE: &str = r#"█▄█▄█"#;
