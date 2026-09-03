# Releasing a new version

Two repositories are involved. This one holds the source and the git tag; the
[DuckDB community extensions registry](https://github.com/duckdb/community-extensions)
holds a pointer to a commit here and builds, signs and hosts the binaries itself.
Nothing is uploaded from this repository, and the GitHub release carries no assets.

## 1. Decide the version

Follow semver on the *output* of the functions, not on the code:

- **minor** bump when any input's code changes (an `rphonetic` upgrade that alters
  an encoder, a fold or dedupe change). Stored codes in users' tables become stale,
  so they must be able to see it in the version.
- **patch** bump for build, CI, docs and dependency updates that leave every output
  unchanged. `test/sql/commons_codec_corpus.test` pins 279 names, so a green run is
  decent evidence of "unchanged".

## 2. Bump the crate version on `main`

```sh
perl -pi -e 's/^version = ".*"$/version = "X.Y.Z"/' Cargo.toml
cargo update -p duckdb_rphonetic     # refreshes Cargo.lock
```

Commit and merge this **before** tagging. The version string embedded in the built
extension comes from the git tag, not from `Cargo.toml`, so a mismatch does not
break anything, but it is confusing (v0.2.0 was tagged with 0.1.0 in `Cargo.toml`).

## 3. Tag and publish the GitHub release

On the merged `main` commit:

```sh
git tag vX.Y.Z && git push origin vX.Y.Z
gh release create vX.Y.Z --generate-notes
```

`--generate-notes` lists the merged PRs since the previous tag. Edit the notes if a
behaviour change deserves a sentence users will actually read.

## 4. Point the registry at the new tag

Edit `extensions/duckdb_rphonetic/description.yml` in a branch of your fork of
`duckdb/community-extensions` (`guizmaii/community-extensions`) and open a PR:

```yaml
extension:
  version: X.Y.Z          # same as the tag, without the v

repo:
  github: guizmaii-opensource/duckdb_rphonetic
  # vX.Y.Z
  ref: <full 40-character SHA the tag points at>   # git rev-list -n1 vX.Y.Z
```

Update the behavioural note in `extended_description` if the output changed. Their
CI builds every platform from that commit; a DuckDB maintainer merges, usually within
a day or two. Past PRs: [#2546](https://github.com/duckdb/community-extensions/pull/2546)
(v0.1.0), [#2600](https://github.com/duckdb/community-extensions/pull/2600) (v0.2.0).

## 5. Verify

Once the registry PR is merged and the build has published:

```sql
FORCE INSTALL duckdb_rphonetic FROM community;
LOAD duckdb_rphonetic;
SELECT extension_version FROM duckdb_extensions() WHERE extension_name = 'duckdb_rphonetic';
```

Existing users pick the new build up with `UPDATE EXTENSIONS;`.

## Notes

- `TARGET_DUCKDB_VERSION` in the `Makefile` and `duckdb_version` in
  `.github/workflows/MainDistributionPipeline.yml` must agree, and the extension is
  built against DuckDB's unstable C API, so it only loads in that exact DuckDB
  version. Moving to a new DuckDB release is itself a release of this extension.
- The Linux job in CI is the only place the sqllogictests run on Linux; the
  distribution pipeline skips tests there. Do not remove it.
