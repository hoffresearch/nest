---
name: afterwork
description: revisão final do trabalho, das referências afetadas e das evidências de validação.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-04
domain: workflow
---

# Auditoria pós-sessão de trabalho

Use esta revisão ao concluir uma tarefa para confirmar que o trabalho atende ao pedido e que o repositório descreve o comportamento entregue. Use o [specs.yaml](specs.yaml) para localizar arquivos que podem precisar de revisão e o [AGENTS.md](../../AGENTS.md) para as orientações de trabalho e validação. Antes de executar qualquer etapa, crie a lista de tarefas na ferramenta de tarefas do agente, conforme o fim deste arquivo.

## 1. Audite o histórico da tarefa

Reúna a transcrição completa dos pedidos, o histórico guardado pelo agente (por exemplo, a pasta `.claude/` no disco do usuário), a memória e os arquivos criados ou alterados. Esse levantamento é a base das etapas seguintes.

## 2. Confira o que foi feito

Compare o pedido com os commits, o diff contra a base adequada e as alterações ainda locais, incluindo arquivos novos. Separe as mudanças desta tarefa das que já estavam no checkout. Em PRs empilhados, considere a base e as dependências reais; depois de um merge, confira também o conteúdo que chegou à `main`.

Identifique os comportamentos alterados: comandos, interfaces, dados, configuração, instalação, publicação ou procedimentos de desenvolvimento. Essa lista define o alcance da revisão.

## 3. Siga as referências afetadas

No `specs.yaml`, cada entrada informa um caminho (`path`), quando revisá-lo (`when`) e, quando necessário, um cuidado específico (`rule`). Leia as entradas relacionadas à mudança e confira os arquivos. Atualize apenas informações incorretas, incompletas ou desatualizadas; um arquivo revisado pode continuar igual.

O catálogo é um ponto de partida. Siga imports, chamadas, links, exemplos e geradores com `rg` ou `git grep`, incluindo os arquivos novos da tarefa. Examine os diretórios relevantes e repita essa revisão quando uma correção afetar outra referência, até resolver as inconsistências relacionadas ao trabalho. Quando o alcance for o repositório inteiro, como depois de uma série de renomeações ou de uma release, use a [factcheck](../factcheck/SKILL.md).

Ao criar, remover ou renomear algo, ajuste os consumidores e os links. Atualize o catálogo quando mudar um ponto de manutenção que ele precisa representar; o inventário detalhado pertence ao ARC. Confira também os symlinks e os arquivos gerados, usando suas fontes e comandos de geração.

## 4. Atualize a informação no lugar certo

| Informação afetada | Destino |
| --- | --- |
| Uso, instalação, modelos e operação de releases | `docs/USAGE.md` |
| Setup de desenvolvimento, testes e contribuição | `docs/CONTRIBUTING.md` |
| Arquitetura, contratos, fluxos e inventário | `docs/ARC.toml` |
| Garantias de segurança e proveniência | `docs/SECURITY.md` |
| Apresentação pública e demonstrações | `README.md` e exemplos pertinentes |
| Mudança perceptível, motivo e evidência histórica | `docs/CHANGELOG` e corpo do PR |
| Decisões de arquitetura e lições de referência | `docs/ADR/` |
| Orientação recorrente para agentes | `.contracts/.ai/.agents/AGENTS.md`; o procedimento de revisão fica nesta skill |

Escreva sobre o estado atual, mantendo histórico apenas onde ajuda a compreender decisões ou migrações. Registre em `docs/ADR/` as decisões de arquitetura e as lições que precisam continuar como referência. Uma lição operacional fica, de forma curta, no documento de uso correspondente. Cada ADR parte do `docs/ADR/TEMPLATE.md` e fica em `docs/ADR/<categoria>/NNNN-titulo-curto.md`, com numeração única em todo o `docs/ADR/` e todos os campos do cabeçalho YAML do modelo preenchidos. Um ADR aceito não é apagado nem tem o corpo reescrito: o novo o cita em `supersedes`, e no antigo só o cabeçalho muda (`status: superseded`, `superseded-by` e `last-updated`). As categorias e o escopo de cada uma estão no `docs/ADR/README.md`. Evite copiar o mesmo procedimento para vários lugares.

## 5. Confira a organização dos arquivos

Revise os arquivos criados ou alterados e os consumidores afetados. Procure código sem uso, imports órfãos, scripts temporários, documentação duplicada e arquivos deixados fora das pastas correspondentes à sua função.

Antes de remover algo, confira também usos indiretos em comandos, workflows, configuração, empacotamento e exemplos. Preserve alterações de outras pessoas e arquivos locais cuja finalidade não esteja esclarecida.

Confira os caminhos e nomes conforme a organização do projeto e o inventário do ARC. Ao mover ou renomear um arquivo, atualize imports, links, scripts, workflows e referências de distribuição. Ajuste o `specs.yaml` quando o caminho estiver representado no catálogo.

Enxugue comentários que repetem o código, narram o andamento da tarefa ou descrevem um comportamento antigo. Preserve explicações sobre decisões, limitações, compatibilidade e invariantes de segurança.

Confira o limite de 639 linhas nos arquivos de código abrangidos pela mudança, seguindo as exceções do AGENTS. Quando necessário, modularize por responsabilidade e valide os consumidores. Registre problemas fora do escopo sem transformar o encerramento em uma refatoração geral.

## 6. Valide e revise o diff final

Execute os checks pertinentes descritos no AGENTS e exigidos pelo projeto. Para scripts de release, valide seus contratos, falhas e integrações; para mudanças no runtime, avalie o gate completo e a medição. Alterações editoriais não precisam reconstruir benchmarks.

Distinga testes aprovados, falhos e pulados, com seus pré-requisitos e o commit testado. Reaproveite evidência quando a mudança posterior não afetar o que foi medido e explique essa relação. Provas de publicação precisam identificar o serviço, a execução e os artefatos usados. Importante: um teste no TestPyPI não comprova a publicação de produção.

Leia o resultado como revisor: confira comandos, links, configuração, exemplos e texto. A contagem de testes pode mudar; a cobertura necessária deve permanecer.

## 7. Confira a entrega e resolva as pendências

Confira a árvore entregue e atualize as referências ou evidências que dependiam da base. Faça a limpeza pertinente dentro das autorizações já dadas, sem descartar trabalho local.

Resolva as pendências do escopo antes de declarar conclusão. Uma prova reservada à próxima release normal pode ficar registrada como tal, sem manter a implementação aberta indefinidamente.

## 8. Crie e revise o PR

Crie o PR no padrão do AGENTS: commits assinados, título e corpo em inglês simples e justificativa das verificações usadas. Depois, revise o PR online: confira se há conflitos com a base e o estado dos checks obrigatórios, incluindo o `rehearsal`. Havendo conflito, resolva-o e confira o diff contra a nova base.

## 9. Apresente o relatório final

Informe o que foi entregue, por que mudou, como foi validado e qualquer limitação relevante. Para um PR, identifique o commit final e diga claramente se está pronto para merge. Se ainda houver uma verificação necessária em andamento, nomeie-a.

Se houver uma ação indispensável na conta do desenvolvedor atual, apresente somente essa ação e seu motivo.


Com esse contexto, crie a sua lista de tarefas na ferramenta de tarefas do agente (TodoWrite, TaskCreate, update_plan ou equivalente) a partir dos épicos abaixo, na ordem de execução. Se surgir algo inesperado que crie um novo item, acrescente-o à lista.

Importante: os épicos abaixo resumem as seções acima, e tudo o que elas pedem deve aparecer na lista. Mantenha a execução organizada, para que o tech lead possa auditar item por item no histórico da transcrição e comparar com o pull request.

## Agora, vamos à execução: crie a lista de tarefas e execute-a

- [ ] 1 Auditar a transcrição dos pedidos, o histórico do agente, a memória e os arquivos alterados ou criados

- [ ] 2 Comparar o pedido com os commits, o diff contra a base, as alterações locais, os PRs empilhados em branches locais e o conteúdo que chegou à `main` remota, separar o que é desta tarefa e definir o alcance pelos comportamentos alterados

- [ ] 3 Seguir o `specs.yaml` e rastrear imports, chamadas, links, exemplos, geradores e symlinks, ajustando os consumidores e o catálogo

- [ ] 4 Atualizar a informação nos documentos responsáveis e registrar em `docs/ADR/` só as decisões de arquitetura e as lições que precisam continuar como referência, sem abrir um ADR para cada lição operacional

- [ ] 5 Organizar os arquivos: remover resíduos depois de conferir os usos indiretos, conferir os nomes pelo `ARC.toml`, enxugar os comentários, respeitar o limite de 639 linhas, modularizar por responsabilidade arquivos de código que excederem esse limite, seguindo as exceções listadas no `AGENTS.md`

- [ ] 6 Executar os checks e testes pertinentes, corrigir a causa dos testes reprovados, registrar no PR os testes aprovados, pulados e dispensados com o commit testado, conferir as provas de publicação e revisar o diff final

- [ ] 7 Conferir a árvore do diff entregue, realizar a higiene dos arquivos soltos e/ou que não são mais necessários e resolver as pendências do escopo, incluindo na lista e executando os itens extras que surgirem

- [ ] 8 Criar o PR no padrão do AGENTS e revisá-lo online, corrigindo os conflitos

- [ ] 9 Apresentar o relatório final de execução com todos os itens confirmados

/goal : O pedido original foi executado e concluído com sucesso, sem pendências sob controle do agente; dependências externas devem ser identificadas com a ação necessária. Ao término da lista de tarefas, o resultado final deverá contemplar os pedidos desta skill auditados; O repositório deve refletir o comportamento entregue; Referências, links e arquivos gerados consistentes, higienizados, sem imports quebrados, pastas soltas, comentários verbosos desnecessários, tipagem fora do padrão; Documentação atualizada e cada informação no documento certo, sem duplicação; O Código organizado, sem resíduos e dentro do limite de linhas; Checks pertinentes aprovados, dispensas justificadas e evidência atrelada ao commit; Itens do `specs.yaml` revisados e todos arquivos necessários auditados, sem dead code e/ou duplicação de informação; Entrega do PR sem conflitos e pronto para merge e relatório final apresentado no chat ou no corpo do PR com todos itens da lista de tarefas concluídos.
