---
name: afterwork
description: revisão final do trabalho, das referências afetadas e das evidências de validação.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-04
domain: workflow
---

# Revisão final

Ao concluir uma tarefa, preparar um PR ou encerrar a sessão, confira se o trabalho atende ao pedido e se o repositório descreve o comportamento entregue. Use o [specs.yaml](specs.yaml) para localizar arquivos que podem precisar de revisão e o [AGENTS.md](../../AGENTS.md) para as orientações de trabalho e validação.

## 1. Confira o que foi feito

Compare o pedido com os commits, o diff contra a base adequada e as alterações ainda locais, incluindo arquivos novos. Separe as mudanças desta tarefa das que já estavam no checkout. Em PRs empilhados, considere a base e as dependências reais; depois de um merge, confira também o conteúdo que chegou à `main`.

Identifique os comportamentos alterados: comandos, interfaces, dados, configuração, instalação, publicação ou procedimentos de desenvolvimento. Essa lista define o alcance da revisão.

## 2. Siga as referências afetadas

No `specs.yaml`, cada entrada informa um caminho (`path`), quando revisá-lo (`when`) e, quando necessário, um cuidado específico (`rule`). Leia as entradas relacionadas à mudança e confira os arquivos. Atualize apenas informações incorretas, incompletas ou desatualizadas; um arquivo revisado pode continuar igual.

O catálogo é um ponto de partida. Siga imports, chamadas, links, exemplos e geradores com `rg` ou `git grep`, incluindo os arquivos novos da tarefa. Examine os diretórios relevantes e repita essa revisão quando uma correção afetar outra referência, até resolver as inconsistências relacionadas ao trabalho.

Ao criar, remover ou renomear algo, ajuste os consumidores e os links. Atualize o catálogo quando mudar um ponto de manutenção que ele precisa representar; o inventário detalhado pertence ao ARC. Confira também os symlinks e os arquivos gerados, usando suas fontes e comandos de geração.

## 3. Atualize a informação no lugar certo

| Informação afetada | Destino |
| --- | --- |
| Uso, instalação, modelos e operação de releases | `docs/USAGE.md` |
| Setup de desenvolvimento, testes e contribuição | `docs/CONTRIBUTING.md` |
| Arquitetura, contratos, fluxos e inventário | `docs/arc/ARC.toml` |
| Garantias de segurança e proveniência | `docs/SECURITY.md` |
| Apresentação pública e demonstrações | `README.md`, `llms.txt` e exemplos pertinentes |
| Mudança perceptível, motivo e evidência histórica | `docs/CHANGELOG` e corpo do PR |
| Orientação recorrente para agentes | `AGENTS.md`; o procedimento de revisão fica neste AFTERWORK |

Escreva sobre o estado atual, mantendo histórico apenas onde ajuda a compreender decisões ou migrações. Registre uma lição recorrente de forma curta no documento de uso correspondente. Evite copiar o mesmo procedimento para vários lugares.

## 4. Valide e revise o diff final

Execute os checks pertinentes descritos no AGENTS e exigidos pelo projeto. Para scripts de release, valide seus contratos, falhas e integrações; para mudanças no runtime, avalie o gate completo e a medição. Alterações editoriais não precisam reconstruir benchmarks.

Distinga testes aprovados, falhos e pulados, com seus pré-requisitos e o commit testado. Reaproveite evidência quando a mudança posterior não afetar o que foi medido e explique essa relação. Provas de publicação precisam identificar o serviço, a execução e os artefatos usados; um teste no TestPyPI não comprova a publicação de produção.

Leia o resultado como revisor: confira comandos, links, configuração, exemplos e texto. Remova resíduos criados pela tarefa, como scripts temporários e imports órfãos, preservando arquivos locais ou mudanças de outra pessoa. Avalie nomes e responsabilidades sem introduzir refatorações alheias ao objetivo. A contagem de testes pode mudar; a cobertura necessária deve permanecer.

## 5. Entregue e encerre

Informe o que foi entregue, por que mudou, como foi validado e qualquer limitação relevante. Para um PR, identifique o commit final e diga claramente se está pronto para merge. Se ainda houver uma verificação necessária em andamento, nomeie-a.

Depois de um merge, confira a árvore entregue e atualize as referências ou evidências que dependiam da base. Faça a limpeza pertinente dentro das autorizações já dadas, sem descartar trabalho local.

Resolva as pendências do escopo antes de declarar conclusão. Uma prova reservada à próxima release normal pode ficar registrada como tal, sem manter a implementação aberta indefinidamente. Se houver uma ação indispensável na conta do mantenedor, apresente somente essa ação e seu motivo.
