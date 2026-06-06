# vendored model: potion-base-8M

a vendored, offline model2vec/potion static embedding table. it is the SEMANTIC
default embedder for forge (python/forge/embed_potion.py): token rows already
carry distilled meaning, so synonyms land close with no torch, no model
download, and no network at runtime.

## source and license

- upstream: https://huggingface.co/minishlab/potion-base-8M
- author: minishlab (the minish lab)
- license: mit (as declared in the upstream model card metadata), redistributable
- retrieved: 2026-06-06, revision bf8b056651a2c21b8d2565580b8569da283cab23

## what is here

- model.safetensors  the embedding table, dtype f32, shape [29528, 256] (~30mb,
  tracked via git-lfs). hidden_dim 256, drop-in with the tiny preset.
- tokenizer.json      the bert-wordpiece (bge-base-en-v1.5) tokenizer, lowercase.
- config.json, modules.json, tokenizer_config.json, special_tokens_map.json  the
  model2vec config (apply_pca 256, apply_zipf, normalize true) and tokenizer config.

## inference

model2vec recipe, reproduced with numpy + tokenizers only (verified to match the
model2vec reference vector-for-vector): tokenize (add_special_tokens=false) ->
gather the token rows -> mean pool -> l2 normalize.

## re-fetch (if the lfs blob is missing)

```
base=https://huggingface.co/minishlab/potion-base-8M/resolve/main
for f in model.safetensors tokenizer.json config.json modules.json \
         tokenizer_config.json special_tokens_map.json; do
  curl -sSL "$base/$f" -o "python/forge/models/potion-base-8M/$f"
done
```

## sha256 (provenance, the model_hash fingerprints these)

- model.safetensors  f65d0f325faadc1e121c319e2faa41170d3fa07d8c89abd48ca5358d9a223de2
- tokenizer.json      e67e803f624fb4d67dea1c730d06e1067e1b14d830e2c2202569e3ef0f70bb50
- config.json         2a6ac0e9aaa356a68a5688070db78fc3a464fefe85d2f06a1905ce3718687553
