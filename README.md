<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/banner-dark.svg">
    <img alt="sifr-diff — built from zero" src=".github/banner-light.svg" width="100%">
  </picture>
</p>

# hunkdiff

Zero-dependency, byte-preserving line diff in Rust. Dynamic-programming LCS,
three context lines, and deterministic deletion-first tie-breaking. Exit 0 =
identical, 1 = different, 2 = error.

```sh
hunkdiff a.txt b.txt
cat b.txt | hunkdiff a.txt -
```

## Example

```console
$ sifr-diff old.txt new.txt
@@ -1,3 +1,3 @@
 same
-old
+new
 end
$ sifr-diff same.txt same.txt ; echo $?
0
```

## Scope and compatibility

- LF-delimited byte lines, including Unicode, invalid UTF-8, and NUL; no
  Unicode normalization, CR stripping, or text encoding conversion.
- `O(n·m)` LCS time and matrix memory; `(n+1)(m+1)` must be at most 64 Mi cells
  (256 MiB for the matrix alone). Checked arithmetic guards allocation size.
- Unified hunks with three context lines; six common lines merge, seven split.
  Output has hunk headers only, without file/timestamp headers.
- GNU-style ranges and no-newline markers. Ambiguous repeated-line alignments
  follow the documented deletion-first policy and can differ from GNU's equally
  valid LCS choices. No binary detection or other `diff` command-line options.

## Build and verify

```sh
cargo build --release --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 tests/differential.py target/debug/sifr-diff
```

The seeded differential suite validates reconstruction, optimal LCS length,
context, malformed CLI/file errors, and deterministic reruns. When GNU diff is
available, it checks semantic equivalence and counts exact hunk-body matches
under the [documented compatible policy](docs/verification.md).

- [Architecture and resource limits](docs/architecture.md)
- [Verification policy and commands](docs/verification.md)
- [Release notes and checklist](docs/release.md)

## License

`MIT` — see `LICENSE`. Built from zero.
