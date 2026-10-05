---
name: run-urna
description: compilar, rodar, testar e dirigir o CLI do urna e o explorer de terminal (urna tui), com capturas de tela em texto; use quando pedirem para rodar, compilar, fazer smoke test, mostrar a tela ou conferir uma mudança no urna de verdade.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-05
domain: workflow
---

# Rodar e dirigir o urna

O urna é um binário (`urna`, pacote `urna` em `crates/clitui`) com dois modos: verbos de linha de comando e o explorer de terminal `urna tui`. O caminho do agente é o `drive.sh` desta pasta. Ele compila, monta o corpus do quickstart, roda ask, retrieve, cite, validate e doctor com verificação e dirige o explorer dentro do tmux, salvando cada tela como texto. Os comandos abaixo partem da raiz do repositório, em macOS ou Linux.

```sh
D=.contracts/.ai/.agents/.skills/run-urna/drive.sh
sh $D            # lista os comandos e as variáveis de ambiente
```

## Pré-requisitos

Rust na versão que o `crates/clitui/Cargo.toml` pede (`rust-version`), `tmux` para o explorer e um Python 3.12 ou mais novo com `numpy` e `tokenizers` para o embedder offline. A tabela potion precisa ser o arquivo real, não o ponteiro do LFS:

```sh
sh script/getpotion.sh
```

O `corpus` chama o forge, que importa a extensão `_urna`. Compile-a uma vez por checkout e de novo depois de mexer em `crates/format`, `crates/engine` ou `crates/bridge`:

```sh
sh $D ext        # compila o urna-bridge para o python de URNA_PYTHON (padrão python3) e copia para python/_urna.so
```

## Agente: build, corpus e smoke

```sh
sh $D build      # cargo build --release -p urna e a versão do binário
sh $D corpus     # o corpus do quickstart; `sh $D corpus outro/spec.toml` monta outro
sh $D smoke      # ask, retrieve, cite, validate e doctor; para no primeiro erro
```

O `corpus` imprime o caminho de cada arquivo que o build escreveu. O `smoke` imprime uma linha `ok` por verbo: `ask` com citação `urna://`, `retrieve` com o número de hits pedido, `cite` resolvendo o `citation_id` do primeiro hit, `validate` e `doctor`.

## Agente: o explorer (`urna tui`)

O explorer roda numa sessão tmux. Cada captura vai para `$SHOTS/<nome>.txt`, e o `shot` imprime o caminho. Em vez de esperar um tempo fixo, o `wait` espera um texto aparecer na tela, com prazo:

```sh
sh $D tui                          # abre o corpus padrão e espera a primeira tela
sh $D shot corpus                  # manifest e tabela de seções
sh $D key a                        # vai para a aba ask
sh $D type "how do citations work"
sh $D key Enter
sh $D wait "hits for"              # a resposta chegou
sh $D shot ask                     # hits com score e o texto citado
sh $D key Tab
sh $D wait "every check passes"    # os checks do doctor terminaram sem falha
sh $D shot health
sh $D quit
```

Se um `wait` estoura o prazo, ele termina com erro e indica o `shot` para ver o que está na tela. Um check do doctor que falha, por exemplo, nunca mostra "every check passes".

`sh $D tui caminho/outro.urna` abre outro arquivo. As variáveis que o `sh $D` lista trocam o binário, o corpus, o Python, a pasta de build, a pasta das capturas, o nome e o tamanho da sessão.

## Humano

`urna tui <arquivo>` num terminal de verdade, com o binário que o `build` gerou; `tab` troca de aba e `ctrl+q` sai.

## Testes

```sh
cargo test --workspace --release
sh $D ext && python3 tests/test_pythonapi.py
```

As suítes Python carregam a extensão, então rode o `ext` antes delas sempre que a extensão puder estar velha.

## Pegadinhas

- O Python dos verbos segue esta ordem: `URNA_PYTHON`, depois o venv do `urna setup`, depois o `.venv` mais próximo, depois `python3`. A linha `[urna] embedder interpreter: ...` vai para o stderr e diz qual foi. Ela pode ser diferente do Python que compilou a extensão; como a extensão usa a ABI estável do Python 3.12, qualquer interpretador 3.12 ou mais novo a carrega.
- O `doctor` acha o embedder pelo layout do repo a partir do caminho do binário, mesmo rodando fora do repo e com um `URNA_DATA_DIR` vazio. Um binário copiado para outro lugar depende do payload instalado.
- Ao abrir, um aviso "opening <arquivo>" cobre a tabela de seções por alguns segundos. O `tui` do driver só devolve depois que o aviso some.
- Na aba ask, `q` vira texto da pergunta. Para sair, use `ctrl+q` (`sh $D key C-q`), que funciona em todas as abas.
- A captura é texto, sem cor: `tmux capture-pane -p`. O conteúdo é o mesmo da tela.
