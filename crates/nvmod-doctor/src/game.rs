// SPDX-License-Identifier: GPL-3.0-only
//! Checks of the game folder: the executable, the xNVSE files and the launch target. Read only; the 4GB state is
//! detected and advised on, never changed (docs/DECISIONS.md, Q6).

use crate::SUPPORTED_GAME_VERSION;
use crate::report::{ExeReport, GameReport};
use nvmod_core::{Finding, Severity};
use nvmod_formats::Mo2Instance;
use nvmod_pe::inspect_file;
use std::path::{Path, PathBuf};

const EXE: &str = "FalloutNV.exe";
const BACKUP: &str = "FalloutNV_backup.exe";
const LOADER: &str = "nvse_loader.exe";
const STEAM_LOADER: &str = "nvse_steam_loader.dll";
const NVSE_DLL: &str = "nvse_1_4.dll";

/// The file in `dir` called `name` ignoring case.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

pub(crate) fn check(
    instance: &Mo2Instance,
    game_dir: Option<&Path>,
    findings: &mut Vec<Finding>,
) -> GameReport {
    let mut game = GameReport::default();
    let Some(dir) = game_dir.or(instance.game_path.as_deref()) else {
        findings.push(Finding::new(
            "game.path-unusable",
            Severity::Warning,
            "gamePath",
            "the instance's game folder is not set or not on this machine; pass --game <dir> to check the game files",
        ));
        return game;
    };
    if !dir.is_dir() {
        findings.push(Finding::new(
            "game.path-unusable",
            Severity::Warning,
            dir.display().to_string(),
            "the game folder does not exist",
        ));
        return game;
    }
    game.path = Some(dir.display().to_string());

    match find_file(dir, EXE) {
        None => findings.push(Finding::new(
            "game.exe-missing",
            Severity::Error,
            EXE,
            "not found in the game folder",
        )),
        Some(path) => game.exe = Some(check_exe(dir, &path, findings)),
    }
    match find_file(dir, NVSE_DLL) {
        None => findings.push(Finding::new(
            "nvse.dll-missing",
            Severity::Error,
            NVSE_DLL,
            "xNVSE is not installed in the game folder: no NVSE plugin can load",
        )),
        Some(path) => match inspect_file(&path) {
            Ok(report) => game.xnvse_version = report.file_version,
            Err(why) => findings.push(Finding::new(
                "nvse.dll-unreadable",
                Severity::Warning,
                NVSE_DLL,
                why.to_string(),
            )),
        },
    }
    game.nvse_loader_present = find_file(dir, LOADER).is_some();
    game
}

fn check_exe(dir: &Path, path: &Path, findings: &mut Vec<Finding>) -> ExeReport {
    let file = path
        .file_name()
        .map_or_else(|| EXE.to_owned(), |n| n.to_string_lossy().into_owned());
    let backup_present = find_file(dir, BACKUP).is_some();
    let mut exe = ExeReport {
        file,
        file_version: None,
        large_address_aware: false,
        imports_nvse_steam_loader: false,
        backup_present,
    };
    let report = match inspect_file(path) {
        Ok(report) => report,
        Err(why) => {
            findings.push(Finding::new(
                "game.exe-unreadable",
                Severity::Error,
                EXE,
                why.to_string(),
            ));
            return exe;
        }
    };
    exe.file_version = report.file_version;
    exe.large_address_aware = report.large_address_aware;
    exe.imports_nvse_steam_loader = report
        .imports
        .iter()
        .any(|i| i.eq_ignore_ascii_case(STEAM_LOADER));
    if !report.is_x86_32bit() {
        findings.push(Finding::new(
            "game.exe-not-32-bit",
            Severity::Error,
            EXE,
            "not a 32-bit x86 executable",
        ));
    }
    match report.file_version {
        Some(v) if v != SUPPORTED_GAME_VERSION => findings.push(Finding::new(
            "game.exe-version",
            Severity::Warning,
            EXE,
            format!("version {v}; the xNVSE this tool knows targets {SUPPORTED_GAME_VERSION}"),
        )),
        Some(_) => {}
        None => findings.push(Finding::new(
            "game.exe-version-unknown",
            Severity::Info,
            EXE,
            "the executable has no readable version resource",
        )),
    }
    if report.large_address_aware {
        if !backup_present {
            findings.push(Finding::new(
                "game.no-backup",
                Severity::Warning,
                EXE,
                format!("the executable is large address aware (patched) but {BACKUP} is not next to it; keep an unpatched copy"),
            ));
        }
    } else {
        findings.push(Finding::new(
            "game.not-4gb",
            Severity::Warning,
            EXE,
            "not large address aware: the game is limited to 2 GiB; a 4GB patch is advised (nvmod only detects it for now)",
        ));
    }
    exe
}

/// Whether anything launches xNVSE: an executable entry for the loader, or an executable that links the Steam stub.
pub(crate) fn launch_target(instance: &Mo2Instance, game: &GameReport) -> Vec<Finding> {
    let launches_loader = instance.executables.iter().any(|e| {
        e.binary
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(LOADER))
    });
    let mut findings = Vec::new();
    if launches_loader && game.path.is_some() && !game.nvse_loader_present {
        findings.push(Finding::new(
            "nvse.loader-missing",
            Severity::Error,
            LOADER,
            "an MO2 executable launches the loader but it is not in the game folder",
        ));
    }
    let steam_stub = game
        .exe
        .as_ref()
        .is_some_and(|e| e.imports_nvse_steam_loader);
    if launches_loader || steam_stub {
        return findings;
    }
    findings.push(match &game.exe {
        None => Finding::new(
            "launch.unchecked",
            Severity::Info,
            "launch target",
            "the game executable could not be read, so it is not known whether xNVSE is started",
        ),
        Some(_) => Finding::new(
            "launch.no-loader",
            Severity::Warning,
            "launch target",
            format!("no MO2 executable launches {LOADER} and {EXE} does not import {STEAM_LOADER}: started as it is, no plugin loads"),
        ),
    });
    findings
}
