---
name: factcheck
description: revisão do repositório contra o estado real, com revisores em paralelo por grupo do specs.yaml; use depois de uma série de mudanças, de uma release ou quando pedirem para revisar tudo.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-05
domain: workflow
---

# Revisão do repositório contra o estado real

Use esta revisão quando uma série de mudanças, uma release ou um pedido de "revisar tudo" deixar o repositório inteiro como alcance. Uma varredura por texto encontra os nomes que você já sabe procurar; esta revisão encontra o resto: nomes antigos sem caminho, contagens que mudaram, comentários que contam o passado e regras de configuração com efeito escondido. O [specs.yaml](../afterwork/specs.yaml) divide o trabalho e o [AGENTS.md](../../AGENTS.md) define onde cada informação mora. Antes de executar qualquer etapa, crie a lista de tarefas na ferramenta de tarefas do agente, conforme o fim deste arquivo.

## 1. Escreva a ficha dos fatos

Reúna em um arquivo temporário, fora do repositório, o estado que os revisores vão usar como referência. A ficha é curta e verificável:

- os nomes atuais de pastas, pacotes, workflows, scripts, testes e artefatos de release;
- os nomes antigos que não podem mais aparecer, e as exceções em que eles continuam válidos (seções datadas do CHANGELOG, o resumo histórico do ARC, corpo de ADR aceito, notas de migração);
- o estado publicado: a última versão de cada canal e o que ela comprovou;
- a regra de destino de cada informação, conforme o AGENTS.

Confira cada linha da ficha no código ou na API do serviço. Uma ficha errada multiplica o erro pelos revisores.

## 2. Divida o catálogo em grupos

Agrupe as entradas do `specs.yaml` por afinidade, de três a cinco grupos, para que cada revisor leia arquivos que se explicam entre si. Uma divisão que funciona: docs públicos e instruções dos agentes; ferramental, crates e workflows; scripts, testes, Python e demos; os caminhos de dados e o setup do CLI. Inclua no grupo os arquivos próximos que o catálogo não lista quando eles fazem parte do mesmo comportamento.

## 3. Lance os revisores em paralelo

Cada revisor recebe a ficha, a lista do seu grupo e as mesmas regras:

- só lê: não edita arquivo nem muda o estado do git;
- confere cada suspeita no código antes de relatar;
- procura afirmações que contradizem a ficha ou o código, caminhos e comandos errados, nomes antigos fora das exceções, contagens desatualizadas, informação duplicada no documento errado, comentários que narram uma mudança antiga e configuração com efeito maior que o pretendido;
- não relata histórico intencional;
- devolve uma lista curta: `arquivo:linha`, o problema e a correção exata, ou "nenhum" para o arquivo limpo.

Lance todos os revisores na mesma mensagem e trabalhe em outra coisa enquanto rodam.

## 4. Consolide e corrija

Leia cada achado e confirme o trecho antes de aplicar. Corrija no documento responsável, sem copiar a mesma informação para outro lugar. Separe três casos:

- correção de texto, caminho, comentário ou contagem: aplique;
- mudança de comportamento, como uma mensagem que deveria listar mais caminhos: registre como fora do escopo;
- texto protegido, como o corpo de um ADR aceito ou uma seção datada do CHANGELOG: deixe como está e registre o motivo.

Quando uma correção mudar outra referência, siga essa referência até não restar inconsistência.

## 5. Valide e entregue

Rode os checks que as correções tocam: formatação e Clippy para comentários em Rust, ruff e as suítes afetadas para Python, `sh -n` para shell, parse de TOML e YAML, `dist generate --check` e o `--check` do rehearsal quando um workflow mudar, e o preflight quando o CHANGELOG ou um manifest mudar. Registre no PR o que cada revisor achou, o que foi aplicado, o que ficou fora e por quê, e o commit testado.

Com esse contexto, crie a sua lista de tarefas na ferramenta de tarefas do agente (TodoWrite, TaskCreate, update_plan ou equivalente) a partir dos épicos abaixo, na ordem de execução. Se surgir algo inesperado que crie um novo item, acrescente-o à lista.

## Agora, vamos à execução: crie a lista de tarefas e execute-a

- [ ] 1 Escrever a ficha dos fatos, conferindo cada linha no código ou na API do serviço

- [ ] 2 Dividir as entradas do `specs.yaml` em grupos afins, incluindo os arquivos próximos do mesmo comportamento

- [ ] 3 Lançar um revisor por grupo, em paralelo e só de leitura, com a ficha e as regras de relato

- [ ] 4 Consolidar os achados, confirmar cada trecho, aplicar as correções no documento responsável e registrar o que ficou fora do escopo ou protegido

- [ ] 5 Rodar os checks que as correções tocam e registrar no PR os achados, as correções, as dispensas e o commit testado

/goal : Todo arquivo do `specs.yaml` foi lido contra fatos conferidos; nenhum nome antigo, contagem errada, caminho quebrado ou comentário narrando o passado ficou fora das exceções registradas; cada correção está no documento responsável, validada e atrelada ao commit; e o relatório separa o que foi aplicado, o que ficou fora do escopo e o que é protegido.
