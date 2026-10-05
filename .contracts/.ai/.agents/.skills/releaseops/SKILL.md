---
name: releaseops
description: preparar, publicar e conferir uma release do urna em todos os canais (GitHub, crates.io, PyPI, npm e Homebrew) e depois atualizar os benchmarks, os datasets e os repositórios ligados a ele; use quando pedirem release, publicação, versão nova ou envio aos registries.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-05
domain: workflow
---

# Release do urna e atualização dos repositórios ligados

Este procedimento leva uma versão da `main` até cada canal de distribuição e, depois que ela sai, atualiza o que depende dela: o próprio repositório, os benchmarks, os datasets dos corpora e os outros repositórios que usam o urna.

O detalhe de cada canal (credenciais, comandos e recuperação) está no checklist do mantenedor, no [USAGE.md](../../../../../docs/USAGE.md). As verificações por tipo de mudança estão no [AGENTS.md](../../AGENTS.md).

Uma versão publicada no crates.io ou no PyPI não pode ser substituída. Confira tudo antes de cada passo sem volta.

Antes de começar, crie a lista de tarefas conforme o fim deste arquivo.

## 1. Confira se a `main` está pronta

O último commit da `main` precisa estar verde no `gatecheck` e no `rehearsal`, e não pode haver PR de release aberto.

O número da versão segue a política 0.x do CHANGELOG: patch para mudanças compatíveis, minor quando algo quebra. Na dúvida, rode o `cargo-semver-checks` no `urna-format`.

Confira também as contas:

- os tokens do npm e do tap do Homebrew estão no prazo (as datas ficam no checklist do USAGE);
- quem envia a tag `v*` é admin do repositório ou da organização, porque o ruleset `release-tags` recusa as outras contas;
- se algum crate vai ser publicado pela primeira vez, inclusive um crate renomeado, o `CARGO_REGISTRY_TOKEN` precisa do escopo `publish-new`;
- o publisher do PyPI aponta para o workflow e o environment atuais, e o environment `pypi` aceita deploy das tags `v*`.

## 2. Revise o que cada registry vai mostrar

Cada canal publica uma página com os metadados do pacote, e essa página só muda na versão seguinte. Revise antes da tag:

- os campos `description`, `keywords`, `categories`, `readme`, `homepage` e `documentation` de cada crate em `crates/*/Cargo.toml`, e os equivalentes em `packs/pyproject.toml`;
- o README, que aparece no crates.io, no PyPI e no npm. Links e imagens usam URL absoluta para a `main`, então um arquivo movido depois da release quebra a página publicada até a próxima versão;
- as imagens de `assets/image/`: se a versão mudou o que o explorer de terminal ou uma interface gráfica mostra, gere imagens novas antes da tag;
- os nomes publicados: o binário e o CLI se chamam `urna`, e os crates levam o prefixo `urna-`. Renomear uma pasta não muda o pacote.

Um crate renomeado é publicado com o nome novo, e o nome antigo fica parado na última versão. Não apague o nome antigo: versões antigas de outros pacotes ainda dependem dele, e um nome liberado pode ser registrado por outra pessoa.

## 3. Prepare o PR da release

Abra primeiro a issue da release, com o label `release`. Depois rode o script de preparação:

```sh
script/releasepr.sh X.Y.Z --issue <N>    # acrescente --epic <E> se a release fizer parte de uma épica
```

O script cria uma worktree e atualiza a versão do workspace, as pins, o lockfile, a seção datada do CHANGELOG e os campos versionados do `CITATION.cff`. Em seguida roda o preflight e abre o PR assinado.

Complete o PR com o parágrafo de resumo da versão, como nas releases anteriores, e com o `fuzz/Cargo.lock`, que o script não atualiza:

```sh
cargo update -p urna-format -p urna-engine --manifest-path fuzz/Cargo.toml
```

O mantenedor revisa e faz o merge. O merge é a aprovação do número da versão.

## 4. Crie a tag e acompanhe a release

Depois do merge, crie a tag anotada e assinada no commit da release, confira a assinatura e envie:

```sh
git tag -s vX.Y.Z -m vX.Y.Z <commit>
git -c gpg.ssh.allowedSignersFile=.github/trustkeys verify-tag vX.Y.Z
git push origin vX.Y.Z
```

Acompanhe o run `Release` até o fim com `gh run watch`. Os jobs rodam nesta ordem: verificação da tag, build dos binários e das wheels, job global, criação da release no GitHub e publicação em cada canal.

O GitHub pode demorar a registrar um push. Antes de confiar no estado de um PR ou de um run, confira se ele aponta para o commit que você enviou.

## 5. Prove cada canal

CI verde não prova publicação. Confira a versão exata em cada serviço:

| Canal | O que conferir |
| --- | --- |
| GitHub | a release com os binários de cada alvo, as wheels, os `.sha256`, o payload, o SBOM e o `sha256.sum`; `gh attestation verify <arquivo> --repo hoffresearch/urna` num binário e numa wheel |
| crates.io | cada crate na versão nova, pela API |
| PyPI | os arquivos da versão e a proveniência PEP 740 (o endpoint de integridade responde 200) |
| npm e Homebrew | a versão servida pelo registry e pela fórmula do tap |

O `runreport` resume o run e cada canal, e o `setuptest` instala o produto por todos eles. Para cada prova, registre o run, o serviço e os artefatos. Um teste no TestPyPI não prova a publicação em produção.

## 6. Recupere falhas sem mover a tag

A tag só pode ser movida enquanto nada irreversível foi publicado, e só por um admin. Depois disso, cada canal se resolve separadamente, em geral com a próxima versão de patch.

Um job de publicação que falhou roda de novo com `gh run rerun <id> --failed`. O upload do PyPI ignora os arquivos que já existem com o mesmo sha256.

Uma falha do `setuptest` causada pelo próprio teste se corrige num branch, e a correção é provada antes do merge:

```sh
gh workflow run setuptest.yml --ref <branch> -f tag=vX.Y.Z
```

## 7. Atualize o urna para a versão publicada

Depois da release, o repositório passa a descrever o que foi publicado:

- no USAGE e no SECURITY, o que a versão provou deixa de aparecer como implementado e ainda não provado. O checklist do mantenedor mostra a versão atual de cada canal e as credenciais que deixaram de ser usadas;
- no ARC, a versão ganha a sua entrada no resumo histórico;
- a issue da release registra os runs, as provas e o que ficou pendente;
- uma lição nova entra, curta, no item 11 do checklist do USAGE.

Credenciais que nenhum workflow usa mais devem ser revogadas no serviço e removidas do repositório. Essa ação é do mantenedor.

## 8. Atualize os benchmarks, os datasets e os repositórios ligados

Os benchmarks públicos, os datasets dos corpora no Hugging Face e os outros repositórios que usam o urna citam a versão, os comandos e os links dele. Depois da release, rode a [benchsync](../benchsync/SKILL.md) para que passem a usar e a descrever a versão publicada.

Com esse contexto, crie a lista de tarefas na ferramenta do agente (TodoWrite, TaskCreate, update_plan ou equivalente), com um item por etapa abaixo, na ordem. Se aparecer algo inesperado, acrescente um item.

## Agora, vamos à execução: crie a lista de tarefas e execute-a

- [ ] 1 Conferir se a `main` está verde, decidir o número da versão e conferir tokens, escopos, publishers e environments

- [ ] 2 Revisar como cada registry vai mostrar os crates e pacotes: descrições, metadados, README, imagens e nomes

- [ ] 3 Abrir a issue da release, preparar o PR com o `releasepr.sh`, escrever o resumo, atualizar o `fuzz/Cargo.lock` e esperar o merge do mantenedor

- [ ] 4 Criar, verificar e enviar a tag assinada e acompanhar o run da release até o fim

- [ ] 5 Provar a versão em cada canal, com attestations, proveniência, `runreport` e `setuptest`, registrando run e artefatos

- [ ] 6 Corrigir falhas sem mover a tag depois de uma publicação irreversível, provando cada correção antes do merge

- [ ] 7 Atualizar USAGE, SECURITY, ARC e a issue da release para a versão publicada

- [ ] 8 Atualizar os benchmarks, os datasets e os repositórios ligados com a benchsync e apresentar as ações que dependem do mantenedor

/goal : A versão está publicada e provada em todos os canais, com run, serviço e artefatos identificados. Os metadados publicados estão corretos, nenhuma tag foi movida depois de uma publicação irreversível, o urna e os repositórios ligados descrevem a versão publicada, e as ações de conta do mantenedor estão nomeadas, cada uma com o motivo.
