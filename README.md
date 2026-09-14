# mjsv

CLI do [MJSV.IO](https://mjsv.io) — VMs, gameservers, firewall, EIPs, pedidos e tickets pela linha
de comando, contra a mesma API que o painel usa (`courtyard-api.mjsv.io`).

| crate | o que é |
|---|---|
| `mjsv-core` | cliente da API: HTTP, cabeçalhos de alvo, login, configuração e renderização |
| `mjsv` | o binário do usuário |

O binário administrativo (`mjsvadm`) vive em `majesticsv/mjsv-cli-admin` e consome `mjsv-core` como
dependência por tag. Nada sobre rota administrativa, flag de sistema ou host interno entra aqui —
este repositório é escrito para poder ser aberto.

## Instalação

```sh
curl -fsSL https://raw.githubusercontent.com/majesticsv/mjsv-cli/master/install.sh | sh
```

O script detecta sistema e arquitetura, confere a soma SHA-256 do que baixou contra o `SHA256SUMS`
da release e instala em `/usr/local/bin` — ou em `~/.local/bin` se o primeiro não for gravável. Ele
**não** chama `sudo` sozinho. Sobrescreva o destino com `MJSV_INSTALL_DIR` e fixe a versão com
`MJSV_VERSION`.

Ou compile:

```sh
cargo install --path crates/mjsv
```

Máquinas internas recebem por RPM (`cicd/rpm/mjsv-cli.spec`).

## Uso

```sh
mjsv login                                 # usuário/senha, com 2FA quando a conta exigir
mjsv login --token                         # autentica com uma chave de API existente
mjsv whoami
mjsv org list | use <id>

mjsv vm list | show | start | stop | reboot | kill | power | metrics | console
mjsv vm firewall list <vm> | add <vm> … | delete <vm> <regra>
mjsv eip list <vm>

mjsv gs list | show | start | stop | restart | kill | reinstall
              | metadata | databases | schedules | sftp

mjsv order list | pending | show | cancel | invoice
mjsv ticket list | categories | show | messages | open | reply | close
mjsv ssh-key list | add | delete
mjsv token list | create | revoke

mjsv completions bash > /etc/bash_completion.d/mjsv
```

Flags globais: `--org`, `--output {table,json,yaml}`, `--api-url`, `--print-request`, `--yes`, `-v`.

**Ids são de serviço, não do recurso.** A coluna `SERVICE` de `mjsv vm list` e `mjsv gs list` é o
que todos os outros subcomandos recebem — as rotas da API são `/vm/:vmServiceId` e
`/gameserver/:gameServerServiceId`. Passar o id da VM dá 403, não 404.

### Autenticação em script

```sh
export MJSV_TOKEN="$(mjsv token create ci --output json | jq -r .token)"
```

`MJSV_TOKEN` vence qualquer arquivo, então o CI nunca precisa escrever credencial em disco. O
segredo só é exibido na criação — `mjsv token list` não o devolve.

Logar pelo CLI **não derruba a sessão do navegador**: o `serviceId` usado aqui é `mjsv-cli`, e o
backend só descarta sessões do mesmo `serviceId`.

### Saída para máquina

`--output json` repassa o corpo da API sem reordenar nem reinterpretar — é o contrato de script.
`--output yaml` é a mesma coisa, legível.

Em saída de máquina os comandos destrutivos **exigem `--yes`**: não há como perguntar, e um script
que apaga recurso sem ninguém confirmar é pior do que um que falha.

### `--print-request`

Mostra a requisição que seria enviada — método, URL, cabeçalhos de alvo e corpo — sem enviá-la. O
bearer nunca é impresso.

```sh
$ mjsv vm firewall add nUME7 --action drop --protocol tcp --ports 65000 --print-request
POST https://courtyard-api.mjsv.io/vm/nUME7/firewall/rules
Authorization: Bearer <omitido>
MJSV-TargetId: nUME7
MJSV-TargetType: VIRTUALMACHINE

{
  "action": "drop",
  "portEnd": 65000,
  "portStart": 65000,
  "protocol": "tcp"
}
```

É a forma barata de revisar o par de cabeçalhos, que é a causa quase certa de qualquer 403 da API.

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
cargo fmt --all --check
./scripts/refresh-routes.sh   # reextrai o inventário de rotas do /doc-json
```

`crates/mjsv-core/tests/api-routes.txt` é o inventário de rotas do backend, vendorizado para o CI
não depender da rede. `tests/contract.rs` confere que toda rota chamada pelo CLI ainda existe — uma
renomeação no backend vira teste vermelho aqui em vez de 404 na máquina de um cliente.

Uma rota que ainda depende de deploy do backend entra em `routes.rs` marcada com `pending`, e um
segundo teste **falha assim que ela chega à produção**, cobrando a remoção da marca.

### Release

Empurrar uma tag `v*` dispara o workflow de release: musl (x86_64/aarch64), macOS
(x86_64/aarch64), Windows (x86_64), mais `SHA256SUMS`.
