"""select a COHORT of entries from a giant zip: whole groups (e.g. every session
of a patient) kept intact, so a group-membership recall harness has real
relevance sets instead of scattered singletons.

streams the archive once and extracts only the group id from each entry with a
regex over a bounded byte prefix (no full json parse, so it is fast and never
holds a whole entry), buckets entry names by group, then emits the names of
entries whose group reaches --min-group members, largest groups first, until
--target-notes is collected or --scan-cap entries have been read. the cohorts
are intact WITHIN the selected set, which is what an intra-corpus recall ruler
needs. writes the entry-name list (one per line); prints only counts.
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import os
import re
import sys
import zipfile


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--zip", required=True)
    ap.add_argument("--out-names", required=True, help="output entry-name list (keep OUT of git for PHI)")
    ap.add_argument("--name-glob", default="*.json")
    ap.add_argument("--group-key", default="patient_file_id")
    ap.add_argument("--min-group", type=int, default=3)
    ap.add_argument("--target-notes", type=int, default=25000)
    ap.add_argument("--scan-cap", type=int, default=300000)
    ap.add_argument("--prefix-bytes", type=int, default=32768, help="bytes read per entry to find the group id")
    args = ap.parse_args()

    pat = re.compile(rb'"' + re.escape(args.group_key.encode()) + rb'"\s*:\s*(\d+)')
    buckets: dict[int, list[str]] = {}
    scanned = matched = 0

    with zipfile.ZipFile(args.zip) as z:
        names = [
            n
            for n in z.namelist()
            if not n.endswith("/") and fnmatch.fnmatch(os.path.basename(n), args.name_glob)
        ]
        names.sort()
        for n in names:
            if scanned >= args.scan_cap:
                break
            with z.open(n) as fh:
                head = fh.read(args.prefix_bytes)
            scanned += 1
            m = pat.search(head)
            if m:
                buckets.setdefault(int(m.group(1)), []).append(n)
                matched += 1
            if scanned % 20000 == 0:
                qual = sum(len(v) for v in buckets.values() if len(v) >= args.min_group)
                print(
                    f"  scanned {scanned}, groups {len(buckets)}, notes-in-cohorts {qual}",
                    file=sys.stderr,
                )
                if qual >= args.target_notes:
                    break

    cohorts = sorted(
        ((pid, v) for pid, v in buckets.items() if len(v) >= args.min_group),
        key=lambda kv: -len(kv[1]),
    )
    selected: list[str] = []
    n_cohorts = 0
    for _pid, v in cohorts:
        selected.extend(v)
        n_cohorts += 1
        if len(selected) >= args.target_notes:
            break
    selected.sort()  # deterministic build order
    with open(args.out_names, "w") as f:
        f.write("\n".join(selected) + "\n")

    print(
        json.dumps(
            {
                "scanned": scanned,
                "matched_group_id": matched,
                "groups_total": len(buckets),
                "cohorts_selected": n_cohorts,
                "notes_selected": len(selected),
                "min_group": args.min_group,
            }
        )
    )


if __name__ == "__main__":
    main()
