# Instruções de desenvolvimento do urna.dev

Pockebok de boas praticas para desenvolvedores humanos e agentes de código (CLAUDE.md, AGENTS.md, kimi, hermes, codex, windsurf, cursor, aider etc.) trabalharem com eficiência nesta aplicação.

# Primcipais arquivos de contexto:

| Arquitetura, contratos e responsáveis por cada parte | [ARCHS.toml](../../docs/ARCHS.toml) |
| Comandos, instalação, modelos e operação das releases | [USAGE.md](../../docs/USAGE.md) |
| Ambiente de desenvolvimento, testes e contribuição | [CONTRIBUTING.md](../../docs/CONTRIBUTING.md) |
| Segurança, proveniência e relato de vulnerabilidades | [SECURITY.md](../../docs/SECURITY.md) |
| Mudanças entregues e histórico | [CHANGELOG](../../docs/CHANGELOG) |
| Decisões de arquitetura e lições de referência | [ADR](../../docs/adr/README.md) |
| Revisão final e arquivos que a mudança pode afetar | [afterwork](skill/afterwork/SKILL.md) e [specs.yaml](skill/afterwork/specs.yaml) |
| Compilar, rodar e dirigir o CLI e o explorer | [driveurna](skill/driveurna/SKILL.md) |
| Revisar o repositório inteiro contra o estado real | [factcheck](skill/factcheck/SKILL.md) |
| Preparar, publicar e provar uma release em todos os canais | [releasing](skill/releasing/SKILL.md) |
| Renomear pastas, crates, pacotes, scripts, workflows ou testes | [renameops](skill/renameops/SKILL.md) |
| Sincronizar benchmarks, datasets e repositórios externos | [benchsync](skill/benchsync/SKILL.md) e [bench.yaml](skill/benchsync/bench.yaml) |
| O que repetir e o que evitar, aprendido em sessões anteriores | [mantra.md](../rules/mantra/mantra.md) e [taboo.md](../rules/taboos/taboo.md) |

Para uma nova task, crie uma branch curta a partir de `origin/main`, sempre analise a razia ntes e faça um commit de checkpoint para nao perder nada; uma worktree isolada ajuda quando já há outro trabalho em andamento.

O dev principal e tech lead da aplicação escreve rápido e usa transcrição de voz. Interprete a intenção apesar de erros de digitação, acentos ausentes, Caps Lock ativo ou maiúsculas. Não interprete erros como fadiga de trabalho, dispersão ou meltdown emocional.

 O fluxo PyPI usa OIDC; confira no USAGE o procedimento de publicação e recuperação.

Antes do primeiro commit, defina o tamanho do trabalho e procure uma issue existente (`gh issue list --search`). Se o pedido já tiver uma issue, use-a. Se o escopo mudar, atualize o corpo da issue e vincule a ela as novas sub-issues.

Uma mudança coesa tem uma issue e um PR. Correções do mesmo assunto formam uma mudança só, mesmo quando tocam arquivos diferentes. Vários itens independentes, como uma série de renomeações ou um conjunto de skills, ganham uma épica com o objetivo e a lista dos itens, e uma sub-issue por item, ligada à épica pelo recurso de sub-issues do GitHub. Cada sub-issue tem o seu PR.

## Principais responsabilidades da aplicação

- `rust/format`: formato binário, leitura, escrita e hashes.
- `rust/engine`: mmap, índices, kernels e busca.
- `rust/clitui`: binário `urna`, comandos e interface de terminal.
- `rust/bridge`: extensão PyO3 distribuída na wheel.
- `rust/bridge/python/urna/`: pacote `urna`, com a API Python, a construção dos corpora, os embedders e o catálogo de modelos.
- `tool/bench/`: benchmarks e gates de medição; `tool/tests/`: as suítes Python.
- `rust/ingest` (schema `.fci` do forge) e `fuzz/`: workspaces Cargo separados; o `Cargo.toml` da raiz exclui o primeiro e comandos no workspace principal não os cobrem.
- `pkgs/`, `tool/tasks/` e `.github/`: empacotamento, instalação, validação e releases.


## Empacotamento e release

`pkgs/wheel/pyproject.toml` é a fonte da wheel; `pkgs/stage/` é gerado. Para mudar a configuração do cargo-dist, rode `dist generate` e depois `python tool/tasks/rehearsal.py generate`. Edite as fontes dos geradores e confira os arquivos resultantes.

A preparação local usa `tool/tasks/releasepr.sh X.Y.Z`, com cargo-release fixado, worktree isolada e commit assinado. Tag e publicação são etapas separadas; a skill [releasing](skill/releasing/SKILL.md) conduz a release do número à prova em cada canal. Preserve os metadados de citação escolhidos pelo mantenedor ao atualizar os campos versionados.

A release constrói binários, payload e wheels antes de publicar. O PyPI recebe as mesmas wheels por um workflow de topo despachado pela release. Os testes de instalação esperam a versão exata; o relatório acompanha também execuções com falha. O USAGE contém os comandos, requisitos de origem, autenticação e recuperação de uploads parciais.

O ensaio anterior à tag verifica empacotamento e artefatos, sem publicar ou reconstruir os corpora de benchmark. Registre separadamente as provas de TestPyPI e de produção: uma validação local ou um CI verde não comprova um upload ainda não executado.

## Organização e escrita

Organize o código por responsabilidade, com nomes claros e comentários que expliquem decisões. Todo nome novo segue a tabela de comprimentos da seção Naming do CONTRIBUTING (ADR-0003) e o léxico de abreviações do `docs/TERMS.md`; o `tool/tasks/namecheck.py` confere os dois no CI. O limite é de 639 linhas por arquivo de código, incluindo comentários e linhas em branco. Ao criar ou alterar um arquivo que ultrapasse esse limite, examine suas responsabilidades, dependências e consumidores e divida-o em módulos coesos. Preserve o comportamento, atualize imports e chamadas e valide os caminhos afetados.

O limite não se aplica à documentação, a arquivos dedicados a testes e fixtures, nem a arquivos gerados, vendorizados, dados estruturados ou lockfiles, como JSON, JSONL, TOML, YAML, CSV e RON. Essas exceções não dispensam organização. Configurações e dados escritos dentro de um arquivo de código continuam sujeitos ao limite desse arquivo.

Mantenha a reorganização ligada à tarefa. Use a revisão final do afterwork para conferir arquivos deslocados, código sem uso e resíduos do trabalho.

Escreva estas instruções em português natural. Mantenha o idioma e as convenções dos demais documentos, com parágrafos curtos, títulos claros e exemplos úteis. Commits e PRs usam inglês simples. Os documentos existentes em maiúsculas, como `USAGE.md`, mantêm seus nomes; código segue o estilo da linguagem e `.editorconfig`.

Atualize aprendizados importandes de arquitetura ou lição de referência em `docs/adr/`. Preserve notas de migração ou contexto histórico quando forem necessárias para usar o produto corretamente.


## Idioma e comunicação
- Sempre responda em português do Brasil (PT-BR) quando o desenvolvedor escrever em português. Não troque para o inglês no meio da sessão.
- Ao traduzir docs para PT-BR, escreva em português natural, não tradução literal, entender e adaptar linguagem expressões idiomaticas e afinas para o PR BR.
- Respostas  diretas. Listas com um item por linha, e um resumo de uma pagrafo no maximo abaixo, se envolve link externo, como (github, hiugginface, ou relacionados), sempre apresentar a url do repositório testando antes de inserir no chat. 

## Jeito de trabalhar
- Para reestruturações, reorganizações ou qualquer coisa destrutiva (por exemplo: apagar e/ou mover pastas, reescrever histórico), proponha um plano curto antes e espere aprovação.
- Quando o usuário der exemplos do resultado esperado, trate cada item individualmente para bater com eles. Não gere em massa com script sem antes realziar um prova de conceito pequena que comprove que não vai quebrar.
- Use o pipeline, os workers e os crons do próprio projeto. Não escreva scripts improvisados para substituir isso. Se for necessários consturir um novo, construa na pasta de scprit da aplicaçao.
- Respostas curtas e diretas. Listas com um item por linha, sem introdução longa.
- Quando pedirem para rodar uma GUI, confirme que a janela está visível de fato (traga para frente, tire um screenshot) antes de dizer que deu certo.
