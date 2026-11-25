#!/usr/bin/env python3
"""Independent unified-diff oracle for the differential test suite.

Requires Python 3.8+; no third-party packages. Run from any directory:
    python3 tests/differential.py path/to/hunkdiff
    python3 tests/differential.py path/to/hunkdiff --cases 5000 --seed 42

The random cases use the same deletion-first dynamic-programming policy as the
Rust implementation, but the DP matrix, parser, patch validator, and generators
are implemented independently in Python. GNU diff --text -U3 is checked for
semantic equivalence whenever available, and exact output matches are counted.
"""

import argparse
import os
import random
import shutil
import subprocess
import sys
import tempfile

ALPHABET = [
    b"A\n",
    b"B\n",
    b"C\n",
    b"D\n",
    "alpha\n".encode(),
    "β\n".encode(),
    "雪\n".encode(),
    "🙂\n".encode(),
    b"invalid:\xff\n",
    b"nul:\x00\n",
    b"carriage:\rreturn\n",
    b"vertical:\vtab\n",
    b"separator:\x1ctest\n",
    b"A",
    b"B",
    b"tail:\xff",
]


def split_lines(data):
    # Only LF delimits a line. bytes.splitlines also splits on several other
    # ASCII control bytes, so use an explicit scan instead.
    lines = []
    start = 0
    for index, byte in enumerate(data):
        if byte == 10:
            lines.append(data[start : index + 1])
            start = index + 1
    if start < len(data):
        lines.append(data[start:])
    return lines


def lcs_matrix(a, b):
    matrix = [[0] * (len(b) + 1) for _ in range(len(a) + 1)]
    for i in range(len(a) - 1, -1, -1):
        for j in range(len(b) - 1, -1, -1):
            matrix[i][j] = (
                matrix[i + 1][j + 1] + 1
                if a[i] == b[j]
                else max(matrix[i + 1][j], matrix[i][j + 1])
            )
    return matrix


def reference_ops(a, b, matrix):
    ops = []
    i = j = 0
    while i < len(a) or j < len(b):
        if i < len(a) and j < len(b) and a[i] == b[j]:
            ops.append((ord(" "), a[i]))
            i += 1
            j += 1
        elif i < len(a) and (j == len(b) or matrix[i + 1][j] >= matrix[i][j + 1]):
            ops.append((ord("-"), a[i]))
            i += 1
        else:
            ops.append((ord("+"), b[j]))
            j += 1
    return ops


def optimal_alignment_is_unique(a, b, matrix):
    """Count tie branches, saturation at two; valid for bounded test inputs."""
    suffix = [[0] * (len(b) + 1) for _ in range(len(a) + 1)]
    for i in range(len(a) + 1):
        suffix[i][len(b)] = 1
    for j in range(len(b) + 1):
        suffix[len(a)][j] = 1
    for i in range(len(a) - 1, -1, -1):
        for j in range(len(b) - 1, -1, -1):
            if a[i] == b[j]:
                suffix[i][j] = min(2, suffix[i + 1][j + 1])
            else:
                total = 0
                if matrix[i + 1][j] == matrix[i][j]:
                    total += suffix[i + 1][j]
                if matrix[i][j + 1] == matrix[i][j]:
                    total += suffix[i][j + 1]
                suffix[i][j] = min(2, total)
    return suffix[0][0] == 1


def hunk_ranges(ops):
    changes = [index for index, (tag, _) in enumerate(ops) if tag != ord(" ")]
    if not changes:
        return []
    groups = []
    start = previous = changes[0]
    for position in changes[1:]:
        if position - previous - 1 > 6:
            groups.append((start, previous))
            start = position
        previous = position
    groups.append((start, previous))
    return [
        (max(0, start - 3), min(len(ops), end + 4)) for start, end in groups
    ]


def format_range(start, count):
    return str(start) if count == 1 else f"{start},{count}"


def reference_output(a, b):
    a = split_lines(a)
    b = split_lines(b)
    matrix = lcs_matrix(a, b)
    ops = reference_ops(a, b, matrix)
    output = bytearray()
    position = 0
    line_a = line_b = 1
    for start, end in hunk_ranges(ops):
        for tag, _ in ops[position:start]:
            line_a += tag in (ord(" "), ord("-"))
            line_b += tag in (ord(" "), ord("+"))
        hunk = ops[start:end]
        a_count = sum(tag in (ord(" "), ord("-")) for tag, _ in hunk)
        b_count = sum(tag in (ord(" "), ord("+")) for tag, _ in hunk)
        a_start = line_a - 1 if a_count == 0 else line_a
        b_start = line_b - 1 if b_count == 0 else line_b
        output.extend(
            f"@@ -{format_range(a_start, a_count)} +{format_range(b_start, b_count)} @@\n".encode()
        )
        for tag, line in hunk:
            output.append(tag)
            output.extend(line)
            if not line.endswith(b"\n"):
                output.extend(b"\n\\ No newline at end of file\n")
            line_a += tag in (ord(" "), ord("-"))
            line_b += tag in (ord(" "), ord("+"))
        position = end
    return bytes(output), matrix[0][0], optimal_alignment_is_unique(a, b, matrix)


def parse_output(output, original_a, original_b, lcs_length):
    """Parse without borrowing the oracle's renderer; validate patch semantics."""
    a = split_lines(original_a)
    b = split_lines(original_b)
    parsed = []
    offset = 0
    while offset < len(output):
        end = output.find(b"\n", offset)
        assert end >= 0, "unterminated hunk header"
        header = output[offset:end]
        offset = end + 1
        assert header.startswith(b"@@ -") and header.endswith(b" @@"), header
        fields = header[4:-3].split(b" +")
        assert len(fields) == 2, header
        ranges = []
        for field in fields:
            parts = field.split(b",")
            assert len(parts) in (1, 2), header
            start = int(parts[0])
            count = int(parts[1]) if len(parts) == 2 else 1
            assert start >= 0 and count >= 0, header
            assert len(parts) == 1 or count != 1, "one-line count was not elided"
            ranges.append((start, count))
        entries = []
        while offset < len(output) and not output.startswith(b"@@ -", offset):
            assert output[offset : offset + 1] in (b" ", b"-", b"+"), output[offset:]
            tag = output[offset]
            offset += 1
            end = output.find(b"\n", offset)
            assert end >= 0, "unterminated diff line"
            line = output[offset : end + 1]
            offset = end + 1
            marker = False
            if output.startswith(b"\\ No newline at end of file\n", offset):
                marker = True
                offset += len(b"\\ No newline at end of file\n")
            entries.append((tag, line, marker))
        parsed.append((ranges, entries))
    assert offset == len(output)

    old_lines = []
    new_lines = []
    old_position = 0
    new_position = 0
    previous_trailing = 0
    for hunk_index, (ranges, entries) in enumerate(parsed):
        old_count = sum(tag in (ord(" "), ord("-")) for tag, _, _ in entries)
        new_count = sum(tag in (ord(" "), ord("+")) for tag, _, _ in entries)
        assert ranges[0][1] == old_count and ranges[1][1] == new_count
        assert ranges[0][0] + max(0, old_count - 1) <= len(a)
        assert ranges[1][0] + max(0, new_count - 1) <= len(b)
        assert entries and any(tag != ord(" ") for tag, _, _ in entries)
        leading = 0
        trailing = 0
        for tag, _, _ in entries:
            if tag == ord(" "):
                leading += 1
            else:
                break
        for tag, _, _ in reversed(entries):
            if tag == ord(" "):
                trailing += 1
            else:
                break
        assert leading <= 3 and trailing <= 3
        if hunk_index > 0:
            assert leading == 3
        if hunk_index < len(parsed) - 1:
            assert trailing == 3

        # With a nonempty range, start is the first consumed line (including
        # any leading context); never subtract that context again. An empty
        # side uses the other side's common context as its insertion anchor.
        old_prefix = ranges[0][0] - 1 if old_count else ranges[0][0]
        new_prefix = ranges[1][0] - 1 if new_count else ranges[1][0]
        if old_count == 0:
            old_prefix = new_prefix + leading
        if new_count == 0:
            new_prefix = old_prefix + leading
        assert old_prefix >= old_position and new_prefix >= new_position
        old_gap = a[old_position:old_prefix]
        new_gap = b[new_position:new_prefix]
        assert old_gap == new_gap
        if hunk_index > 0:
            assert len(old_gap) + previous_trailing + leading > 6, (
                "adjacent hunks should have merged"
            )
        old_lines.extend(old_gap)
        new_lines.extend(new_gap)
        old_position = old_prefix
        new_position = new_prefix
        for tag, line, marker in entries:
            # Output always terminates a displayed line. Use the source bytes
            # to determine whether that LF came from input or patch formatting.
            source = b[new_position] if tag == ord("+") else a[old_position]
            assert marker == (not source.endswith(b"\n"))
            if marker:
                line = line[:-1]
            if tag == ord(" "):
                assert a[old_position] == line == b[new_position]
                old_position += 1
                new_position += 1
            elif tag == ord("-"):
                assert a[old_position] == line
                old_position += 1
            else:
                assert b[new_position] == line
                new_position += 1
        assert old_position - old_prefix == old_count
        assert new_position - new_prefix == new_count
        old_lines.extend(a[old_prefix:old_position])
        new_lines.extend(b[new_prefix:new_position])
        previous_trailing = trailing
    old_lines.extend(a[old_position:])
    new_lines.extend(b[new_position:])
    assert old_lines == a, "old side did not reconstruct"
    assert new_lines == b, "new side did not reconstruct"
    # Count unchanged lines by replaying the parsed edit script over the full
    # reconstructed inputs; omitted hunk context is part of the LCS too.
    common = len(a) - sum(
        tag == ord("-") for _, entries in parsed for tag, _, _ in entries
    )
    assert common == lcs_length, (common, lcs_length)
    assert bool(parsed) == (a != b)


def gnu_body(path_a, path_b):
    env = dict(os.environ, LC_ALL="C")
    result = subprocess.run(
        ["diff", "--text", "-U3", "--label", "a", "--label", "b", path_a, path_b],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        check=False,
    )
    assert result.returncode in (0, 1) and not result.stderr
    if not result.stdout:
        return b""
    assert result.stdout.startswith(b"--- a\n+++ b\n")
    return b"\n".join(result.stdout.split(b"\n")[2:])


def check_case(binary, directory, index, a, b, use_gnu):
    path_a = os.path.join(directory, "a")
    path_b = os.path.join(directory, "b")
    with open(path_a, "wb") as stream:
        stream.write(a)
    with open(path_b, "wb") as stream:
        stream.write(b)
    result = subprocess.run([binary, path_a, path_b], capture_output=True, check=False)
    expected, lcs_length, unique = reference_output(a, b)
    assert result.returncode == (0 if a == b else 1), (index, a, b, result)
    assert result.stderr == b"", (index, result.stderr)
    assert result.stdout == expected, (index, a, b, result.stdout, expected)
    parse_output(result.stdout, a, b, lcs_length)
    repeated = subprocess.run([binary, path_a, path_b], capture_output=True, check=False)
    assert (repeated.returncode, repeated.stdout, repeated.stderr) == (
        result.returncode,
        result.stdout,
        result.stderr,
    ), (index, "nondeterministic result")
    exact_gnu = 0
    if use_gnu:
        expected_gnu = gnu_body(path_a, path_b)
        assert result.returncode == (0 if not expected_gnu else 1)
        # GNU's backtracking can select a different occurrence of a repeated
        # new-side line even when the optimal LCS itself is unique. Both outputs
        # must be valid optimal patches; exact GNU equality is reported, not
        # imposed as an incorrect universal tie-break requirement.
        parse_output(expected_gnu, a, b, lcs_length)
        exact_gnu = result.stdout == expected_gnu
    return unique, exact_gnu


def regression_cases():
    return [
        (b"", b""),
        (b"", b"x\n"),
        (b"x\n", b""),
        (b"x\n", b"x\ny\n"),
        (b"x\n1\n2\n3\n4\n", b"x\n1\n2\n3\n4\ny\n"),
        (b"x\n", b"y\nx\n"),
        (b"A\nB\n", b"B\nA\n"),
        (b"x", b"x\n"),
        (b"x\n", b"x"),
        (b"x", b"y"),
        (b"old\nend", b"new\nend"),
        (b"a\xff", b"a\xfe\n"),
        (b"nul:\x00old\n", b"nul:\x00new\n"),
        ("α\nβ\n".encode(), "α\nγ\n".encode()),
        ("🙂\n雪\n".encode(), "🙂\n雨\n".encode()),
        (b"a\n1\n2\n3\n4\n5\n6\nb\n", b"A\n1\n2\n3\n4\n5\n6\nB\n"),
        (b"a\n1\n2\n3\n4\n5\n6\n7\nb\n", b"A\n1\n2\n3\n4\n5\n6\n7\nB\n"),
        (b"x\n" * 20 + b"old\n" + b"x\n" * 20, b"x\n" * 20 + b"new\n" + b"x\n" * 20),
    ]


def random_case(rng):
    length_a = rng.randrange(0, 25)
    length_b = rng.randrange(0, 25)
    a = [rng.choice(ALPHABET) for _ in range(length_a)]
    if rng.randrange(3) == 0:
        b = a[:]
        for _ in range(rng.randrange(0, 8)):
            operation = rng.randrange(3)
            position = rng.randrange(len(b) + 1)
            if operation == 0 and b:
                del b[min(position, len(b) - 1)]
            elif operation == 1:
                b.insert(position, rng.choice(ALPHABET))
            elif b:
                b[min(position, len(b) - 1)] = rng.choice(ALPHABET)
    else:
        b = [rng.choice(ALPHABET) for _ in range(length_b)]
    if a and rng.randrange(4) == 0:
        a[-1] = a[-1].removesuffix(b"\n")
    if b and rng.randrange(4) == 0:
        b[-1] = b[-1].removesuffix(b"\n")
    return b"".join(a), b"".join(b)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary")
    parser.add_argument("--cases", type=int, default=300)
    parser.add_argument("--seed", type=int, default=20261120)
    args = parser.parse_args()
    if args.cases < 0:
        parser.error("--cases must be non-negative")
    binary = os.path.abspath(args.binary)
    rng = random.Random(args.seed)
    use_gnu = shutil.which("diff") is not None
    cases = regression_cases() + [random_case(rng) for _ in range(args.cases)]
    unique = exact_gnu = 0
    with tempfile.TemporaryDirectory(prefix="hunkdiff-oracle-") as directory:
        for index, (a, b) in enumerate(cases):
            is_unique, matches_gnu = check_case(binary, directory, index, a, b, use_gnu)
            unique += is_unique
            exact_gnu += matches_gnu
    print(
        f"PASS: {len(cases)} cases ({len(cases) - len(regression_cases())} random, "
        f"seed {args.seed}); {unique} uniquely aligned; "
        f"GNU semantic checks {len(cases) if use_gnu else 0}, "
        f"exact hunk-body matches {exact_gnu}; each output validated twice"
    )


if __name__ == "__main__":
    try:
        main()
    except AssertionError as error:
        print(f"FAIL: {error!r}", file=sys.stderr)
        raise
