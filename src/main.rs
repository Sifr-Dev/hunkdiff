//! sifr-diff — zero-dependency, byte-preserving line diff.
//!   sifr-diff [a|-] [b|-]
//! Exit 0 = identical, 1 = different, 2 = error. Unified hunks have three
//! lines of context on each side and use deterministic deletion-first LCS ties.
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::ops::Range;
use std::process::ExitCode;

const CONTEXT: usize = 3;
const MAX_DP_CELLS: usize = 64 * 1024 * 1024;

type Op<'a> = (u8, &'a [u8]);

fn read_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            out.push(&bytes[start..i + 1]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push(&bytes[start..]);
    }
    out
}

fn checked_dp_cells(n: usize, m: usize) -> Option<usize> {
    n.checked_add(1)?.checked_mul(m.checked_add(1)?)
}

fn compute_ops<'a>(a: &[&'a [u8]], b: &[&'a [u8]], dp: &[u32], m: usize) -> Vec<Op<'a>> {
    let (n, idx) = (a.len(), |i: usize, j: usize| i * (m + 1) + j);
    let mut ops = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            ops.push((b' ', a[i]));
            i += 1;
            j += 1;
            continue;
        }
        // A deletion on an equal-value choice is part of the output contract.
        if i < n && (j >= m || dp[idx(i + 1, j)] >= dp[idx(i, j + 1)]) {
            ops.push((b'-', a[i]));
            i += 1;
            continue;
        }
        ops.push((b'+', b[j]));
        j += 1;
    }
    ops
}

fn split_hunks(ops: &[Op<'_>]) -> Vec<Range<usize>> {
    let change_positions: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, (tag, _))| *tag != b' ')
        .map(|(i, _)| i)
        .collect();
    if change_positions.is_empty() {
        return Vec::new();
    }

    // Six common lines are shared by adjacent context windows; seven split.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = change_positions[0];
    let mut prev = change_positions[0];
    for &position in &change_positions[1..] {
        let unchanged_between = position - prev - 1;
        if unchanged_between > 2 * CONTEXT {
            groups.push((start, prev));
            start = position;
        }
        prev = position;
    }
    groups.push((start, prev));

    groups
        .into_iter()
        .map(|(start, end)| Range {
            start: start.saturating_sub(CONTEXT),
            end: (end + CONTEXT + 1).min(ops.len()),
        })
        .collect()
}

fn advance_position(tag: u8, line_a: &mut usize, line_b: &mut usize) {
    match tag {
        b' ' => {
            *line_a += 1;
            *line_b += 1;
        }
        b'-' => *line_a += 1,
        b'+' => *line_b += 1,
        _ => unreachable!("invalid diff operation"),
    }
}

fn write_range(out: &mut impl Write, start: usize, count: usize) -> io::Result<()> {
    // GNU unified syntax omits the count for a one-line range. For an empty
    // range, start is the line immediately before the insertion/deletion point.
    if count == 1 {
        write!(out, "{start}")
    } else {
        write!(out, "{start},{count}")
    }
}

fn write_unified(out: &mut impl Write, ops: &[Op<'_>], hunks: &[Range<usize>]) -> io::Result<()> {
    let mut position = 0usize;
    let mut line_a = 1usize; // next line number, not the last consumed line
    let mut line_b = 1usize;

    for hunk in hunks {
        for (tag, _) in &ops[position..hunk.start] {
            advance_position(*tag, &mut line_a, &mut line_b);
        }

        let mut a_count = 0usize;
        let mut b_count = 0usize;
        for (tag, _) in &ops[hunk.clone()] {
            match tag {
                b' ' => {
                    a_count += 1;
                    b_count += 1;
                }
                b'-' => a_count += 1,
                b'+' => b_count += 1,
                _ => unreachable!("invalid diff operation"),
            }
        }
        let a_start = if a_count == 0 { line_a - 1 } else { line_a };
        let b_start = if b_count == 0 { line_b - 1 } else { line_b };

        write!(out, "@@ -")?;
        write_range(out, a_start, a_count)?;
        write!(out, " +")?;
        write_range(out, b_start, b_count)?;
        writeln!(out, " @@")?;

        for (tag, line) in &ops[hunk.clone()] {
            out.write_all(&[*tag])?;
            out.write_all(line)?;
            if !line.ends_with(b"\n") {
                out.write_all(b"\n\\ No newline at end of file\n")?;
            }
            advance_position(*tag, &mut line_a, &mut line_b);
        }
        position = hunk.end;
    }
    out.flush()
}

fn read_input(path: &OsStr) -> io::Result<Vec<u8>> {
    if path == OsStr::new("-") {
        let mut buffer = Vec::new();
        io::stdin().lock().read_to_end(&mut buffer)?;
        Ok(buffer)
    } else {
        fs::read(path)
    }
}

fn run(args: Vec<OsString>) -> Result<bool, String> {
    if args.len() != 3 {
        return Err("usage: sifr-diff [a|-] [b|-]".to_owned());
    }

    let both_stdin = args[1] == OsStr::new("-") && args[2] == OsStr::new("-");
    let a_data = read_input(&args[1]).map_err(|error| {
        format!(
            "sifr-diff: cannot read {}: {error}",
            args[1].to_string_lossy()
        )
    })?;
    let b_data = if both_stdin {
        None
    } else {
        Some(read_input(&args[2]).map_err(|error| {
            format!(
                "sifr-diff: cannot read {}: {error}",
                args[2].to_string_lossy()
            )
        })?)
    };

    let a = read_lines(&a_data);
    let b = match &b_data {
        Some(data) => read_lines(data),
        None => a.clone(), // Like GNU diff, "- -" compares the same stream.
    };
    let (n, m) = (a.len(), b.len());
    let cells = checked_dp_cells(n, m)
        .ok_or_else(|| "sifr-diff: too large (line-count overflow)".to_owned())?;
    if cells > MAX_DP_CELLS {
        return Err(format!(
            "sifr-diff: too large ({cells} DP cells; limit is {MAX_DP_CELLS})"
        ));
    }

    let idx = |i: usize, j: usize| i * (m + 1) + j;
    let mut dp = vec![0u32; cells];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[idx(i, j)] = if a[i] == b[j] {
                dp[idx(i + 1, j + 1)] + 1
            } else {
                dp[idx(i + 1, j)].max(dp[idx(i, j + 1)])
            };
        }
    }

    let ops = compute_ops(&a, &b, &dp, m);
    let hunks = split_hunks(&ops);
    if hunks.is_empty() {
        return Ok(false);
    }
    write_unified(&mut io::stdout().lock(), &ops, &hunks)
        .map_err(|error| format!("sifr-diff: write failed: {error}"))?;
    Ok(true)
}

fn main() -> ExitCode {
    match run(std::env::args_os().collect()) {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff<'a>(a: &'a [u8], b: &'a [u8]) -> (Vec<Op<'a>>, Vec<Range<usize>>) {
        let a = read_lines(a);
        let b = read_lines(b);
        let cells = checked_dp_cells(a.len(), b.len()).unwrap();
        let idx = |i: usize, j: usize| i * (b.len() + 1) + j;
        let mut dp = vec![0u32; cells];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                dp[idx(i, j)] = if a[i] == b[j] {
                    dp[idx(i + 1, j + 1)] + 1
                } else {
                    dp[idx(i + 1, j)].max(dp[idx(i, j + 1)])
                };
            }
        }
        let ops = compute_ops(&a, &b, &dp, b.len());
        let hunks = split_hunks(&ops);
        (ops, hunks)
    }

    fn render(a: &[u8], b: &[u8]) -> Vec<u8> {
        let (ops, hunks) = diff(a, b);
        let mut output = Vec::new();
        write_unified(&mut output, &ops, &hunks).unwrap();
        output
    }

    #[test]
    fn equal_lcs_choices_delete_first() {
        let (ops, _) = diff(b"A\nB\n", b"B\nA\n");
        let actual: Vec<(u8, &[u8])> = ops.into_iter().collect();
        assert_eq!(
            actual,
            vec![
                (b'-', &b"A\n"[..]),
                (b' ', &b"B\n"[..]),
                (b'+', &b"A\n"[..])
            ]
        );
    }

    #[test]
    fn hunk_context_merges_at_six_and_splits_at_seven() {
        let (_, merged) = diff(b"a\n1\n2\n3\n4\n5\n6\nb\n", b"A\n1\n2\n3\n4\n5\n6\nB\n");
        assert_eq!(merged, vec![0..10]);
        let (_, split) = diff(
            b"a\n1\n2\n3\n4\n5\n6\n7\nb\n",
            b"A\n1\n2\n3\n4\n5\n6\n7\nB\n",
        );
        assert_eq!(split, vec![0..5, 6..11]);
    }

    #[test]
    fn empty_sides_and_single_lines_use_gnu_ranges() {
        assert_eq!(render(b"", b"x\n"), b"@@ -0,0 +1 @@\n+x\n");
        assert_eq!(render(b"x\n", b""), b"@@ -1 +0,0 @@\n-x\n");
        assert_eq!(render(b"", b""), b"");
    }

    #[test]
    fn unterminated_and_non_utf8_lines_are_preserved() {
        assert_eq!(
            render(b"a\xff", b"a\xfe\n"),
            b"@@ -1 +1 @@\n-a\xff\n\\ No newline at end of file\n+a\xfe\n"
        );
    }

    #[test]
    fn unchanged_unterminated_context_gets_marker() {
        assert_eq!(
            render(b"old\nend", b"new\nend"),
            b"@@ -1,2 +1,2 @@\n-old\n+new\n end\n\\ No newline at end of file\n"
        );
    }

    #[test]
    fn dp_cell_arithmetic_is_checked() {
        assert_eq!(checked_dp_cells(0, 0), Some(1));
        assert_eq!(checked_dp_cells(usize::MAX, 1), None);
        assert_eq!(checked_dp_cells(8_191, 8_191), Some(MAX_DP_CELLS));
        assert!(checked_dp_cells(8_192, 8_192).unwrap() > MAX_DP_CELLS);
        assert!(checked_dp_cells(255, 255).unwrap() <= MAX_DP_CELLS);
    }
}
