// SPDX-License-Identifier: GPL-3.0-only
//! Running the game and tools through an Instance's staged view: the executables registry, the Instance lock,
//! the launch plan and the seam a launcher plugs into. No program starts except through a [`Launcher`].

mod launch;
mod lock;
mod plan;
mod registry;

pub use launch::{LaunchSpec, Launcher, ProcessLauncher, Running};
pub use lock::{Holder, InstanceLock, LOCK_FILE, LockError, find_process, mo2_running, pid_alive};
pub use plan::{
    ENTRY_GAME, ENTRY_NV_RS, GAME_APP_ID, Platform, Request, RunPlan, StagingAction, data_dir,
    is_secret_name, plan, plan_staging, split_arguments, wine_path,
};
pub use registry::{Imported, Patch, RESERVED, add, edit, import_mo2, position, remove};
