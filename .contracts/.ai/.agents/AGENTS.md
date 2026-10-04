---
project: urna
audience: diretrizes globais para desenvolvedores humanos e agentes de código
status: active
last-updated: 2026-10-04
domain: repo-ops
---

# Instruções de desenvolvimento do urna.dev

Este é o ponto de partida para desenvolvedores humanos e agentes de código (CLAUDE.md, AGENTS.md, kimi, hermes, codex, windsurf, cursor, aider etc.) trabalharem com eficiência nesta aplicação.

| Preciso entender… | Onde consultar |
| --- | --- |
| Arquitetura, contratos e responsáveis por cada parte | [ARC.toml](../../../docs/ARC.toml) |
| Comandos, instalação, modelos e operação das releases | [USAGE.md](../../../docs/USAGE.md) |
| Ambiente de desenvolvimento, testes e contribuição | [CONTRIBUTING.md](../../../docs/CONTRIBUTING.md) |
| Segurança, proveniência e relato de vulnerabilidades | [SECURITY.md](../../../docs/SECURITY.md) |
| Mudanças entregues e histórico | [CHANGELOG](../../../docs/CHANGELOG) |
| Decisões de arquitetura e lições de referência | [ADR](../../../docs/ADR/README.md) |
| Revisão final e arquivos que a mudança pode afetar | [AFTERWORK.md](.skills/afterwork/AFTERWORK.md) e [specs.yaml](.skills/afterwork/specs.yaml) |

## Começar e conduzir o trabalho

Leia a solicitação, confira a branch e o estado do checkout e examine a parte da arquitetura envolvida. Preserve alterações existentes do usuário. Para um assunto novo, use uma branch curta a partir de `origin/main`; uma worktree isolada ajuda quando já há outro trabalho em andamento.

Prossiga com as decisões de implementação e verificações necessárias ao objetivo autorizado. Melhore o desenho quando houver motivo, explicando a escolha e validando seus efeitos. Convenções podem evoluir na mesma mudança; proteja a compatibilidade e as garantias públicas ao fazer isso.

O dev principal e tech lead da aplicação escreve rápido e usa transcrição de voz. Interprete a intenção apesar de erros de digitação, acentos ausentes, Caps Lock ativo ou maiúsculas. Não interprete erros como fadiga de trabalho, dispersão ou meltdown emocional.

## Autonomia e cuidado com o repositório

Branches, edições, testes, commits, PRs e workflows sem publicação fazem parte do trabalho autorizado. Use caminhos explícitos ao preparar commits e confira o diff para não incluir alterações de outra tarefa.

Publicar, enviar ou mover tags, alterar permissões ou secrets e remover branches remotas exige autorização compatível com a ação. Uma autorização já dada continua válida durante o trabalho. Force-push numa branch de trabalho exige autorização e `--force-with-lease`; preserve a `main` e seus requisitos de revisão, sem bypass de administrador. Não desative verificações para esconder falhas.

Credenciais ficam nos mecanismos de secrets ou autenticação do serviço, fora dos arquivos e dos logs. Uma credencial exposta deve ser tratada como comprometida. O fluxo PyPI usa OIDC; confira no USAGE o procedimento de publicação e recuperação.

O projeto usa commits assinados e squash merge. Ao atualizar um PR empilhado, resolva os conflitos e confira o diff contra a nova base. Depois do merge, compare o conteúdo entregue com a árvore aprovada e explique qualquer diferença. A troca do hash pelo squash é esperada.

## Principais responsabilidades da aplicação

- `crates/urna-format`: formato binário, leitura, escrita e hashes.
- `crates/urna-engine`: mmap, índices, kernels e busca.
- `crates/urna-clitui`: binário `urna`, comandos e interface de terminal.
- `crates/urna-bridge`: extensão PyO3 distribuída na wheel.
- `python/`: API Python, construção dos corpora, embedders e catálogo de modelos.
- `crates/urna-ingest` (schema `.fci` do forge) e `fuzz/`: workspaces Cargo separados; o `Cargo.toml` da raiz exclui o primeiro e comandos no workspace principal não os cobrem.
- `packs/`, `script/` e `.github/`: empacotamento, instalação, validação e releases.

Os manifests definem as versões e os requisitos das ferramentas. Consulte-os ao mudar dependências, features ou compatibilidade; o crate da CLI pode ter um MSRV diferente do restante do workspace.

## Cuidados técnicos essenciais

- Antes de alterar layout, codecs ou identificadores, confira o contrato no ARC e as definições em `crates/urna-format/src/layout/`. Teste leitura, escrita e rejeição de entradas inválidas; mudanças em decoders também pedem os testes de mutação e fuzzing pertinentes.
- Trate dados externos com limites e aritmética checados, erros tipados e comprimentos validados em release. Documente o invariante de cada `unsafe`. Use os leitores de bytes e a ordenação de scores compartilhados, evitando duplicar essas verificações.
- Mantenha a identidade dos modelos: dimensão compatível não basta. Na API Python, informe `expected_model_hash` ao consultar com um embedder conhecido e `query_text` quando a busca precisar do texto original. Presets, snapshots, tokenizer e arquivos baixados participam dessa identidade.
- Preserve as garantias offline. O runtime não abre sockets, o binário não incorpora uma pilha de rede e o setup baixa arquivos pelo `curl` do sistema. Downloads de modelos e execução de código remoto seguem os consentimentos existentes.
- Use os caminhos compartilhados para descobrir dados, resolver Python, instalar modelos e apresentar erros. A CLI e a interface de terminal devem observar o mesmo comportamento; workers da interface comunicam resultados por canais, sem escrever por cima da tela.
- Preserve o rerank exato, a interpretação dos hashes e as citações do texto canônico armazenado. Ao mudar esses caminhos, use os contratos e testes existentes como referência.
- Distinga as superfícies de instalação: o binário Rust oferece `setup`, `doctor` e a interface de terminal; o comando da wheel é um console de leitura da API Python. Teste cada um conforme seu contrato.

Limitações relevantes ao escolher modelos e interpretar resultados: Potion é voltado ao inglês; o tokenizer BM25 atual perde qualidade em CJK, tailandês e lao; consultas com sentence-transformers carregam Python e o modelo, custo que as medições do engine não incluem. Considere essas limitações quando a tarefa tocar na área e valide qualquer melhoria proposta.

## Validar a mudança

Escolha verificações que exercitem o comportamento alterado e cumpram os checks exigidos pelo projeto. Reaproveite testes existentes; acrescente casos quando houver uma garantia nova ou uma regressão a demonstrar. Fixtures reais e respostas controladas de serviços têm usos diferentes e podem se complementar.

| Mudança | Verificação pertinente |
| --- | --- |
| Rust | Testes dos crates afetados, `cargo fmt --all --check` e Clippy; confira também `--no-default-features` ao tocar na CLI |
| Python | Scripts `tests/test_*.py` e testes próximos ao módulo, mais `sh script/ruff_check.sh` |
| `crates/urna-ingest/` ou `fuzz/` | Comandos no manifesto próprio, conforme CONTRIBUTING e `fuzz/README.md` |
| Formato, busca, desempenho ou integração ampla | `./script/release_check.sh`, incluindo a medição quando aplicável |
| Empacotamento e distribuição | Testes dos scripts afetados, coerência dos workflows gerados e ensaio de empacotamento |
| Documentação, links ou comentários | Conferência de caminhos, exemplos e sintaxe afetados; sem repetir a medição de corpora |

O CI, o ensaio e o gate local têm coberturas diferentes. Leia seus scripts para saber o que executam e justifique no PR as verificações usadas. O check `rehearsal` é obrigatório: a dispensa explícita por ausência de impacto é válida; falha, cancelamento ou build necessário pulado não comprovam o ensaio.

Reconstrua a extensão antes dos testes Python quando alterar `urna-format`, `urna-engine` ou `urna-bridge`: `cargo build --release -p urna-bridge --features pyo3/extension-module`. Use o interpretador dos testes e siga o procedimento de cópia da biblioteca no CONTRIBUTING. O gate completo faz essa preparação; uma extensão antiga pode produzir resultados enganosos.

Registre o que passou, falhou ou foi pulado e em qual commit. Um teste dispensado por falta de ferramenta ou dependência não conta como executado. Consulte os pré-requisitos antes de confiar no resumo de uma suíte, incluindo a versão fixada do cargo-release nos testes de preparação.

## Empacotamento e release

`packs/pyproject.toml` é a fonte da wheel; `packs/staging/` é gerado. Para mudar a configuração do cargo-dist, rode `dist generate` e depois `python script/rehearsal.py generate`. Edite as fontes dos geradores e confira os arquivos resultantes.

A preparação local usa `script/releasepr.sh X.Y.Z`, com cargo-release fixado, worktree isolada e PR assinado. Tag e publicação são etapas separadas. Preserve os metadados de citação escolhidos pelo mantenedor ao atualizar os campos versionados.

A release constrói binários, payload e wheels antes de publicar. O PyPI recebe as mesmas wheels por um workflow de topo despachado pela release. Os testes de instalação esperam a versão exata; o relatório acompanha também execuções com falha. O USAGE contém os comandos, requisitos de origem, autenticação e recuperação de uploads parciais.

O ensaio anterior à tag verifica empacotamento e artefatos, sem publicar ou reconstruir os corpora de benchmark. Registre separadamente as provas de TestPyPI e de produção: uma validação local ou um CI verde não comprova um upload ainda não executado.

## Organização e escrita

Organize o código por responsabilidade, com nomes claros e comentários que expliquem decisões. O limite é de 639 linhas por arquivo de código, incluindo comentários e linhas em branco. Ao criar ou alterar um arquivo que ultrapasse esse limite, examine suas responsabilidades, dependências e consumidores e divida-o em módulos coesos. Preserve o comportamento, atualize imports e chamadas e valide os caminhos afetados.

O limite não se aplica à documentação, a arquivos dedicados a testes e fixtures, nem a arquivos gerados, vendorizados, dados estruturados ou lockfiles, como JSON, JSONL, TOML, YAML, CSV e RON. Essas exceções não dispensam organização. Configurações e dados escritos dentro de um arquivo de código continuam sujeitos ao limite desse arquivo.

Mantenha a reorganização ligada à tarefa. Use a revisão final do AFTERWORK para conferir arquivos deslocados, código sem uso e resíduos do trabalho.

Escreva estas instruções em português natural. Mantenha o idioma e as convenções dos demais documentos, com parágrafos curtos, títulos claros e exemplos úteis. Commits e PRs usam inglês simples. Os documentos existentes em maiúsculas, como `USAGE.md`, mantêm seus nomes; código segue o estilo da linguagem e `.editorconfig`.

Atualize a explicação atual no documento responsável. Registre o motivo e o histórico no CHANGELOG e no PR, e uma decisão de arquitetura ou lição de referência em `docs/ADR/`. Preserve notas de migração ou contexto histórico quando forem necessárias para usar o produto corretamente.

## Cuidados que evitam retrabalho

- Busque no código do projeto com `rg` ou `git grep`, respeitando arquivos ignorados. `tools/` e `TMP/` podem conter clones de exemplos opensource, estudos e anotações da equipe de desenvolvimento.
- Testes de instalação devem usar diretórios isolados e um binário fora de `target/`, para não encontrar acidentalmente o payload, o Python ou os modelos da máquina.
- Atualize `python/forge/catalog.json` pelo gerador ao mudar o registry e confira `python python/forge/model_catalog.py --check`. Resolva snapshots pelas revisões fixadas e preserve a lista de arquivos usada no fingerprint.
- Ao tocar em SIMD ou Miri, confira os requisitos do compilador em `build.rs` e as limitações documentadas nos testes. Mudanças nesses caminhos precisam manter o fallback compatível.
- Mantenha arquivos de dados e pesos nos destinos e políticas de LFS existentes. O hook `script/pre-commit` verifica os dados preparados para commit; a release obtém a tabela Potion pelo script com hash fixado.

Antes de concluir, faça a [revisão final](.skills/afterwork/AFTERWORK.md). Entregue o resultado, as evidências e as limitações relevantes. Se depender do desenvolvedor humano, indique a ação indispensável de forma direta.
