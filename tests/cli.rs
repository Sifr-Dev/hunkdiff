use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "hunkdiff-cli-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self { directory }
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.directory.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn binary() -> &'static OsStr {
    OsStr::new(env!("CARGO_BIN_EXE_hunkdiff"))
}

fn invoke<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(binary()).args(args).output().unwrap()
}

fn invoke_stdin(a: &Path, b: &Path, input: &[u8]) -> Output {
    let mut child = Command::new(binary())
        .arg(a)
        .arg(b)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn argument_count_and_io_errors_exit_two() {
    for arguments in [vec![], vec![OsStr::new("only-one")]] {
        let output = invoke(arguments);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, b"");
        assert_eq!(output.stderr, b"usage: hunkdiff [a|-] [b|-]\n");
    }
    let fixture = Fixture::new();
    let file = fixture.file("file", b"same\n");
    let output = invoke([file.as_os_str(), file.as_os_str(), OsStr::new("extra")]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"usage: hunkdiff [a|-] [b|-]\n");

    let output = invoke([fixture.path("missing").as_os_str(), file.as_os_str()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("hunkdiff: cannot read "), "{stderr:?}");
    assert!(stderr.contains("missing"), "{stderr:?}");

    let output = invoke([fixture.directory.as_os_str(), file.as_os_str()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .starts_with("hunkdiff: cannot read "));
}

#[test]
fn stdin_and_two_dashes_follow_documented_policy() {
    let fixture = Fixture::new();
    let old = fixture.file("old", b"old\ncommon\n");
    let new = fixture.file("new", b"new\ncommon\n");
    let expected = invoke([old.as_os_str(), new.as_os_str()]);
    assert_eq!(expected.status.code(), Some(1));

    let actual = invoke_stdin(&old, OsStr::new("-").as_ref(), b"new\ncommon\n");
    assert_eq!(actual.status.code(), expected.status.code());
    assert_eq!(actual.stdout, expected.stdout);
    assert_eq!(actual.stderr, b"");

    let output = invoke_stdin(
        OsStr::new("-").as_ref(),
        OsStr::new("-").as_ref(),
        b"arbitrary\n",
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
}

#[test]
fn output_is_byte_exact_and_deterministic() {
    let fixture = Fixture::new();
    let old = fixture.file("old", b"same\nold\xff");
    let new = fixture.file("new", b"same\nnew\xfe\n");
    let expected = b"@@ -1,2 +1,2 @@\n same\n-old\xff\n\\ No newline at end of file\n+new\xfe\n";
    for _ in 0..8 {
        let output = invoke([old.as_os_str(), new.as_os_str()]);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, b"");
    }
}

#[test]
fn dp_limit_accepts_exact_rectangular_matrix_and_rejects_larger() {
    let fixture = Fixture::new();
    let old = fixture.file("old", &vec![b'\n'; 32 * 1024 * 1024 - 1]);
    let one_line = fixture.file("one-line", b"\n");
    let two_lines = fixture.file("two-lines", b"\n\n");

    // 33,554,431 LF bytes form n lines; with m=1, (n+1)(m+1) is exact.
    assert_eq!(32 * 1024 * 1024 * 2, 67_108_864);
    let output = invoke([old.as_os_str(), one_line.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.starts_with(b"@@ -1,33554431 +1 @@\n"));
    assert_eq!(output.stderr, b"");

    // Increasing m from one to two exceeds the cell budget.
    assert_eq!(32 * 1024 * 1024 * 3, 67_108_864 + 32 * 1024 * 1024);
    let output = invoke([old.as_os_str(), two_lines.as_os_str()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        output.stderr,
        b"hunkdiff: too large (100663296 DP cells; limit is 67108864)\n"
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_missing_paths_are_supported_and_reported() {
    use std::os::unix::ffi::OsStringExt;

    let fixture = Fixture::new();
    let name = std::ffi::OsString::from_vec(b"bad-\xff".to_vec());
    let file = fixture.file("other", b"x\n");
    let missing = fixture.directory.join(&name);
    let output = invoke([missing.as_os_str(), file.as_os_str()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert!(String::from_utf8_lossy(&output.stderr).contains("bad-�"));
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn non_utf8_file_paths_are_supported() {
    use std::os::unix::ffi::OsStringExt;

    // macOS can reject these names with EILSEQ at the filesystem boundary.
    let fixture = Fixture::new();
    let old = fixture
        .directory
        .join(std::ffi::OsString::from_vec(b"old-\xff".to_vec()));
    let new = fixture
        .directory
        .join(std::ffi::OsString::from_vec(b"new-\xfe".to_vec()));
    fs::write(&old, b"old\n").unwrap();
    fs::write(&new, b"new\n").unwrap();
    let output = invoke([old.as_os_str(), new.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"@@ -1 +1 @@\n-old\n+new\n");
}
