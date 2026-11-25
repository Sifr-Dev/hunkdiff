# Verification and compatible policy

Run from the repository root:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 tests/differential.py target/debug/hunkdiff
# A larger reproducible run (change the seed to explore a different corpus):
python3 tests/differential.py target/debug/hunkdiff --cases 5000 --seed 42
```

`tests/differential.py` requires Python 3.8+ and no third-party packages. It
uses GNU `diff` when present; the `--text -U3` reference is supported with GNU
diffutils (CI uses Ubuntu). macOS/Windows CI still runs all Rust tests, while
Ubuntu runs the complete differential job.

## Exact compatibility policy

The oracle is exact for the following deliberately specified behavior:

- LF-delimited byte lines, including non-UTF-8 and NUL bytes (GNU `--text`
  behavior); empty input and unterminated final lines;
- suffix-DP LCS, with deletion preferred whenever deletion and insertion choices
  have equal DP values;
- unified output with no `---`/`+++` file/timestamp headers;
- three context lines, merging gaps of at most six common lines;
- GNU-style range spelling: omit `,1`, and use the preceding line number for an
  empty range;
- output newline followed by the standard no-newline marker when needed;
- status 0 for equality and 1 for a difference.

This is a **documented compatible subset**, not a claim of universal byte
identity with GNU or Python `difflib`. GNU may choose another equally optimal
alignment when lines repeat, including a different occurrence of a repeated
new-side line when the optimal LCS is unique. The Python oracle checks all cases
against its independently implemented DP policy. When GNU is available, its
hunk body (after removing file headers) must pass the same reconstruction,
coordinates, context, and optimal-LCS checks. The report counts exact hunk-body
matches separately; GNU is not required to use this project's tie policy.

`difflib.SequenceMatcher` is not used as an LCS oracle: its Ratcliff/Obershelp
style matching heuristic can produce a different non-minimal edit script and
its autojunk heuristic is inappropriate for this specification.

## Corpus and assertions

The seeded corpus mixes repeated short lines, mutated copies, empty inputs,
multiple Unicode encodings, invalid UTF-8, NUL and other control bytes, missing
newlines, and context-gap boundaries. Defaults: 18 regressions + 300 random
cases, seed `20261120`. Each case runs twice. The parser independently checks:

- old/new hunk coordinates and all omitted gaps;
- reconstruction of both complete inputs from patch operations;
- common-line count equals the independently computed optimal LCS length;
- one-count range elision, no-newline markers, and context/hunk boundaries;
- deterministic status, stdout, and stderr.

The Rust tests pin the known original regressions: lossy non-UTF-8 output,
malformed no-newline lines, incorrect empty/single-line ranges, duplicate-line
offset safety, tie behavior, extra arguments, non-UTF-8 paths, stdin, and the
64 Mi-cell matrix boundary. The boundary test uses a rectangular
`33,554,432 × 2` matrix for exact acceptance (a difference), and
`33,554,432 × 3` for rejection.
