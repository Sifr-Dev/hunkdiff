# Release notes and checklist

## 0.1.1

This patch release makes the canonical `hunkdiff` executable name consistent
across the Cargo package, CLI diagnostics, tests, documentation, CI, and release
artifact. Diff behavior and exit statuses are unchanged from the verified 0.1.0
implementation. The `v0.1.0` source archive predates this naming correction and
must not be used for the new package recipe.

The remaining verification notes are retained below as the project's release
checklist. No further version bump is implied.

## Historical verification notes

- Preserve arbitrary input bytes in unified output; invalid UTF-8 is no longer
  replaced by U+FFFD.
- Correct no-newline marker placement, zero/one-count unified ranges, hunk
  relocation, extra-operand rejection, and stdin/read/write error handling.
- Check `(n + 1) * (m + 1)` with overflow-safe arithmetic before allocation and
  document the actual 64 Mi-cell / 256 MiB DP-matrix limit.
- Specify deterministic deletion-first LCS ties and add regression, CLI,
  randomized differential, structural patch, and GNU differential tests.
- Add Linux/macOS/Windows CI, architecture, compatibility, and limits docs.
- Use `hunkdiff` consistently for the repository, Cargo package, executable, and
  release artifact. Existing diff behavior and exit statuses are unchanged.

No package-version bump or release is claimed by this verification branch.
Maintainers should update `Cargo.toml`, `Cargo.lock`, and this file together
when selecting a release version.

## Checklist

1. Ensure the intended base branch and release version are explicitly approved.
2. Run the commands in `docs/verification.md` and require the CI workflow to pass.
3. Build `cargo build --release --locked`; smoke-test equal, different, empty,
   unterminated, and non-UTF-8 files and confirm exit statuses 0/1/2.
4. Confirm release notes describe the fixed DP-cell limit, output compatibility
   subset, lack of file headers, and resource requirements.
5. Publish the binary/source archive using the project's normal release process;
   do not infer a tag, default-branch change, or merge from this checklist.
