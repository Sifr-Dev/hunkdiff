//! sifr-diff — zero-dependency line diff.
//!   sifr-diff [a|-] [b|-]
//! Exit 0 = identical, 1 = different (like diff). Unified hunks with up to
//! 3 lines of context to each side; separated hunks when the gap exceeds
//! 6 unchanged lines. LCS via dynamic programming (files ≤ 4 Mi lines total).
use std::fs;
use std::io::{self, Read, Write};

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

fn compute_ops<'a>(a: &[&'a [u8]], b: &[&'a [u8]], dp: &[u32], m: usize) -> Vec<(u8, &'a [u8])> {
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

fn split_hunks<'a>(ops: &[(u8, &'a [u8])]) -> Vec<Vec<(u8, &'a [u8])>> {
    const CTX: usize = 3;
    let change_positions: Vec<usize> = ops.iter().enumerate().filter(|(_, (t, _))| *t != b' ').map(|(i, _)| i).collect();
    if change_positions.is_empty() {
        return Vec::new();
    }
    // group changes whose neighbouring context windows overlap
    let mut groups: Vec<(usize, usize)> = Vec::new(); // (first op idx, last op idx)
    let mut start = change_positions[0];
    let mut prev = change_positions[0];
    for &p in &change_positions[1..] {
        let unchanged_between = (p - prev - 1) as i64;
        if unchanged_between > (2 * CTX) as i64 {
            groups.push((start, prev));
            start = p;
        }
        prev = p;
    }
    groups.push((start, prev));
    // expand each group to include CTX context ops on each side
    groups.into_iter().map(|(s, e)| {
        let from = s.saturating_sub(CTX);
        let to = (e + CTX).min(ops.len() - 1);
        ops[from..=to].to_vec()
    }).collect()
}

fn hunk_header(before: &[(u8, &[u8])]) -> (usize, usize, usize, usize) {
    let mut a_start = 0usize;
    let mut b_start = 0usize;
    let mut found_a = false;
    let mut found_b = false;
    let mut a_count = 0usize;
    let mut b_count = 0usize;
    // line numbers: count ops before this hunk in the original ops stream is unknown here;
    // compute from the hunk position instead — caller passes nothing; compute 1-based from 0.
    // Instead: we track by counting in the caller. Here we just count tokens.
    for (tag, _) in before.iter() {
        match tag {
            b' ' => { a_count += 1; b_count += 1; }
            b'-' => a_count += 1,
            b'+' => b_count += 1,
            _ => {}
        }
        if !found_a && (*tag == b' ' || *tag == b'-') {
            // a_start assigned by caller via prefix counts — handled below
            found_a = true;
        }
        if !found_b && (*tag == b' ' || *tag == b'+') {
            found_b = true;
        }
    }
    let _ = (found_a, found_b, &mut a_start, &mut b_start);
    (a_start, a_count, b_start, b_count)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: sifr-diff [a|-] [b|-]");
        std::process::exit(2);
    }
    let read = |p: &str| -> Vec<u8> {
        if p == "-" {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf).expect("stdin read failed");
            buf
        } else {
            fs::read(p).unwrap_or_else(|e| { eprintln!("sifr-diff: cannot read {p}: {e}"); std::process::exit(2); })
        }
    };
    let a_data = read(&args[1]);
    let b_data = read(&args[2]);
    let a = read_lines(&a_data);
    let b = read_lines(&b_data);
    let (n, m) = (a.len(), b.len());
    if (n as u64) * (m as u64) > 64 * 1024 * 1024 {
        eprintln!("sifr-diff: too large ({}x{} cells)", n, m);
        std::process::exit(2);
    }
    let idx = |i: usize, j: usize| i * (m + 1) + j;
    let mut dp = vec![0u32; (n + 1) * (m + 1)];
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
        std::process::exit(0); // identical
    }
    // compute line offsets for each hunk by walking ops
    let mut line_a = 1usize; // 1-based
    let mut line_b = 1usize;
    let mut op_iter = ops.iter().peekable();
    let mut out = io::stdout().lock();
    let mut hi = 0usize;
    for (start, _) in hunks.iter().enumerate() {
        // advance line counters to the start of this hunk
        // easiest: recount by walking ops with an index
        let _ = (&mut line_a, &mut line_b, &mut op_iter);
        let _ = hi;
        hi += 1;
        let _ = start;
    }
    // single pass: emit with running line counters
    let mut pos = 0usize; // index into ops
    for hunk in &hunks {
        let first_op_global = ops.iter().position(|o| std::ptr::eq(o.1.as_ptr(), hunk[0].1.as_ptr()) && o.0 == hunk[0].0).unwrap_or(pos);
        // advance counters past everything before this hunk
        for k in pos..first_op_global {
            match ops[k].0 {
                b' ' => { line_a += 1; line_b += 1; }
                b'-' => line_a += 1,
                b'+' => line_b += 1,
                _ => {}
            }
        }
        pos = first_op_global;
        // hunk header
        let mut a_count = 0usize;
        let mut b_count = 0usize;
        for (tag, _) in hunk.iter() {
            match tag {
                b' ' => { a_count += 1; b_count += 1; }
                b'-' => a_count += 1,
                b'+' => b_count += 1,
                _ => {}
            }
        }
        writeln!(out, "@@ -{},{} +{},{} @@", line_a, a_count, line_b, b_count).expect("write failed");
        for (tag, line) in hunk.iter() {
            write!(out, "{}{}", *tag as char, String::from_utf8_lossy(line)).expect("write failed");
            if !line.ends_with(b"\n") {
                writeln!(out, "\\ No newline at end of file").expect("write failed");
            }
            match tag {
                b' ' => { line_a += 1; line_b += 1; }
                b'-' => line_a += 1,
                b'+' => line_b += 1,
                _ => {}
            }
        }
        pos += hunk.len();
    }
    std::process::exit(1);
}
