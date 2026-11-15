// SPDX-License-Identifier: GPL-3.0-only
//! The nvmod GUI window painted with GPUI. Everything it shows comes from `nvmod-gui-shell`'s
//! [`SetupView`]; this crate only paints it and forwards presses (`docs/design/gui/04-view-model-api.md`).

use gpui::{
    App, Bounds, Context, FontWeight, Rgba, TitlebarOptions, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, relative, rgba, size,
};
use nvmod_gui_shell::setup::{Pressed, SetupShell, SetupView, StepRow, Tone};
use nvmod_gui_shell::theme::{Color, Palette};
use nvmod_ui_service::Service;
use std::time::Duration;

/// A theme colour as GPUI takes it.
fn c(color: Color) -> Rgba {
    rgba(color.rgba_u32())
}

fn tone(p: &Palette, t: Tone) -> Rgba {
    c(match t {
        Tone::Ok => p.ok,
        Tone::Accent => p.accent,
        Tone::Faint => p.faint,
        Tone::Muted => p.muted,
        Tone::Err => p.err,
    })
}

#[cfg(windows)]
const MONO: &str = "Consolas";
#[cfg(not(windows))]
const MONO: &str = "DejaVu Sans Mono";

/// The setup wizard window: the shell's controller and the palette.
pub struct SetupWindow<S: Service> {
    shell: SetupShell<S>,
    palette: Palette,
    went_home: bool,
}

impl<S: Service + 'static> SetupWindow<S> {
    /// A window in Mojave Dusk.
    pub fn new(shell: SetupShell<S>) -> Self {
        Self {
            shell,
            palette: Palette::DUSK,
            went_home: false,
        }
    }

    /// The view as it stands now (for tests and the host).
    pub fn view(&self) -> SetupView {
        self.shell.view()
    }

    /// Whether the person chose to leave setup for Home.
    pub fn went_home(&self) -> bool {
        self.went_home
    }

    fn header(&self, v: &SetupView) -> impl IntoElement + use<S> {
        let p = &self.palette;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(16.)).h(px(3.)).bg(c(p.accent)))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(c(p.accent))
                            .child(v.eyebrow),
                    ),
            )
            .child(
                div()
                    .text_size(px(32.))
                    .font_weight(FontWeight::BOLD)
                    .child(v.title),
            )
            .child(div().text_color(c(p.muted)).child(v.lead))
    }

    fn step_row(&self, i: usize, s: &StepRow) -> impl IntoElement + use<S> {
        let p = &self.palette;
        let mut row = div()
            .id(("step", i))
            .flex()
            .flex_col()
            .gap_1()
            .py_3()
            .border_b_1()
            .border_color(c(p.edge))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(88.))
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(tone(p, s.tone))
                            .child(s.state),
                    )
                    .child(div().flex_1().child(s.description.clone()))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_sm()
                            .text_color(c(p.faint))
                            .child(s.name.clone()),
                    ),
            );
        if let Some(detail) = &s.detail {
            row = row.child(
                div()
                    .pl(px(100.))
                    .text_sm()
                    .text_color(c(p.muted))
                    .child(detail.clone()),
            );
        }
        row
    }

    fn steps_panel(&self, v: &SetupView) -> impl IntoElement + use<S> {
        let p = &self.palette;
        let rows: Vec<_> = if v.steps.is_empty() {
            vec![
                div()
                    .py_3()
                    .text_color(c(p.muted))
                    .child("The steps appear here once setup has read your game folder.")
                    .into_any_element(),
            ]
        } else {
            v.steps
                .iter()
                .enumerate()
                .map(|(i, s)| self.step_row(i, s).into_any_element())
                .collect()
        };
        div()
            .id("steps")
            .flex_1()
            .flex()
            .flex_col()
            .p_4()
            .bg(c(p.surface))
            .border_1()
            .border_color(c(p.edge))
            .rounded_tl_xl()
            .rounded_br_xl()
            .rounded_tr_sm()
            .rounded_bl_sm()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .pb_2()
                    .child(div().font_weight(FontWeight::BOLD).child("Steps"))
                    .child(
                        div()
                            .id("progress-text")
                            .text_color(c(p.muted))
                            .child(v.progress_text.clone()),
                    ),
            )
            .child(
                div()
                    .h(px(8.))
                    .w_full()
                    .bg(c(p.bg2))
                    .border_1()
                    .border_color(c(p.control))
                    .rounded_sm()
                    .child(div().h_full().w(relative(v.progress)).bg(c(p.accent))),
            )
            .children(rows)
            .child(
                div()
                    .id("status")
                    .pt_3()
                    .text_color(tone(p, v.status.1))
                    .child(v.status.0.clone()),
            )
    }

    fn error_card(&self, v: &SetupView) -> Option<impl IntoElement + use<S>> {
        let p = &self.palette;
        let e = v.error.as_ref()?;
        let mut card = div()
            .id("error")
            .flex()
            .flex_col()
            .gap_1()
            .p_4()
            .border_2()
            .border_color(c(p.err))
            .rounded_tl_xl()
            .rounded_br_xl()
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_color(c(p.err))
                    .child(format!("Error: {}", e.class)),
            )
            .child(div().child(e.what.clone()))
            .child(div().text_color(c(p.muted)).child(e.effect.clone()));
        if let Some(h) = &e.hint {
            card = card.child(div().text_color(c(p.muted)).child(format!("To fix: {h}")));
        }
        if let Some(r) = &e.repro {
            card = card.child(
                div()
                    .font_family(MONO)
                    .text_sm()
                    .text_color(c(p.sky))
                    .child(r.clone()),
            );
        }
        Some(card)
    }

    fn footer(&self, v: &SetupView, cx: &mut Context<Self>) -> impl IntoElement + use<S> {
        let p = self.palette;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(c(p.muted))
                            .child("The same thing on the command line"),
                    )
                    .child(
                        div()
                            .id("command")
                            .font_family(MONO)
                            .text_sm()
                            .text_color(c(p.sky))
                            .px_2()
                            .py_1()
                            .bg(c(p.bg2))
                            .rounded_tl_xl()
                            .rounded_br_sm()
                            .child(v.command.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap_1()
                    .child(
                        div()
                            .id("primary")
                            .debug_selector(|| "primary".to_owned())
                            .flex()
                            .items_center()
                            .justify_center()
                            .h_10()
                            .px_6()
                            .bg(c(p.accent))
                            .text_color(c(p.accent_ink))
                            .font_weight(FontWeight::BOLD)
                            .border_2()
                            .border_color(c(p.accent2))
                            .rounded_tl_xl()
                            .rounded_br_xl()
                            .rounded_tr_sm()
                            .rounded_bl_sm()
                            .cursor_pointer()
                            .hover(move |s| s.bg(c(p.accent2)))
                            .on_click(cx.listener(|this, _e, _w, cx| {
                                if this.shell.press_primary() == Pressed::GoHome {
                                    this.went_home = true;
                                }
                                cx.notify();
                            }))
                            .child(v.primary.label),
                    )
                    .child(div().text_xs().text_color(c(p.muted)).child(v.primary.help)),
            )
    }
}

impl<S: Service + 'static> Render for SetupWindow<S> {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let v = self.shell.view();
        let p = self.palette;
        div()
            .size_full()
            .bg(c(p.bg))
            .text_color(c(p.text))
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .child(self.header(&v))
            .child(self.steps_panel(&v))
            .children(self.error_card(&v))
            .child(self.footer(&v, cx))
    }
}

/// Opens the setup window and runs the application until the window closes. With `quit_after`, the application
/// quits by itself after that long (the launch check of CI: it proves a window opened and survived).
///
/// # Panics
/// When the window cannot be opened, which makes the process exit non-zero: that is what the launch check reads.
pub fn run<S: Service + 'static>(shell: SetupShell<S>, quit_after: Option<Duration>) {
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_kit::init(cx);
        let bounds = Bounds::centered(None, size(px(960.), px(680.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("nvmod".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| SetupWindow::new(shell)))
            .expect("the window could not be opened");
        cx.on_window_closed(|cx, _id| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        if let Some(after) = quit_after {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(after).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
        cx.activate(true);
        eprintln!("nvmod-gui: window opened");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use nvmod_gui_shell::setup::{demo_request, demo_service, refusing_service};

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let b = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("no element with the selector {selector}"));
        cx.simulate_click(b.center(), Modifiers::none());
    }

    #[gpui::test]
    fn clicking_the_primary_button_walks_the_wizard_to_the_end(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(|_, _| {
            SetupWindow::new(SetupShell::new(demo_service(), demo_request()))
        });
        cx.run_until_parked();
        assert_eq!(
            window.read_with(cx, |w, _| w.view().primary.label),
            "Start setup"
        );

        click(cx, "primary");
        assert_eq!(window.read_with(cx, |w, _| w.view().steps.len()), 6);
        assert_eq!(
            window.read_with(cx, |w, _| w.view().primary.label),
            "Run next step"
        );

        for finished in 1..=6 {
            click(cx, "primary");
            let done = window.read_with(cx, |w, _| {
                w.view().steps.iter().filter(|s| s.state == "Done").count()
            });
            assert_eq!(done, finished);
        }
        assert_eq!(
            window.read_with(cx, |w, _| w.view().primary.label),
            "Open Home"
        );

        click(cx, "primary");
        assert!(window.read_with(cx, |w, _| w.went_home()));
    }

    #[gpui::test]
    fn a_refusal_shows_the_error_card(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(|_, _| {
            SetupWindow::new(SetupShell::new(refusing_service(), demo_request()))
        });
        cx.run_until_parked();
        click(cx, "primary");
        cx.run_until_parked();
        let v = window.read_with(cx, |w, _| w.view());
        assert!(v.error.is_some());
        assert_eq!(v.primary.label, "Try again");
    }
}
