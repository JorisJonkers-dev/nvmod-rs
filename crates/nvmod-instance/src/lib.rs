// SPDX-License-Identifier: GPL-3.0-only
//! The nvmod Instance: which storefront a base copy of the game came from, and the skeleton folder an Instance
//! lives in. The base copy is only ever read here; an Instance is a separate folder (docs/DECISIONS.md).

mod instance;
mod storefront;

pub use instance::{Config, INSTANCE_FILE, InstanceError, NvmodInstance};
pub use storefront::{Storefront, find_ci};
