// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod-gui`: the first window, the setup wizard on a scripted service.
//!
//! `--smoke[=MS]` quits by itself after MS milliseconds (default 3000), for the launch check of CI.

use nvmod_gui_gpui::run;
use nvmod_gui_shell::setup::{SetupShell, demo_request, demo_service};
use std::time::Duration;

fn main() {
    let smoke = std::env::args().skip(1).find_map(|a| {
        let rest = a.strip_prefix("--smoke")?;
        let ms = rest
            .strip_prefix('=')
            .map_or(3000, |n| n.parse().unwrap_or(3000));
        Some(Duration::from_millis(ms))
    });
    run(SetupShell::new(demo_service(), demo_request()), smoke);
}
