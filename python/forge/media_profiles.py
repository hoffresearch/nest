"""Measured media recipes behind `[media] profile = "..."` (RFC-1).

A profile resolves into knob defaults BEFORE the explicit keys of the
spec are applied, so an explicit key always wins and no use case is
closed off by choosing one. The `quality` table deep-merges: a profile
may pin gate floors and the spec may still override single keys.

  near-dup:  corpora with visual near-duplicates (card reprints, frames,
             scans): cluster ordering + per-segment gop lets inter pay
             (-29% on same-artwork reprints) without losing O(1) access
             on unique segments.
  stills:    unique images: all-intra + still tune (best size at O(1) seek).
  archive:   byte-reversible JPEG repack (jxl-transcode, 1.12x, sha256-
             verified roundtrip): for corpora where loss is not acceptable.
  retrieval: the corpus only serves search, nobody looks at the pixels.
             all-intra + still tune at a fixed crf 50, speed 6. measured
             2026-09-03 on 38627 cards: 532671548 B self-contained, 7.46x
             vs the jpeg source, no measurable txt@1 loss on 100 queries;
             the default drift floor (p10 0.942 < 0.98) would have vetoed
             it, so this profile pins crf instead of running the gate.
  retrieval-auto: the same recipe with crf="auto" gated by TASK UTILITY
             instead of drift: the visual and drift floors are disabled
             (negative), and every ladder rung must keep text-to-image
             hit@1 within `utility_tol` of the lossless source. measured
             2026-09-12 on the same 38627 cards: drift p10 falls 0.932
             (crf40) -> 0.829 (crf60) and vetoes every rung, while hit@1
             on 100 queries does not move up to crf50. drift is a
             stability signal; utility is the floor retrieval needs.
"""

from __future__ import annotations

MEDIA_PROFILES: dict[str, dict] = {
    "near-dup": {"order": "cluster", "gop": "auto", "tune": "still"},
    "stills": {"gop": "intra", "tune": "still"},
    "archive": {"backend": "jxl-transcode"},
    "retrieval": {"gop": "intra", "tune": "still", "speed": 6, "crf": 50},
    "retrieval-auto": {
        "gop": "intra",
        "tune": "still",
        "speed": 6,
        "crf": "auto",
        "quality": {
            "visual_floor_p10": -1e9,
            "visual_floor_min": -1e9,
            "drift_floor_p10": -1.0,
            "utility_floor_hit1": 0.0,
            "utility_tol": 0.02,
            "crf_ladder": [40, 45, 50, 55, 60],
        },
    },
}


def merge_profile(profile: dict, explicit: dict) -> dict:
    """Profile defaults under the explicit [media] keys; `quality` deep-merges
    one level so an explicit [media.quality] key wins over the profile's."""
    quality = {**profile.get("quality", {}), **explicit.get("quality", {})}
    merged = {**profile, **explicit}
    if quality:
        merged["quality"] = quality
    return merged
