// SPDX-License-Identifier: GPL-3.0-only
//! Shared types of nvmod: how serious a finding is, what a finding looks like, the case-insensitive index of the
//! files that mods provide, and the Instance and Profile data model with its schema. No input or output happens
//! here; the crates that read files fill these types.

mod finding;
mod index;
mod model;

pub use finding::{Finding, ParseSeverityError, Severity};
pub use index::{FileIndex, Provider, normalize_path};
pub use model::{
    INSTANCE_SCHEMA_VERSION, Instance, LoadError, LoadedInstance, PROFILE_SCHEMA_VERSION, Profile,
    ProfileMod, Storefront, Target, instance_schema, load_instance, load_profile, profile_schema,
};
