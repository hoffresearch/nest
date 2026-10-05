---
name: run-urna
description: run, build, start, smoke-test, drive and screenshot the urna CLI and its terminal explorer (urna tui); rodar, compilar, testar e capturar a tela do urna.
project: urna
audience: agentes de código e contribuidores humanos
status: active
last-updated: 2026-10-05
domain: workflow
---

# Rodar e dirigir o urna

O urna é um binário (`urna`, pacote `urna` em `crates/clitui`) com dois modos: verbos de linha de comando e o explorer de terminal `urna tui`. O caminho do agente é o `drive.sh` desta pasta: ele compila, monta o corpus do quickstart, roda o ciclo dos cinco verbos com verificação e dirige o explorer dentro do tmux, salvando cada tela como texto. Os caminhos abaixo partem da raiz do repositório.

## Pré-requisitos

Rust (o CLI pede 1.88), `tmux` e um Python com `numpy` e `tokenizers` para o embedder offline. A tabela potion precisa ser o arquivo real, não o ponteiro do LFS:

```sh
sh script/getpotion.sh
```

O `corpus` chama o forge, que importa a extensão `_urna`. Num checkout limpo, compile-a antes:

```sh
cargo build --release -p urna-bridge --features pyo3/extension-module
cp target/release/lib_urna.dylib python/_urna.so   # macOS
cp target/release/lib_urna.so   python/_urna.so    # linux
```

## Agente: build, corpus e smoke

```sh
D=.contracts/.ai/.agents/.skills/run-urna/drive.sh
sh $D build     # cargo build --release -p urna; imprime `urna 0.5.4`
sh $D corpus    # demos/quickstart/out/quickstart.urna (12 chunks, potion)
sh $D smoke     # ask, retrieve, cite, validate e doctor; para no primeiro erro
```

O `smoke` imprime uma linha `ok` por verbo: `ask` com citação `urna://`, `retrieve` com 2 hits, `cite` resolvendo o `citation_id` do primeiro hit, `validate` e `doctor`.

## Agente: o explorer (`urna tui`)

O explorer roda na sessão tmux `urna`, em 120x36. A tela é capturada como texto em `$SHOTS` (padrão `/tmp/urna-shots/<nome>.txt`).

```sh
D=.contracts/.ai/.agents/.skills/run-urna/drive.sh
sh $D tui                          # abre o corpus do quickstart na aba corpus
sh $D shot corpus                  # manifest e tabela de seções
sh $D key a                        # vai para a aba ask
sh $D type "how do citations work"
sh $D key Enter && sleep 5
sh $D shot ask                     # hits com score e o texto citado
sh $D key Tab && sleep 3
sh $D shot health                  # os checks do doctor
sh $D quit
```

`sh $D tui caminho/outro.urna` abre outro arquivo. `URNA_BIN` e `URNA_FILE` trocam o binário e o corpus padrão.

## Humano

`target/release/urna tui demos/quickstart/out/quickstart.urna` num terminal de verdade; `tab` troca de aba e `ctrl+q` sai.

## Testes

```sh
cargo test --workspace --release
```

As suítes Python carregam a extensão: depois de mexer em `crates/format`, `crates/engine` ou `crates/bridge`, recompile antes de rodá-las (macOS; no Linux o arquivo é `lib_urna.so`):

```sh
cargo build --release -p urna-bridge --features pyo3/extension-module && cp target/release/lib_urna.dylib python/_urna.so
python3 tests/test_pythonapi.py
```

## Pegadinhas

- O Python dos verbos segue esta ordem: `URNA_PYTHON`, depois o venv do `urna setup` (`~/.local/share/urna/venv`), depois o `.venv` mais próximo, depois `python3`. A linha `[urna] embedder interpreter: ...` vai para o stderr e diz qual foi. Para forçar o `.venv` do repo: `URNA_PYTHON=.venv/bin/python target/release/urna doctor`.
- O `doctor` acha o embedder pelo layout do repo a partir do caminho do binário, mesmo rodando fora do repo e com um `URNA_DATA_DIR` vazio. Um binário copiado para outro lugar depende do payload instalado.
- Ao abrir, um aviso "opening quickstart.urna" cobre a tabela de seções por uns 3 segundos. O `tui` do driver espera 4 antes de devolver; uma captura feita antes disso sai com a tabela cortada.
- Na aba ask, `q` vira texto da pergunta. Para sair, use `ctrl+q` (`sh $D key C-q`), que funciona em todas as abas.
- O `ask` responde em 100 a 200 ms depois que o Python sobe; a primeira chamada da sessão demora mais. Espere uns 5 segundos antes do `shot ask`.
- A captura é texto, sem cor: `tmux capture-pane -p`. O conteúdo é o mesmo da tela.
