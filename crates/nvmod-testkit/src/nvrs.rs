// SPDX-License-Identifier: GPL-3.0-only
#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]
//! Fakes for the nv-rs install step, built in code: a tiny cargo workspace that stands in for the nv-rs source, a
//! fake release (an archive and its `SHA256SUMS`), a local HTTP server that counts requests, and shell scripts
//! that stand in for `gh` and `curl`. The fake `nv-viewer` answers `--check` like the real one (exit 0 when the game
//! can start, 1 when not) and prints the arguments it got. Nothing here is a real nv-rs file or game file.

// Test support: it builds fixtures with `tar` and `chmod`, which is not product code (ADR-0005).
#![allow(clippy::disallowed_methods)]

use crate::write_file;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// What the fake nv-rs binary prints, so a test can tell it ran.
pub const FAKE_BUILT_BANNER: &str = "fake nv-rs built from source";
/// The bytes of the binary inside the fake release.
pub const FAKE_RELEASE_BINARY: &[u8] = b"#!/bin/sh\necho fake nv-rs from a release \"$@\"\n";
/// The bytes of the binary inside a fake release whose `--check` fails.
pub const FAKE_FAILING_RELEASE_BINARY: &[u8] =
    b"#!/bin/sh\necho fake-check-refused \"$@\" >&2\nexit 1\n";
/// What the fake source-built `nv-viewer` prints on a failing `--check`.
pub const FAKE_CHECK_REFUSED: &str = "fake-check-refused";
/// The asset name the fake release uses.
pub const FAKE_ASSET: &str = "nv-rs-linux-x86_64-dev.tar.gz";

/// A tiny cargo workspace whose `nv-viewer` package builds a program that prints [`FAKE_BUILT_BANNER`].
#[derive(Debug, Clone, Default)]
pub struct FakeNvRs {
    broken: bool,
    failing_check: bool,
}

impl FakeNvRs {
    /// A workspace that builds.
    pub fn new() -> FakeNvRs {
        FakeNvRs::default()
    }

    /// A workspace whose build fails with a compile error that names `fake-compile-error`.
    #[must_use]
    pub fn failing_build(mut self) -> FakeNvRs {
        self.broken = true;
        self
    }

    /// A workspace whose `nv-viewer --check` exits 1 and prints [`FAKE_CHECK_REFUSED`].
    #[must_use]
    pub fn failing_check(mut self) -> FakeNvRs {
        self.failing_check = true;
        self
    }

    /// Write the workspace into `root`.
    pub fn write(&self, root: &Path) {
        write_file(
            root,
            "Cargo.toml",
            b"[workspace]\nmembers = [\"viewer\"]\nresolver = \"2\"\n",
        );
        write_file(
            root,
            "Cargo.lock",
            b"# synthetic\nversion = 4\n\n[[package]]\nname = \"nv-viewer\"\nversion = \"0.0.0\"\n",
        );
        write_file(
            root,
            "viewer/Cargo.toml",
            b"[package]\nname = \"nv-viewer\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        );
        let main = if self.broken {
            "fn main() { let fake-compile-error = ; }\n".to_owned()
        } else {
            let verdict = if self.failing_check {
                format!("eprintln!(\"{FAKE_CHECK_REFUSED}\"); std::process::exit(1);")
            } else {
                "println!(\"fake check passed {}\", args.join(\" \"));".to_owned()
            };
            format!(
                "fn main() {{ let args: Vec<String> = std::env::args().skip(1).collect(); \
                 if args.iter().any(|a| a == \"--check\") {{ {verdict} return; }} \
                 println!(\"{FAKE_BUILT_BANNER}\"); }}\n"
            )
        };
        write_file(root, "viewer/src/main.rs", main.as_bytes());
    }
}

/// A fake release folder: an archive holding a fake `nv-viewer` and a `SHA256SUMS` listing it.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // one switch per way a fake release can be wrong
pub struct FakeRelease {
    wrong_sum: bool,
    malformed_sums: bool,
    unlisted: bool,
    manifest: Option<String>,
    failing_check: bool,
    escaping_member: bool,
    capabilities: bool,
}

impl Default for FakeRelease {
    fn default() -> Self {
        FakeRelease::new()
    }
}

impl FakeRelease {
    /// A release whose checksum is right.
    pub fn new() -> FakeRelease {
        FakeRelease {
            wrong_sum: false,
            malformed_sums: false,
            unlisted: false,
            manifest: None,
            failing_check: false,
            escaping_member: false,
            capabilities: false,
        }
    }

    /// `SHA256SUMS` carries a wrong hash for the asset.
    #[must_use]
    pub fn wrong_checksum(mut self) -> FakeRelease {
        self.wrong_sum = true;
        self
    }

    /// `SHA256SUMS` is not a checksum list.
    #[must_use]
    pub fn malformed_sums(mut self) -> FakeRelease {
        self.malformed_sums = true;
        self
    }

    /// `SHA256SUMS` does not list the asset.
    #[must_use]
    pub fn asset_not_listed(mut self) -> FakeRelease {
        self.unlisted = true;
        self
    }

    /// The archive also carries this `release-manifest.json` text.
    #[must_use]
    pub fn with_manifest(mut self, text: &str) -> FakeRelease {
        self.manifest = Some(text.to_owned());
        self
    }

    /// The archive's `nv-viewer --check` exits 1.
    #[must_use]
    pub fn failing_check(mut self) -> FakeRelease {
        self.failing_check = true;
        self
    }

    /// The archive also carries a synthetic `capabilities.json` (one extender, 3 of 4 commands).
    #[must_use]
    pub fn with_capabilities(mut self) -> FakeRelease {
        self.capabilities = true;
        self
    }

    /// The archive holds a member whose path leaves the extraction folder.
    #[must_use]
    pub fn escaping_member(mut self) -> FakeRelease {
        self.escaping_member = true;
        self
    }

    /// Write the release into `dir` (`hash` computes the SHA-256 hex of bytes). Returns the asset's bytes.
    pub fn write(&self, dir: &Path, hash: fn(&[u8]) -> String) -> Vec<u8> {
        let work = dir.join(".pack");
        let binary = if self.failing_check {
            FAKE_FAILING_RELEASE_BINARY
        } else {
            FAKE_RELEASE_BINARY
        };
        write_file(&work, "nv-rs/nv-viewer", binary);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                work.join("nv-rs/nv-viewer"),
                std::fs::Permissions::from_mode(0o755),
            )
            .expect("make nv-viewer executable");
        }
        let mut members = vec!["nv-rs/nv-viewer"];
        if let Some(manifest) = &self.manifest {
            write_file(&work, "nv-rs/release-manifest.json", manifest.as_bytes());
            members.push("nv-rs/release-manifest.json");
        }
        if self.capabilities {
            write_file(
                &work,
                "nv-rs/capabilities.json",
                capabilities_json(&[("synthetic", "Synthetic", 3, 4)]).as_bytes(),
            );
            members.push("nv-rs/capabilities.json");
        }
        let mut cmd = Command::new("tar");
        cmd.arg("-czf")
            .arg(dir.join(FAKE_ASSET))
            .arg("-C")
            .arg(&work);
        if self.escaping_member {
            cmd.arg("--transform=s,^nv-rs/nv-viewer,../escaped,");
        }
        let status = cmd.args(&members).status().expect("run tar");
        assert!(status.success(), "tar failed");
        std::fs::remove_dir_all(&work).expect("remove pack folder");
        let bytes = std::fs::read(dir.join(FAKE_ASSET)).expect("read asset");
        let sums = if self.malformed_sums {
            "this is not a checksum list\n".to_owned()
        } else if self.unlisted {
            format!("{}  some-other-file.zip\n", hash(b"other"))
        } else if self.wrong_sum {
            format!("{}  {FAKE_ASSET}\n", hash(b"not the asset"))
        } else {
            format!("{}  {FAKE_ASSET}\n", hash(&bytes))
        };
        write_file(dir, "SHA256SUMS", sums.as_bytes());
        bytes
    }
}

/// Write an executable shell script `name` into `dir` and return its path.
///
/// The script is never written through a descriptor of this process. A test binary runs tests on several threads,
/// and a thread that forks while another still holds a file open for writing hands that descriptor to its child
/// until the child execs; running the file then fails with `ETXTBSY` ("Text file busy"). So the bytes go to a
/// plain staging file, and a short-lived `install` process copies it into place with the executable bit set. The
/// descriptor open for writing exists only in that process, which is gone when this function returns.
pub fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    write_executable(dir, name, format!("#!/bin/sh\n{body}\n").as_bytes())
}

/// Write the executable file `name` into `dir` with these bytes, without the `ETXTBSY` race (see [`write_script`]).
pub fn write_executable(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    let staging = dir.join(format!(".{name}.staging"));
    write_file(dir, &format!(".{name}.staging"), bytes);
    let status = Command::new("install")
        .args(["-m", "755"])
        .arg(&staging)
        .arg(&path)
        .status()
        .expect("run install");
    assert!(status.success());
    std::fs::remove_file(&staging).expect("remove the staging file");
    path
}

/// What the fake server answers to one request.
#[derive(Debug, Clone)]
pub enum Reply {
    /// Status 200 and this body.
    Body(Vec<u8>),
    /// This status and an empty body.
    Status(u16),
    /// Bytes that are not an HTTP response.
    Garbage,
    /// Headers promising 100 bytes, then 5 bytes and a closed connection.
    Truncated,
}

type Handler = dyn Fn(&str, usize) -> Reply + Send + Sync;

/// A local HTTP server that answers each request by calling the handler with the path and the number of earlier
/// requests for that path, and records every path asked for. Stops when dropped.
pub struct FakeServer {
    addr: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeServer {
    /// Start the server on a free local port.
    pub fn start(handler: impl Fn(&str, usize) -> Reply + Send + Sync + 'static) -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr").to_string();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let handler: Arc<Handler> = Arc::new(handler);
        let thread = {
            let (requests, stop) = (requests.clone(), stop.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        serve(stream, &requests, handler.as_ref());
                    }
                }
            })
        };
        FakeServer {
            addr,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    /// The base URL, without a trailing slash.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every path asked for so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("lock").clone()
    }

    /// How many times `path` was asked for.
    pub fn count(&self, path: &str) -> usize {
        self.requests().iter().filter(|p| *p == path).count()
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, requests: &Mutex<Vec<String>>, handler: &Handler) {
    let mut buf = [0u8; 4096];
    let mut head = Vec::new();
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
    }
    let text = String::from_utf8_lossy(&head);
    let path = text
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .trim_start_matches('/')
        .to_owned();
    let nth = {
        let mut seen = requests.lock().expect("lock");
        let nth = seen.iter().filter(|p| **p == path).count();
        seen.push(path.clone());
        nth
    };
    let reply = match handler(&path, nth) {
        Reply::Body(body) => {
            let mut out = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            out.extend(body);
            out
        }
        Reply::Status(code) => {
            format!("HTTP/1.1 {code} Fake\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .into_bytes()
        }
        Reply::Garbage => b"\x00\x01 this is not http \xff\xfe\r\n\r\n".to_vec(),
        Reply::Truncated => {
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort".to_vec()
        }
    };
    let _ = stream.write_all(&reply);
}

/// A synthetic `capabilities.json` of nv-rs's schema: per extender `(id, name, implemented, total)`, with made up
/// command names (`Command1`, `Command2`, ...) for the implemented ones.
pub fn capabilities_json(extenders: &[(&str, &str, u64, u64)]) -> String {
    let list: Vec<String> = extenders
        .iter()
        .map(|(id, name, implemented, total)| {
            let names: Vec<String> = (1..=*implemented).map(|n| format!("\"Command{n}\"")).collect();
            format!(
                "{{\"id\":\"{id}\",\"name\":\"{name}\",\"commands\":{{\"implemented\":{implemented},\"total\":{total},\"implemented_names\":[{}]}}}}",
                names.join(",")
            )
        })
        .collect();
    format!(
        "{{\"schema_version\":1,\"extenders\":[{}]}}",
        list.join(",")
    )
}
