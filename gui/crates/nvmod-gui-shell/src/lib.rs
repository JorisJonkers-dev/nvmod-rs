// SPDX-License-Identifier: GPL-3.0-only
//! The toolkit-free part of the nvmod GUI shell.
//!
//! * [`theme`]: the Mojave Dusk and Bleach tokens.
//! * [`setup`]: the setup wizard screen as plain data ([`setup::SetupView`]), driven by `nvmod-ui-model`'s
//!   `SetupWizard` and by any `nvmod-ui-service` `Service`.
//!
//! The GPUI crate and the egui canary both paint exactly this, so changing the toolkit changes only painting.

#![allow(clippy::must_use_candidate, clippy::module_name_repetitions)]
#![cfg_attr(test, allow(clippy::assert_is_empty))]

pub mod setup;
pub mod theme;
