#!/bin/sh
# Regenera o inventário de rotas a partir do spec da API.
#
# `sh -eu` e nunca `-x`: nada aqui manipula segredo hoje, mas o padrão dos workflows do MJSV é esse
# justamente porque o xtrace já vazou credencial em log de CI uma vez.
#
# A escrita é feita em arquivo temporário e só então movida: redirecionar direto para o destino
# trunca o inventário antes de saber se o comando funcionou, e um erro no meio deixaria o
# repositório com uma lista vazia que passa no teste de contrato por acidente.
set -eu

API_URL="${MJSV_API_URL:-https://courtyard-api.mjsv.io}"
DEST="$(dirname "$0")/../crates/mjsv-core/tests/api-routes.txt"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

# O spec passou a ser restrito a staff. O token vem de MJSV_TOKEN (chave de API ou sessão) — o
# mesmo que o CLI usa — ou do credentials.toml de quem já rodou `mjsv login`.
TOKEN="${MJSV_TOKEN:-}"
if [ -z "$TOKEN" ] && [ -f "${HOME}/.config/mjsv/credentials.toml" ]; then
    TOKEN=$(sed -n 's/^token[ ]*=[ ]*"\(.*\)"/\1/p' "${HOME}/.config/mjsv/credentials.toml" | head -1)
fi

if [ -z "$TOKEN" ]; then
    echo "erro: o /doc-json é restrito a administradores." >&2
    echo "Rode \`mjsv login\` com uma conta de staff, ou exporte MJSV_TOKEN." >&2
    exit 1
fi

curl -fsS -H "Authorization: Bearer ${TOKEN}" "$API_URL/doc-json" | API_URL="$API_URL" python3 -c '
import datetime
import json
import os
import sys

spec = json.load(sys.stdin)
metodos = ("get", "post", "put", "patch", "delete")
rotas = sorted(
    "%s %s" % (metodo.upper(), caminho)
    for caminho, ops in spec["paths"].items()
    for metodo in ops
    if metodo.lower() in metodos
)

if len(rotas) < 100:
    sys.exit("spec devolveu apenas %d rotas — recusando sobrescrever o inventário" % len(rotas))

print("# Inventário de rotas da API do MJSV.IO, extraído de %s/doc-json." % os.environ["API_URL"])
print("#")
print("# Existe para que uma rota renomeada no backend apareça como teste vermelho aqui, e não como 404")
print("# na máquina de um cliente. É só a lista de rotas: o spec não tem schema de resposta nenhum,")
print("# então ele não serve para gerar tipos — só para conferir cobertura.")
print("#")
print("# Vendorizado de propósito, para o CI não depender da rede. Atualize com:")
print("#   ./scripts/refresh-routes.sh")
print("# e trate a mudança como revisão: se uma rota sumiu, algum comando do CLI vai quebrar.")
print("#")
print("# Coletado em %s." % datetime.date.today().isoformat())
print("\n".join(rotas))
' > "$TMP"

mv "$TMP" "$DEST"
trap - EXIT

echo "atualizado: $DEST ($(grep -vc '^#' "$DEST") rotas)"
