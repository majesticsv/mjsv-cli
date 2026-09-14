#!/bin/sh
# Instalador do mjsv.
#
#   curl -fsSL https://raw.githubusercontent.com/majesticsv/mjsv-cli/master/install.sh | sh
#
# `sh -eu` e nunca `-x`: o padrão dos scripts do MJSV, porque o xtrace já vazou credencial em log
# de CI uma vez. Aqui não há segredo, mas a regra vale para não abrir exceção que alguém copie.
#
# POSIX sh de propósito — este script roda antes de qualquer coisa estar instalada, inclusive em
# imagens que não têm bash.
set -eu

REPO="majesticsv/mjsv-cli"
BIN="mjsv"
PREFIX="${MJSV_INSTALL_DIR:-}"

erro() {
    echo "erro: $*" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || erro "$1 é necessário e não está instalado"
}

need uname
need tar

# Token opcional. Enquanto o repositório for privado a API do GitHub responde 404 sem
# autenticação — o mesmo 404 de "não existe", o que faz o erro parecer outra coisa. Com
# GITHUB_TOKEN (ou GH_TOKEN) o script funciona nos dois casos, e continua sem exigir nada quando o
# repositório for público.
TOKEN="${GITHUB_TOKEN:-${GH_TOKEN:-}}"

# curl ou wget: nenhum dos dois está garantido, então o script aceita os dois em vez de exigir um.
if command -v curl >/dev/null 2>&1; then
    baixar() {
        if [ -n "$TOKEN" ]; then
            curl -fsSL -H "Authorization: Bearer ${TOKEN}" -H "Accept: application/octet-stream" "$1" -o "$2"
        else
            curl -fsSL "$1" -o "$2"
        fi
    }
    ler() {
        if [ -n "$TOKEN" ]; then
            curl -fsSL -H "Authorization: Bearer ${TOKEN}" "$1"
        else
            curl -fsSL "$1"
        fi
    }
elif command -v wget >/dev/null 2>&1; then
    baixar() {
        if [ -n "$TOKEN" ]; then
            wget -q --header="Authorization: Bearer ${TOKEN}" --header="Accept: application/octet-stream" -O "$2" "$1"
        else
            wget -qO "$2" "$1"
        fi
    }
    ler() {
        if [ -n "$TOKEN" ]; then
            wget -q --header="Authorization: Bearer ${TOKEN}" -O- "$1"
        else
            wget -qO- "$1"
        fi
    }
else
    erro "curl ou wget é necessário"
fi

case "$(uname -s)" in
    Linux)  SO="unknown-linux-musl" ;;
    Darwin) SO="apple-darwin" ;;
    *)      erro "sistema não suportado: $(uname -s). No Windows, baixe o .zip da página de releases." ;;
esac

case "$(uname -m)" in
    x86_64|amd64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)             erro "arquitetura não suportada: $(uname -m)" ;;
esac

ALVO="${ARCH}-${SO}"

VERSAO="${MJSV_VERSION:-}"
if [ -z "$VERSAO" ]; then
    VERSAO=$(ler "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
        | sed -n 's/.*"tag_name"[ ]*:[ ]*"\([^"]*\)".*/\1/p' | head -1)
    if [ -z "$VERSAO" ]; then
        erro "não foi possível descobrir a última versão.
Se o repositório for privado, exporte GITHUB_TOKEN com acesso de leitura a ${REPO};
ou fixe a versão com MJSV_VERSION=vX.Y.Z."
    fi
fi

# Com token, o download sai pela API de assets: a URL de browser
# (github.com/OWNER/REPO/releases/download/...) redireciona para um CDN que NÃO aceita o header
# Authorization, e o redirecionamento devolve 404 em repositório privado.
if [ -n "$TOKEN" ]; then
    ASSETS=$(ler "https://api.github.com/repos/${REPO}/releases/tags/${VERSAO}")
    # A API do GitHub devolve JSON **formatado**, um campo por linha — então nada de partir por
    # chaves: o `name` fica numa linha e o `id` do asset em outra. Cada asset vem na ordem url,
    # id, node_id, name, logo o id do asset é o último `"id":` visto ANTES do nome procurado.
    #
    # `/"id":/` não casa com `"node_id":` (ali o que precede o `id` é `_`, não aspas), e o id da
    # release e o do autor ficam antes do primeiro asset, então são sobrescritos.
    achar_asset() {
        echo "$ASSETS" | awk -v alvo="$1" '
            /"id":/ { match($0, /[0-9]+/); id = substr($0, RSTART, RLENGTH) }
            index($0, "\"name\": \"" alvo "\"") { print id; exit }
        '
    }
fi

NOME="${BIN}-${VERSAO}-${ALVO}"
BASE="https://github.com/${REPO}/releases/download/${VERSAO}"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

echo "Baixando ${NOME}..."
if [ -n "$TOKEN" ]; then
    ID=$(achar_asset "${NOME}.tar.gz")
    [ -n "$ID" ] || erro "a release ${VERSAO} não tem ${NOME}.tar.gz"
    baixar "https://api.github.com/repos/${REPO}/releases/assets/${ID}" "${TMP}/${NOME}.tar.gz" \
        || erro "falha ao baixar ${NOME}.tar.gz"
else
    baixar "${BASE}/${NOME}.tar.gz" "${TMP}/${NOME}.tar.gz" || erro "falha ao baixar ${NOME}.tar.gz"
fi

# Conferir a soma é o motivo de o release publicar SHA256SUMS. Sem isto o script instalaria
# qualquer coisa que voltasse da rede — e ele roda com um pipe para o shell.
if [ -n "$TOKEN" ]; then
    SOMAS_ID=$(achar_asset "SHA256SUMS")
    SOMAS_URL="https://api.github.com/repos/${REPO}/releases/assets/${SOMAS_ID}"
else
    SOMAS_URL="${BASE}/SHA256SUMS"
fi

if baixar "$SOMAS_URL" "${TMP}/SHA256SUMS" 2>/dev/null; then
    if command -v sha256sum >/dev/null 2>&1; then
        SOMA=$(sha256sum "${TMP}/${NOME}.tar.gz" | cut -d' ' -f1)
    elif command -v shasum >/dev/null 2>&1; then
        SOMA=$(shasum -a 256 "${TMP}/${NOME}.tar.gz" | cut -d' ' -f1)
    else
        SOMA=""
        echo "aviso: sem sha256sum/shasum, pulando a verificação" >&2
    fi

    if [ -n "$SOMA" ]; then
        ESPERADA=$(grep " ${NOME}.tar.gz\$" "${TMP}/SHA256SUMS" | cut -d' ' -f1)
        [ -n "$ESPERADA" ] || erro "SHA256SUMS não tem entrada para ${NOME}.tar.gz"
        [ "$SOMA" = "$ESPERADA" ] || erro "soma de verificação NÃO confere — download corrompido ou adulterado"
        echo "Soma conferida."
    fi
else
    echo "aviso: SHA256SUMS indisponível nesta release, pulando a verificação" >&2
fi

tar xzf "${TMP}/${NOME}.tar.gz" -C "$TMP"

# Sem sudo automático: um instalador que escala privilégio sozinho é o tipo de coisa que ninguém
# revisa quando vem por pipe. Se /usr/local/bin não for gravável, cai em ~/.local/bin e diz o que
# fazer para o PATH.
if [ -z "$PREFIX" ]; then
    if [ -w /usr/local/bin ] 2>/dev/null; then
        PREFIX=/usr/local/bin
    else
        PREFIX="${HOME}/.local/bin"
    fi
fi

mkdir -p "$PREFIX"
install -m 0755 "${TMP}/${NOME}/${BIN}" "${PREFIX}/${BIN}"

echo "Instalado em ${PREFIX}/${BIN}"

case ":${PATH}:" in
    *":${PREFIX}:"*) ;;
    *) echo "Adicione ao PATH:  export PATH=\"${PREFIX}:\$PATH\"" ;;
esac

"${PREFIX}/${BIN}" --version
