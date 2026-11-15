// SPDX-License-Identifier: GPL-3.0-only
//! Starting a program. [`Launcher`] is the seam: `nvmod run` builds a [`LaunchSpec`] and hands it over, so the
//! code that actually starts a process can move onto the `nvmod-runner` crate (nv-linux #75, ADR-0005) without
//! touching the plan. [`ProcessLauncher`] is the one implementation here and hands the spec to `nvmod-runner`.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Exactly what to start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchSpec {
    /// The program.
    pub program: String,
    /// Its arguments, one word each (never a shell line).
    pub args: Vec<String>,
    /// Variables set on top of the caller's environment.
    pub env: BTreeMap<String, String>,
    /// The folder to run in.
    pub working_dir: PathBuf,
}

/// A started program.
pub trait Running {
    /// Its process id.
    fn pid(&self) -> u32;
    /// Wait for it to end and return its exit code (128 plus the signal number when a signal ended it).
    ///
    /// # Errors
    /// Waiting failed.
    fn wait(self: Box<Self>) -> Result<i32, String>;
}

/// Starts programs.
pub trait Launcher {
    /// Start `spec`. With `attached` the program shares the caller's terminal; otherwise its input is closed and
    /// its output dropped, and it runs in its own process group so the caller's Ctrl+C does not reach it.
    ///
    /// # Errors
    /// The program could not be started.
    fn start(&self, spec: &LaunchSpec, attached: bool) -> Result<Box<dyn Running>, String>;
}

/// Starts the program through `nvmod-runner` (ADR-0005).
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessLauncher;

struct Child(nvmod_runner::Started);

impl Running for Child {
    fn pid(&self) -> u32 {
        self.0.pid()
    }

    fn wait(self: Box<Self>) -> Result<i32, String> {
        self.0.wait()
    }
}

impl Launcher for ProcessLauncher {
    fn start(&self, spec: &LaunchSpec, attached: bool) -> Result<Box<dyn Running>, String> {
        nvmod_runner::start_program(
            &spec.program,
            &spec.args,
            &spec.env,
            &spec.working_dir,
            attached,
        )
        .map(|c| Box::new(Child(c)) as Box<dyn Running>)
    }
}
