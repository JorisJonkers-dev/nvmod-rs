// SPDX-License-Identifier: GPL-3.0-only
//! The Doctor job, called in process on synthetic fixtures: a base copy, an nvmod Instance and a Mod Organizer 2
//! instance. Plugins and executables are synthetic headers built by the testkit; no game data is used.

use nvmod_api::doctor::{CheckOutput, CheckRequest, OPEN_REFUSAL, Report, Severity, Target};
use nvmod_api::{CancelToken, ErrorKind, JobError, NoProgress, Outcome};
use nvmod_instance::NvmodInstance;
use nvmod_testkit::{
    BaseBuilder, Mo2Builder, ModSpec, PeBuilder, PluginBuilder, TempDir, snapshot, write_file,
};
use std::fs;
use std::path::PathBuf;

const MASTERLIST: &str = r"
globals:
  - {type: say, content: 'Masterlist for tests'}
";

/// The job name of a Doctor refusal or failure that is not about the Instance file's schema.
const CHECK: &str = "doctor.check";

fn run(target: Target, fail_on: Severity) -> Result<Outcome<CheckOutput>, JobError> {
    nvmod_api::doctor::check(
        &CheckRequest { target, fail_on },
        &mut NoProgress,
        &CancelToken::new(),
    )
}

fn done(target: Target, fail_on: Severity) -> CheckOutput {
    run(target, fail_on)
        .expect("the check runs")
        .done()
        .expect("not cancelled")
}

fn refused(target: Target) -> JobError {
    run(target, Severity::Error).expect_err("refused")
}

/// An nvmod Instance with a Default profile, staged, and a masterlist beside it. No Mods.
struct Instance {
    _base: TempDir,
    inst: TempDir,
}

impl Instance {
    fn new() -> Self {
        let base = TempDir::new("doctor-base");
        BaseBuilder::steam().write(base.path());
        write_file(base.path(), "nvse_1_4.dll", &PeBuilder::dll().build());
        write_file(
            base.path(),
            "Data/FalloutNV.esm",
            &PluginBuilder::master().build(),
        );
        let inst = TempDir::new("doctor-inst");
        let root = inst.path().join("fnv");
        let instance = NvmodInstance::create(&root, base.path()).expect("create the Instance");
        instance
            .create_profile("Default")
            .expect("create the profile");
        nvmod_stage::stage(&instance, "Default", nvmod_stage::Options::default()).expect("stage");
        fs::write(inst.path().join("masterlist.yaml"), MASTERLIST).expect("masterlist");
        Self { _base: base, inst }
    }

    fn root(&self) -> PathBuf {
        self.inst.path().join("fnv")
    }

    fn masterlist(&self) -> PathBuf {
        self.inst.path().join("masterlist.yaml")
    }

    fn target(&self) -> Target {
        Target::Instance {
            folder: self.root(),
            profile: None,
            game: None,
            masterlist: Some(self.masterlist()),
            catalog: None,
        }
    }
}

/// A Mod Organizer 2 instance with a Default and an Other profile, Default selected. Its JIP plugin has a
/// Nexus id that does not match its mod folder, so the Doctor has a warning to report.
fn mo2() -> (TempDir, TempDir) {
    let game = TempDir::new("doctor-game");
    write_file(
        game.path(),
        "FalloutNV.exe",
        &PeBuilder::exe()
            .versions([1, 4, 0, 525], [1, 4, 0, 525])
            .large_address_aware(true)
            .import("nvse_steam_loader.dll")
            .build(),
    );
    write_file(game.path(), "FalloutNV_backup.exe", b"b");
    write_file(
        game.path(),
        "nvse_1_4.dll",
        &PeBuilder::dll()
            .versions([6, 4, 0, 0], [6, 4, 0, 0])
            .build(),
    );
    let root = TempDir::new("doctor-mo2");
    Mo2Builder::new(game.path())
        .mod_dir(ModSpec::new("JIP LN NVSE Plugin").file(
            "NVSE/Plugins/jip_nvse.dll",
            &PeBuilder::nvse_plugin().nvse_version(1, 3).build(),
        ))
        .mod_dir(ModSpec::new("MAC-TEN").nexus(10, "14.5"))
        .profile(
            "Default",
            &["+JIP LN NVSE Plugin", "+MAC-TEN"],
            &["FalloutNV.esm"],
        )
        .profile("Other", &[], &[])
        .selected("Default")
        .write(root.path());
    (game, root)
}

fn mo2_target(game: &TempDir, root: &TempDir) -> Target {
    Target::Instance {
        folder: root.path().to_path_buf(),
        profile: None,
        game: Some(game.path().to_path_buf()),
        masterlist: None,
        catalog: None,
    }
}

#[test]
fn a_base_copy_is_checked_and_reported_without_writing() {
    let dir = TempDir::new("doctor-steam");
    BaseBuilder::steam().write(dir.path());
    let before = snapshot(dir.path());
    let out = done(
        Target::Base {
            folder: dir.path().to_path_buf(),
        },
        Severity::Error,
    );
    let Report::Base(report) = &out.report else {
        panic!("a base copy is reported as a base copy");
    };
    assert!(report.exe.is_some(), "the executable is found");
    assert!(!out.found, "{:?}", report.findings);
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn an_nvmod_instance_is_checked_and_reported_as_an_instance() {
    let s = Instance::new();
    let out = done(s.target(), Severity::Error);
    let Report::Instance(report) = &out.report else {
        panic!("an nvmod Instance is reported as one");
    };
    assert!(report.target.is_some(), "the Instance's target is reported");
    assert!(!out.found, "{:?}", report.findings);
}

#[test]
fn a_mod_organizer_instance_is_checked_and_the_fail_on_severity_decides_found() {
    let (game, root) = mo2();
    let at_error = done(mo2_target(&game, &root), Severity::Error);
    assert!(matches!(at_error.report, Report::Mo2(_)));
    assert!(!at_error.found, "only warnings: below the default error");
    let at_warning = done(mo2_target(&game, &root), Severity::Warning);
    assert!(at_warning.found, "a warning is found at --fail-on warning");
    assert!(
        at_warning.report.to_text().contains("meta.modid-mismatch"),
        "{}",
        at_warning.report.to_text()
    );
}

#[test]
fn a_base_copy_the_doctor_refuses_is_a_refusal_with_copyable_text() {
    let dir = TempDir::new("doctor-epic");
    BaseBuilder::epic().write(dir.path());
    let error = refused(Target::Base {
        folder: dir.path().to_path_buf(),
    });
    assert_eq!(error.kind, ErrorKind::Refused);
    assert_eq!(error.job, CHECK);
    assert_ne!(error.summary, "");
    assert!(error.copy_text().contains("job: doctor.check"));
}

#[test]
fn a_base_folder_that_is_not_a_folder_is_a_refusal() {
    let dir = TempDir::new("doctor-gone");
    let error = refused(Target::Base {
        folder: dir.path().join("missing"),
    });
    assert_eq!(error.kind, ErrorKind::Refused);
    assert!(error.summary.contains("is not a folder"), "{error}");
}

#[test]
fn an_instance_file_from_a_newer_schema_is_refused_under_the_open_job() {
    let s = Instance::new();
    write_file(
        &s.root(),
        "nvmod-instance.json",
        br#"{"schema_version":3,"base":"/x","storefront":"steam","target":"vanilla"}"#,
    );
    let error = refused(s.target());
    assert_eq!(error.kind, ErrorKind::Refused);
    assert_eq!(error.job, OPEN_REFUSAL);
    assert!(error.summary.contains("newer"), "{error}");
}

#[test]
fn an_instance_file_that_cannot_be_read_is_a_failure_not_a_refusal() {
    let s = Instance::new();
    write_file(&s.root(), "nvmod-instance.json", b"not json");
    let error = run(s.target(), Severity::Error).expect_err("fails");
    assert_eq!(error.kind, ErrorKind::Failed);
    assert_eq!(error.job, CHECK);
}

#[test]
fn a_catalogue_that_cannot_be_read_is_a_refusal_naming_the_file() {
    let s = Instance::new();
    let target = Target::Instance {
        folder: s.root(),
        profile: None,
        game: None,
        masterlist: None,
        catalog: Some(s.inst.path().join("missing.toml")),
    };
    let error = refused(target);
    assert_eq!(error.kind, ErrorKind::Refused);
    assert_eq!(error.job, CHECK);
    assert!(error.summary.contains("missing.toml"), "{error}");
}

#[test]
fn an_instance_profile_it_does_not_have_is_a_refusal() {
    let s = Instance::new();
    let target = Target::Instance {
        folder: s.root(),
        profile: Some("Nope".to_owned()),
        game: None,
        masterlist: None,
        catalog: None,
    };
    let error = refused(target);
    assert_eq!(error.kind, ErrorKind::Refused);
    assert!(error.summary.contains("Nope"), "{error}");
}

#[test]
fn a_folder_that_is_not_a_mod_organizer_instance_is_a_failure() {
    let dir = TempDir::new("doctor-none");
    let error = run(
        Target::Instance {
            folder: dir.path().to_path_buf(),
            profile: None,
            game: None,
            masterlist: None,
            catalog: None,
        },
        Severity::Error,
    )
    .expect_err("fails");
    assert_eq!(error.kind, ErrorKind::Failed);
    assert!(error.summary.contains("Mod Organizer 2"), "{error}");
}

#[test]
fn a_mod_organizer_instance_without_a_selected_profile_is_a_failure() {
    let dir = TempDir::new("doctor-no-profile");
    write_file(dir.path(), "ModOrganizer.ini", b"[General]\r\n");
    let error = run(
        Target::Instance {
            folder: dir.path().to_path_buf(),
            profile: None,
            game: None,
            masterlist: None,
            catalog: None,
        },
        Severity::Error,
    )
    .expect_err("fails");
    assert_eq!(error.kind, ErrorKind::Failed);
    assert!(error.summary.contains("no selected profile"), "{error}");
}

#[test]
fn a_mod_organizer_profile_name_that_is_not_a_folder_name_is_a_failure() {
    let (game, root) = mo2();
    let target = Target::Instance {
        folder: root.path().to_path_buf(),
        profile: Some("../x".to_owned()),
        game: Some(game.path().to_path_buf()),
        masterlist: None,
        catalog: None,
    };
    let error = run(target, Severity::Error).expect_err("fails");
    assert_eq!(error.kind, ErrorKind::Failed);
    assert!(
        error.summary.contains("not a profile folder name"),
        "{error}"
    );
}

#[test]
fn a_check_whose_cancel_is_set_before_the_call_stops_at_once_and_changes_nothing() {
    let s = Instance::new();
    let before = snapshot(s.inst.path());
    let cancel = CancelToken::new();
    cancel.cancel();
    let request = CheckRequest {
        target: s.target(),
        fail_on: Severity::Error,
    };
    let outcome = nvmod_api::doctor::check(&request, &mut NoProgress, &cancel).expect("no error");
    assert!(matches!(outcome, Outcome::Cancelled));
    assert_eq!(snapshot(s.inst.path()), before);
}

#[test]
fn a_check_cancelled_during_its_work_stops_before_the_next_step_and_changes_nothing() {
    let s = Instance::new();
    let before = snapshot(s.inst.path());
    let cancel = CancelToken::new();
    let request = CheckRequest {
        target: s.target(),
        fail_on: Severity::Error,
    };
    // The first step starts the work; the stop is asked for from inside that first event.
    let mut events = Vec::new();
    let mut progress = |event: &nvmod_api::Event| {
        events.push(format!("{event:?}"));
        cancel.cancel();
    };
    let outcome = nvmod_api::doctor::check(&request, &mut progress, &cancel).expect("no error");
    assert!(matches!(outcome, Outcome::Cancelled), "cancelled mid-run");
    assert_eq!(events.len(), 1, "only the first step ran: {events:?}");
    assert_eq!(snapshot(s.inst.path()), before, "nothing was written");
}

#[test]
fn a_request_without_fail_on_takes_the_error_severity() {
    let request: CheckRequest =
        serde_json::from_str(r#"{"target": {"kind": "base", "folder": "/b"}}"#).unwrap();
    assert_eq!(request.fail_on, Severity::Error);
    assert_eq!(
        request.target,
        Target::Base {
            folder: PathBuf::from("/b")
        }
    );
}

#[test]
fn a_check_request_round_trips_through_json() {
    let request = CheckRequest {
        target: Target::Instance {
            folder: PathBuf::from("/x"),
            profile: Some("Default".to_owned()),
            game: Some(PathBuf::from("/g")),
            masterlist: None,
            catalog: Some(PathBuf::from("/c.toml")),
        },
        fail_on: Severity::Warning,
    };
    let text = serde_json::to_string(&request).unwrap();
    let back: CheckRequest = serde_json::from_str(&text).unwrap();
    assert_eq!(back, request);
}
