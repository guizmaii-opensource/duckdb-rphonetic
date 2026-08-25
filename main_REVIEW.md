# Audit: duckdb-rphonetic (main @ 52e2b51)

Full-repo audit of the AI-written DuckDB extension. Every claim below was
verified by executing code on this machine (macOS arm64, DuckDB v1.5.5,
rustc stable, Corretto 25), not by reading alone.

**Correctness bar used:** the two SQL functions must behave as documented in the
README — Kölner Phonetik per `rphonetic` 3.1.0 (with the documented divergence
from Commons Codec), Daitch-Mokotoff in exact agreement with Apache Commons
Codec 1.22.1 — with sane NULL/edge/vector-type behavior and no way to crash the
DuckDB process.

## Verdict: **APPROVE-WITH-NITS**

No correctness, safety, or fidelity defects found. The code is small, honest
about its one behavioral divergence, and unusually well tested for an
extension of this size. Two cosmetic nits below.

## Verified green

Each item lists the check actually executed.

1. **Build and full test suite pass.** `make debug && make test` →
   `commons_codec_corpus.test`, `cologne_phonetic.test`, `daitch_mokotoff.test`
   all SUCCESS against DuckDB v1.5.5.
2. **The committed Commons Codec oracle is genuine.** Ran
   `./test/oracle/run.sh` (downloads commons-codec 1.22.1 from Maven Central,
   runs `Oracle.java` over the 279-name corpus). The regenerated
   `test/corpus/commons-codec.tsv` is **byte-identical** to the committed one.
   Combined with the passing corpus test, this proves: Daitch-Mokotoff agrees
   with Commons Codec on all 279 names (values *and* order), and Cologne agrees
   except for exactly the 36 recorded divergences.
3. **The README's divergence explanation is factual.** A scratch crate pinned
   to `rphonetic 3.1.0` reproduces `Cologne.encode("Dieter") == "227"` (Commons
   Codec: `27`, confirmed in the regenerated oracle TSV, line 133).
4. **The DM `dedupe` is justified, not invented.** Raw
   `DaitchMokotoffSoundex::inner_soundex("Bergisch-Gladbach", true)` returns
   `["795458", "795458"]` and `"mönchengladbach"` returns
   `["664658", "664658", "665658", "665658"]` — rphonetic really does emit
   duplicates for separator inputs, and the dedupe restores Commons Codec's
   set semantics. `dedupe()` itself (src/lib.rs:37) is a correct in-place,
   order-preserving, first-occurrence filter (read-verified; also pinned by the
   `[795458]` / `[664658, 665658]` assertions in daitch_mokotoff.test:83).
5. **Non-flat input vectors are handled.** The classic bug in C-API scalar
   functions is assuming `flat_vector(0)` on a constant or dictionary input.
   With `PRAGMA disable_optimizer`, `cologne_phonetic('Müller')` over 3000 rows
   returns `657` for every row, and both functions return correct values over a
   filtered scan (`WHERE i % 2 = 1` on a 10k-row table with NULLs, selection
   vectors exercised). DuckDB flattens the chunk before the C-API callback, so
   the code's flat assumption holds in practice.
6. **Multithreaded execution is stable.** 2M rows, `SET threads=8`: correct
   distinct-code counts, no crash, no corruption (also validates the
   `LazyLock<DaitchMokotoffSoundex>` shared across threads).
7. **No panic-able input found, and a panic could not kill the process
   anyway.** Fuzzed with NUL bytes, tabs, combining-character runs, Arabic,
   CJK, ligatures (ĳ, ǆ, ﬀ), mathematical alphanumerics, ß, emoji — no panic,
   and degenerate outputs match the documented behavior (`''` /
   `[000000]`). Independently confirmed duckdb-rs wraps the scalar callback in
   `catch_unwind` and converts panics to DuckDB errors
   (`duckdb-1.10505.0/src/callback.rs:35` via `contain_callback`,
   `vscalar/mod.rs:186`), so even a future rphonetic panic surfaces as a SQL
   error, not an abort.
8. **Pathological DM branching is bounded.** `repeat('Rosochowaciec', 200)`
   (800 ambiguous positions) and `repeat('tch', 2000)` both encode in ~50 ms
   total — branches converge on the fixed 6-digit code, no exponential blowup.
9. **List-vector edge cases hold.** The zero-length child reserve
   (all-NULL chunk), NULL-chunk-then-values chunk, empty relation, and
   chunk-boundary offset tests in daitch_mokotoff.test:130-169 all pass; the
   two-pass reserve-then-write in src/lib.rs:112-151 is the correct pattern for
   the child-reallocation hazard its comment describes.
10. **Hygiene.** `cargo fmt --check` clean; `cargo clippy` no warnings;
    Apache-2.0 LICENSE + NOTICE with rphonetic/Commons Codec attribution;
    CI workflow present (`.github/workflows/MainDistributionPipeline.yml`,
    pinned to `v1.5-variegata` / DuckDB v1.5.5, matching the Makefile);
    generated files (`build/`, `configure/`, `target/`, venv) untracked;
    `Cargo.lock` committed (right call for a binary artifact).

## Findings

### NIT 1 — stale file names in `test/oracle/Oracle.java:7-8`
The header comment references `test/oracle/run.sh` regenerating the TSV and
"`test/sql/corpus.test` asserts the extension matches it, with the known
divergences listed in `test/corpus/divergences.tsv`". The actual files are
`test/sql/commons_codec_corpus.test` and `test/corpus/cologne-divergences.tsv`.
Fix: update the two names.

### NIT 2 — `read_varchar_column` copies every string (src/lib.rs:50-68)
Each input row is materialized as an owned `String` per chunk. For
`daitch_mokotoff` the copy is justified by the two-pass write; for
`cologne_phonetic` the encoder could run directly off the borrowed
`DuckString::as_str()` inside the loop, saving one allocation+copy per row.
Measured impact is small (2M rows encode in a couple of seconds in a debug
build); leave as-is or change only if profiling ever says so.

## Residuals / out of scope

- **Release and cross-platform builds not exercised locally** (only `debug`
  osx_arm64). The CI pipeline builds/tests the distribution matrix; nothing in
  the source is platform-sensitive beyond what the template provides.
- **WASM build not exercised** (`src/wasm_lib.rs` is the unmodified template
  shim; nothing to review).
- **Local `make debug` stamps a stale extension version** (`a0456ac`, the
  commit at which `make configure` last ran) because
  `configure/extension_version.txt` is only refreshed by `make configure`.
  Template behavior, untracked file, cosmetic — re-run `make configure` before
  cutting a release.
- The Cologne divergence from Commons Codec is **inherent to rphonetic**, is
  documented at length in the README, and is pinned name-by-name by
  `commons_codec_corpus.test` (which also fails on stale divergence rows). It
  is a residual, not a defect.

## Open facts to confirm

None blocking. If you later want byte-level certainty that DuckDB always
flattens scalar-function input chunks (item 5 relies on observed behavior plus
DuckDB's documented C-API contract), the check is: build a debug DuckDB and
break on `duckdb_data_chunk` handling in
`CScalarFunctionInfo::GetFunction` — not worth it given the empirical coverage.
