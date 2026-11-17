// SPDX-License-Identifier: GPL-3.0-only
//! Stand-in programs for tests: a tiny compiled program that records its arguments, prints a fixed text, writes a
//! file below the folder an `-o<folder>` argument names, and exits with a fixed code. It is compiled with the
//! toolchain that builds the tests, so it starts wherever the tests run. A shell script does not start on Windows.
//!
//! Each behaviour is compiled once and kept in the temporary folder under a name made from a hash of its source, so
//! the tests of one run, and later runs, share it.

// Test support: it runs `rustc`, which is not product code (ADR-0005).
#![allow(clippy::disallowed_methods)]

use crate::write_executable;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

/// One compile at a time in this process; other processes are kept apart by the atomic rename.
static BUILD: Mutex<()> = Mutex::new(());
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// What a stand-in program does when it runs. By default it does nothing and exits 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Standin {
    log: bool,
    stdout: Option<String>,
    stderr: Option<String>,
    exit: i32,
    file: Option<(String, String)>,
}

impl Standin {
    /// A program that does nothing and exits 0.
    #[must_use]
    pub fn new() -> Standin {
        Standin::default()
    }

    /// Append the arguments, joined by spaces, as one line to `<name>.log` next to the program (`gh` and `gh.exe`
    /// both write `gh.log`), like `echo "$@" >> gh.log`.
    #[must_use]
    pub fn logging(mut self) -> Standin {
        self.log = true;
        self
    }

    /// Print `text`, a space and the arguments joined by spaces on standard output, like `echo text "$@"`.
    #[must_use]
    pub fn says(mut self, text: &str) -> Standin {
        self.stdout = Some(text.to_owned());
        self
    }

    /// Print `text`, a space and the arguments joined by spaces on standard error, like `echo text "$@" >&2`.
    #[must_use]
    pub fn complains(mut self, text: &str) -> Standin {
        self.stderr = Some(text.to_owned());
        self
    }

    /// Exit with `code`.
    #[must_use]
    pub fn exits(mut self, code: i32) -> Standin {
        self.exit = code;
        self
    }

    /// Write `contents` to `relative` below the folder named by the first `-o<folder>` argument (how `7z x`
    /// names its output folder), creating folders.
    #[must_use]
    pub fn extracts(mut self, relative: &str, contents: &str) -> Standin {
        self.file = Some((relative.to_owned(), contents.to_owned()));
        self
    }

    /// The Rust source of the program.
    fn source(&self) -> String {
        let mut src = String::from("// A test stand-in program made by nvmod-testkit.\n");
        let _ = writeln!(src, "const LOG: bool = {};", self.log);
        let _ = writeln!(src, "const OUT: Option<&str> = {:?};", self.stdout);
        let _ = writeln!(src, "const ERR: Option<&str> = {:?};", self.stderr);
        let _ = writeln!(src, "const EXIT: i32 = {};", self.exit);
        let _ = writeln!(src, "const FILE: Option<(&str, &str)> = {:?};", self.file);
        src.push_str(PROGRAM);
        src
    }

    /// The bytes of the compiled program.
    ///
    /// # Panics
    /// When `rustc` cannot compile it.
    #[must_use]
    pub fn program(&self) -> Vec<u8> {
        std::fs::read(build(&self.source())).expect("read the compiled stand-in")
    }

    /// Write the program into `dir` as `name` (with `.exe` on Windows) and return its path.
    ///
    /// # Panics
    /// When `rustc` cannot compile it or the file cannot be written.
    #[must_use = "the path is where the program was written"]
    pub fn write(&self, dir: &Path, name: &str) -> PathBuf {
        write_executable(
            dir,
            &format!("{name}{}", std::env::consts::EXE_SUFFIX),
            &self.program(),
        )
    }
}

const PROGRAM: &str = r#"
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let line = args.join(" ");
    if LOG {
        let log = std::env::current_exe().expect("own path").with_extension("log");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .expect("open the log");
        writeln!(file, "{line}").expect("write the log");
    }
    if let Some(text) = OUT {
        println!("{text} {line}");
    }
    if let Some(text) = ERR {
        eprintln!("{text} {line}");
    }
    if let Some((relative, contents)) = FILE {
        let dir = args.iter().find_map(|a| a.strip_prefix("-o")).expect("an -o<folder> argument");
        let path = std::path::Path::new(dir).join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create folders");
        std::fs::write(&path, contents).expect("write the file");
    }
    std::process::exit(EXIT);
}
"#;

/// `rustc` of the toolchain that runs cargo (next to `cargo`), else the one on `PATH`.
fn rustc() -> PathBuf {
    let sibling =
        Path::new(env!("CARGO")).with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    if sibling.is_file() {
        sibling
    } else {
        PathBuf::from("rustc")
    }
}

/// FNV-1a, 64 bits: enough to tell sources apart in a cache folder.
fn hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Compile `source` once and return the path of the program.
fn build(source: &str) -> PathBuf {
    let rustc = rustc();
    let key = format!("{source}\n{}", rustc.display());
    let cache = std::env::temp_dir().join("nvmod-testkit-standins");
    let program = cache.join(format!(
        "standin-{:016x}{}",
        hash(&key),
        std::env::consts::EXE_SUFFIX
    ));
    let _guard = BUILD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if program.is_file() {
        return program;
    }
    let work = cache.join(format!(
        "build-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).expect("create the build folder");
    let src = work.join("standin.rs");
    std::fs::write(&src, source).expect("write the stand-in source");
    let out = work.join(format!("standin{}", std::env::consts::EXE_SUFFIX));
    let result = Command::new(&rustc)
        .args([
            "--edition",
            "2021",
            "--crate-name",
            "standin",
            "-C",
            "debuginfo=0",
            "-o",
        ])
        .arg(&out)
        .arg(&src)
        .output()
        .unwrap_or_else(|e| panic!("run {}: {e}", rustc.display()));
    assert!(
        result.status.success(),
        "{} could not compile the stand-in:\n{}",
        rustc.display(),
        String::from_utf8_lossy(&result.stderr)
    );
    // Another test process may have put the same program in place first; either copy is the same program.
    if std::fs::rename(&out, &program).is_err() {
        assert!(
            program.is_file(),
            "move the stand-in to {}",
            program.display()
        );
    }
    let _ = std::fs::remove_dir_all(&work);
    program
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TempDir;

    #[test]
    fn a_standin_logs_prints_writes_and_exits_as_told() {
        let dir = TempDir::new("standin");
        let tool = Standin::new()
            .logging()
            .says("out")
            .complains("err")
            .extracts("Meshes/p.nif", "packed\n")
            .exits(7)
            .write(dir.path(), "tool");
        let target = dir.path().join("target");
        let target_arg = format!("-o{}", target.display());
        let run = Command::new(&tool)
            .args(["x", "-y", &target_arg, "two words"])
            .output()
            .unwrap();
        assert_eq!(run.status.code(), Some(7));
        let line = format!("x -y {target_arg} two words");
        assert_eq!(
            String::from_utf8(run.stdout).unwrap().trim_end(),
            format!("out {line}")
        );
        assert_eq!(
            String::from_utf8(run.stderr).unwrap().trim_end(),
            format!("err {line}")
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("tool.log")).unwrap(),
            format!("{line}\n")
        );
        assert_eq!(
            std::fs::read_to_string(target.join("Meshes/p.nif")).unwrap(),
            "packed\n"
        );
        // A second call appends to the log.
        Command::new(&tool).arg(&target_arg).output().unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("tool.log"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }

    #[test]
    fn the_default_standin_is_silent_and_one_program_is_compiled_once() {
        let dir = TempDir::new("standin-default");
        let tool = Standin::new().write(dir.path(), "quiet");
        let run = Command::new(&tool).arg("a").output().unwrap();
        assert_eq!(run.status.code(), Some(0));
        assert!(run.stdout.is_empty() && run.stderr.is_empty());
        assert!(!dir.path().join("quiet.log").exists());
        assert_eq!(Standin::new().program(), std::fs::read(&tool).unwrap());
        assert_ne!(Standin::new().source(), Standin::new().exits(1).source());
    }
}
