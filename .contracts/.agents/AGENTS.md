# Agentes
`AGENTS.md` e `CLAUDE.md` são um symlink para esta única fonte de diretrizes para desenvolvedores agênticos. Não crie instruções agênticas adicionais na raiz, como GEMINI.md, CODEX.md, CLAUDE.md, regras do Cursor ou qualquer outro arquivo de instruções de agente.

# Comece aqui

1. Leia `docs/arc/ARC.toml` numa passada curta: a arquitetura, o inventário de arquivos, os fluxos de build e de consulta.
2. Trabalhe numa branch de vida curta a partir de `origin/main`, um pull request por assunto, squash merge.
3. Toda mudança de código vem com testes de verdade: caminho feliz, caminho de erro, um caso de borda, contra artefatos reais (arquivos `.urna` construídos, fixtures golden, corpora reais), sem mocks. Nenhum código entra sem prova executável.
4. Antes do pull request: `.contracts/.agents/.skills/afterwork/AFTERWORK.md` (ele lê o `specs.yaml` ao lado, a lista de todo arquivo que uma mudança pode deixar desatualizado; cada um atualizado no próprio lugar) e, para uma mudança de código, `./scripts/release_check.sh` (o gate); uma mudança só de docs diz no pull request que o gate não rodou.
5. As regras duras abaixo protegem três coisas: o formato de arquivo congelado, a promessa offline (nenhuma pilha de rede no binário, nenhum socket em tempo de consulta) e os canais de release. Todo o resto é julgamento. Quando uma regra atrapalha um design melhor, diga isso no pull request e mude a regra junto com a mudança.

# Autonomia

Por conta própria: branches, commits, pull requests, as suítes de teste, workflows que não publicam nada (CI, um dispatch de `install-test`), instalações em prefixos temporários, os docs que uma mudança possui.

Pergunte antes, porque publica ou é difícil de desfazer:

- Empurrar ou mover uma tag. Uma tag `v*` publica no crates.io, npm, Homebrew e PyPI, e uma versão no crates.io é permanente.
- Mesclar por cima de um review bloqueado (`--admin`): nunca. Espere o mantenedor.
- Force-push: na `main` nunca (o ruleset bloqueia); numa branch de feature só com `--force-with-lease` e um ok explícito.
- `git add -A`, `--no-verify`, apagar uma branch remota, mudar configurações do repositório ou da organização, rotacionar um secret.

Os secrets vivem nos secrets do repositório no GitHub (`CARGO_REGISTRY_TOKEN`, `NPM_TOKEN`, `HOMEBREW_TAP_TOKEN`) e no environment `pypi`. Um token nunca vai para a árvore, um commit, um log, um pull request ou uma mensagem de chat; um que foi está comprometido e é revogado. O caminho de trusted publisher (OIDC) não precisa de token nenhum: prefira-o onde um registry oferecer.

# Build e testes

- `cargo build --workspace`, `cargo build --release --workspace`
- `cargo test --workspace`: todo teste Rust (unitário, integração, golden); a contagem que ele imprime é a que o `docs/CHANGELOG` cita
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` (warnings são erros)
- `cargo clippy -p urna --no-default-features --all-targets -- -D warnings`: a CLI só com o engine, sem a interface de terminal, também tem que ficar limpa; o CI roda os dois
- `cargo deny check`: advisories, licenças (a allowlist em `deny.toml`), bans e fontes sobre o lockfile. Um crate yanked, uma licença só copyleft ou uma dependência git falham o CI; o mesmo arquivo de política cobre os outros dois workspaces com `--manifest-path forge-core/Cargo.toml` e `--manifest-path fuzz/Cargo.toml`
- `cargo semver-checks -p urna-format --baseline-rev origin/main`: a API Rust do formato congelado contra a base do pull request; o CI falha um pull request que a quebre (em 0.x um bump minor é o bump major)
- `cargo bench -p urna-runtime --no-run`: os benches do criterion (simd, rerank, hnsw_build) têm que compilar; o CI confere isso, os números não são gate
- `sh scripts/ruff_check.sh`: ruff sobre a única lista de arquivos Python compartilhada com o CI (`URNA_PYTHON=.venv/bin/python` escolhe o interpretador)
- `./scripts/release_check.sh`: o pipeline completo (a suíte Rust em release, a extensão reconstruída, quinze suítes Python, ruff quando importável) mais os gates de regressão contra `data/measure/baseline.json`; sai com código diferente de zero em qualquer falha. É a definição de pronto para pull request
- `forge-core/` é um workspace cargo separado, fora de `crates/` (a camada de ingestão, o schema `.fci` congelado). `--workspace` e `release_check.sh` nunca o alcançam; rode `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings` e `cargo fmt --all --check` com `--manifest-path forge-core/Cargo.toml`
- `fuzz/` é o terceiro workspace cargo (cargo-fuzz, toolchain nightly): `sh scripts/fuzz_soak.sh [segundos]` roda todos os alvos com o corpus mantido em `fuzz/corpus/`; `fuzz/README.md` tem os alvos e como um achado vira teste

Alvos individuais:

- `cargo test -p urna-format`, `cargo test -p urna-runtime`
- `cargo test --release -p urna-runtime --test hnsw_recall`: a regressão de recall; em debug é 30x mais lento e estoura o timeout do cargo test
- `cargo test -p urna`: os testes de integração da CLI (o cargo constrói o binário que eles rodam, `CARGO_BIN_EXE_urna`)
- `cargo run -p urna-format --example regen_golden`: regenera a fixture golden congelada byte a byte, só quando o formato mudou de verdade

## Python

A extensão é construída à mão no dev:

```
cargo build --release -p urna-python --features pyo3/extension-module
cp target/release/lib_urna.dylib python/_urna.so   # macos
cp target/release/lib_urna.so   python/_urna.so    # linux
```

`pyo3/extension-module` mantém a libpython fora da cdylib; sem ele o `.so` amarra um caminho de libpython e dá segfault sob interpretadores embutidos estaticamente (o python-build-standalone do uv). O `release_check.sh` constrói com a feature e fixa `PYO3_PYTHON` no interpretador dos testes.

A wheel publicada é maturin, em staging; nunca edite `packaging/staging/` à mão:

```
python scripts/stage_wheel.py
(cd packaging/staging && maturin build --release)   # a wheel cai em target/wheels/
```

`packaging/pyproject.toml` é a única fonte do projeto da wheel. O script de staging copia ele mais `python/urna.py` (como `urna/__init__.py`), `python/urna_cli.py` (como `urna/_cli.py`), `python/forge/embed_potion.py` e a tabela potion. abi3, Python 3.12 em diante.

A wheel instala seu próprio console script chamado `urna` (`python/urna_cli.py`): um shim somente leitura sobre a API da biblioteca (`validate`, `inspect`, `stats`, `search`) para que `uvx --from urna urna ...` funcione. Não é o binário Rust; `ask`, `retrieve`, `build`, `doctor` e a interface de terminal existem só lá. `pip install "urna[embed]"` adiciona numpy e tokenizers para `urna.embed_potion`; a superfície básica não precisa de nada além da wheel.

Os testes Python são scripts simples com `if __name__ == "__main__"`; `pytest tests/` não funciona. Eles precisam do `.so` construído antes:

```
python tests/test_e2e.py
python tests/test_builder.py
python tests/test_search_text_model_hash.py
python tests/test_offline_guard.py
python tests/test_blob_bridge.py
python tests/test_space_bridge.py
python tests/test_image_corpus.py
python tests/test_forge_spec.py
python tests/test_quality_gate.py
python tests/test_cli_space.py
python tests/test_query_embedder_routing.py
python tests/test_embedder_payload.py
python tests/test_bench_runner.py
python tests/test_model_catalog.py
python tests/test_model_install.py
python tests/test_release_preflight.py
python tests/test_pypi_release.py
python tests/test_release_rehearsal.py
```

O `release_check.sh` roda quinze deles; `test_offline_guard.py`, `test_blob_bridge.py` e `test_space_bridge.py` rodam à mão.

`test_image_corpus.py` cobre o pilar de imagem do forge (encode e decode, sondagem de GOP, sharding, ordenação) com um embedder stub e pula limpo sem os encoders AV1 e AVIF do FFmpeg. Construir um corpus de imagens de verdade (`python/forge/embed_image.py`) precisa de `open_clip` e torch, fora do grupo de dependências padrão do forge.

O embedder padrão do lado de build do forge é a tabela estática model2vec/potion-base-8M vendorizada (`python/forge/embed_potion.py`): offline, sem torch, sem rede.

- Seu autoteste é `python python/forge/test_embed_potion.py`. Precisa de numpy e tokenizers (`uv pip install numpy tokenizers`, o grupo de dependências `forge`) e da tabela, que é git-lfs: `git lfs pull`, ou `sh scripts/fetch_potion.sh` quando o LFS não estiver disponível.
- O autoteste prova o salto semântico (car ~ automobile bem acima de car ~ banana), determinismo, estabilidade em f32 e que nenhum socket abre no momento do embed. `python python/forge/recall_harness.py` mostra o recall por consulta contra o piso.
- O piso léxico bag-of-words (`python/forge/embed_default.py`) é só stdlib, com autoteste próprio: `python python/forge/test_embed_default.py`.
- Os dois geram um fingerprint para um `model_hash` registrado na proveniência; nenhum dos dois roda no `release_check.sh`.
- Mais três autotestes do forge, também fora do `release_check.sh`: `python python/forge/test_model_registry.py` (o contrato do registry, sem deps de ML), `python python/forge/test_embed_st.py` (o worker sentence-transformers contra um snapshot local do wemm-2b; pula sem ele) e `python python/forge/test_open_clip_snapshot.py` (a consulta SigLIP2 offline só a partir do snapshot fixado, o `model_hash` do build, um arquivo faltante nomeado; pula sem torch, open_clip, transformers ou o snapshot).

Ponto de entrada Python: `sys.path.insert(0, "python"); import urna`. O loader encontra `_urna.so` ou `lib_urna.dylib`.

# Layout

```
crates/urna-format    container v1 congelado: layout, manifest, seções, encodings, hashes, reader, writer
crates/urna-runtime   abertura por mmap, dispatch simd, hnsw, bm25, grafo, busca exact/ann/graph/hybrid com rerank exato obrigatório
crates/urna-cli       o binário `urna`, publicado como o crate `urna`: verbos do engine em cmd/*.rs, verbos de agente em cmd/agent/*.rs,
                      o único gate de modelo em cmd/embed_gate.rs, a interface de terminal em src/tui (feature `tui`, ligada por padrão)
crates/urna-python    a ponte pyo3, cdylib `_urna`, abi3-py312; sai como a wheel, não como crate
forge-core/           workspace cargo separado: o schema .fci congelado de intermediário canônico da camada de ingestão
fuzz/                 workspace cargo separado (nightly, cargo-fuzz): quatro alvos, seeds/, README.md; corpus/ é local, nunca commitado
docker/               Dockerfile: o binário musl estático numa imagem scratch; só verbos do engine, sem python dentro
python/               o pipeline do writer, fingerprint de modelo, embedders de consulta, e forge/ (builds declarativos, registry de modelos, gate de qualidade)
tests/                scripts de teste python
data/                 o corpus demo em lfs, measure/ com as baselines de regressão, demo/ com as fontes (gitignored, ver Instructions.md)
docs/                 arc/ARC.toml, USAGE.md, BENCH.md, CHANGELOG, SECURITY.md, CONTRIBUTING.md
scripts/              release_check.sh, ruff_check.sh, pre-commit, install.sh / install.ps1, fetch_potion.sh, stage_*.py
packaging/            pyproject.toml, a única fonte da wheel (staging/ é gerado)
examples/             quickstart (o laço de cinco verbos sobre doze parágrafos cc0), fastapi, flask, jupyter
assets/images/        o cabeçalho do readme, as thumbs sociais, as capturas do setup e da tui
```

O mapa completo (todo arquivo, os fluxos, os contratos) é `docs/arc/ARC.toml`. Deps Rust principais: memmap2, rayon, zstd, half, bytemuck, sha2, thiserror, clap, serde.

Três workspaces cargo, uma política cada: o `Cargo.toml` da raiz (`crates/*`, o que é distribuído), `forge-core/` e `fuzz/`. `deny.toml` é a política do cargo-deny para os três. `clippy.toml` fixa a complexidade cognitiva em 15 (o padrão do Clippy é 25) e 7 argumentos por função; uma exceção legítima recebe `#[allow(clippy::...)]` no local, o limite nunca se move. `rustfmt.toml` fixa os padrões stable com largura 100.

A CLI tem três grupos, que `urna --help` marca e ordena:

- Verbos do engine, arquivo e vetor na entrada: `inspect`, `validate`, `stats`, `media`, `search`, `search-ann`, `search-graph`, `search-space`, `search-text`, `benchmark`, `cite`, `doctor`. Dois deles rodam Python: `search-text` (o embedder sentence-transformers, `python/embed_query.py`) e `doctor` (ele sonda o ambiente Python e roda um embed potion); os outros dez nunca rodam.
- Verbos de agente sobre o mesmo engine, `cmd/agent/`: `build` (um build declarativo de corpus, lançando `python/tools/urna_forge.py`), `ask` (texto na entrada, resposta citada na saída, `--disclose answer|explain`), `retrieve` (JSON ou JSONL de spans citados; `score` é o valor do rerank exato). Eles fazem embed offline e roteiam o embedder de consulta pelo modelo do manifest: corpora potion ficam com o script potion, modelos do registry passam por `python/forge/embed_query_model.py`, que entrega a `python/embed_query.py` um modelo sentence-transformers que nenhum preset nomeia. O caminho de busca é roteado pelo que o arquivo carrega (`MmapUrnaFile::search_routed`, compartilhado com `search-text`): hybrid quando há seção BM25, HNSW quando há seção HNSW, exact caso contrário; o `index_type` do manifest nomeia só o índice vetorial, então um arquivo do preset `hybrid` (`index_type = "hnsw"`, `supports_bm25 = true`) toma a rota hybrid. O grafo só é alcançado por `search-graph`. O contrato de build são as seções 12 a 14 de `docs/USAGE.md`.
- A interface de terminal, `src/tui`:
  - `urna setup` é o instalador em que todo canal termina: sonda a máquina, mostra o plano, instala, verifica. O payload vem por um processo filho `curl` do sistema com o SHA256 conferido enquanto transmite; `--yes` para scripts; `--model <nome>` (repetível, `all`) instala também modelos do catálogo, `--allow-remote-code <nome>` é o consentimento separado para o código do repositório de um modelo. Códigos de saída: 10 download, 11 checksum, 12 unpack, 13 ambiente Python, 14 bloqueado, 15 instalação de modelo, acima do 2 a 6 do doctor.
  - `urna tui [arquivo]` é o explorer: home, corpus, ask, health, um seletor de arquivos, uma passagem para o setup. Um `urna` puro num terminal o abre; num pipe imprime a ajuda e sai com 2.
  - `URNA_RELEASE_BASE=file:///dir` aponta o setup para uma release local (os testes e2e fazem isso).

# Contrato

Os invariantes do formato e do runtime. Uma mudança que os toca precisa dos testes nomeados ao lado de cada um.

- Rust edição 2024, resolver 3, erros com `thiserror`, nunca um panic em código de biblioteca. `repr(C)` mais `bytemuck::Pod` para o layout binário, inteiros little-endian sem sinal.
- Todo bloco `unsafe` carrega um comentário `// SAFETY:` nomeando o invariante; o Clippy nega os não documentados e `unwrap` fora de testes (`[workspace.lints]`). Prefira casts do `bytemuck` a raw parts; mantenha kernels de ponteiro cru atrás de um dispatcher seguro que confere todo comprimento em release.
- Leitura e limites: campos de largura fixa por `urna_format::bytes::{le_u32, le_u64, le_f32, array32}` (`UnexpectedEof` tipado, nunca `try_into().unwrap()`); tamanhos derivados do header com aritmética checada; verificações de cursor como `need > remaining`, nunca `pos + need > len`; scores f32 ordenados pelo módulo `order` do runtime (`crates/urna-runtime/src/order.rs`, ordem total com NaN por último), nunca `partial_cmp(..).unwrap_or(Equal)`.
- O formato binário v1 está congelado. Encodings 4 a 255 e ids de seção 0x09 em diante são reservados dentro do v1 e aditivos; `URNA_FORMAT_VERSION` só se move quando um campo existente muda de significado. O mapa de ids de seção é o array `contract` do `ARC.toml`. Os ids que o writer emite, nenhum deles livre para um codec novo: encodings 1 zstd, 2 float16, 3 int8, 4 intpack, 5 zstd com dicionário treinado, 7 int4, 9 fsst, 10 txt_streams (`layout/mod.rs` nomeia todos, 6 e 8 são reservados e não escritos); seções 0x07 hnsw, 0x08 bm25, 0x0A dictionary, 0x0B dedup map, 0x0C graph, 0x14 blob_refs, 0x15 space_table, 0x16 blob_span_overlay, 0x17 blob_data, e a banda de embeddings 0x20 a 0x2F. As seções de índice, grafo, mídia e espaço ficam fora do content_hash; 0x0A e 0x0B são as tabelas laterais dos codecs de texto e decodificam para o texto canônico, que está dentro dele. Confira `layout/mod.rs` antes de tomar um id.
- Uma mudança de decoder roda o harness de mutação antes do pull request: `cargo test -p urna-format --test mutation_fuzz -p urna-runtime --test mutation_fuzz` (`URNA_MUTATION_ITERS=25000` para um soak), depois `sh scripts/fuzz_soak.sh` (nightly, cargo-fuzz). Um codec novo ganha um braço em `fuzz/fuzz_targets/section_decoders.rs`; um achado vira um `tests/negative_*.rs` antes do conserto e um `fuzz/seeds/regress-*.bin`.
- Quatro verificações SHA-256, duas formas. As duas reportadas como `sha256:<64 hex minúsculos>`: `file_hash` (o arquivo inteiro, footer incluído; o próprio footer guarda o digest de tudo antes dele, `[0, size-40)`) e `content_hash` (as seções canônicas decodificadas, estável entre encodings). As duas guardadas cruas no layout: `header_checksum` e o `checksum` de cada seção são os primeiros 8 bytes do SHA-256 sobre os bytes físicos (`layout/header.rs`, `layout/section_entry.rs`; `inspect` os imprime como 16 caracteres hex). Mesmos chunks, mesmo fingerprint de modelo e `reproducible=True` dão arquivos byte a byte idênticos, então `urna://content_hash/chunk_id` aponta para conteúdo, não para uma cópia.
- `UrnaFileBuilder` é um builder consumidor (`add_chunk(self) -> Self`). `preset=` aceita `exact` (raw + f32), `compressed` (zstd + f16), `tiny` (zstd + int8 + hnsw), `nano` (zstd + int4 bloco 64 + hnsw) ou `hybrid` (zstd + f32 + hnsw + bm25). `micro` nas tabelas não é um valor de preset: é o nome publicado de `tiny` com `mrl_dim=256`, construído com `urna.build(text_encoding="zstd", dtype="int8", mrl_dim=256, with_hnsw=True)`.
- O truncamento Matryoshka é um kwarg de build (`mrl_dim`): o builder Python fatia cada linha L2-normalizada nos seus primeiros K componentes e renormaliza antes da quantização; o `embedding_dim` do header vira K e `full_dim` registra a origem. Nenhuma mudança de kernel no runtime.
  - int4 precisa da dimensão efetiva divisível por 64, então sua escada é 256, 192, 128. O content_hash cobre os vetores truncados.
  - O corpus MiniLM distribuído não é treinado com MRL, então o truncamento custa recall medido (`measure_presets.py --variants mrl<DIM>-<dtype>`).
- Builds de HNSW são determinísticos dado um seed; o índice BM25 é ordenado por termo.
- `model_hash` é o fingerprint de `(model_id, files_hash, tokenizer_hash, pooling_config_hash, embedding_dim, normalize_embeddings)`. `urna.build` (e `builder.Pipeline`) recusam o placeholder zero na hora de escrever, a menos que `allow_placeholder_model_hash=True` (fixtures de teste); o `UrnaFileBuilder` Rust aceita qualquer hash bem formado (a fixture golden congelada carrega o placeholder), e o gate da CLI recusa um corpus com placeholder em tempo de consulta. Um modelo de runtime que difere do modelo do corpus falha com um erro tipado.
- Dispatch SIMD: AVX2 em x86_64, NEON em aarch64, fallback escalar, acumuladores f32. `URNA_FORCE_SCALAR=1` força o escalar.
- A fixture golden `crates/urna-format/tests/fixtures/golden_v1_minimal.urna` está congelada byte a byte em 1366 bytes.
- O reader também aceita o magic `NEST` de arquivos escritos pela 0.4.0, antes do rename (`tests/legacy_magic.rs`, fixture `legacy_v040_minimal.nest`, mesmo layout e hashes); o writer nunca o emite, e qualquer outro magic é rejeitado.
- O `search` da CLI recebe um array JSON de f32; `search-text` chama `python/embed_query.py` e confere seu `model_hash` contra o manifest.
- API Python: `urna.open(path)` devolve um `UrnaFile` com `search`, `search_ann`, `search_hybrid`, `search_graph`, `search_space`, `retrieve`, `validate`, `inspect`, mais `chunk_ids()` (identidade na ordem do arquivo, a chave para casar hits sob qualquer ordenação de mídia), `blob_refs()`, `has_blobs` e `blob_bytes(i)` (um blob embutido fatiado do mmap); os hits carregam `citation_id`, `source_uri`, offsets e o `score` do rerank exato.
- `cite` é só tier-1: o texto canônico armazenado mais os hashes que o verificam, nunca uma reabertura dos bytes originais. `ask` e `retrieve` imprimem o mesmo texto. Nenhum texto de ajuda ou doc afirma o contrário.
- O binário não linka pilha de rede e o runtime nunca abre um socket. O setup baixa pelo `curl` do sistema.

# Fluxo de trabalho

- Remoto `git@github.com:hoffresearch/urna.git`, dono Hoff Research, mantenedor Brenner Cruvinel (`brenner@hoffresearch.com`).
- `main` é a única branch de vida longa. O ruleset exige pull requests, commits assinados por SSH e verificados, e histórico linear; pull requests entram por squash merge. Apague a branch depois do merge e comece a próxima a partir de `origin/main`.
- Tags só na `main`; a versão do workspace em `Cargo.toml` acompanha a última tag. Cortar uma release é o passo 8 do checklist do mantenedor em `docs/USAGE.md`; o item 11 de lá é o que a tag 0.5.0 ensinou (LFS, dist, mover uma tag, a execução do PyPI, nomes no npm). Leia os dois antes de tocar em `release.yml`, `pypi.yml` ou na config do dist; depois de editar a config do dist rode `dist generate` e então `python scripts/release_rehearsal.py generate`, nunca edite `release.yml` ou `release-rehearsal.yml` à mão (passo 12 do usage).
- `.github/workflows/ci.yml` roda em todo push na `main` e todo pull request: fmt, Clippy com os lints de deny (a CLI completa e a CLI só engine com `--no-default-features`), build e teste em Ubuntu (AVX2) e macOS (NEON), os benches compilados, os harnesses de mutation-fuzz com contagem maior, a guarda das 639 linhas, o gate do forge-core, cargo-deny nos três workspaces, cargo-semver-checks em `urna-format` contra a base do pull request, um job Windows (Clippy, os testes unitários da CLI e `setup_e2e`: a única verificação Windows antes de uma tag, então uma mudança de crossterm ou de caminho que quebra só lá aparece lá), ruff, e um smoke limitado de cargo-fuzz no nightly. É o `release_check.sh` menos a medição do corpus em LFS.
- No agendamento noturno (ou `workflow_dispatch`) o CI roda, em vez disso, um soak de 30 minutos por alvo de fuzz com o corpus em cache entre as noites, e a suíte do `urna-format` sob Miri.
- Uma tag `v*` roda a release:
  - `tag-verify.yml` (a assinatura, depois `scripts/release_preflight.py --tag`) é o primeiro job do `build-wheels.yml` dentro da release, e roda de novo antes de um upload ao PyPI: uma tag leve ou não assinada, uma chave ausente de `.github/allowed_signers`, ou uma versão que não bate param a release antes de o job host liberar qualquer coisa.
  - `release.yml` (cargo-dist): arquivos para 5 alvos, checksums, attestations Sigstore, a fórmula Homebrew `urna`, o pacote npm `@urna/cli`, o payload do embedder, e `publish-crates.yml` para urna-format, urna-runtime e urna.
  - As wheels (maturin abi3, 4 plataformas) são construídas em `build-wheels.yml`, um job local-artifacts do dist, então o host e todo job de publicação esperam por elas; a release as carrega com um `.sha256` e uma attestation cada. O PyPI é o job de publicação `publish-pypi.yml`: ele despacha o `pypi.yml` de topo na tag (trusted publishing não aceita um workflow reutilizável) e espera essa execução, cujo upload pula wheels já publicadas com o mesmo sha256.
  - `install-test.yml` roda dentro da execução da release assim que ela é anunciada e testa o produto instalado por plataforma e por canal (one-liner, Homebrew, npm, bun, pnpm, yarn, binstall, wheel), cada um terminando em `urna setup --yes` e `urna doctor`.
- O Git LFS rastreia `*.urna`, `*.safetensors` e datasets, incluindo `data/corpus_next.v1.urna` e a tabela potion; as fixtures golden ficam no git normal. Nenhum job de release toca no LFS (`scripts/fetch_potion.sh` busca a tabela do seu upstream fixado e a confere contra o pointer).
- `data/demo/` é gitignored; `data/demo/Instructions.md` nomeia o que ela guarda e onde o corpus pt-BR é reconstruído (o repositório fakenews-ptbr-urna-benchmark, não este). Só `measure_presets.py` e `release_check.sh` precisam do corpus baseline, e eles leem o arquivo em LFS, nunca os datasets. `data/measure/corpus_*.urna` são artefatos de regeneração e gitignored; as baselines JSON ao lado deles são rastreadas.
- `scripts/pre-commit` aborta um commit que põe em stage um artefato de dados fora da allow-list (o anteparo de PHI). Instale por clone: `cp scripts/pre-commit .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit` (uma cópia, não `core.hooksPath`, para que os hooks do LFS continuem funcionando).
- Mensagens de commit em inglês simples, sem prefixo de conventional-commits; o corpo explica o porquê, o diff mostra o quê. O título do pull request diz o que muda, o corpo o porquê e como foi testado; no squash eles viram o commit na `main`.

# Docs e estilo

- Sem emoji. Sem travessão, o caractere de traço longo: use vírgula, ponto e vírgula, ponto ou um hífen comum.
- Parágrafos curtos, voz direta, sem texto de marketing. Docs são orientados a tarefa: o que faz, como rodar, um exemplo.
- Todo doc começa com um cabeçalho YAML: `project`, `audience`, `status`, `last-updated`, `domain` (skills também carregam `name` e `description`). Isentos: `README.md` (empacotado pelo crates.io, PyPI e npm; o GitHub renderiza front matter como tabela), `docs/LICENSE`, `.github/pull_request_template.md` (seu texto vira o corpo do pull request), `llms.txt` (segue o formato llms.txt) e os documentos do corpus demo em `python/forge/demo_corpus/` (são dados).
- `llms.txt` é descoberta para LLMs e busca: título, resumo, links com uma linha cada. Ele aponta para os docs e não carrega instrução; este arquivo é a instrução.
- `docs/arc/ARC.toml` é a única referência de arquitetura: narrativa (system_view, contract, quality, risks), inventário de arquivos e o mapa Mermaid dos fluxos de build e consulta (`diagram.source`). Depois de qualquer mudança em módulo, fronteira, fluxo, contrato público, armazenamento ou comportamento de runtime, atualize-o na mesma mudança: avance `last-updated`, acrescente uma nota datada à `summary`, adicione arquivos novos ao inventário. Nenhum segundo documento de arquitetura.
- Docs são corrigidos no próprio lugar. Histórico e decisões, incluindo uma decisão que se mostrou errada e o que a substituiu, vão para `docs/CHANGELOG`, o commit e o pull request; nunca como notas "mudou x para y" dentro de um doc.
- Nomes: diretórios, docs e assets em kebab-case em inglês; arquivos fonte idiomáticos à sua linguagem. Proponha um rename como comandos `mv`, corrija todo import que ele toca, rode os testes.
- `.editorconfig` é a formatação base: UTF-8, LF, indentação de 4 espaços (2 para TOML, YAML, JSON), newline final.

# Higiene de arquivos

O limite duro é 639 linhas por arquivo de código. A memória de trabalho humana segura 4 mais ou menos 1 blocos de cada vez (Cowan 2001, refinando Miller), e um arquivo que não cabe nessa janela força troca de contexto, diffs mais pesados e mais bugs.

Um arquivo criado ou modificado que passa de 639 linhas é lido por inteiro (o que faz, do que depende, quem o importa) e dividido por responsabilidade em módulos que fazem uma coisa cada, com imports e superfície pública mantidos e os testes passando com a mesma contagem. Isentos: testes, dados e arquivos gerados, lockfiles, JSON, YAML, TOML, RON, JSONL, CSV, datasets e arquivos vendorizados. O `ci.yml` impõe o limite em `crates/**/src/**`; o `release_check.sh` conta todo `.rs` fora de teste em `crates/`, benches e exemplos incluídos.

# Encerrando uma tarefa

Antes de encerrar uma tarefa ou sessão, rode o AFTERWORK contra o diff final. Siga cada comportamento mudado pelos seus chamadores, workflows, configuração, documentação e exemplos; atualize toda referência afetada no mesmo trabalho. Repita a verificação para as referências afetadas por essas atualizações até não restar nenhuma inconsistente. Instruções operacionais pertencem ao USAGE, arquitetura ao ARC, e histórico ao CHANGELOG. Preserve arquivos não afetados e distinga comportamento implementado de publicação verificada. Reporte os commits testados, as limitações restantes e qualquer ação indispensável do mantenedor.

`.contracts/.agents/.skills/afterwork/AFTERWORK.md` é o procedimento e `.contracts/.agents/.skills/afterwork/specs.yaml` a lista: todo arquivo que o walk revisa, a mudança que o deixa desatualizado (`when`) e como ele é atualizado (`rule`). Um arquivo criado, removido ou renomeado tem sua entrada ajustada na mesma mudança. No caminho, varra as mudanças da sessão em busca de código morto, scripts temporários, arquivos perdidos e arquivos fora da pasta a que seu papel pertence; apague ou mova, corrija o que eles tocavam, rode os testes. Escreva um manifesto temporário da tarefa na sua pasta tmp, nunca na árvore.

# Gotchas

- Reconstrua `python/_urna.so` depois de toda mudança Rust em urna-format, urna-runtime ou urna-python. Os testes Python fazem `dlopen` dele, então um `.so` velho passa nos testes contra código antigo. O `release_check.sh` o reconstrói; à mão você tem que lembrar.
- `crates/urna-cli` tem seu próprio MSRV, 1.88 (o piso do Ratatui 0.30); urna-format, urna-runtime e urna-python mantêm o 1.85 do workspace. Com 1.88 o Clippy sugere let-chains na CLI, e é por isso que seus `if let` aninhados estão colapsados.
- f16 em NEON: `float16x4_t` e `vcvt_f32_f16` são stable desde o rustc 1.94, acima do MSRV do workspace. `crates/urna-runtime/build.rs` sonda o compilador e emite `cfg(neon_f16)` em 1.94 e acima; esse cfg guarda `simd/neon.rs::dot_f32_f16_neon`, toolchains mais antigas tomam o kernel f16 escalar, e o kernel carrega `#[clippy::msrv = "1.94"]`. Mantenha o build.rs e o cfg até o `rust-version` do workspace chegar a 1.94.
- `urna-format` roda sob Miri no agendamento noturno. Um teste novo lá que chame zstd (código C que o Miri não roda), converta f16 pelo `half` em aarch64 (asm inline) ou construa uma tabela fsst (lento demais interpretado) carrega `#[cfg_attr(miri, ignore)]` com o motivo, como os existentes; os harnesses de mutação e de propriedade ficam nativos.
- A interface de terminal é dona do stdout e do stderr enquanto uma tela está aberta: nenhum `println!` ou `eprintln!` de código que a UI chama (`pyenv::set_quiet(true)` silencia a nota do interpretador; workers reportam por canais). Todo frame termina com `pal::fit`, a redução para 256 cores e `NO_COLOR`. Para ver uma tela de verdade: `tmux new-session -d -x 112 -y 28`, `tmux capture-pane -p -e -N`, renderize os escapes (o estado SGR atravessa linhas; tire o OSC 8 antes de medir colunas).
- Links hyperrat passam por `hud::link`: o hyperrat põe a sequência OSC 8 inteira numa célula e o diff do Ratatui então pula essa quantidade de células; `hud::link` força a largura do diff à do rótulo. Nunca renderize `hyperrat::Link` diretamente.
- `ask` e `retrieve` fazem embed offline, roteados pelo modelo do manifest:
  - Um corpus potion usa `python/forge/embed_query_potion.py` (numpy + tokenizers, sem torch, sem socket).
  - Um corpus cujo espaço de texto padrão é um modelo do registry (wemm, CLIP, SigLIP2, Jina) passa por `python/forge/embed_query_model.py`, que o carrega localmente; rede só com `URNA_ALLOW_DOWNLOAD=1`. Um preset open_clip com `revision` (SigLIP2) carrega pesos e tokenizer de `snapshots/<revision>` do cache do HF, nunca `refs/main` nem o nome do hub: pelo nome, o AutoTokenizer do transformers falha offline num repositório sem `config.json` mesmo totalmente em cache. Seu `model_hash` não pode se mover, então o checkpoint vai para a arquitetura embutida com o preprocess da tag (`local-dir:` constrói o preprocess a partir do JSON e seu repr difere); um arquivo faltante do snapshot é `SnapshotMissing`, saída 3 com `urna-fetch:`. Esses pins (`hf_repo`, `revision`, `weights_file`, `snapshot_files`) são separados do `install` (`InstallSpec`): o instalador verifica um fingerprint de arquivo, e um `model_hash` open_clip faz hash dos tensores carregados, então o SigLIP2 fica fora do catálogo com esse motivo.
  - O modelo do corpus demo pt-BR é o preset do registry `minilm-multilingual` (tipo `st_text`): seu adapter chama o próprio `load`/`encode`/`fingerprint` de `python/embed_query.py`, então corpora construídos antes do preset mantêm seu `model_hash` (`tests/test_query_embedder_routing.py`, caso 10, confere o corpus do benchmark). Qualquer outro modelo sentence-transformers que nenhum preset nomeia toma o mesmo caminho do `embed_query.py`. O modelo tem que estar no cache local do HF: offline por padrão, `URNA_ALLOW_DOWNLOAD=1` o busca uma vez. O torch carrega a cada consulta, então um ask nesse caminho leva segundos, não milissegundos.
  - Um embed que falha é um valor tipado de `cmd/embed_failure.rs`: script ausente, payload incompleto (um script dentro de um payload instalado a que falta um arquivo de `cmd/payload.rs` REQUIRED, ou um erro de import nomeando um módulo do payload: nunca uma dica de pip), pacotes ausentes (o embedder imprime `urna-needs: <spec pip>...`), pesos ausentes (`urna-fetch: <modelo>`), modelo incompatível (sem backend, um preset recusado, um nome ou dimensão que não bate) ou um model_hash que não bate. A CLI imprime a forma longa com o conserto, a aba ask do explorer o `short` de uma linha. Um modo de falha novo ganha uma variante lá, não uma comparação de string na UI.
  - O interpretador é `pyenv::resolve_interpreter`, uma escada só para todo processo filho Python (a seção 11 do usage é a cópia voltada ao usuário): `URNA_PYTHON`, depois o venv que `urna setup` criou (`<raiz de dados>/urna/venv`), depois o `.venv/bin/python` mais próximo subindo até quatro ancestrais do cwd, depois o `python3` no path. Todo script de embedder, o do `search-text` incluído, resolve por `embed_gate::installed_script_in`: o layout do repositório (`python/<rel>`, a partir do cwd e do próprio checkout de um binário dev), depois `<raiz>/urna/<rel>` para cada raiz de dados por vez (`URNA_DATA_DIR`, `XDG_DATA_HOME`, `~/.local/share`, `%LOCALAPPDATA%`, `<exe>/../share`). Sem as deps do forge a etapa de embed falha com `ModuleNotFoundError`. O payload da release carrega todo embedder de consulta (`urna/embed_query.py` e `urna/model_fingerprint.py` ao lado de `urna/forge/`, mais `urna/VERSION`, a release de que veio; o setup substitui um payload de outra release, sem o carimbo ou com um arquivo de `cmd::payload::REQUIRED` faltando, como uma etapa restaurável com o carimbo escrito por último, nunca o venv); um modelo do registry ainda precisa das próprias deps (torch, sentence-transformers ou open_clip) nesse venv, que `embed_query_model.py` nomeia, saída 4.
  - A instalação de modelo é uma operação só, `tui/setup/models.rs`, compartilhada por `urna setup --model` e pelo painel de instalação do explorer (`tui/app/offer.rs`, aberto por uma consulta `DepsMissing` ou `WeightsMissing` num modelo que o catálogo oferece). O catálogo é `python/forge/catalog.json`, gerado do registry por `python/forge/model_catalog.py` e versionado: um preset só é oferecido com um `InstallSpec` (revisão fixada, os arquivos exatos, seu `model_hash`), e o script de staging e `tests/test_model_catalog.py` recusam uma cópia defasada; rode `python python/forge/model_catalog.py --write` depois de mudar um preset. A lista de arquivos é parte da identidade: o fingerprint faz hash de todo arquivo relevante presente, então buscar o `pytorch_model.bin` de um repositório ao lado do `model.safetensors` muda o hash. Pacotes vão só para o venv que o setup gerencia (setup: o da sua própria pasta de dados; o explorer: o que a escada de consulta roda), nunca para um pin de `URNA_PYTHON`. `forge/install_model.py` busca por `hf_hub_download` com uma classe de progresso: o hub serve pesos por Xet, que não escreve arquivo parcial, então um watcher na pasta de blobs não vê nada até o fim.
  - Os testes e2e do flagship (`cli_e2e.rs`, `python/forge/test_retrieve.py`) precisam dessas deps e pulam sem elas; o `release_check.sh` não os roda.
- O fingerprint pt-BR: o fingerprint do modelo lê o cache local do sentence-transformers. Popule-o uma vez, `python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2')"`, ou o teste de fingerprint falha.
- Um conflito resolvido mesclando a `main` numa branch de pull request (o botão do GitHub incluído) pode manter um lado de uma definição e os usuários do outro lado: o merge do #271 manteve o `ModelPreset` da `main` sem os quatro campos de snapshot enquanto o preset SigLIP2 ainda os passava, e a `main` parou de importar o registry. O ruff não vê isso; rode `release_check.sh` (ou ao menos `python python/forge/model_catalog.py --check`, que o job python do CI roda) na árvore mesclada antes do squash.
- O squash merge substitui o histórico da branch: o pull request cai na `main` como um commit com hash novo, então uma branch que continua viva depois do merge conflita em todo arquivo que o squash tocou. Apague branches mescladas; nunca faça rebase de trabalho antigo sobre uma branch mesclada. Um pull request empilhado sobre outro carrega os commits pré-squash da base: uma vez que a base é squashada, mescle `origin/main` na branch empilhada localmente (assinado, `git merge -s ours origin/main` quando a branch já contém o conteúdo da base), confira que `git diff origin/main...HEAD` mostra só a própria mudança, faça push, e só então mescle. Resolver esse conflito no editor web manteve os dois lados do changelog do #280. Depois de cada merge, `git diff <commit testado> origin/main` tem que ficar vazio.
- `cargo clean` custa 30 a 60 s de rebuild; a compilação incremental dá conta da maioria das edições.
- Renomes só de caixa no macOS: o sistema de arquivos ignora a caixa e o git roda com `core.ignorecase=true`, então renomear `usage.md` para `USAGE.md` no disco não registra. Use `git mv -f velho Novo`.
- Substituições em todo o repositório passam por `git grep -l`, nunca `grep -r`: clones de terceiros gitignored vivem em `tools/` e `TMP/`, e um grep recursivo edita eles também. Rode gerenciadores de pacotes (npm, bun, pnpm) de uma pasta temporária, não da raiz do repositório, ou eles deixam um `package.json` para trás.
- Um binário construído em dev sempre acha o próprio checkout (`embed_gate::exe_repo_root`), então um teste do que um binário instalado resolve a partir das raízes de dados copia o binário para fora de `target/` primeiro e aponta `HOME`, `XDG_DATA_HOME` e `URNA_DATA_DIR` para a sua pasta de rascunho: `paths::data_roots` inclui `~/.local/share`, então de outro modo ele acha o payload e o venv reais da máquina (`crates/urna-cli/tests/embedder_resolution.rs`).
- O macOS mata um binário sobrescrito: copiar um `target/*/urna` fresco por cima de um existente faz toda execução seguinte sair com 137 (a assinatura de código não bate mais). Dê `rm -f` no destino antes do `cp`.
- O `release_check.sh` esconde a saída do Clippy: quando ele para no Clippy, rode `cargo clippy --workspace --all-targets -- -D warnings` para ver o lint. Ele também esconde os nomes dos testes que falham (só `passed=N failed=M`): rode `cargo test --release --workspace` à mão para vê-los.
- Um teste unitário nunca lê o ambiente do processo pelo código que testa: `URNA_PYTHON`, `URNA_DATA_DIR` e companhia vazam do shell que roda o gate (`URNA_PYTHON=.venv/bin/python ./scripts/release_check.sh` é a forma documentada). A sondagem (`Scan::probe`, `resolve_interpreter`) lê o env uma vez e passa um valor adiante; a função pura sob teste recebe esse valor.
- Verificar uma tag ou commit assinado localmente: `git -c gpg.ssh.allowedSignersFile=.github/allowed_signers verify-tag vX.Y.Z`. Um `git tag -v` puro falha sem a configuração, e `%G?` imprime `N` mesmo para um commit assinado.

# Limitações conhecidas

Limitações documentadas, não bugs para consertar de passagem. Aponte-as em qualquer trabalho que toque nessas áreas.

- `search-text` sobe um processo Python por chamada: fork, import de sentence-transformers e torch, carga do modelo, embed, saída. Cerca de 4 s quente e 7 s frio num Mac M-series (sentence-transformers 6.1, torch 2.14, o MiniLM); `ask` e `retrieve` num corpus sentence-transformers pagam o mesmo. As tabelas de latência medem o caminho de busca depois que o vetor está pronto, não de ponta a ponta; cargas dirigidas por Python (`UrnaFile.search` num laço) evitam isso.
- O tokenizer do BM25 é segmentado só por palavra (`crates/urna-runtime/src/bm25/tokenize.rs`, fronteiras Unicode não alfanuméricas): certo para latino, cirílico, grego, devanágari; errado para CJK, tailandês, lao, onde uma sequência de caracteres sem espaço é um token só, então só uma sequência idêntica casa e o recall cai. Desligue o BM25 lá (`with_bm25=False`) até sair um tokenizer ciente de idioma.
- Os canais de pacote distribuem o binário puro; `urna setup` é uma etapa, não um hook. Homebrew, npm, cargo install e binstall deixam só o binário, `urna doctor` falha até o setup rodar (a primeira verificação que falha nomeia o código: 2 sem interpretador, 3 com um a que faltam numpy e tokenizers, 4 com as deps presentes e sem payload do embedder), e nenhum canal o roda pelo usuário (o npm esconde a saída do postinstall, o dist não tem hook de fórmula). O setup precisa de `curl` no path e de uv ou de um python3 com `venv`.
- comfy-tabs e comfy-toaster não são dependências de propósito: os dois são source-available sob SA-PS:DA (uso comercial exige licença), incompatível com um produto MIT. As pílulas de aba são desenhadas em `app/chrome.rs`; os toasts adaptam o `ratatui-toaster` MIT/Unlicense.
- Os modelos ST do registry têm penhascos de custo medidos: o wemm-2b roda em fp16 no MPS com `image_max_side=768` (cerca de 0,6 img/s); o jina-v5-omni-nano não tem padrão de `image_max_side` e faz embed na resolução nativa (cerca de 0,3 img/s); mudar qualquer um dos dois invalida o cache daquele modelo por design (o botão entra no hash da receita).
- O embedder semântico padrão é inglês: o `potion-base-8M` é destilado do `bge-base-en-v1.5`. Sinônimos em inglês se agrupam bem (car ~ automobile +0,78, car ~ banana +0,04); texto em outra língua cavalga linhas de subpalavras inglesas e o sinal é fraco (carro ~ automovel +0,08, carro ~ banana -0,05). Um corpus majoritariamente não inglês precisa de um modelo sentence-transformers multilíngue ou de uma tabela potion multilíngue; o piso léxico é agnóstico de idioma, mas literal.

# Documentação

- `README.md`: a vitrine: o que é, instalação, as duas telas, quickstart, Python, CLI, benchmarks.
- `docs/USAGE.md`: o como-fazer de cada verbo, `urna setup` e `urna tui`, presets, modo offline, citações, o registry de modelos e espaços multimodelo (seção 12), builds declarativos (13), as alavancas de compressão e o gate de qualidade duplo (14), e a seção de referência: a tabela de toda variável de ambiente `URNA_*`, todo canal de instalação, verificação, notas de offline, o checklist do mantenedor.
- `docs/arc/ARC.toml`: a referência de arquitetura descrita acima.
- `docs/CHANGELOG`: toda release e os deltas não lançados, com o porquê e os números medidos.
- `docs/BENCH.md`: Urna contra usearch, hnswlib, sqlite-vec e LanceDB; regenerado por `python/tools/bench_competitors.py`, nunca editado à mão.
- `docs/SECURITY.md`: como reportar, versões suportadas, escopo, hardening, a postura de governança de dados.
- `docs/CONTRIBUTING.md`, `docs/CODE_OF_CONDUCT.md`, `docs/LICENSE` (MIT).
- `data/demo/Instructions.md`: o corpus pt-BR congelado do gate e seus hashes, as fontes de imagem.
- `scripts/release_check.sh`: leia; ele documenta o gate sendo o gate.
- `fuzz/README.md`: os quatro alvos do cargo-fuzz, rodar um soak, regenerar as sementes, transformar um achado em teste.
- `.contracts/.agents/.skills/afterwork/AFTERWORK.md`: o walk de fim de tarefa, sobre a lista de arquivos em `.contracts/.agents/.skills/afterwork/specs.yaml`; `.github/pull_request_template.md` o carrega como checkboxes.
- `llms.txt`: descoberta para LLMs e busca

# Sobre as entradas de comportamento humano nas sessões de código com agentes

O autor principal escreve rápido e usa transcrição de voz: erros de digitação, caps lock e acentos faltando são comuns. Leia a intenção; não aponte tom nem projete risco emocional.
