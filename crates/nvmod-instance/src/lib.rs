// SPDX-License-Identifier: GPL-3.0-only
//! The nvmod Instance on disk: which storefront a base copy of the game came from, and the skeleton folder an
//! Instance lives in. The base copy is only ever read here; an Instance is a separate folder (docs/DECISIONS.md).
//! The Instance and Profile types themselves are owned by `nvmod-core`.

mod instance;
mod launch;
mod masters;
mod profile_name;
mod storefront;

pub use instance::{INSTANCE_FILE, InstanceError, KIND_MIGRATE, NvmodInstance, PROFILE_FILE};
pub use launch::{DEFAULT_PROTON, GAME_EXE, LaunchEntry, LaunchPlan, NVSE_LOADER, launch_plan};
pub use masters::{BASE_MASTERS, base_masters_present};
pub use nvmod_core::Storefront;
pub use profile_name::check_profile_name;
pub use storefront::{detect_storefront, find_ci};
