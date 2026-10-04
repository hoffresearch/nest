---
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-04
domain: repo-ops
---

# Trabalhar na Urna

Este é o ponto de partida para trabalhar no repositório. Os arquivos `AGENTS.md` e `CLAUDE.md` da raiz são links para esta fonte. Mantenha as orientações compartilhadas aqui e consulte os documentos abaixo conforme a tarefa.

| Preciso entender… | Onde consultar |
| --- | --- |
| Arquitetura, contratos e responsáveis por cada parte | [ARC.toml](../../../docs/arc/ARC.toml) |
| Comandos, instalação, modelos e operação das releases | [USAGE.md](../../../docs/USAGE.md) |
| Ambiente de desenvolvimento, testes e contribuição | [CONTRIBUTING.md](../../../docs/CONTRIBUTING.md) |
| Segurança, proveniência e relato de vulnerabilidades | [SECURITY.md](../../../docs/SECURITY.md) |
| Mudanças entregues e decisões | [CHANGELOG](../../../docs/CHANGELOG) |
| Revisão final e arquivos que a mudança pode afetar | [AFTERWORK.md](.skills/afterwork/AFTERWORK.md) e [specs.yaml](.skills/afterwork/specs.yaml) |

## Começar e conduzir o trabalho

Leia a solicitação, confira a branch e o estado do checkout e examine a parte da arquitetura envolvida. Preserve alterações existentes do usuário. Para um assunto novo, use uma branch curta a partir de `origin/main`; uma worktree isolada ajuda quando já há outro trabalho em andamento.

Prossiga com as decisões de implementação e verificações necessárias ao objetivo autorizado. Melhore o desenho quando houver motivo, explicando a escolha e validando seus efeitos. Convenções podem evoluir na mesma mudança; proteja a compatibilidade e as garantias públicas ao fazer isso.

O mantenedor escreve rápido e usa transcrição de voz. Interprete a intenção apesar de erros de digitação, acentos ausentes ou maiúsculas. Pergunte apenas quando uma ambiguidade mudar o resultado do trabalho.

## Autonomia e cuidado com o repositório

Branches, edições, testes, commits, PRs e workflows sem publicação fazem parte do trabalho autorizado. Use caminhos explícitos ao preparar commits e confira o diff para não incluir alterações de outra tarefa.

Publicar, enviar ou mover tags, alterar permissões ou secrets e remover branches remotas exige autorização compatível com a ação. Uma autorização já dada continua válida durante o trabalho. Force-push numa branch de trabalho exige autorização e `--force-with-lease`; preserve a `main` e seus requisitos de revisão, sem bypass de administrador. Não desative verificações para esconder falhas.

Credenciais ficam nos mecanismos de secrets ou autenticação do serviço, fora dos arquivos e dos logs. Uma credencial exposta deve ser tratada como comprometida. O fluxo PyPI usa OIDC; confira no USAGE o procedimento de publicação e recuperação.

O projeto usa commits assinados e squash merge. Ao atualizar um PR empilhado, resolva os conflitos e confira o diff contra a nova base. Depois do merge, compare o conteúdo entregue com a árvore aprovada e explique qualquer diferença. A troca do hash pelo squash é esperada.

## Onde cada parte vive

- `crates/urna-format`: formato binário, leitura, escrita e hashes.
- `crates/urna-runtime`: mmap, índices, kernels e busca.
- `crates/urna-cli`: binário `urna`, comandos e interface de terminal.
- `crates/urna-python`: extensão PyO3 distribuída na wheel.
- `python/`: API Python, construção dos corpora, embedders e catálogo de modelos.
- `forge-core/` e `fuzz/`: workspaces Cargo separados; comandos no workspace principal não os cobrem.
- `packaging/`, `scripts/` e `.github/`: empacotamento, instalação, validação e releases.

Os manifests definem as versões e os requisitos das ferramentas. Consulte-os ao mudar dependências, features ou compatibilidade; o crate da CLI pode ter um MSRV diferente do restante do workspace.

## Cuidados técnicos essenciais

- Preserve a compatibilidade do formato v1 e as fixtures congeladas. Antes de alterar layout, codecs ou identificadores, confira o contrato no ARC e as definições em `crates/urna-format/src/layout/`. Teste leitura, escrita e rejeição de entradas inválidas; mudanças em decoders também pedem os testes de mutação e fuzzing pertinentes.
- Trate dados externos com limites e aritmética checados, erros tipados e comprimentos validados em release. Documente o invariante de cada `unsafe`. Use os leitores de bytes e a ordenação de scores compartilhados, evitando duplicar essas verificações.
- Mantenha a identidade dos modelos: dimensão compatível não basta. Na API Python, informe `expected_model_hash` ao consultar com um embedder conhecido e `query_text` quando a busca precisar do texto original. Presets, snapshots, tokenizer e arquivos baixados participam dessa identidade.
- Preserve as garantias offline. O runtime não abre sockets e o binário usa o `curl` do sistema para downloads de setup. Downloads de modelos e execução de código remoto seguem os consentimentos existentes.
- Use os caminhos compartilhados para descobrir dados, resolver Python, instalar modelos e apresentar erros. A CLI e a interface de terminal devem observar o mesmo comportamento; workers da interface comunicam resultados por canais, sem escrever por cima da tela.
- Distinga as superfícies de instalação: o binário Rust oferece `setup`, `doctor` e a interface de terminal; o comando da wheel é um console de leitura da API Python. Teste cada um conforme seu contrato.

## Validar a mudança

Escolha verificações que exercitem o comportamento alterado e cumpram os checks exigidos pelo projeto. Reaproveite testes existentes; acrescente casos quando houver uma garantia nova ou uma regressão a demonstrar. Fixtures reais e respostas controladas de serviços têm usos diferentes e podem se complementar.

| Mudança | Verificação pertinente |
| --- | --- |
| Rust | Testes dos crates afetados, `cargo fmt --all --check` e Clippy; confira também `--no-default-features` ao tocar na CLI |
| Python | Scripts `tests/test_*.py` e testes próximos ao módulo, mais `sh scripts/ruff_check.sh` |
| `forge-core/` ou `fuzz/` | Comandos no manifesto próprio, conforme CONTRIBUTING e `fuzz/README.md` |
| Formato, busca, desempenho ou integração ampla | `./scripts/release_check.sh`, incluindo a medição quando aplicável |
| Empacotamento e distribuição | Testes dos scripts afetados, coerência dos workflows gerados e ensaio de empacotamento |
| Documentação, links ou comentários | Conferência de caminhos, exemplos e sintaxe afetados; sem repetir a medição de corpora |

O CI, o ensaio e o gate local têm coberturas diferentes. Leia seus scripts para saber o que executam e justifique no PR as verificações usadas. O check `rehearsal` é obrigatório: a dispensa explícita por ausência de impacto é válida; falha, cancelamento ou build necessário pulado não comprovam o ensaio.

Reconstrua a extensão antes dos testes Python quando alterar `urna-format`, `urna-runtime` ou `urna-python`: `cargo build --release -p urna-python --features pyo3/extension-module`. Use o interpretador dos testes e siga o procedimento de cópia da biblioteca no CONTRIBUTING. O gate completo faz essa preparação; uma extensão antiga pode produzir resultados enganosos.

Registre o que passou, falhou ou foi pulado e em qual commit. Um teste dispensado por falta de ferramenta ou dependência não conta como executado. Consulte os pré-requisitos antes de confiar no resumo de uma suíte, incluindo a versão fixada do cargo-release nos testes de preparação.

## Empacotamento e release

`packaging/pyproject.toml` é a fonte da wheel; `packaging/staging/` é gerado. Para mudar a configuração do cargo-dist, rode `dist generate` e depois `python scripts/release_rehearsal.py generate`. Edite as fontes dos geradores e confira os arquivos resultantes.

A preparação local usa `scripts/release_prepare.sh X.Y.Z`, com cargo-release fixado, worktree isolada e PR assinado. Tag e publicação são etapas separadas. Preserve os metadados de citação escolhidos pelo mantenedor ao atualizar os campos versionados.

A release constrói binários, payload e wheels antes de publicar. O PyPI recebe as mesmas wheels por um workflow de topo despachado pela release. Os testes de instalação esperam a versão exata; o relatório acompanha também execuções com falha. O USAGE contém os comandos, requisitos de origem, autenticação e recuperação de uploads parciais.

O ensaio anterior à tag verifica empacotamento e artefatos, sem publicar ou reconstruir os corpora de benchmark. Registre separadamente as provas de TestPyPI e de produção: uma validação local ou um CI verde não comprova um upload ainda não executado.

## Organização e escrita

Prefira módulos por responsabilidade, nomes claros e comentários que expliquem decisões. Respeite o limite de código cobrado pelos checks atuais; ele não impõe dividir documentação ou arquivos gerados. Refatorações devem servir à mudança, preservando os comportamentos e sua cobertura.

Escreva estas instruções em português natural. Mantenha o idioma e as convenções dos demais documentos, com parágrafos curtos, títulos claros e exemplos úteis. Commits e PRs usam inglês simples. Os documentos existentes em maiúsculas, como `USAGE.md`, mantêm seus nomes; código segue o estilo da linguagem e `.editorconfig`.

Atualize a explicação atual no documento responsável. Registre o motivo e o histórico no CHANGELOG e no PR. Preserve notas de migração ou contexto histórico quando forem necessárias para usar o produto corretamente.

## Cuidados que evitam retrabalho

- Busque no código do projeto com `rg` ou `git grep`, respeitando arquivos ignorados. `tools/` e `TMP/` podem conter clones de terceiros.
- Testes de instalação devem usar diretórios isolados e um binário fora de `target/`, para não encontrar acidentalmente o payload, o Python ou os modelos da máquina.
- Atualize `python/forge/catalog.json` pelo gerador ao mudar o registry e confira `python python/forge/model_catalog.py --check`. Resolva snapshots pelas revisões fixadas e preserve a lista de arquivos usada no fingerprint.
- Ao tocar em SIMD ou Miri, confira os requisitos do compilador em `build.rs` e as limitações documentadas nos testes. Mudanças nesses caminhos precisam manter o fallback compatível.
- Mantenha arquivos de dados e pesos nos destinos e políticas de LFS existentes. O hook `scripts/pre-commit` verifica os dados preparados para commit; a release obtém a tabela Potion pelo script com hash fixado.

Antes de concluir, faça a [revisão final](.skills/afterwork/AFTERWORK.md). Entregue o resultado, as evidências e as limitações relevantes. Se depender do mantenedor, indique a ação indispensável de forma direta.
