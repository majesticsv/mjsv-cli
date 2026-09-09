# mjsv

CLI do [MJSV.IO](https://mjsv.io) — VMs, gameservers, firewall e faturamento pela linha de comando,
contra a mesma API que o painel usa (`courtyard-api.mjsv.io`).

O repositório tem dois crates:

| crate | o que é |
|---|---|
| `mjsv-core` | cliente da API: HTTP, cabeçalhos de alvo, login, configuração e renderização |
| `mjsv` | o binário do usuário |

O binário administrativo (`mjsvadm`) vive em um repositório privado e consome `mjsv-core` como
dependência — nada sobre rota administrativa, flag de sistema ou host interno entra aqui.

## Instalação

```sh
cargo install --path crates/mjsv
```

Binários de release (Linux musl, macOS, Windows) saem em GitHub Releases.

## Uso

```sh
mjsv login                    # usuário/senha, com 2FA quando a conta exigir
mjsv login --token            # autentica com uma chave de API já existente
mjsv whoami
mjsv org list
mjsv org use <id>
mjsv token list | create <nome> | revoke <id>
```

Flags globais: `--org`, `--output {table,json,yaml}`, `--api-url`, `--print-request`, `--yes`, `-v`.

### Autenticação em script

```sh
export MJSV_TOKEN="$(mjsv token create ci --output json | jq -r .token)"
```

`MJSV_TOKEN` vence qualquer arquivo, então o CI nunca precisa escrever credencial em disco. O
segredo só é exibido na criação — `mjsv token list` não o devolve.

Logar pelo CLI **não derruba a sessão do navegador**: o `serviceId` usado aqui é `mjsv-cli`, e o
backend só descarta sessões do mesmo `serviceId`.

### `--print-request`

Mostra a requisição que seria enviada — método, URL, cabeçalhos de alvo e corpo — sem enviá-la. O
bearer nunca é impresso.

```sh
$ mjsv whoami --print-request
GET https://courtyard-api.mjsv.io/auth/verify
Authorization: Bearer <omitido>
MJSV-TargetId: 0000
MJSV-TargetType: ROOT
```

É a forma barata de revisar o par de cabeçalhos de um comando, que é a causa quase certa de
qualquer 403 da API.

## Arquivos

| caminho | conteúdo |
|---|---|
| `~/.config/mjsv/config.toml` | org corrente, URL da API, formato preferido |
| `~/.config/mjsv/credentials.toml` | o token — criado com modo `0600` |

São dois arquivos de propósito: o primeiro é inofensivo e pode ser copiado entre máquinas, o
segundo guarda um JWT de 365 dias.

## Desenvolvimento

```sh
cargo test
cargo clippy --all-targets -- -D warnings
./scripts/refresh-routes.sh   # reextrai o inventário de rotas do /doc-json
```

`crates/mjsv-core/tests/api-routes.txt` é o inventário de rotas do backend, vendorizado para o CI
não depender da rede. `tests/contract.rs` confere que toda rota chamada pelo CLI ainda existe — uma
renomeação no backend vira teste vermelho aqui em vez de 404 na máquina de um cliente.
