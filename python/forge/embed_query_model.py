"""embed_query_model.py — registry-backed query embedder for the rust CLI.

argv (matching embed_query_potion.py, additively):
  <interp> embed_query_model.py [--model-path P] [--preset NAME] [--mrl-dim N]
      <manifest_embedding_model> <query>

stdout: one-line JSON {model_hash, fingerprint, embedding_model,
embedding_dim, vector}. The preset resolves from --preset, else by reverse
lookup of the manifest model name; the query is embedded with the preset's
text_query_mode (asymmetric models treat queries and documents differently).
--mrl-dim slices+renormalizes the query and reports the truncated dim, for
corpora whose default space was built with mrl_dim.

exit codes: 0 ok, 2 usage, 3 model asset missing, 4 deps/preset problem.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model-path")
    ap.add_argument("--preset")
    ap.add_argument("--mrl-dim", type=int)
    ap.add_argument("model")
    ap.add_argument("query", nargs="?")
    args = ap.parse_args()
    if not args.query:
        print("error: query required", file=sys.stderr)
        return 2

    from forge import model_registry as mr

    if args.preset:
        try:
            preset = mr.get_preset(args.preset)
        except mr.RegistryError as e:
            print(f"error: {e}", file=sys.stderr)
            return 4
    else:
        preset = mr.preset_for_embedding_model(args.model)
        if preset is None:
            valid = ", ".join(sorted(p.embedding_model for p in mr.PRESETS.values()))
            print(
                f"error: no registry preset embeds '{args.model}'. known manifest "
                f"models: {valid}. pass --preset to force one.",
                file=sys.stderr,
            )
            return 4

    try:
        # query-side loads are user-initiated on a corpus the user built, so
        # the remote-code opt-in was already given at build time; the pinned
        # hash allowlist still applies inside create_embedder.
        emb = mr.create_embedder(
            preset.name,
            model_path=args.model_path,
            allow_remote_code=frozenset({preset.name}),
            allow_heavy=True,
        )
        vec = emb.embed_texts([args.query], role="query")[0]
    except mr.RegistryError as e:
        print(f"error: {e}", file=sys.stderr)
        return 4
    except FileNotFoundError as e:
        print(f"error: model asset missing: {e}", file=sys.stderr)
        return 3

    dim = int(vec.shape[0])
    if args.mrl_dim:
        vec = mr.slice_renorm(vec.reshape(1, -1), args.mrl_dim)[0]
        dim = args.mrl_dim

    json.dump(
        {
            "model_hash": emb.model_hash,
            "fingerprint": emb.fingerprint(),
            "embedding_model": emb.embedding_model,
            "embedding_dim": dim,
            "vector": [float(x) for x in vec],
        },
        sys.stdout,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
