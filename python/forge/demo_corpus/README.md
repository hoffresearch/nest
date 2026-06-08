# demo corpus (license-clean)

a tiny, vendored, permissively-licensed document folder for the flagship
one-gif demo: build a `.nest` from this folder, ask a question, get an answer
with a clickable `nest://` citation that reopens the exact span.

## license

these documents are ORIGINAL prose written for the nest demo and are released
into the public domain under cc0 1.0
(https://creativecommons.org/publicdomain/zero/1.0/). there is no third-party
or academic-only data here, so the corpus can be redistributed with the
project without any licensing risk. that is deliberate: the flagship demo must
ship a corpus anyone can copy.

## contents

short explainer documents about nest itself (what a `.nest` file is, what
forge does, how citations work, why it is offline by construction). they are
self-referential on purpose: an agent retrieves cited spans about the very
tool it is running on, which is the clearest possible demo.

## intended use

paired with the default static embedder (`python/forge/embed_default.py`),
which is offline and deterministic, this folder builds a byte-identical `.nest`
with no network access, so the one-gif demo runs anywhere.
