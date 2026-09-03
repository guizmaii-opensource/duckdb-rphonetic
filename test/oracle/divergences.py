#!/usr/bin/env python3
"""Regenerate test/corpus/cologne-divergences.tsv.

Loads the locally built extension, runs cologne_phonetic over the corpus and
records every name where it disagrees with Apache Commons Codec (as captured in
test/corpus/commons-codec.tsv by test/oracle/run.sh).

Run `make debug && ./test/oracle/run.sh` first, then:

    ./configure/venv/bin/python3 test/oracle/divergences.py
"""

import csv
import pathlib
import sys

import duckdb

ROOT = pathlib.Path(__file__).resolve().parents[2]
CORPUS = ROOT / "test" / "corpus"
EXTENSION = ROOT / "build" / "debug" / "duckdb_rphonetic.duckdb_extension"


def main() -> int:
    if not EXTENSION.exists():
        print(f"{EXTENSION} not found; run `make debug` first", file=sys.stderr)
        return 1

    con = duckdb.connect(config={"allow_unsigned_extensions": "true"})
    con.execute(f"LOAD '{EXTENSION}'")

    with (CORPUS / "commons-codec.tsv").open(encoding="utf-8", newline="") as f:
        reader = csv.reader(f, delimiter="\t", quoting=csv.QUOTE_NONE)
        next(reader)
        oracle = [(row[0], row[1]) for row in reader]

    divergences = []
    for name, commons in oracle:
        actual = con.execute("SELECT cologne_phonetic(?)", [name]).fetchone()[0]
        if actual != commons:
            divergences.append((name, commons, actual))

    out = CORPUS / "cologne-divergences.tsv"
    with out.open("w", encoding="utf-8", newline="") as f:
        f.write("name\tcommons_codec\trphonetic\n")
        for row in sorted(divergences):
            f.write("\t".join(row) + "\n")

    print(f"Wrote {out} ({len(divergences)} of {len(oracle)} names diverge)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
