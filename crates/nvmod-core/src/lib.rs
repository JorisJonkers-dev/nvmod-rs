// SPDX-License-Identifier: GPL-3.0-only
//! Shared types of nvmod: how serious a finding is, what a finding looks like, the case-insensitive index of the
//! files that mods provide, the Instance and Profile data model with its schema, and the plain form of a Windows
//! path. No input or output happens here; the crates that read files fill these types.

pub mod cancel;
mod finding;
mod index;
mod model;
mod paths;

pub use finding::{Finding, ParseSeverityError, Severity};
pub use index::{FileIndex, Provider, normalize_path};
pub use model::{
    Executable, INSTANCE_SCHEMA_VERSION, Instance, LoadError, LoadedInstance,
    PROFILE_SCHEMA_VERSION, Profile, ProfileMod, Storefront, Target, instance_schema,
    load_instance, load_profile, profile_schema,
};
pub use paths::plain_path;
