---
name: renameops
description: renomear ou mover pastas, crates, pacotes, scripts, workflows e testes sem quebrar referências; use quando pedirem rename, mover pasta, trocar nome de crate, arquivo ou workflow.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-06
domain: workflow
---

# Renomear sem quebrar

Use este procedimento para trocar o nome ou o lugar de uma pasta, um crate, um pacote, um script, um workflow ou um teste. Um nome aparece em mais formas do que uma busca pelo caminho encontra, e cada forma esquecida quebra um teste, um workflow ou uma página publicada. A convenção de nomes está na seção Naming do [CONTRIBUTING.md](../../../../../docs/CONTRIBUTING.md) ([ADR-0002](../../../../../docs/ADR/STRUCTURE/0002-short-folder-names-published-package-names.md)); as referências a revisar, no [specs.yaml](../afterwork/specs.yaml). Antes de executar qualquer etapa, crie a lista de tarefas na ferramenta de tarefas do agente, conforme o fim deste arquivo.

## 1. Defina o que muda

Separe o nome da pasta do nome publicado. A pasta pode ter qualquer nome; o pacote publicado (crates.io, PyPI, npm) é o que os usuários instalam, e trocá-lo cria um pacote novo e deixa o antigo parado na última versão. Quando o pedido não disser, pergunte de forma explícita, com as duas consequências, e não troque o nome publicado sem resposta. Confira também se o novo nome segue o comprimento da convenção e se nenhuma ferramenta fixa o nome atual, como o `release.yml` que o cargo-dist escreve.

## 2. Mapeie todas as formas do nome

Liste cada referência antes de mexer, com `git grep` e `rg`:

- o caminho com `/` e com `\` (passos do Windows nos workflows);
- o caminho relativo entre irmãos, como `../format` dentro de outro crate;
- o nome sem caminho: rótulos de suíte, prefixos de log, mensagens, nomes de módulo em testes, nomes de job;
- o nome do pacote nos manifests, nos lockfiles, nos comandos `-p`, nos artefatos de release e nos índices dos registries;
- as URLs dos registries (crates.io, docs.rs, PyPI), que só mudam quando o nome publicado muda;
- os testes que supõem que a pasta tem o nome do pacote;
- os arquivos gerados, que mudam pelo gerador e nunca à mão.

Mantenha intocado o histórico intencional: seções datadas do CHANGELOG, o resumo histórico do ARC e o corpo de ADR aceito.

## 3. Uma mudança por PR, a partir da `main`

Abra uma sub-issue e um PR para cada item, sempre a partir da `main` atualizada e depois que o mantenedor fizer o merge do anterior. Não empilhe PRs.

## 4. Troque e confira

Mova com `git mv` e substitua com regras que respeitem as exceções: um caminho com `crates/` não pode reescrever `crates.io/crates/`. Depois de cada troca:

- rode `cargo fmt --all` e `sh script/ruffcheck.sh`: um nome mais curto muda a quebra de linha;
- regenere o que é gerado: `dist generate` e `python script/rehearsal.py generate`;
- repita a busca pelo nome antigo em todas as formas do passo 2;
- confira que nenhuma pasta antiga sobrou com arquivos ignorados, como um `target/`;
- rode uma checagem rápida do que a troca toca: `cargo check` ou `cargo metadata`, os geradores com `--check`, as suítes que citam o caminho e, num módulo Python, `pyright` e `lint-imports` (comandos na seção Naming do CONTRIBUTING).

## 5. Valide a série e registre

No fim da série:

- rode a bateria completa uma vez: `cargo fmt --all --check`, Clippy e `cargo test` nos workspaces afetados, a extensão Python recompilada com as suítes que a carregam, as suítes de release, o ruff e o `cargo deny`;
- depois do push, confira que o PR aponta para o commit enviado;
- acompanhe todos os checks, não só o obrigatório: o `rehearsal` passa mesmo com o `gatecheck` vermelho;
- registre a decisão de nomes em ADR quando ela for uma convenção nova, e a mudança perceptível no CHANGELOG, com o que os usuários veem na próxima release.

Com esse contexto, crie a sua lista de tarefas na ferramenta de tarefas do agente (TodoWrite, TaskCreate, update_plan ou equivalente) a partir das etapas abaixo, na ordem de execução. Se surgir algo inesperado que crie um novo item, acrescente-o à lista.

## Agora, vamos à execução: crie a lista de tarefas e execute-a

- [ ] 1 Definir se muda só a pasta ou também o nome publicado, com as consequências de cada um, e conferir a convenção de nomes

- [ ] 2 Mapear todas as formas do nome antigo, incluindo `\`, caminhos entre irmãos, nomes sem caminho, pacotes, URLs de registry, testes e arquivos gerados

- [ ] 3 Abrir uma sub-issue e um PR por item a partir da `main` atualizada, depois do merge do anterior pelo mantenedor

- [ ] 4 Trocar com regras que respeitem as exceções, formatar, regenerar, repetir a busca e rodar a checagem rápida em cada item

- [ ] 5 Rodar a bateria completa no fim da série, acompanhar todos os checks no commit enviado e registrar ADR e CHANGELOG quando couber

/goal : O nome antigo não aparece em nenhuma forma fora das exceções registradas; o nome publicado só mudou quando o pedido disse; cada item entrou num PR próprio a partir da `main`; os arquivos gerados vieram dos geradores; e a bateria completa e todos os checks passaram no commit entregue.
