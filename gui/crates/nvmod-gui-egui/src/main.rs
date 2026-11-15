// SPDX-License-Identifier: GPL-3.0-only
//! The egui canary: paints the same `SetupView` the GPUI window paints, from the same controller, in the same
//! colours. It exists so a change of toolkit stays possible (ADR-0004); it is not a second product. Only
//! `nvmod-gui-shell` knows what the screen means.

use eframe::egui::{self, Color32, RichText};
use nvmod_gui_shell::setup::{SetupShell, Tone, demo_request, demo_service};
use nvmod_gui_shell::theme::{Color, Palette};
use nvmod_ui_service::Service;

fn col(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn tone(p: &Palette, t: Tone) -> Color32 {
    col(match t {
        Tone::Ok => p.ok,
        Tone::Accent => p.accent,
        Tone::Faint => p.faint,
        Tone::Muted => p.muted,
        Tone::Err => p.err,
    })
}

struct Canary<S: Service> {
    shell: SetupShell<S>,
    palette: Palette,
}

impl<S: Service> eframe::App for Canary<S> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let p = self.palette;
        let ctx = ui.ctx().clone();
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = col(p.bg);
        visuals.override_text_color = Some(col(p.text));
        visuals.widgets.inactive.weak_bg_fill = col(p.accent);
        visuals.widgets.hovered.weak_bg_fill = col(p.accent2);
        ctx.set_visuals(visuals);

        let v = self.shell.view();
        let mut pressed = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(v.eyebrow)
                    .small()
                    .strong()
                    .color(col(p.accent)),
            );
            ui.heading(v.title);
            ui.label(RichText::new(v.lead).color(col(p.muted)));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Steps").strong());
                ui.label(RichText::new(&v.progress_text).color(col(p.muted)));
            });
            ui.add(egui::ProgressBar::new(v.progress).fill(col(p.accent)));
            if v.steps.is_empty() {
                ui.label(
                    RichText::new("The steps appear here once setup has read your game folder.")
                        .color(col(p.muted)),
                );
            }
            for s in &v.steps {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(s.state).strong().color(tone(&p, s.tone)));
                    ui.label(&s.description);
                    ui.label(RichText::new(&s.name).monospace().color(col(p.faint)));
                });
                if let Some(d) = &s.detail {
                    ui.label(RichText::new(d).small().color(col(p.muted)));
                }
            }
            ui.label(RichText::new(&v.status.0).color(tone(&p, v.status.1)));
            if let Some(e) = &v.error {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("Error: {}", e.class))
                        .strong()
                        .color(col(p.err)),
                );
                ui.label(&e.what);
                ui.label(RichText::new(&e.effect).color(col(p.muted)));
            }
            ui.add_space(12.0);
            ui.label(RichText::new(&v.command).monospace().color(col(p.sky)));
            let label = RichText::new(v.primary.label)
                .strong()
                .color(col(p.accent_ink));
            pressed = ui.add(egui::Button::new(label)).clicked();
            ui.label(RichText::new(v.primary.help).small().color(col(p.muted)));
        });
        if pressed {
            self.shell.press_primary();
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 680.0])
            .with_title("nvmod (egui canary)"),
        ..Default::default()
    };
    eframe::run_native(
        "nvmod (egui canary)",
        options,
        Box::new(|_| {
            Ok(Box::new(Canary {
                shell: SetupShell::new(demo_service(), demo_request()),
                palette: Palette::DUSK,
            }))
        }),
    )
}
