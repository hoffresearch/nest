note: this folder is the RESEARCH (read-only reference). the ACTION layer (plan, kanban, task work-orders) lives in ../plan. the complete lossless source is _raw/genome.json (use it to train models).

================================================================================================
nest research :: start here
183 reference projects distilled into a build plan, in small navigable files
================================================================================================

see README.txt for the full map

this is the output of the nest-genome study: ~200 agents read one reference project each,
distilled them by category, and merged the result into a build plan for nest. provenance: run id
wf_778ca04e-7ef. no source files were changed by the study itself.

how to use this
------------------------------------------------------------------------------------------------
  - building something next? open kanban.txt (the board) and master-plan/03-roadmap.txt.
  - want the strategy? read master-plan/00-vision.txt then 01-compression.txt.
  - working a specific area? open synthesis/<category>.txt for recommendations, then
    genome/<category>.txt for the raw per-repo extracts.

files
------------------------------------------------------------------------------------------------
  README.txt                 this map
  kanban.txt                 the build board (now/next/later/research)
  master-plan/00-vision.txt  vision, architecture, six pillars
  master-plan/01-compression.txt  the per-data-type compression strategy
  master-plan/02-format.txt  additive format evolution (encoding 4-9, sections 0x09-0x10)
  master-plan/03-roadmap.txt phased build order
  master-plan/04-risks-quickwins.txt  risks and quick wins
  synthesis/<category>.txt   per-category distillation (20 files)
  genome/<category>/         one small file per project (00-index.txt lists them)

counts
------------------------------------------------------------------------------------------------
  projects 183   categories 20   must:22  high:101  medium:45  low:10  reference:5

categories  (synthesis/<name>.txt + genome/<name>.txt)
------------------------------------------------------------------------------------------------
  compression-general    12  general-purpose byte compression and entropy coding
  compression-integer     6  integer and posting-list compression (bitpacking, varint, elias-fano)
  hashing-crypto          5  hashing, checksums, content addressing, authenticated encryption
  simd-lowlevel          12  SIMD, vectorization, portable intrinsics, fast string ops
  serialization-parsing   7  zero-copy serialization, fast JSON/CSV parsing, mmap-friendly layouts
  ann-index              14  approximate nearest neighbor indexes and libraries
  vector-quantization     6  vector quantization and bit-compression of embeddings (the compression key for vectors)
  vector-db              11  vector databases and sqlite vector/columnar extensions
  graph-db               15  graph databases, engines, and traversal
  semantic-rdf            7  RDF triple stores, semantic web, graph query languages
  embedded-db             8  embedded / datalog / temporal / kv / column storage engines (edit + real-time)
  fulltext-search        16  full-text and lexical search engines (inverted index, BM25, edge cases)
  embeddings-models      15  embedding and representation models (text + image, including tiny/static)
  tokenizers              4  tokenization (the front of every embed + a compression primitive)
  model-runtime           3  inference runtimes and tensor libraries for on-device embedding
  rag-orchestration      18  RAG, agent, LLM orchestration, and memory systems
  data-pipeline           3  ML data lakes, dataset tooling, embedding benchmarks
  content-addressed       4  content-addressed, reproducible, and distributed storage
  factcheck-datasets     12  fact-checking / misinformation resources and datasets (nest demo domain + truth features)
  bindings-cli            5  language bindings, CLI ergonomics, distribution/client surfaces
