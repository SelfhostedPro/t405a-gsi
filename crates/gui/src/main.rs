//! Step-by-step egui front-end for the onn 11" Core (T405A) installer.

use eframe::egui;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use t405a_core::{self as core, DeviceSummary, Reporter, Settings};

const BG: egui::Color32 = egui::Color32::from_rgb(0x0e, 0x11, 0x16);
const PANEL: egui::Color32 = egui::Color32::from_rgb(0x15, 0x19, 0x21);
const CARD: egui::Color32 = egui::Color32::from_rgb(0x1a, 0x1f, 0x28);
const INPUT: egui::Color32 = egui::Color32::from_rgb(0x0b, 0x0e, 0x13);
const STROKE: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x32, 0x3f);
const TEXT: egui::Color32 = egui::Color32::from_rgb(0xe8, 0xec, 0xf3);
const MUTED: egui::Color32 = egui::Color32::from_rgb(0x9a, 0xa6, 0xb8);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x5b, 0x9c, 0xff);
const ACCENT_DARK: egui::Color32 = egui::Color32::from_rgb(0x2f, 0x6f, 0xe0);
const GOOD: egui::Color32 = egui::Color32::from_rgb(0x4a, 0xd6, 0x8a);
const WARN: egui::Color32 = egui::Color32::from_rgb(0xf2, 0xb3, 0x5c);
const WARN_BG: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x22, 0x15);
const DANGER: egui::Color32 = egui::Color32::from_rgb(0xff, 0x7a, 0x7a);
const DANGER_BG: egui::Color32 = egui::Color32::from_rgb(0x2b, 0x17, 0x19);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Step {
    Connect,
    Unlock,
    Backup,
    Flash,
    Restore,
}

impl Step {
    fn name(self) -> &'static str {
        match self {
            Step::Connect => "connect",
            Step::Unlock => "unlock",
            Step::Backup => "backup",
            Step::Flash => "flash",
            Step::Restore => "restore",
        }
    }
}

const SHOT_STEPS: [Step; 5] = [
    Step::Connect,
    Step::Unlock,
    Step::Backup,
    Step::Flash,
    Step::Restore,
];

enum Msg {
    Log(String),
    Progress(f32),
    Summary(Box<DeviceSummary>),
    BackupDone(PathBuf),
    StepDone(Step),
    Done(Result<(), String>),
}

struct ChannelReporter {
    tx: Sender<Msg>,
    ctx: egui::Context,
}

impl Reporter for ChannelReporter {
    fn log(&self, msg: &str) {
        let _ = self.tx.send(Msg::Log(msg.to_string()));
        self.ctx.request_repaint();
    }

    fn progress(&self, done: u64, total: Option<u64>) {
        if let Some(total) = total {
            if total > 0 {
                let _ = self.tx.send(Msg::Progress(done as f32 / total as f32));
                self.ctx.request_repaint();
            }
        }
    }
}

struct App {
    settings: Settings,
    workdir_text: String,
    backup_text: String,
    custom_image: String,
    use_custom: bool,
    gapps: bool,
    step: Step,
    done_steps: HashSet<Step>,
    log: String,
    busy: bool,
    progress: f32,
    rx: Option<Receiver<Msg>>,
    summary: Option<DeviceSummary>,
    last_error: Option<String>,
    unlock_ack: bool,
    unlock_phrase: String,
    restore_ack: bool,
    screenshot_dir: Option<PathBuf>,
    shot_index: usize,
    shot_frames: u32,
    shot_pending: bool,
}

fn expand(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

fn latest_backup(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("backup-"))
                    .unwrap_or(false)
        })
        .collect();
    found.sort();
    found.pop()
}

fn apply_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = INPUT;
    visuals.faint_bg_color = CARD;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARN;
    visuals.error_fg_color = DANGER;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, STROKE);
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.weak_bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, STROKE);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x2b, 0x33, 0x40);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.6));
    visuals.widgets.active.bg_fill = ACCENT_DARK;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.open.bg_fill = CARD;
    visuals.window_corner_radius = egui::CornerRadius::same(10);
    visuals.menu_corner_radius = egui::CornerRadius::same(8);

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size.y = 34.0;
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(25.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.5));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.5));
    ctx.set_style(style);
}

fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0_f32, STROKE))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn info(ui: &mut egui::Ui, text: &str) {
    card(ui, |ui| {
        ui.add(egui::Label::new(egui::RichText::new(text).size(13.0).color(MUTED)).wrap());
    });
}

fn warning(ui: &mut egui::Ui, title: &str, body: &str) {
    egui::Frame::new()
        .fill(WARN_BG)
        .stroke(egui::Stroke::new(1.0_f32, WARN.gamma_multiply(0.45)))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("⚠").color(WARN).size(20.0));
                ui.label(egui::RichText::new(title).color(WARN).strong().size(15.0));
            });
            ui.add_space(4.0);
            ui.add(egui::Label::new(egui::RichText::new(body).color(WARN).size(13.0)).wrap());
        });
}

fn danger_card(ui: &mut egui::Ui, title: &str, body: &str) {
    egui::Frame::new()
        .fill(DANGER_BG)
        .stroke(egui::Stroke::new(1.0_f32, DANGER.gamma_multiply(0.45)))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("⚠").color(DANGER).size(20.0));
                ui.label(egui::RichText::new(title).color(DANGER).strong().size(15.0));
            });
            ui.add_space(4.0);
            ui.add(egui::Label::new(egui::RichText::new(body).color(DANGER).size(13.0)).wrap());
        });
}

fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(text)
            .strong()
            .color(egui::Color32::WHITE),
    )
    .fill(ACCENT)
    .corner_radius(egui::CornerRadius::same(8))
    .min_size(egui::vec2(0.0, 36.0));
    ui.add_enabled(enabled, button)
}

fn danger_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(text)
            .strong()
            .color(egui::Color32::WHITE),
    )
    .fill(DANGER)
    .corner_radius(egui::CornerRadius::same(8))
    .min_size(egui::vec2(0.0, 36.0));
    ui.add_enabled(enabled, button)
}

fn secondary(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let button = egui::Button::new(egui::RichText::new(text).color(TEXT))
        .corner_radius(egui::CornerRadius::same(8))
        .min_size(egui::vec2(0.0, 36.0));
    ui.add_enabled(enabled, button)
}

fn chip(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.55)))
        .corner_radius(egui::CornerRadius::same(20))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(color).size(12.0).strong());
        });
}

fn field(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).color(MUTED).size(12.0));
    ui.label(egui::RichText::new(value).color(TEXT).size(13.0));
}

fn step_header(ui: &mut egui::Ui, number: usize, title: &str, subtitle: &str) {
    ui.label(
        egui::RichText::new(format!("STEP {number} OF 5"))
            .color(ACCENT)
            .size(11.0)
            .strong(),
    );
    ui.add_space(2.0);
    ui.label(egui::RichText::new(title).size(26.0).strong().color(TEXT));
    ui.add_space(2.0);
    ui.label(egui::RichText::new(subtitle).size(14.0).color(MUTED));
    ui.add_space(14.0);
}

fn checklist(ui: &mut egui::Ui, number: &str, text: &str) {
    ui.horizontal(|ui| {
        chip(ui, number, ACCENT);
        ui.label(egui::RichText::new(text).size(13.5).color(TEXT));
    });
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, screenshot_dir: Option<PathBuf>) -> Self {
        apply_theme(&cc.egui_ctx);

        let workdir = dirs::data_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("t405a-gsi");
        let settings = Settings::new(workdir.clone());
        let backup_text = latest_backup(&workdir)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "/home/you/stock-backup/20261028".to_string());
        let demo = screenshot_dir.is_some();
        let mut done_steps = HashSet::new();
        if demo {
            done_steps.insert(Step::Connect);
            done_steps.insert(Step::Unlock);
            done_steps.insert(Step::Backup);
        }
        Self {
            workdir_text: workdir.display().to_string(),
            settings,
            backup_text,
            custom_image: String::new(),
            use_custom: false,
            gapps: demo,
            step: if demo { SHOT_STEPS[0] } else { Step::Connect },
            done_steps,
            log: if demo {
                "[demo] connected: onn11Core / T405A\n\
                 [demo] bootloader: UNLOCKED (orange)\n\
                 [demo] stock backup verified (63 images)\n"
                    .to_string()
            } else {
                String::new()
            },
            busy: false,
            progress: 0.0,
            rx: None,
            summary: if demo {
                Some(DeviceSummary {
                    model: "onn11Core".into(),
                    board: "T405A".into(),
                    fingerprint:
                        "onn/onn11Core/onn11Core:16/BP2A.250605.031.A3/1777011005:user/release-keys"
                            .into(),
                    incremental: "1777011005".into(),
                    patch: "2026-04-05".into(),
                    slot: "_a".into(),
                    locked: "0".into(),
                    verified_boot: "orange".into(),
                    lineage: "23.2-20260524-VANILLA-EXT4-GSI".into(),
                    bootloader: "n/a (android)".into(),
                    supported_lk: true,
                })
            } else {
                None
            },
            last_error: None,
            unlock_ack: demo,
            unlock_phrase: if demo {
                "UNLOCK T405A".to_string()
            } else {
                String::new()
            },
            restore_ack: demo,
            screenshot_dir,
            shot_index: 0,
            shot_frames: 0,
            shot_pending: false,
        }
    }

    fn current_settings(&self) -> Settings {
        Settings::new(expand(&self.workdir_text))
    }

    fn backup_ready(&self) -> bool {
        let p = expand(self.backup_text.trim());
        p.is_dir() && (p.join("SHA256SUMS").exists() || p.join("manifest.json").exists())
    }

    fn status_for(&self, step: Step) -> (egui::Color32, &'static str) {
        let done = match step {
            Step::Connect => self.summary.as_ref().is_some_and(|s| s.is_t405a()),
            Step::Unlock => self.summary.as_ref().is_some_and(|s| s.locked == "0"),
            Step::Backup => self.done_steps.contains(&Step::Backup) || self.backup_ready(),
            Step::Flash => self.done_steps.contains(&Step::Flash),
            Step::Restore => self.done_steps.contains(&Step::Restore),
        };
        if done {
            (GOOD, "•")
        } else if self.step == step {
            (ACCENT, "▶")
        } else {
            (MUTED, "•")
        }
    }

    fn push_log(&mut self, line: &str) {
        self.log.push_str(line);
        self.log.push('\n');
    }

    fn start_job<F>(&mut self, ctx: &egui::Context, job: F)
    where
        F: FnOnce(&dyn Reporter, Sender<Msg>) -> anyhow::Result<()> + Send + 'static,
    {
        if self.busy {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.busy = true;
        self.progress = 0.0;
        self.last_error = None;
        let reporter = ChannelReporter {
            tx: tx.clone(),
            ctx: ctx.clone(),
        };
        let ui_ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = job(&reporter, tx.clone());
            let _ = tx.send(Msg::Done(result.map_err(|e| format!("{e:#}"))));
            ui_ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        let Some(rx) = self.rx.take() else {
            return;
        };
        let mut keep = true;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Log(line) => self.push_log(&line),
                Msg::Progress(p) => self.progress = p,
                Msg::Summary(s) => self.summary = Some(*s),
                Msg::BackupDone(path) => {
                    self.backup_text = path.display().to_string();
                    self.push_log(&format!("backup: {}", path.display()));
                }
                Msg::StepDone(step) => {
                    self.done_steps.insert(step);
                }
                Msg::Done(result) => {
                    keep = false;
                    self.busy = false;
                    self.progress = 0.0;
                    match result {
                        Ok(()) => self.push_log("done."),
                        Err(err) => {
                            self.last_error = Some(err.clone());
                            self.push_log(&format!("ERROR: {err}"));
                        }
                    }
                }
            }
        }
        if keep {
            self.rx = Some(rx);
        }
    }

    fn screenshot_tick(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.screenshot_dir.clone() else {
            return;
        };
        let captured = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = captured {
            if self.shot_pending {
                self.shot_pending = false;
                let name = SHOT_STEPS
                    .get(self.shot_index)
                    .map(|s| s.name())
                    .unwrap_or("step");
                let path = dir.join(format!("{:02}-{name}.png", self.shot_index + 1));
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = image::save_buffer(
                    &path,
                    image.as_raw(),
                    image.width() as u32,
                    image.height() as u32,
                    image::ExtendedColorType::Rgba8,
                );
                self.shot_index += 1;
                if self.shot_index >= SHOT_STEPS.len() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                self.step = SHOT_STEPS[self.shot_index];
                self.shot_frames = 0;
            }
        }
        if !self.shot_pending {
            self.shot_frames += 1;
            if self.shot_frames > 10 {
                self.shot_pending = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    fn step_item(&mut self, ui: &mut egui::Ui, step: Step, number: usize, title: &str) {
        let (color, icon) = self.status_for(step);
        let selected = self.step == step;
        egui::Frame::new()
            .fill(if selected {
                egui::Color32::from_rgb(0x1e, 0x28, 0x38)
            } else {
                egui::Color32::TRANSPARENT
            })
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon).color(color).strong().size(14.0));
                    let text = format!("{number}. {title}");
                    let label =
                        egui::RichText::new(text).color(if selected { TEXT } else { MUTED });
                    if ui.selectable_label(selected, label).clicked() {
                        self.step = step;
                    }
                });
            });
    }

    fn step_list(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("STEPS")
                .size(11.0)
                .strong()
                .color(MUTED),
        );
        ui.add_space(4.0);
        self.step_item(ui, Step::Connect, 1, "Connect & check");
        self.step_item(ui, Step::Unlock, 2, "Unlock bootloader");
        self.step_item(ui, Step::Backup, 3, "Back up stock");
        self.step_item(ui, Step::Flash, 4, "Flash GSI");
        self.step_item(ui, Step::Restore, 5, "Restore stock");

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("WORK DIRECTORY")
                .size(11.0)
                .strong()
                .color(MUTED),
        );
        let name = Path::new(&self.workdir_text)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("work");
        ui.add(egui::Label::new(
            egui::RichText::new(format!("…/{name}"))
                .size(12.0)
                .color(TEXT),
        ))
        .on_hover_text(&self.workdir_text);
        if ui
            .selectable_label(
                false,
                egui::RichText::new("Change in Connect")
                    .size(12.0)
                    .color(ACCENT),
            )
            .clicked()
        {
            self.step = Step::Connect;
        }
    }

    fn next_row(&mut self, ui: &mut egui::Ui, next: Option<Step>) {
        if let Some(next) = next {
            ui.add_space(14.0);
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Next step  >").color(TEXT))
                        .corner_radius(egui::CornerRadius::same(8))
                        .min_size(egui::vec2(0.0, 34.0)),
                )
                .clicked()
            {
                self.step = next;
            }
        }
    }

    fn step_connect(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(
            ui,
            1,
            "Connect the tablet",
            "Enable USB debugging and confirm the tool sees this exact model.",
        );

        card(ui, |ui| {
            ui.label(egui::RichText::new("Before you start").strong().size(15.0));
            ui.add_space(6.0);
            checklist(
                ui,
                "1",
                "Enable Developer options (tap Build number seven times).",
            );
            checklist(ui, "2", "Turn on USB debugging.");
            checklist(ui, "3", "Connect the tablet with a USB-C data cable.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let can = !self.busy;
                if primary(ui, "Check device", can).clicked() {
                    self.start_job(ctx, |rep, tx| {
                        if let Some(s) = core::check(rep)? {
                            let _ = tx.send(Msg::Summary(Box::new(s)));
                            let _ = tx.send(Msg::StepDone(Step::Connect));
                        }
                        Ok(())
                    });
                }
                if secondary(ui, "Download / verify images", can).clicked() {
                    let settings = self.current_settings();
                    self.start_job(ctx, move |rep, _tx| {
                        core::ensure_workdir(&settings)?;
                        core::download_all(rep, &settings)
                    });
                }
            });
        });

        ui.add_space(12.0);
        if let Some(s) = self.summary.clone() {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Device").strong().size(15.0));
                    chip(
                        ui,
                        if s.locked == "0" {
                            "UNLOCKED"
                        } else {
                            "LOCKED"
                        },
                        if s.locked == "0" { GOOD } else { WARN },
                    );
                    if !s.is_t405a() {
                        chip(ui, "WRONG DEVICE", DANGER);
                    }
                });
                ui.add_space(8.0);
                egui::Grid::new("device-grid")
                    .num_columns(2)
                    .spacing([24.0, 7.0])
                    .show(ui, |ui| {
                        field(ui, "Model / board", &format!("{} / {}", s.model, s.board));
                        ui.end_row();
                        field(
                            ui,
                            "Build / patch",
                            &format!("{} / {}", s.incremental, s.patch),
                        );
                        ui.end_row();
                        field(ui, "Slot", if s.slot.is_empty() { "-" } else { &s.slot });
                        ui.end_row();
                        field(
                            ui,
                            "Boot state",
                            &format!(
                                "{} ({})",
                                if s.locked == "0" {
                                    "unlocked"
                                } else {
                                    "locked"
                                },
                                s.verified_boot
                            ),
                        );
                        ui.end_row();
                        if !s.lineage.is_empty() {
                            field(ui, "Running", &s.lineage);
                            ui.end_row();
                        }
                        ui.label(egui::RichText::new("Fingerprint").color(MUTED).size(12.0));
                        ui.add(egui::Label::new(
                            egui::RichText::new(&s.fingerprint)
                                .monospace()
                                .size(11.5)
                                .color(MUTED),
                        ));
                        ui.end_row();
                    });
            });
        } else {
            info(
                ui,
                "No device checked yet. Connect the tablet and press Check device.",
            );
        }

        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(egui::RichText::new("Work directory").strong().size(15.0));
            ui.label(
                egui::RichText::new("Images, downloads, logs and backups live here.")
                    .size(12.5)
                    .color(MUTED),
            );
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.workdir_text)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            ui.add_space(6.0);
            if secondary(ui, "Use this directory", !self.busy).clicked() {
                self.settings = self.current_settings();
                self.backup_text = latest_backup(&self.settings.workdir)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
            }
        });

        ui.add_space(12.0);
        info(
            ui,
            "The raw-USB unlock is most reliable on Linux. On Windows use WSL2 with usbipd; on \
             macOS the MediaTek LK may hit xHCI pipe errors. Everything after unlock uses normal \
             adb/fastboot.",
        );
        self.next_row(ui, Some(Step::Unlock));
    }

    fn step_unlock(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(
            ui,
            2,
            "Unlock the bootloader",
            "The one irreversible step. The tool verifies the bootloader before sending anything.",
        );

        danger_card(
            ui,
            "Irreversible",
            "Writes lock state 3 into seccfg, invalidates userdata encryption and has no BROM/DA \
             recovery. A bad write bricks the tablet. The payload is sent exactly once.",
        );

        ui.add_space(12.0);
        card(ui, |ui| {
            ui.checkbox(
                &mut self.unlock_ack,
                "I understand this is irreversible, wipes data, and can brick the tablet.",
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Type UNLOCK T405A to enable the button")
                    .size(12.5)
                    .color(MUTED),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.unlock_phrase)
                    .hint_text("UNLOCK T405A")
                    .desired_width(280.0)
                    .font(egui::TextStyle::Monospace),
            );
            ui.add_space(10.0);
            let ready =
                self.unlock_ack && self.unlock_phrase.trim() == "UNLOCK T405A" && !self.busy;
            if danger_button(ui, "Unlock bootloader", ready).clicked() {
                let settings = self.current_settings();
                self.start_job(ctx, move |rep, tx| {
                    core::ensure_workdir(&settings)?;
                    core::unlock::unlock(rep, &settings)?;
                    let _ = tx.send(Msg::StepDone(Step::Unlock));
                    Ok(())
                });
            }
        });

        ui.add_space(12.0);
        info(
            ui,
            "After the payload the tablet reboots to recovery with init_user0_failed. On the \
             tablet: Power + Volume Up, then Wipe data/factory reset -> Factory data reset -> \
             Reboot system now. The tool waits for Android to come back.",
        );
        self.next_row(ui, Some(Step::Backup));
    }

    fn step_backup(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(
            ui,
            3,
            "Back up the stock firmware",
            "A complete, restorable copy before anything is flashed.",
        );

        card(ui, |ui| {
            ui.label(egui::RichText::new("What gets dumped").strong().size(15.0));
            ui.add_space(4.0);
            for line in [
                "super — both slots' system, vendor, product, system_ext and DLKMs",
                "boot, init_boot, vendor_boot, dtbo and every vbmeta",
                "lk, nvram, nvdata, nvcfg, persist, protect1/2, seccfg, proinfo and the rest",
                "excluded: userdata, cache and the whole-disk LUNs",
            ] {
                ui.label(
                    egui::RichText::new(format!("•  {line}"))
                        .size(13.0)
                        .color(MUTED),
                );
            }
        });

        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("Backup directory (optional)")
                    .strong()
                    .size(15.0),
            );
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.backup_text)
                    .hint_text("empty = workdir/backup-TIMESTAMP")
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            ui.add_space(10.0);
            if primary(ui, "Back up stock", !self.busy).clicked() {
                let settings = self.current_settings();
                let out = if self.backup_text.trim().is_empty() {
                    None
                } else {
                    Some(expand(self.backup_text.trim()))
                };
                self.start_job(ctx, move |rep, tx| {
                    core::ensure_workdir(&settings)?;
                    let path = core::backup::backup(rep, &settings, out.as_deref())?;
                    let _ = tx.send(Msg::BackupDone(path));
                    let _ = tx.send(Msg::StepDone(Step::Backup));
                    Ok(())
                });
            }
        });

        ui.add_space(12.0);
        info(
            ui,
            "The backup boots a userdebug GSI through DSU to get adb root; it does not touch \
             super. Needs about 12 GB free. The DSU is removed afterwards and stock returns.",
        );
        self.next_row(ui, Some(Step::Flash));
    }

    fn step_flash(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(
            ui,
            4,
            "Flash a GSI",
            "LineageOS 23.2 is the default. Any raw ext4 arm64 GSI can be used instead.",
        );

        let backup_ok = !self.backup_text.trim().is_empty();
        let image_ok = !self.use_custom || !self.custom_image.trim().is_empty();

        card(ui, |ui| {
            ui.label(egui::RichText::new("Stock backup").strong().size(15.0));
            ui.label(
                egui::RichText::new("Required: supplies vbmeta_a.img for the verification patch.")
                    .size(12.5)
                    .color(MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.backup_text)
                        .desired_width(430.0)
                        .font(egui::TextStyle::Monospace),
                );
                if secondary(ui, "Use latest", !self.busy).clicked() {
                    if let Some(p) = latest_backup(&self.current_settings().workdir) {
                        self.backup_text = p.display().to_string();
                    }
                }
            });

            ui.add_space(12.0);
            ui.checkbox(&mut self.use_custom, "Use a custom raw ext4 GSI image");
            if self.use_custom {
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.custom_image)
                        .hint_text("/path/to/system.img")
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            } else {
                ui.add_space(6.0);
                ui.checkbox(&mut self.gapps, "Include Google apps (GApps GSI)");
                ui.label(
                    egui::RichText::new(
                        "Uses the LineageOS 23.2 GApps build. The official MindTheGapps zip is \
                         installed from a custom recovery, which this tablet does not have.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let ready = !self.busy && backup_ok && image_ok;
                if primary(ui, "Flash GSI", ready).clicked() {
                    let settings = self.current_settings();
                    let backup = expand(self.backup_text.trim());
                    let image = if self.use_custom {
                        Some(expand(self.custom_image.trim()))
                    } else {
                        None
                    };
                    let gapps = self.gapps && !self.use_custom;
                    self.start_job(ctx, move |rep, tx| {
                        core::flash::flash(
                            rep,
                            &settings,
                            &backup,
                            image.as_deref(),
                            gapps,
                            false,
                        )?;
                        let _ = tx.send(Msg::StepDone(Step::Flash));
                        Ok(())
                    });
                }
                if secondary(ui, "Dry run", ready).clicked() {
                    let settings = self.current_settings();
                    let backup = expand(self.backup_text.trim());
                    let image = if self.use_custom {
                        Some(expand(self.custom_image.trim()))
                    } else {
                        None
                    };
                    let gapps = self.gapps && !self.use_custom;
                    self.start_job(ctx, move |rep, _tx| {
                        core::flash::flash(rep, &settings, &backup, image.as_deref(), gapps, true)
                    });
                }
            });
        });

        ui.add_space(12.0);
        warning(
            ui,
            "Factory reset required",
            "After flashing, do the recovery factory reset (Wipe data/factory reset -> Factory \
             data reset -> Reboot system now). The tool waits for Android to boot.",
        );
        self.next_row(ui, Some(Step::Restore));
    }

    fn step_restore(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(
            ui,
            5,
            "Restore stock",
            "Put the stock boot, vbmeta and super images back from your backup.",
        );

        let backup_ok = !self.backup_text.trim().is_empty();
        card(ui, |ui| {
            ui.label(egui::RichText::new("Stock backup").strong().size(15.0));
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.backup_text)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
        });

        ui.add_space(12.0);
        danger_card(
            ui,
            "This overwrites the system",
            "Flashes boot, init_boot, vendor_boot, dtbo, vbmeta* and super. It never writes lk, \
             preloader, tee, gz, seccfg, protect, nvram, nvdata, nvcfg or the GPT.",
        );

        ui.add_space(12.0);
        card(ui, |ui| {
            ui.checkbox(
                &mut self.restore_ack,
                "I understand this overwrites the system.",
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if secondary(ui, "Dry run", !self.busy && backup_ok).clicked() {
                    let settings = self.current_settings();
                    let backup = expand(self.backup_text.trim());
                    self.start_job(ctx, move |rep, _tx| {
                        core::flash::restore(rep, &settings, &backup, false, true)
                    });
                }
                let ready = !self.busy && backup_ok && self.restore_ack;
                if danger_button(ui, "Restore stock", ready).clicked() {
                    let settings = self.current_settings();
                    let backup = expand(self.backup_text.trim());
                    self.start_job(ctx, move |rep, tx| {
                        core::flash::restore(rep, &settings, &backup, false, false)?;
                        let _ = tx.send(Msg::StepDone(Step::Restore));
                        Ok(())
                    });
                }
            });
        });
    }

    fn ui_top(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(22, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("t405a-gsi")
                                .size(23.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.label(
                            egui::RichText::new("onn 11\" Core (T405A) installer")
                                .size(12.5)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.busy {
                            ui.add(egui::Spinner::new().size(16.0));
                            ui.label(egui::RichText::new("working").color(MUTED).size(12.5));
                        } else {
                            ui.label(egui::RichText::new("ready").color(MUTED).size(12.5));
                        }
                        if let Some(s) = &self.summary {
                            if s.is_t405a() {
                                let (label, color) = if s.locked == "0" {
                                    (format!("{} · unlocked", s.model), GOOD)
                                } else {
                                    (format!("{} · locked", s.model), WARN)
                                };
                                chip(ui, &label, color);
                            } else {
                                chip(ui, "wrong device", DANGER);
                            }
                        } else {
                            chip(ui, "no device", MUTED);
                        }
                    });
                });
                if self.busy && self.progress > 0.0 {
                    ui.add_space(8.0);
                    ui.add(
                        egui::ProgressBar::new(self.progress)
                            .fill(ACCENT)
                            .desired_width(ui.available_width()),
                    );
                }
                if let Some(err) = &self.last_error {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(err).color(DANGER).size(13.0));
                }
            });
    }

    fn ui_log(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("log")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(0x0a, 0x0c, 0x10))
                    .inner_margin(egui::Margin::symmetric(20, 12)),
            )
            .resizable(true)
            .default_height(180.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("ACTIVITY LOG")
                            .size(11.0)
                            .strong()
                            .color(MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Clear").clicked() {
                            self.log.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let empty = self.log.is_empty();
                        let text = if empty {
                            "Waiting for a step…"
                        } else {
                            self.log.as_str()
                        };
                        ui.add(egui::Label::new(
                            egui::RichText::new(text)
                                .monospace()
                                .size(12.0)
                                .color(if empty { MUTED } else { TEXT }),
                        ));
                    });
            });
    }

    fn ui_steps(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("steps")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(14, 18)),
            )
            .resizable(false)
            .default_width(236.0)
            .show(ctx, |ui| {
                self.step_list(ui);
            });
    }

    fn ui_main(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(28, 22)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let width = (ui.available_width() - 4.0).min(860.0);
                    ui.set_width(width);
                    match self.step {
                        Step::Connect => self.step_connect(ui, ctx),
                        Step::Unlock => self.step_unlock(ui, ctx),
                        Step::Backup => self.step_backup(ui, ctx),
                        Step::Flash => self.step_flash(ui, ctx),
                        Step::Restore => self.step_restore(ui, ctx),
                    }
                });
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        self.screenshot_tick(ctx);

        self.ui_top(ctx);
        self.ui_log(ctx);
        self.ui_steps(ctx);
        self.ui_main(ctx);
    }
}

fn main() -> eframe::Result<()> {
    let mut screenshot_dir = None;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshots" && i + 1 < args.len() {
            screenshot_dir = Some(PathBuf::from(&args[i + 1]));
            i += 2;
        } else {
            i += 1;
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 820.0])
            .with_min_inner_size([900.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "t405a-gsi — onn 11\" Core installer",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, screenshot_dir)))),
    )
}
