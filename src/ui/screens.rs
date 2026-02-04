use egui::{Align, Align2, Area, Color32, FontId, RichText, Vec2};

use super::{EditorState, GameSettings, LightingMode, UiMessage};
use crate::voxel::BlockType;

pub fn main_menu(ctx: &egui::Context) -> Option<UiMessage> {
    let mut msg = None;

    Area::new(egui::Id::new("main_menu"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);

                ui.label(
                    RichText::new("THE DROP")
                        .font(FontId::proportional(64.0))
                        .color(Color32::WHITE),
                );

                ui.add_space(40.0);

                if menu_button(ui, "Play") {
                    msg = Some(UiMessage::OpenLevelSelect);
                }

                ui.add_space(10.0);

                if menu_button(ui, "Editor") {
                    msg = Some(UiMessage::OpenEditor);
                }

                ui.add_space(10.0);

                if menu_button(ui, "Settings") {
                    msg = Some(UiMessage::Settings);
                }

                ui.add_space(10.0);

                if menu_button(ui, "Exit") {
                    msg = Some(UiMessage::Exit);
                }
            });
        });

    msg
}

pub fn pause_menu(ctx: &egui::Context) -> Option<UiMessage> {
    let mut msg = None;

    // Dim background
    egui::Area::new(egui::Id::new("pause_bg"))
        .anchor(Align2::LEFT_TOP, Vec2::ZERO)
        .show(ctx, |ui| {
            let screen = ui.ctx().input(|i| i.viewport_rect());
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha(150));
        });

    Area::new(egui::Id::new("pause_menu"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("PAUSED")
                        .font(FontId::proportional(48.0))
                        .color(Color32::WHITE),
                );

                ui.add_space(30.0);

                if menu_button(ui, "Resume") {
                    msg = Some(UiMessage::Resume);
                }

                ui.add_space(10.0);

                if menu_button(ui, "Settings") {
                    msg = Some(UiMessage::Settings);
                }

                ui.add_space(10.0);

                if menu_button(ui, "Quit to Menu") {
                    msg = Some(UiMessage::QuitToMenu);
                }
            });
        });

    msg
}

pub fn settings(ctx: &egui::Context, settings: &mut GameSettings) -> Option<UiMessage> {
    let mut msg = None;

    Area::new(egui::Id::new("settings"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("SETTINGS")
                        .font(FontId::proportional(48.0))
                        .color(Color32::WHITE),
                );

                ui.add_space(30.0);

                // Settings panel
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(100))
                    .inner_margin(20.0)
                    .corner_radius(8.0)
                    .show(ui, |ui| {
                        ui.set_min_width(300.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("FOV").color(Color32::WHITE));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                ui.add(
                                    egui::Slider::new(&mut settings.fov, 60.0..=120.0)
                                        .show_value(true),
                                );
                            });
                        });

                        ui.add_space(10.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Sensitivity").color(Color32::WHITE));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                ui.add(
                                    egui::Slider::new(&mut settings.sensitivity, 0.001..=0.01)
                                        .show_value(true),
                                );
                            });
                        });

                        ui.add_space(10.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Lighting").color(Color32::WHITE));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                egui::ComboBox::from_id_salt("lighting_mode")
                                    .selected_text(settings.lighting_mode.name())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut settings.lighting_mode,
                                            LightingMode::Simple,
                                            "Simple",
                                        );
                                        ui.selectable_value(
                                            &mut settings.lighting_mode,
                                            LightingMode::PathTracing,
                                            "Path Tracing",
                                        );
                                    });
                            });
                        });
                    });

                ui.add_space(20.0);

                if menu_button(ui, "Back") {
                    msg = Some(UiMessage::Back);
                }
            });
        });

    msg
}

pub fn hud(ctx: &egui::Context, fps: f32) {
    // Crosshair - draw directly with painter to avoid capturing input
    let screen_rect = ctx.input(|i| i.viewport_rect());
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.text(
        screen_rect.center(),
        Align2::CENTER_CENTER,
        "+",
        FontId::monospace(24.0),
        Color32::WHITE,
    );

    // FPS counter
    Area::new(egui::Id::new("fps"))
        .anchor(Align2::LEFT_TOP, Vec2::new(10.0, 10.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!("{:.0} FPS", fps))
                    .font(FontId::monospace(16.0))
                    .color(Color32::YELLOW),
            );
        });
}

fn menu_button(ui: &mut egui::Ui, text: &str) -> bool {
    let button = egui::Button::new(
        RichText::new(text)
            .font(FontId::proportional(24.0))
            .color(Color32::WHITE),
    )
    .min_size(Vec2::new(200.0, 50.0))
    .fill(Color32::from_rgba_unmultiplied(60, 60, 80, 200))
    .corner_radius(8.0);

    ui.add(button).clicked()
}

fn small_button(ui: &mut egui::Ui, text: &str) -> bool {
    let button = egui::Button::new(
        RichText::new(text)
            .font(FontId::proportional(16.0))
            .color(Color32::WHITE),
    )
    .min_size(Vec2::new(120.0, 35.0))
    .fill(Color32::from_rgba_unmultiplied(60, 60, 80, 200))
    .corner_radius(6.0);

    ui.add(button).clicked()
}

pub fn level_select(ctx: &egui::Context, levels: &[String]) -> Option<UiMessage> {
    let mut msg = None;

    Area::new(egui::Id::new("level_select"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("SELECT LEVEL")
                        .font(FontId::proportional(48.0))
                        .color(Color32::WHITE),
                );

                ui.add_space(30.0);

                egui::Frame::new()
                    .fill(Color32::from_black_alpha(100))
                    .inner_margin(20.0)
                    .corner_radius(8.0)
                    .show(ui, |ui| {
                        ui.set_min_width(300.0);
                        ui.set_max_height(300.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if levels.is_empty() {
                                ui.label(
                                    RichText::new("No levels found")
                                        .color(Color32::GRAY)
                                        .italics(),
                                );
                            } else {
                                for level in levels {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(level)
                                                    .font(FontId::proportional(18.0))
                                                    .color(Color32::WHITE),
                                            )
                                            .min_size(Vec2::new(260.0, 40.0))
                                            .fill(Color32::from_rgba_unmultiplied(80, 80, 100, 180))
                                            .corner_radius(4.0),
                                        )
                                        .clicked()
                                    {
                                        msg = Some(UiMessage::LoadLevel(level.clone()));
                                    }
                                }
                            }
                        });
                    });

                ui.add_space(20.0);

                if menu_button(ui, "New Level") {
                    msg = Some(UiMessage::NewLevel);
                }

                ui.add_space(10.0);

                if menu_button(ui, "Back") {
                    msg = Some(UiMessage::Back);
                }
            });
        });

    msg
}

pub fn editor_hud(ctx: &egui::Context, editor: &mut EditorState, fps: f32) -> Option<UiMessage> {
    // Crosshair - draw directly with painter to avoid capturing input
    let screen_rect = ctx.input(|i| i.viewport_rect());
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.text(
        screen_rect.center(),
        Align2::CENTER_CENTER,
        "+",
        FontId::monospace(24.0),
        Color32::LIGHT_GREEN,
    );

    // Top info bar
    Area::new(egui::Id::new("editor_top"))
        .anchor(Align2::LEFT_TOP, Vec2::new(10.0, 10.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{:.0} FPS", fps))
                        .font(FontId::monospace(14.0))
                        .color(Color32::YELLOW),
                );
                ui.separator();
                ui.label(
                    RichText::new("EDITOR MODE")
                        .font(FontId::monospace(14.0))
                        .color(Color32::LIGHT_GREEN),
                );
            });
        });

    // Bottom toolbar
    Area::new(egui::Id::new("editor_toolbar"))
        .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -20.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(180))
                .inner_margin(10.0)
                .corner_radius(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Block:").color(Color32::WHITE));

                        let blocks = [
                            (BlockType::Stone, "Stone"),
                            (BlockType::Dirt, "Dirt"),
                            (BlockType::Grass, "Grass"),
                        ];

                        for (block, name) in blocks {
                            let selected = editor.selected_block == block;
                            let color = if selected {
                                Color32::from_rgb(100, 150, 100)
                            } else {
                                Color32::from_rgba_unmultiplied(60, 60, 80, 200)
                            };

                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(name)
                                            .font(FontId::proportional(14.0))
                                            .color(Color32::WHITE),
                                    )
                                    .fill(color)
                                    .corner_radius(4.0),
                                )
                                .clicked()
                            {
                                editor.selected_block = block;
                            }
                        }

                        ui.separator();

                        ui.label(RichText::new("Speed:").color(Color32::WHITE));
                        ui.add(egui::Slider::new(&mut editor.fly_speed, 5.0..=50.0).show_value(false));
                    });
                });
        });

    // Controls hint
    Area::new(egui::Id::new("editor_hints"))
        .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-10.0, -20.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(150))
                .inner_margin(8.0)
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("LMB: Remove | RMB: Place | Tab: UI | ESC: Menu | Ctrl+S: Save")
                            .font(FontId::monospace(12.0))
                            .color(Color32::LIGHT_GRAY),
                    );
                });
        });

    None
}

pub fn editor_pause(ctx: &egui::Context) -> Option<UiMessage> {
    let mut msg = None;

    // Dim background
    egui::Area::new(egui::Id::new("editor_pause_bg"))
        .anchor(Align2::LEFT_TOP, Vec2::ZERO)
        .show(ctx, |ui| {
            let screen = ui.ctx().input(|i| i.viewport_rect());
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha(150));
        });

    Area::new(egui::Id::new("editor_pause_menu"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("EDITOR PAUSED")
                        .font(FontId::proportional(48.0))
                        .color(Color32::WHITE),
                );

                ui.add_space(30.0);

                if menu_button(ui, "Resume") {
                    msg = Some(UiMessage::Resume);
                }

                ui.add_space(10.0);

                if menu_button(ui, "Save Level") {
                    msg = Some(UiMessage::SaveLevel(String::new())); // Will trigger save dialog
                }

                ui.add_space(10.0);

                if menu_button(ui, "Quit to Menu") {
                    msg = Some(UiMessage::EditorQuitToMenu);
                }
            });
        });

    msg
}

pub fn save_dialog(ctx: &egui::Context, level_name: &mut String) -> Option<UiMessage> {
    let mut msg = None;

    // Dim background
    egui::Area::new(egui::Id::new("save_dialog_bg"))
        .anchor(Align2::LEFT_TOP, Vec2::ZERO)
        .show(ctx, |ui| {
            let screen = ui.ctx().input(|i| i.viewport_rect());
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha(180));
        });

    Area::new(egui::Id::new("save_dialog"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(40, 40, 50, 240))
                .inner_margin(30.0)
                .corner_radius(12.0)
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Save Level")
                                .font(FontId::proportional(32.0))
                                .color(Color32::WHITE),
                        );

                        ui.add_space(20.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Name:").color(Color32::WHITE));
                            ui.add(
                                egui::TextEdit::singleline(level_name)
                                    .desired_width(200.0)
                                    .font(FontId::proportional(18.0)),
                            );
                        });

                        ui.add_space(20.0);

                        ui.horizontal(|ui| {
                            if small_button(ui, "Save") && !level_name.trim().is_empty() {
                                msg = Some(UiMessage::SaveLevel(level_name.trim().to_string()));
                            }

                            ui.add_space(10.0);

                            if small_button(ui, "Cancel") {
                                msg = Some(UiMessage::Back);
                            }
                        });
                    });
                });
        });

    msg
}
