---
name: releaseops
description: preparar, publicar e conferir uma release do urna em todos os canais (GitHub, crates.io, PyPI, npm, Homebrew), com crates, metadados, descrições e documentação; use quando pedirem release, publicar, cortar versão ou subir para os registries.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-05
domain: workflow
---

# Release do urna, da versão à prova em cada canal

Use este procedimento para levar uma versão da `main` até todos os canais e provar o que cada um serve. A operação detalhada, as credenciais e a recuperação de cada canal estão no checklist do mantenedor do [USAGE.md](../../../../../docs/USAGE.md); a validação por impacto, no [AGENTS.md](../../AGENTS.md). Uma versão publicada no crates.io ou no PyPI é permanente: confira antes de cada passo sem volta. Antes de executar qualquer etapa, crie a lista de tarefas na ferramenta de tarefas do agente, conforme o fim deste arquivo.

## 1. Confira se a `main` está pronta

A `main` precisa estar verde no `gatecheck` e no `rehearsal` do último commit, sem PR de release aberto. Decida o número pela política 0.x do CHANGELOG: o patch para mudanças compatíveis, o minor quando houver quebra; `cargo-semver-checks` no `urna-format` ajuda a decidir.

Confira as contas antes de começar:

- tokens do npm e do tap do Homebrew dentro do prazo (as datas estão no checklist do USAGE);
- um crate publicado pela primeira vez, inclusive um crate renomeado, exige o escopo `publish-new` no `CARGO_REGISTRY_TOKEN`;
- o push da tag `v*` exige uma conta admin do repositório ou da organização: o ruleset `release-tags` recusa as demais;
- o publisher do PyPI aponta para o workflow e o environment atuais, e o environment `pypi` aceita deploy a partir das tags `v*`.

## 2. Revise o que cada registry vai mostrar

Cada canal publica uma página com os metadados do pacote, e ela só muda na próxima versão. Revise antes da tag:

- `description`, `keywords`, `categories`, `readme`, `homepage` e `documentation` de cada crate publicado em `crates/*/Cargo.toml`, e os campos equivalentes em `packs/pyproject.toml`;
- o README, que o crates.io, o PyPI e o npm renderizam: links e imagens apontam para a `main` por URL absoluta, então um arquivo movido depois da release quebra a página publicada até a versão seguinte;
- os nomes publicados: o binário `urna`, os crates com o prefixo `urna-` e o CLI como `urna`. Uma pasta renomeada não muda o pacote. Um crate renomeado publica com o nome novo, e o nome antigo fica na última versão: não o apague, porque versões antigas de outros pacotes dependem dele e o nome livre poderia ser registrado por outra pessoa.

## 3. Prepare o PR da release

`script/releasepr.sh X.Y.Z` cria a worktree, atualiza a versão do workspace, as pins, o lockfile, a seção datada do CHANGELOG e os campos versionados do `CITATION.cff`, roda o preflight e abre o PR assinado. Escreva no PR o parágrafo de resumo da seção da versão, como as releases anteriores têm. O `fuzz/Cargo.lock` fica de fora da preparação: atualize-o com `cargo update -p urna-format -p urna-engine --manifest-path fuzz/Cargo.toml`. O mantenedor revisa e mescla o PR; o merge é a aprovação do número.

## 4. Crie a tag e acompanhe a release

Depois do merge, crie a tag anotada e assinada no commit da release, confira a assinatura e envie:

```sh
git tag -s vX.Y.Z -m vX.Y.Z <commit>
git -c gpg.ssh.allowedSignersFile=.github/trustkeys verify-tag vX.Y.Z
git push origin vX.Y.Z
```

Acompanhe o run `Release` até o fim com `gh run watch`. A ordem é a verificação da tag, os builds dos binários e das wheels, o job global, o host que cria a release no GitHub e os jobs de publicação. Antes de confiar no estado de um PR ou de um run, confira que ele aponta para o commit enviado: o GitHub pode demorar a registrar um push.

## 5. Prove cada canal

Confira a versão exata em cada serviço, não o CI verde:

- GitHub: a release com os binários de cada alvo, as wheels, os `.sha256`, o payload, o SBOM e o `sha256.sum`; `gh attestation verify <arquivo> --repo hoffresearch/urna` num binário e numa wheel;
- crates.io: cada crate na versão, pela API;
- PyPI: os arquivos da versão e a proveniência PEP 740 (o endpoint de integridade responde 200);
- npm e Homebrew: a versão servida pelo registry e pela fórmula do tap.

O `runreport` resume o run e cada canal; o `setuptest` instala o produto por todos os canais. Registre o run, o serviço e os artefatos de cada prova. Um teste no TestPyPI não comprova a publicação de produção.

## 6. Recupere uma falha sem mover a tag

Uma tag só pode ser movida enquanto nada irreversível saiu, e só por um admin, porque o ruleset `release-tags` recusa os demais. Depois disso, cada canal é decidido separadamente, normalmente com a próxima versão de patch. Um job de publicação que falhou volta com `gh run rerun <id> --failed`; o upload do PyPI ignora o arquivo que já tem o mesmo sha256. Uma falha do `setuptest` que vem do próprio teste se corrige num branch, e a correção se prova antes do merge com `gh workflow run setuptest.yml --ref <branch> -f tag=vX.Y.Z`.

## 7. Atualize o que a release mudou

Depois da release, o repositório precisa descrever o estado publicado:

- USAGE e SECURITY: o que a versão comprovou deixa de ser descrito como implementado e ainda não provado; o checklist do mantenedor mostra as versões atuais de cada canal e as credenciais que deixaram de ser usadas;
- ARC: a entrada da versão no resumo histórico;
- a issue que acompanha a release: os runs, as provas e as pendências;
- uma lição nova vai, curta, para o item 11 do checklist do USAGE.

Credenciais que nenhum workflow usa mais devem ser revogadas no serviço e removidas do repositório; essa ação é do mantenedor.

Com esse contexto, crie a sua lista de tarefas na ferramenta de tarefas do agente (TodoWrite, TaskCreate, update_plan ou equivalente) a partir dos épicos abaixo, na ordem de execução. Se surgir algo inesperado que crie um novo item, acrescente-o à lista.

## Agora, vamos à execução: crie a lista de tarefas e execute-a

- [ ] 1 Conferir a `main` verde, decidir o número da versão e conferir tokens, escopos, publishers e environments

- [ ] 2 Revisar descrições, metadados, README e nomes publicados de cada crate e pacote, como cada registry vai mostrá-los

- [ ] 3 Preparar o PR com `script/releasepr.sh`, escrever o resumo da versão, atualizar o `fuzz/Cargo.lock` e aguardar o merge do mantenedor

- [ ] 4 Criar, verificar e enviar a tag assinada e acompanhar o run da release, conferindo o commit de cada PR e run

- [ ] 5 Provar cada canal pelo serviço, com attestations, proveniência, `runreport` e `setuptest`, registrando run e artefatos

- [ ] 6 Recuperar falhas sem mover a tag depois de uma publicação irreversível, provando cada correção antes do merge

- [ ] 7 Atualizar USAGE, SECURITY, ARC e a issue da release para o estado publicado e apresentar as ações que dependem do mantenedor

/goal : A versão está publicada e provada em todos os canais, com run, serviço e artefatos identificados; os metadados e as descrições publicados estão corretos; nenhuma tag foi movida depois de uma publicação irreversível; o repositório descreve o estado publicado; e as ações de conta do mantenedor estão nomeadas com o motivo.
