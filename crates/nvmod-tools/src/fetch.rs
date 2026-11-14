// SPDX-License-Identifier: GPL-3.0-only
//! Fetching a URL behind a trait. The real client shells out to `curl`, which every supported system has and which
//! gives `https`, redirects, a hard timeout and `file://` without a TLS stack in nvmod (`docs/DECISIONS.md`).

use nvmod_runner::{Call, NETWORK_ENV, Runner, ToolError};
use std::path::PathBuf;
use std::time::Duration;

/// The time one request may take.
pub const TIMEOUT_SECS: u32 = 60;
/// How often a failed request is tried again after the first try.
pub const RETRIES: u32 = 3;
/// The largest response accepted.
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Why a request gave no usable answer.
#[derive(Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The answer is not a well-formed response (not HTTP, or cut short). Never retried.
    Malformed(String),
    /// The server or the network failed or timed out. Retried.
    Transient(String),
    /// Retrying cannot help (the client program is missing, the URL is not usable).
    Fatal(String),
}

/// Something that fetches the bytes behind a URL.
pub trait Fetcher {
    /// Fetch `url`.
    ///
    /// # Errors
    /// See [`FetchError`].
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError>;
}

/// The `curl` command line as a fetcher.
#[derive(Debug, Clone)]
pub struct Curl {
    /// The `curl` program.
    pub program: PathBuf,
}

impl Curl {
    fn args(url: &str) -> Vec<String> {
        [
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--proto",
            "=http,https,file",
            "--proto-redir",
            "=http,https",
            "--max-time",
            &TIMEOUT_SECS.to_string(),
            "--max-filesize",
            &MAX_BYTES.to_string(),
            url,
        ]
        .map(str::to_owned)
        .to_vec()
    }
}

impl Fetcher for Curl {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        // curl has its own `--max-time`; the runner's timeout only backs it up.
        let runner = Runner {
            timeout: Duration::from_secs(u64::from(TIMEOUT_SECS) + 30),
            max_output: usize::try_from(MAX_BYTES).unwrap_or(usize::MAX),
            ..Runner::default()
        }
        .inherit(NETWORK_ENV)
        .inherit(&["HOME"]);
        let args = Self::args(url);
        let out = runner
            .run_any(&Call::new("curl", &self.program, &args))
            .map_err(|e| match e {
                ToolError::Failed { reason, .. } if reason.starts_with("cannot start") => {
                    FetchError::Fatal(reason)
                }
                other => FetchError::Transient(format!("{url}: curl: {other}")),
            })?;
        if out.exit_code == 0 {
            return Ok(out.stdout);
        }
        let why = format!("{url}: curl exit {}: {}", out.exit_code, out.stderr.trim());
        Err(match Some(out.exit_code) {
            // 1 unsupported or unparsable reply, 8 weird reply, 18 partial file, 61 bad encoding.
            Some(1 | 8 | 18 | 61) => FetchError::Malformed(why),
            // 3 bad URL, 37 a file:// that cannot be read.
            Some(3 | 37) => FetchError::Fatal(why),
            _ => FetchError::Transient(why),
        })
    }
}

/// Fetch `url`, trying again up to [`RETRIES`] times after a transient failure, waiting `delay` between tries.
///
/// # Errors
/// The last error. A malformed or fatal error ends the attempts at once.
pub fn get_with_retries(
    fetcher: &dyn Fetcher,
    url: &str,
    delay: Duration,
) -> Result<Vec<u8>, FetchError> {
    let mut tries = 0;
    loop {
        match fetcher.get(url) {
            Err(FetchError::Transient(why)) if tries < RETRIES => {
                tries += 1;
                let _ = why;
                std::thread::sleep(delay);
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Scripted(Cell<u32>, fn(u32) -> Result<Vec<u8>, FetchError>);

    impl Fetcher for Scripted {
        fn get(&self, _: &str) -> Result<Vec<u8>, FetchError> {
            let n = self.0.get();
            self.0.set(n + 1);
            (self.1)(n)
        }
    }

    #[test]
    fn transient_failures_are_retried_three_times() {
        let f = Scripted(Cell::new(0), |_| Err(FetchError::Transient("x".into())));
        assert!(get_with_retries(&f, "u", Duration::ZERO).is_err());
        assert_eq!(f.0.get(), 4);
    }

    #[test]
    fn malformed_and_fatal_errors_are_not_retried() {
        let f = Scripted(Cell::new(0), |_| Err(FetchError::Malformed("x".into())));
        assert!(matches!(
            get_with_retries(&f, "u", Duration::ZERO),
            Err(FetchError::Malformed(_))
        ));
        assert_eq!(f.0.get(), 1);
        let f = Scripted(Cell::new(0), |_| Err(FetchError::Fatal("x".into())));
        assert!(get_with_retries(&f, "u", Duration::ZERO).is_err());
        assert_eq!(f.0.get(), 1);
    }

    #[test]
    fn a_success_after_failures_is_returned() {
        let f = Scripted(Cell::new(0), |n| {
            if n < 3 {
                Err(FetchError::Transient("x".into()))
            } else {
                Ok(vec![1])
            }
        });
        assert_eq!(get_with_retries(&f, "u", Duration::ZERO), Ok(vec![1]));
    }

    #[test]
    fn the_curl_command_line_has_the_timeout() {
        let args = Curl::args("http://h/x").join(" ");
        assert!(args.contains("--max-time 60"), "{args}");
        assert!(args.ends_with("http://h/x"));
    }
}
