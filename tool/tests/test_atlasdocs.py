"""Prove docs/ATLAS.toml's summary says what urna is today, not its history.

the summary grew into a changelog: one dated paragraph per change, 17,789
characters. the history lives in docs/CHANGELOG; the atlas keeps the
current state, and the rule at the top of the file says so. this suite
holds the summary to that:

- happy path: the checked-in summary has at most three sentences, fits the
  size limit and carries no date;
- error path: a dated paragraph, a fourth sentence or an oversized summary
  is named;
- edge case: the rule stays at the top of the file, and a version or a path
  with dots is not counted as a sentence end.

Run: python tool/tests/test_atlasdocs.py
"""

import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ATLAS = REPO / "docs" / "ATLAS.toml"

MAX_CHARS = 639  # the number of the 639-line guard, as a character cap
MAX_SENTENCES = 3
DATE = re.compile(r"\b20\d\d-\d\d-\d\d\b")
# a sentence ends at . ! or ? followed by whitespace or the end; `.urna`,
# `v0.5.4` and `rust/format` do not end one.
END = re.compile(r"[.!?](?=\s|$)")


def problems(summary: str) -> list[str]:
    found = []
    if len(summary) > MAX_CHARS:
        found.append(f"{len(summary)} characters, over {MAX_CHARS}")
    sentences = len(END.findall(summary.strip()))
    if sentences > MAX_SENTENCES:
        found.append(f"{sentences} sentences, over {MAX_SENTENCES}")
    for date in DATE.findall(summary):
        found.append(f"dated history ({date}) belongs in docs/CHANGELOG")
    return found


def test_summary_is_current_state():
    summary = tomllib.loads(ATLAS.read_text(encoding="utf-8"))["summary"]
    assert problems(summary) == [], problems(summary)


def test_history_is_named():
    dated = "urna is a stack. 2026-08-10: the media pillar landed."
    assert any("2026-08-10" in p for p in problems(dated)), problems(dated)
    four = "One. Two. Three. Four."
    assert problems(four) == ["4 sentences, over 3"], problems(four)
    long = "x" * (MAX_CHARS + 1)
    assert problems(long) == [f"{MAX_CHARS + 1} characters, over {MAX_CHARS}"]


def test_dots_inside_a_sentence_do_not_end_it():
    one = "A `.urna` file of v0.5.4 in rust/format, read by urna.open()."
    assert problems(one) == [], problems(one)


def test_rule_is_at_the_top():
    head = ATLAS.read_text(encoding="utf-8").split("\n\n", 1)[0]
    assert "docs/CHANGELOG" in head and "Current state only" in head, head


def main() -> int:
    tests = [v for k, v in globals().items() if k.startswith("test_") and callable(v)]
    for test in tests:
        test()
        print(f"ok  {test.__name__}")
    print(f"atlasdocs: {len(tests)} cases passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
