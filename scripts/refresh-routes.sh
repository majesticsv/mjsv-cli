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

# O spec exige autenticação. O token vem de MJSV_TOKEN (chave de API ou sessão) — o mesmo que o
# CLI usa — ou do credentials.toml de quem já rodou `mjsv login`.
TOKEN="${MJSV_TOKEN:-}"
if [ -z "$TOKEN" ] && [ -f "${HOME}/.config/mjsv/credentials.toml" ]; then
    TOKEN=$(sed -n 's/^token[ ]*=[ ]*"\(.*\)"/\1/p' "${HOME}/.config/mjsv/credentials.toml" | head -1)
fi

if [ -z "$TOKEN" ]; then
    echo "erro: o /doc-json exige autenticação." >&2
    echo "Rode \`mjsv login\` ou exporte MJSV_TOKEN." >&2
    exit 1
fi

curl -fsS -H "Authorization: Bearer ${TOKEN}" "$API_URL/doc-json" | API_URL="$API_URL" python3 -c '
import json
import os
import re
import sys

spec = json.load(sys.stdin)
metodos = ("get", "post", "put", "patch", "delete")
rotas = sorted(
    "%s %s" % (metodo.upper(), caminho)
    for caminho, ops in spec["paths"].items()
    for metodo in ops
    if metodo.lower() in metodos
    # Só rotas de cliente entram no inventário: nada sob /admin, exceto cloud-images (VM_LIST).
    if not re.search(r"/admin(/|$)", caminho) or caminho == "/vm/admin/cloud-images"
    if (metodo.upper(), caminho) not in (("GET", "/org"), ("GET", "/eips"))
)

if len(rotas) < 100:
    sys.exit("spec devolveu apenas %d rotas — recusando sobrescrever o inventário" % len(rotas))

print("# Inventário de rotas da API do MJSV.IO usadas como referência de cobertura.")
print("#")
print("# Existe para que uma rota renomeada no backend apareça como teste vermelho aqui, e não como 404")
print("# na máquina de um cliente. É só a lista de rotas de cliente: rotas sob `/admin` ficam de fora")
print("# (a única exceção é `GET /vm/admin/cloud-images`, que um cliente com a flag VM_LIST chama).")
print("#")
print("# Vendorizado de propósito, para o CI não depender da rede. Atualize com:")
print("#   ./scripts/refresh-routes.sh")
print("# e trate a mudança como revisão: se uma rota sumiu, algum comando do CLI vai quebrar.")
print("#")
print("\n".join(rotas))
' > "$TMP"

mv "$TMP" "$DEST"
trap - EXIT

echo "atualizado: $DEST ($(grep -vc '^#' "$DEST") rotas)"
