# Architecture and limits

## Pipeline

`sifr-diff` is a zero-runtime-dependency Rust command-line program.

1. **Input** reads each path as bytes. `-` reads standard input once; `- -`
   compares that one stream with itself, matching GNU `diff - -`.
2. **Line splitting** recognizes LF (`0x0a`) only. A terminating LF belongs to
   its line. Remaining bytes after the last LF form one unterminated line. Empty
   input has no lines. UTF-8 is not required and NUL is an ordinary byte.
3. **LCS** builds a suffix dynamic-programming matrix. A cell contains the LCS
   length for the remaining suffixes as a `u32`. Equal lines advance diagonally;
   otherwise deletion wins an equal-value choice, then insertion. This is a
   deterministic minimum-edit line alignment under the LCS objective, not a
   promise to match every GNU tie choice.
4. **Hunks** retain at most three common operations at either edge. Changes with
   six or fewer common lines between them merge; seven or more split. Hunks are
   ranges into the operation vector, not searches for line addresses.
5. **Output** counts operations to produce unified ranges, emits each original
   line byte-for-byte, and writes a separate `\ No newline at end of file` marker
   after an unterminated line. There are deliberately no file/timestamp headers.

`cargo test` exercises internals and the executable through its CLI. The
independent Python oracle reimplements the documented policy and validates its
output structurally; see `docs/verification.md`.

## Resource limits

The matrix has `(n + 1) * (m + 1)` cells, including one sentinel row and column.
Checked `usize` arithmetic precedes allocation. More than `67,108,864` cells is
an error (exit 2); exactly that many is accepted. The matrix occupies exactly
`4 * cells` bytes, **256 MiB at the limit**, excluding allocator bookkeeping.

This is not a total-memory guarantee. Input buffers, per-line vectors, output
operations, hunks, process overhead, and temporarily overlapping allocations add
memory. In particular, a small number of very long lines can require a large
input buffer while using a small matrix. There is no separate input-byte limit.
The CLI loads whole inputs and is not intended for unbounded streams.

The worst-case DP running time is `O(n * m)`; line scanning and alignment output
are linear in their respective sizes. Equal inputs take the same DP path, so
there is no quadratic-work fast path. For large or adversarial files, use a diff
implementation with a subquadratic or bounded-memory algorithm.

## Portability and errors

Paths are passed as `OsString`; file contents and output do not require UTF-8.
Non-UTF-8 paths in diagnostics may be rendered with a replacement character.
Exactly two operands are required. Missing files, directory reads, stdin/read
failures, oversized matrices, and stdout write failures produce exit 2 and a
diagnostic instead of a Rust panic. Broken-pipe reporting follows the host's
standard I/O error behavior. The normal status codes are 0 (identical), 1
(different), and 2 (error). Signals and resource-exhaustion termination are not
translated into those statuses.
