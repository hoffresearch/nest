---
name: benchsync
description: revisar os benchmarks públicos e os outros repositórios externos que usam o urna, junto com os seus datasets, comparar com a versão atual e atualizá-los; use depois de uma release, de uma renomeação ou quando pedirem para sincronizar, revisar ou medir de novo um benchmark.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-05
domain: workflow
---

# Sincronização dos benchmarks e dos repositórios externos com o urna

Os benchmarks públicos e outros repositórios externos usam o urna. Cada um tem um repositório de código e, quando publica dados, também um dataset. Todos citam o urna, dependem dele e têm links para arquivos do repositório dele. Esta skill mantém tudo isso de acordo com a versão publicada.

O [benches.yaml](benches.yaml) é o catálogo desses repositórios. Ele informa onde cada um está, onde fixa a versão do urna, o que usa do urna e como validá-lo. Tudo o que muda com o tempo fica só no catálogo: quando entra um repositório novo ou o urna muda de estrutura, você atualiza o catálogo, e este texto continua valendo.

As regras de trabalho do [AGENTS.md](../../AGENTS.md) também valem nesses repositórios.

Só dê o trabalho por concluído quando cada problema encontrado tiver sido resolvido ou encaminhado, e quando nada do que você começou tiver ficado pela metade. Antes de começar, crie a lista de tarefas conforme o fim deste arquivo.

## 1. Veja o que mudou no urna

Compare a versão publicada com a versão que cada repositório usa, lendo o CHANGELOG entre as duas. Separe as mudanças em dois tipos:

| O que mudou no urna | O que fazer no repositório |
| --- | --- |
| algo que altera comportamento ou resultado | medir de novo, porque o resultado pode mudar |
| apenas nomes, caminhos ou empacotamento | corrigir textos, comandos e links |

Confira no código, com um diff entre as duas versões, o que o repositório realmente usa. O CHANGELOG ajuda a saber onde olhar, mas não garante que nada mudou.

## 2. Revise cada repositório

Veja primeiro se já existe um clone na máquina, nas pastas de projetos do usuário. Se existir, atualize com o remoto sem perder as alterações que já estiverem nele; se não existir, clone. Leia todos os arquivos versionados, e não só alguns: o catálogo mostra por onde começar, e a leitura completa encontra o resto.

Em cada arquivo, verifique:

- se o que ele diz sobre o urna (versão, nome, caminho, comando, comportamento) continua valendo na versão publicada;
- se os links e as imagens abrem. Teste cada endereço, sem deduzir pelo caminho;
- se o que ele afirma bate com o resto do repositório, com o dataset e com o que o urna diz dele;
- se algum resultado dele contradiz o que o urna ou outro repositório do catálogo afirma.

Faça a mesma verificação no sentido contrário: tudo o que o urna diz sobre o repositório precisa conferir com ele.

Verifique também o CI, a proteção da branch principal e a assinatura dos commits, comparando com o urna. Use o que a plataforma mostra, e não a configuração local.

## 3. Revise o dataset

A página do dataset precisa dizer o mesmo que o repositório, e os arquivos que existem nos dois lugares precisam ser idênticos, byte a byte. Verifique também a estrutura publicada, a licença, os metadados calculados pela plataforma e se os comandos de download apontam para uma versão fixa.

Qualquer mudança feita direto no dataset precisa constar no changelog do repositório.

## 4. Liste o que encontrou e decida o que fazer com cada item

Monte uma tabela com tudo o que encontrou, uma linha por problema: onde está, o tipo (quebrado, desatualizado, inconsistente ou melhoria possível) e o que será feito. Cada linha termina em uma destas saídas:

| Saída | Quando usar |
| --- | --- |
| PR | a correção cabe no repositório e nesta sincronização |
| issue no repositório | é uma melhoria ou uma correção maior, para depois |
| issue no urna | a correção depende de mudar o urna |
| ação do mantenedor | depende de conta, permissão, chave ou decisão do mantenedor |
| descartado | o problema não se confirmou; registre o motivo |

Nenhum problema fica de fora, nem os que você mencionou só de passagem. Essa tabela vai no relatório final.

Só meça de novo quando o urna mudou o que o repositório mede. Um número que não foi medido de novo continua indicando a versão que o mediu.

## 5. Faça as correções no padrão do urna

Em cada repositório, siga o fluxo de issues e PRs do AGENTS e prefira a solução mais simples que resolva. Em qualquer mudança:

- se o mesmo conteúdo existe em mais de um lugar, ou se outro arquivo depende dele (um checksum, um manifesto), atualize todos ao mesmo tempo. As mudanças em cada lugar fazem referência umas às outras e entram juntas;
- os repositórios e os datasets seguem a mesma organização. O conteúdo de cada um é diferente, mas documentos equivalentes ficam no mesmo lugar e com as mesmas seções, e uma correção feita num vale para os outros;
- nos links para o urna, use uma versão fixa e não a branch principal, para que o link continue funcionando depois da próxima reorganização. Em texto público, cite apenas o que já foi publicado;
- depois do push, confira na plataforma se o commit aparece como verificado: uma assinatura válida na sua máquina pode não ser reconhecida lá.

## 6. Valide cada repositório

Valide cada repositório contra a versão publicada, usando a validação indicada no catálogo. Se ela não existir ou não cobrir a mudança, use a equivalente que o repositório oferece e registre qual usou. Anote a versão do urna, o commit e os hashes.

Validações demoradas rodam em segundo plano, mas você acompanha até o fim. O PR fica aberto até o resultado sair, e o resultado vai para o PR e, se o repositório tiver um registro de validações, também para ele.

Se o urna ficou para trás em relação aos repositórios, atualize o que ele cita: números, links e afirmações que uma medição contradisse.

## 7. Conclua

Antes de encerrar, confira se:

- toda validação, todo check e todo PR que você começou terminou, com o resultado registrado;
- cada item tem o seu PR, e cada linha da tabela teve a saída prevista;
- as issues que agrupam o trabalho foram atualizadas como histórico, com cada item, o PR e a validação;
- os clones, worktrees e arquivos temporários que você criou foram apagados, sem deixar referência para eles.

Se algo depender de outra pessoa ou de uma espera fora do seu controle, registre a situação no PR e na issue e diga no relatório o que falta e quem precisa agir.

Termine com o relatório: para cada repositório, o que mudou e como foi validado; depois, a tabela do que foi encontrado e as ações do mantenedor, cada uma com o motivo.

Com esse contexto, crie a lista de tarefas na ferramenta do agente (TodoWrite, TaskCreate, update_plan ou equivalente), com um item por etapa abaixo, na ordem. Se aparecer algo inesperado, acrescente um item.

## Agora, vamos à execução: crie a lista de tarefas e execute-a

- [ ] 1 Ver a versão publicada e o que mudou no urna desde a versão usada em cada repositório, conferindo no código o que cada um usa

- [ ] 2 Revisar cada repositório por inteiro, nos dois sentidos (o que ele diz do urna e o que o urna diz dele), testando cada link

- [ ] 3 Revisar cada dataset contra o repositório: conteúdo, arquivos repetidos, licença, metadados e versão fixa nos downloads

- [ ] 4 Montar a tabela do que foi encontrado, com a saída de cada item, e decidir o que medir de novo

- [ ] 5 Corrigir cada repositório e dataset no padrão do urna, atualizando junto o conteúdo repetido, com links para versões fixas e commits verificados

- [ ] 6 Validar cada repositório contra a versão publicada, acompanhando cada validação até o fim, e atualizar o urna se ele ficou para trás

- [ ] 7 Concluir sem nada pela metade: validações e PRs terminados, issues atualizadas como histórico, limpeza feita e relatório com a tabela

/goal : Cada repositório e o seu dataset usam e descrevem a versão publicada do urna, sem link quebrado nem versão desatualizada, e seguem a mesma organização. Cada número informa a versão que o mediu, e o que o urna diz deles confere. Todo problema encontrado foi resolvido ou encaminhado, nenhuma validação ou PR ficou sem resultado, as issues contam o histórico, e as ações do mantenedor estão listadas, cada uma com o motivo.
