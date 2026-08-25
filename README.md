# duckdb-rphonetic

A DuckDB extension for **phonetic name matching** — finding names that *sound*
alike despite being spelled differently.

DuckDB ships `soundex()`, which is tuned for English. This extension adds two
encoders that do a much better job on German and Central/Eastern European
names:

| Function | Returns | Algorithm |
|---|---|---|
| `cologne_phonetic(VARCHAR)` | `VARCHAR` | Kölner Phonetik (Cologne phonetics) |
| `daitch_mokotoff(VARCHAR)` | `LIST(VARCHAR)` | Daitch-Mokotoff Soundex |

Both are provided by the [`rphonetic`](https://crates.io/crates/rphonetic)
crate, a Rust port of the phonetic encoders in
[Apache Commons Codec](https://commons.apache.org/proper/commons-codec/).

## Install

Once the extension is accepted into the DuckDB community repository:

```sql
INSTALL rphonetic FROM community;
LOAD rphonetic;
```

Until then, build it yourself — see [Building](#building).

## `cologne_phonetic`

Kölner Phonetik maps a word to a digit string. Two names match when their codes
are equal.

```sql
SELECT cologne_phonetic('Müller');   -- 657
SELECT cologne_phonetic('Mueller');  -- 657
SELECT cologne_phonetic('Meier');    -- 67
SELECT cologne_phonetic('Mayr');     -- 67
```

Umlauts fold to their base vowel, `ß` is an s-sound, and case is irrelevant, so
the usual German spelling variants collapse together:

```sql
SELECT DISTINCT cologne_phonetic(name)
FROM (VALUES ('müller'), ('Müller'), ('MÜLLER')) t(name);
-- 657
```

Joining two tables on the code:

```sql
SELECT a.name, b.name
FROM people a
JOIN watchlist b ON cologne_phonetic(a.name) = cologne_phonetic(b.name);
```

Input with no encodable letter returns the **empty string**, not NULL:

```sql
SELECT cologne_phonetic('') = '', cologne_phonetic('123') = '';  -- true, true
```

## `daitch_mokotoff`

Daitch-Mokotoff Soundex is designed for Slavic and Yiddish surnames, and it is
deliberately **not** a single code. An ambiguous letter sequence branches, and
the encoder returns *every* reading:

```sql
SELECT daitch_mokotoff('Peters');  -- [734000, 739400]
SELECT daitch_mokotoff('Schwarz'); -- [474000, 479400]
```

Two names match when their code lists **overlap** — use `list_has_any`. (This
is the DuckDB equivalent of matching two Postgres `text[]` columns with `&&`.)

```sql
SELECT list_has_any(daitch_mokotoff('Schwarz'), daitch_mokotoff('Schwartz'));
-- true
```

Comparing only the first code would miss that pair: `Schwarz` starts at
`474000` and `Schwartz` is `479400`. They agree only on `Schwarz`'s *second*
branch. `Cohen` (`[456000, 556000]`) and `Kagan` (`[556000]`) are the same
story.

Joining on overlap:

```sql
SELECT a.name, b.name
FROM people a
JOIN watchlist b
  ON list_has_any(daitch_mokotoff(a.name), daitch_mokotoff(b.name));
```

That join does not scale: `list_has_any(a, b)` is not an equality, so the
planner cannot hash-join on it and falls back to comparing every pair of rows.
Unnest the codes instead and join on equality, which does hash-join:

```sql
CREATE TABLE people_codes AS
SELECT id, unnest(daitch_mokotoff(name)) AS code FROM people;

SELECT DISTINCT p.id, w.id
FROM people_codes p JOIN watchlist_codes w USING (code);
```

Input with no encodable letter returns a single all-zero code, matching Commons
Codec:

```sql
SELECT daitch_mokotoff('');  -- [000000]
```

## NULL handling

Both functions propagate NULL — `f(NULL)` is `NULL`, the same as DuckDB's
built-in `soundex()`. Note that this is distinct from the empty-input cases
above: `cologne_phonetic('')` is `''` and `daitch_mokotoff('')` is `['000000']`,
whereas both return `NULL` for a NULL input.

## Cologne dedupe behaviour

**If you are comparing this extension against another Kölner Phonetik
implementation, read this section.** Implementations genuinely disagree, and
this one may not produce what you expect.

Kölner Phonetik assigns each letter a digit, then collapses repeated digits and
drops the `0`s. The disagreement is over the *order* of those last two steps.

Take `Dieter`. The raw codes are `D`→2, `I`→0, `E`→0, `T`→2, `E`→0, `R`→7:

| Order | Steps | Result |
|---|---|---|
| Collapse repeats, *then* drop zeros | `200207` → `20207` → `227` | **`227`** |
| Suppress each code equal to the last *emitted* code | the `0`s are never emitted, so the second `2` is suppressed | `27` |

**This extension produces `227`**, because that is what `rphonetic` does. Apache
Commons Codec produces `27`.

Mechanically, `rphonetic`'s encoder tracks the last *candidate* code, including
the `0`s and `H`s it goes on to drop, so a dropped code keeps a run of identical
digits apart. Commons Codec tracks the last code it actually wrote, so a dropped
code does not break the run.

This is not limited to `Dieter`. Over the 279-name corpus in `test/corpus/`, the
two disagree on **36 names** — about 13%:

| Name | This extension | Commons Codec |
|---|---|---|
| `Dieter` | `227` | `27` |
| `Koch` | `44` | `4` |
| `Mannheim` | `666` | `6` |
| `Neumann` | `666` | `6` |
| `Hoffmann` | `0366` | `036` |
| `Zimmermann` | `86766` | `8676` |

The full list is committed at
[`test/corpus/cologne-divergences.tsv`](test/corpus/cologne-divergences.tsv),
and `test/sql/commons_codec_corpus.test` pins every entry, so a `rphonetic`
upgrade cannot change the output without failing the build.

Widely published reference vectors are unaffected — the two agree on
`Müller-Lüdenscheidt` → `65752682`, `Wikipedia` → `3412`, and `Breschnew` →
`17863`.

Daitch-Mokotoff has no such caveat: it agrees with Commons Codec on all 279
names in the corpus. The only adjustment is that this extension deduplicates the
returned list (order preserving), because a repeated code carries no information
for overlap matching and Commons Codec's `soundex()` does not emit one either.
Cologne output is passed through unmodified.

## Building

Requires Rust, Python 3, and `make`.

```sh
git clone --recurse-submodules https://github.com/guizmaii-opensource/duckdb-rphonetic
cd duckdb-rphonetic
make configure
make debug        # or: make release
make test
```

The extension lands in `build/debug/rphonetic.duckdb_extension`. To load an
unsigned local build:

```sh
duckdb -unsigned
```
```sql
LOAD 'build/debug/rphonetic.duckdb_extension';
```

### Regenerating the Commons Codec oracle

`test/corpus/commons-codec.tsv` is the output of Apache Commons Codec 1.22.1
over `test/corpus/names.txt`. Regenerating it needs Java 11+ (the jar is
downloaded from Maven Central into a temp directory; nothing is added to the
repo):

```sh
./test/oracle/run.sh                                     # refresh the oracle
./configure/venv/bin/python3 test/oracle/divergences.py  # refresh the divergence list
```

## Scope

Two functions, deliberately. `rphonetic` also offers Beider-Morse, Caverphone,
Double Metaphone, Match Rating Approach, Metaphone, NYSIIS, Phonex, Refined
Soundex and Soundex; those are out of scope for a first release. Issues and pull
requests welcome if you need one.

## Licence and attribution

This extension is licensed under the **Apache License 2.0** — see
[`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

The algorithms are not original work here. They come from:

- [`rphonetic`](https://github.com/Dalvany/rphonetic) by Dalvany, Apache-2.0 —
  the Rust implementation this extension wraps.
- [Apache Commons Codec](https://commons.apache.org/proper/commons-codec/),
  Apache-2.0 — the Java implementation `rphonetic` was ported from, and the
  reference oracle for this extension's test suite. Some test vectors in
  `test/corpus/names.txt` come from its unit tests.

Built with [duckdb-rs](https://github.com/duckdb/duckdb-rs) and
[extension-template-rs](https://github.com/duckdb/extension-template-rs).
