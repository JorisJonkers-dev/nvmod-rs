// SPDX-License-Identifier: GPL-3.0-only
//! The setup wizard screen (`docs/design/gui/` mockups 03 to 06): what to show, as plain data, and what a press
//! of the primary button does.
//!
//! The wizard of `nvmod-ui-model` is fed the event stream of `nvmod setup --json`. Here the stream comes from a
//! [`Service`]: the service runs the setup and returns the report, and the report is replayed as the same events
//! the command line prints, one step at a time, so the screen shows a run the way a live job would. When the
//! CLI-subprocess service streams its lines live (ADR-0005), only [`SetupShell::start`] changes.

use nvmod_ui_model::error::{Retry, ViewError};
use nvmod_ui_model::setup::{Applied, Finish, FinishStatus, Next, Phase, SetupWizard, StepState};
use nvmod_ui_service::dto::SetupReport;
use nvmod_ui_service::{FakeService, Service, ServiceError, SetupRequest};
use serde_json::{Value, json};
use std::collections::VecDeque;

/// How a state or a message is toned (colour never carries meaning alone: a word always goes with it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Success.
    Ok,
    /// The current step and the primary action.
    Accent,
    /// Not reached yet.
    Faint,
    /// Skipped or neutral.
    Muted,
    /// Failure.
    Err,
}

/// One step of the plan as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRow {
    /// The step name (`init`, `nv-rs`, ...).
    pub name: String,
    /// What it does, in words.
    pub description: String,
    /// The state in words ("Done", "Running", "To do", ...).
    pub state: &'static str,
    /// The tone of the state word.
    pub tone: Tone,
    /// The step's output, when it has one.
    pub detail: Option<String>,
}

/// The error card: cause, effect and the command that reproduces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorCard {
    /// What went wrong.
    pub what: String,
    /// What changed or did not.
    pub effect: String,
    /// The exit class in words.
    pub class: &'static str,
    /// The command that reproduces it.
    pub repro: Option<String>,
    /// A hint when trying again needs a fix first.
    pub hint: Option<String>,
}

/// The primary button: the one thing to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Primary {
    /// The label.
    pub label: &'static str,
    /// The plain-words help line under it.
    pub help: &'static str,
}

/// The whole screen.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupView {
    /// The small tracked line above the title.
    pub eyebrow: &'static str,
    /// The page heading.
    pub title: &'static str,
    /// One plain sentence on what the page is for.
    pub lead: &'static str,
    /// The steps, in plan order (empty before the plan is known).
    pub steps: Vec<StepRow>,
    /// "3 of 6", for the live region.
    pub progress_text: String,
    /// 0.0 to 1.0.
    pub progress: f32,
    /// A status line with its tone, for the foot of the steps list.
    pub status: (String, Tone),
    /// The primary button.
    pub primary: Primary,
    /// The error card, when the run did not succeed.
    pub error: Option<ErrorCard>,
    /// The `nvmod` command that does the same.
    pub command: String,
}

/// What a press of the primary button asked of the shell's host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// The shell handled it (started, advanced, retried).
    Handled,
    /// Setup is finished and the person chose to leave: the host shows Home.
    GoHome,
}

/// The setup screen's controller: a wizard, the events waiting to be shown and the service that makes them.
pub struct SetupShell<S: Service> {
    service: S,
    request: SetupRequest,
    wizard: SetupWizard,
    queue: VecDeque<String>,
    error: Option<ViewError>,
}

impl<S: Service> SetupShell<S> {
    /// A shell that has not started.
    pub fn new(service: S, request: SetupRequest) -> Self {
        Self {
            service,
            request,
            wizard: SetupWizard::new(),
            queue: VecDeque::new(),
            error: None,
        }
    }

    /// The wizard, for tests and adapters that want more than the view.
    pub fn wizard(&self) -> &SetupWizard {
        &self.wizard
    }

    /// The service.
    pub fn service(&self) -> &S {
        &self.service
    }

    /// Asks the service to run the setup and queues its events. A failed call becomes the error card.
    pub fn start(&mut self) {
        self.wizard = SetupWizard::new();
        self.queue.clear();
        self.error = None;
        match self.service.setup(&self.request) {
            Ok(report) => {
                self.queue = events_of(&self.request, &report).into();
                // Show the plan first (the first line); the steps follow one press at a time.
                if let Some(plan) = self.queue.pop_front() {
                    self.apply(&plan);
                }
            }
            Err(e) => self.error = Some(e.view_error()),
        }
    }

    /// Shows events up to and including the next step that ends (done, skipped or failed) or the end of the run.
    pub fn advance(&mut self) {
        while let Some(line) = self.queue.pop_front() {
            let ends = ends_a_step(&line);
            self.apply(&line);
            if ends {
                break;
            }
        }
        // The end of the run belongs to the last step: no extra press for it.
        let only_the_end = self.queue.len() == 1
            && self
                .queue
                .front()
                .is_some_and(|l| l.contains("\"finished\""));
        if only_the_end && let Some(last) = self.queue.pop_front() {
            self.apply(&last);
        }
    }

    fn apply(&mut self, line: &str) {
        // A line the wizard drops is stale or unknown; the stream is ours, so this is a bug if it happens.
        let applied = self.wizard.feed_line(line);
        debug_assert_eq!(applied, Ok(Applied::Yes), "{line}");
    }

    /// The primary button was pressed.
    pub fn press_primary(&mut self) -> Pressed {
        if self.error.is_some() || matches!(self.wizard.phase(), Phase::Waiting) {
            self.start();
            return Pressed::Handled;
        }
        if !self.queue.is_empty() {
            self.advance();
            return Pressed::Handled;
        }
        if self.wizard.next() == Next::Done {
            return Pressed::GoHome;
        }
        self.start();
        Pressed::Handled
    }

    /// The screen as it stands now.
    pub fn view(&self) -> SetupView {
        let (done, total) = self.wizard.progress();
        let progress = if total == 0 {
            0.0
        } else {
            // Step counts are tiny.
            #[allow(clippy::cast_precision_loss)]
            let f = done as f32 / total as f32;
            f
        };
        let steps = self
            .wizard
            .steps()
            .iter()
            .map(|s| {
                let (state, tone) = match s.state {
                    StepState::Pending => ("To do", Tone::Faint),
                    StepState::Planned => ("Planned", Tone::Faint),
                    StepState::Running => ("Running", Tone::Accent),
                    StepState::Done => ("Done", Tone::Ok),
                    StepState::Skipped => ("Skipped", Tone::Muted),
                    StepState::Failed => ("Failed", Tone::Err),
                };
                StepRow {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    state,
                    tone,
                    detail: s.detail.clone(),
                }
            })
            .collect();
        let error = self
            .error
            .clone()
            .or_else(|| self.wizard.error())
            .map(|e| card(&e));
        SetupView {
            eyebrow: "SETUP",
            title: "Set up your game",
            lead: "nvmod makes a copy of your game that it can change safely, and installs the engine and your mods into it. Your original game is never touched.",
            steps,
            progress_text: self.wizard.progress_text(),
            progress,
            status: self.status(error.is_some()),
            primary: self.primary(error.is_some()),
            error,
            command: self.command(),
        }
    }

    fn status(&self, failed: bool) -> (String, Tone) {
        if failed {
            return ("Setup did not finish.".to_owned(), Tone::Err);
        }
        match self.wizard.phase() {
            Phase::Waiting => ("Ready when you are.".to_owned(), Tone::Muted),
            Phase::Planned => (
                "The plan is ready. Nothing has run yet.".to_owned(),
                Tone::Accent,
            ),
            Phase::Running => (
                format!("{} steps finished.", self.wizard.progress_text()),
                Tone::Accent,
            ),
            Phase::Finished(Finish { status, .. }) => match status {
                FinishStatus::Ok => ("Setup finished.".to_owned(), Tone::Ok),
                FinishStatus::DryRun => (
                    "Dry run finished. Nothing was written.".to_owned(),
                    Tone::Ok,
                ),
                _ => ("Setup stopped.".to_owned(), Tone::Muted),
            },
        }
    }

    fn primary(&self, failed: bool) -> Primary {
        if failed {
            return Primary {
                label: "Try again",
                help: "Runs setup again from the start.",
            };
        }
        if !self.queue.is_empty() {
            return Primary {
                label: "Run next step",
                help: "Runs the next step and shows what it did.",
            };
        }
        match self.wizard.next() {
            Next::Nothing if matches!(self.wizard.phase(), Phase::Waiting) => Primary {
                label: "Start setup",
                help: "Reads your game folder and shows the plan. Nothing is written yet.",
            },
            Next::Nothing => Primary {
                label: "Run next step",
                help: "Runs the next step and shows what it did.",
            },
            Next::Done => Primary {
                label: "Open Home",
                help: "Your game is set up. Go to the main menu.",
            },
            Next::RunForReal => Primary {
                label: "Run for real",
                help: "Runs the same plan and writes the copy.",
            },
            Next::ResumeOrRestart => Primary {
                label: "Resume setup",
                help: "Continues where it stopped; the finished steps stay finished.",
            },
            Next::ChangeInputs => Primary {
                label: "Change inputs",
                help: "Setup refused to start. Fix what the message names.",
            },
            Next::StartAgain => Primary {
                label: "Start again",
                help: "Begins a new setup.",
            },
        }
    }

    fn command(&self) -> String {
        if matches!(self.wizard.phase(), Phase::Waiting) {
            let args = self.request.args().join(" ");
            format!("nvmod setup {args}")
        } else {
            self.wizard.equivalent_command()
        }
    }
}

fn card(e: &ViewError) -> ErrorCard {
    ErrorCard {
        what: e.what.clone(),
        effect: e.effect.clone(),
        class: e.exit.words(),
        repro: e.repro.clone(),
        hint: match &e.retry {
            Retry::AfterFix(h) => Some(h.clone()),
            _ => None,
        },
    }
}

fn ends_a_step(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|v| {
        let event = v["event"].as_str();
        event == Some("finished")
            || event == Some("skipped")
            || (event == Some("step") && matches!(v["status"].as_str(), Some("done" | "failed")))
    })
}

/// The `nvmod setup --json` lines a finished report stands for: the plan, then each step, then the end.
fn events_of(request: &SetupRequest, report: &SetupReport) -> Vec<String> {
    let total = report.steps.len();
    let mut lines = vec![json!({
        "event": "plan",
        "dry_run": report.dry_run,
        "resume": report.resume,
        "instance": request.instance,
        "steps": report.steps.iter()
            .map(|s| json!({"step": s.name, "description": s.description}))
            .collect::<Vec<_>>(),
    })];
    for (i, s) in report.steps.iter().enumerate() {
        let index = i + 1;
        match s.state {
            StepState::Done => {
                for status in ["started", "done"] {
                    lines.push(json!({
                        "event": "step", "step": s.name, "status": status, "index": index, "total": total,
                        "description": s.description, "detail": s.detail,
                    }));
                }
            }
            StepState::Skipped => lines.push(json!({
                "event": "skipped", "step": s.name, "index": index, "total": total,
                "reason": "finished earlier",
            })),
            _ => {}
        }
    }
    lines.push(json!({
        "event": "finished",
        "status": serde_json::to_value(report.finish.status).unwrap_or(Value::Null),
        "exit_code": report.finish.exit.code().map_or(-1, i64::from),
        "rolled_back": report.finish.rolled_back,
        "resumable": report.finish.resumable,
        "failed_step": report.finish.failed_step,
        "reason": report.finish.reason,
    }));
    lines
        .into_iter()
        .enumerate()
        .map(|(i, mut v)| {
            v["seq"] = json!(i + 1);
            v.to_string()
        })
        .collect()
}

/// The service the demo window runs on: a scripted fake whose setup succeeds with the steps of a first run.
/// (The real window will take the CLI-subprocess service; ADR-0005.)
pub fn demo_service() -> FakeService {
    use nvmod_ui_model::error::ExitClass;
    use nvmod_ui_model::setup::StepView;
    let step = |name: &str, description: &str, detail: &str| StepView {
        name: name.to_owned(),
        description: description.to_owned(),
        state: StepState::Done,
        detail: Some(detail.to_owned()),
    };
    let fake = FakeService::new();
    // Two runs, so "Start again" has an answer.
    for _ in 0..2 {
        fake.on_setup(Ok(SetupReport {
            steps: vec![
                step("init", "Make the Instance folder", "Instance made."),
                step(
                    "copy-base",
                    "Copy your game into the Instance",
                    "Game copied.",
                ),
                step("nv-rs", "Install the engine", "Engine installed."),
                step("mods", "Install your mods", "No mods in this demo."),
                step("sort", "Sort the load order", "Load order sorted."),
                step(
                    "verify",
                    "Check that everything fits together",
                    "Everything checks out.",
                ),
            ],
            finish: Finish {
                status: FinishStatus::Ok,
                exit: ExitClass::Ok,
                rolled_back: false,
                resumable: false,
                failed_step: None,
                reason: None,
            },
            dry_run: false,
            resume: false,
        }));
    }
    fake
}

/// The request the demo window shows (paths are placeholders; the fake service does not read them).
pub fn demo_request() -> SetupRequest {
    SetupRequest::fresh(
        "~/Games/fnv-instance",
        "~/Games/fnv-base",
        "masterlist.yaml",
    )
}

/// A failed first run, for the error card: the service refuses.
pub fn refusing_service() -> FakeService {
    let fake = FakeService::new();
    let mut e = ViewError::new(
        nvmod_ui_model::error::ExitClass::Refused,
        "the base folder does not look like Fallout: New Vegas",
        "Nothing was changed.",
    );
    e.retry = Retry::AfterFix("choose the folder that holds the game".to_owned());
    fake.on_setup(Err(ServiceError::Failed(e)));
    fake
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> SetupShell<FakeService> {
        SetupShell::new(demo_service(), demo_request())
    }

    #[test]
    fn before_start_the_screen_waits_with_the_command_it_will_run() {
        let v = shell().view();
        assert!(v.steps.is_empty());
        assert_eq!(v.primary.label, "Start setup");
        assert!(
            v.command
                .starts_with("nvmod setup --instance ~/Games/fnv-instance"),
            "{}",
            v.command
        );
        assert_eq!(v.progress, 0.0);
    }

    #[test]
    fn start_asks_the_service_once_and_shows_the_plan() {
        let mut s = shell();
        assert_eq!(s.press_primary(), Pressed::Handled);
        assert_eq!(s.service().setup_calls().len(), 1);
        let v = s.view();
        assert_eq!(v.steps.len(), 6);
        assert!(
            v.steps
                .iter()
                .all(|r| r.state == "Planned" || r.state == "To do"),
            "{:?}",
            v.steps
        );
        assert_eq!(v.primary.label, "Run next step");
        assert_eq!(v.command, "nvmod setup --instance ~/Games/fnv-instance");
    }

    #[test]
    fn each_press_finishes_one_step_until_the_end() {
        let mut s = shell();
        s.press_primary();
        for done in 1..=6 {
            assert_eq!(s.press_primary(), Pressed::Handled);
            let v = s.view();
            assert_eq!(v.steps.iter().filter(|r| r.state == "Done").count(), done);
        }
        let v = s.view();
        assert_eq!(v.progress_text, "6 of 6");
        assert!((v.progress - 1.0).abs() < f32::EPSILON);
        assert_eq!(v.status.0, "Setup finished.");
        assert_eq!(v.primary.label, "Open Home");
        assert!(v.error.is_none());
        assert_eq!(s.press_primary(), Pressed::GoHome);
        assert_eq!(s.service().setup_calls().len(), 1);
    }

    #[test]
    fn a_refusal_becomes_the_error_card_and_try_again_asks_again() {
        let mut s = SetupShell::new(refusing_service(), demo_request());
        s.press_primary();
        let v = s.view();
        let card = v.error.expect("an error card");
        assert!(card.what.contains("base folder"));
        assert_eq!(card.effect, "Nothing was changed.");
        assert_eq!(card.class, "refused; nothing was changed");
        assert_eq!(
            card.hint.as_deref(),
            Some("choose the folder that holds the game")
        );
        assert_eq!(v.primary.label, "Try again");
        assert_eq!(v.status.1, Tone::Err);
    }

    #[test]
    fn the_replayed_stream_is_accepted_whole_by_the_wizard() {
        let fake = demo_service();
        let request = demo_request();
        let report = fake.setup(&request).expect("scripted");
        let mut wizard = SetupWizard::new();
        for line in events_of(&request, &report) {
            assert_eq!(wizard.feed_line(&line), Ok(Applied::Yes), "{line}");
        }
        assert!(matches!(wizard.phase(), Phase::Finished(_)));
        assert_eq!(wizard.progress(), (6, 6));
    }
}
