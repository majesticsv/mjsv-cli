# mjsv-cli — o CLI do MJSV.IO para as máquinas internas.
#
# Diferente dos pacotes do stack XDP, este NÃO tem unidade systemd, não roda como serviço e não
# toca em /etc: é um binário de linha de comando e mais nada. Credencial não é empacotada — ela
# nasce em ~/.config/mjsv/credentials.toml, por usuário, quando alguém roda `mjsv login`.
#
# O binário é o mesmo artefato musl publicado no GitHub Releases: estático, então não depende da
# glibc da máquina e o mesmo RPM serve para toda a frota, do EL8 ao EL9.

# O binário já vem pronto e sem DWARF. Sem isto o rpmbuild roda o find-debuginfo, não acha fonte
# nenhuma no buildroot, escreve um debugsourcefiles.list vazio e aborta o build com
#     error: Empty %%files file .../debugsourcefiles.list
# — o mesmo tropeço que os pacotes XDP levaram na primeira build real.
%global debug_package %{nil}

Name:           mjsv-cli
Version:        %{?_version}%{!?_version:0.0.0}
Release:        %{?_release}%{!?_release:1}%{?dist}
Summary:        CLI do MJSV.IO

License:        MIT
URL:            https://github.com/majesticsv/mjsv-cli
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  coreutils
ExclusiveArch:  x86_64

%description
Gerencia VMs, gameservers, firewall, EIPs, pedidos e tickets do MJSV.IO pela linha de comando,
contra a mesma API que o painel usa.

Autentica com `mjsv login` (usuário/senha, com 2FA quando a conta exigir) ou com uma chave de API
em MJSV_TOKEN. A credencial fica em ~/.config/mjsv/credentials.toml com modo 0600 e NÃO é
fornecida por este pacote.

%prep
%setup -q

%build
# Vazio de propósito — o binário de release é construído no workflow.

%install
rm -rf %{buildroot}
install -D -m 0755 mjsv %{buildroot}%{_bindir}/mjsv

# Completions: geradas no workflow, opcionais. O `|| :` evita quebrar a build quando o tarball vem
# só com o binário.
install -d %{buildroot}%{_datadir}/bash-completion/completions
install -m 0644 completions/mjsv.bash %{buildroot}%{_datadir}/bash-completion/completions/mjsv || :

%files
%{_bindir}/mjsv
%{_datadir}/bash-completion/completions/mjsv

%changelog
* Fri Nov 27 2026 MJSV <infra@mjsv.io> - 0.3.0-1
- Cobertura de todas as rotas de cliente da API (vpc, volume, protection, notification, session,
  profile, product, payment; vm, gs, org, order e ticket completos).
- Sistema visual no terminal: banner, cartões de erro e sucesso, confirmações por nível, metros e
  marcadores de estado. Só em terminal interativo; json/yaml inalterados.
* Sun Sep 14 2026 MJSV <infra@mjsv.io> - 0.1.0-1
- Primeira versão: auth, org, vm, gameserver, firewall, eip, order, ticket, ssh-key.
