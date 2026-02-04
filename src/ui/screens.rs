use egui::{Align, Align2, Area, Color32, FontId, RichText, Vec2};

use super::{GameSettings, LightingMode, UiMessage};

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
                    msg = Some(UiMessage::Play);
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

pub fn hud(ctx: &egui::Context) {
    // Crosshair
    Area::new(egui::Id::new("crosshair"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("+")
                    .font(FontId::monospace(24.0))
                    .color(Color32::WHITE),
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
