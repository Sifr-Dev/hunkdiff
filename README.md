<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/banner-dark.svg">
    <img alt="sifr-diff — built from zero" src=".github/banner-light.svg" width="100%">
  </picture>
</p>

# sifr-diff

zero-dependency line diff in Rust. LCS via dynamic programming, 3 lines of
context per side, hunks split when the unchanged gap exceeds 6 lines.
Exit 0 = identical, 1 = different — `diff` semantics.

```sh
sifr-diff a.txt b.txt
cat b.txt | sifr-diff a.txt -
```

## evidence (this machine, 2026-11-17)

byte-identical hunk bodies vs `diff -u` on 3 fixtures:

```console
$ sifr-diff /tmp/far_a.txt /tmp/far_b.txt
@@ -2,7 +2,7 @@
 line 2
 line 3
 line 4
-line 5
+CHANGED_AT_5
 line 6
 line 7
 line 8
@@ -32,7 +32,7 @@
 line 32
 line 33
 line 34
-line 35
+CHANGED_AT_35
 line 36
 line 37
 line 38
$ sifr-diff same.txt same.txt ; echo $?
0
```

## honest scope

- perfect line LCS via DP — O(n·m) time/memory; refuses files where n·m > 64M cells (~4 Mi lines total) — for huge files use `diffutils`
- unified hunks with 3 lines of context, same hunk splitting as `diff -u`; no `--side-by-side`, no word-diff, no binary detection
- hunk bodies byte-identical to `diff -u` (verified); header lines intentionally carry no timestamps

## build

```sh
cargo build --release
```

## license

`MIT` — see `LICENSE`. built from zero.
