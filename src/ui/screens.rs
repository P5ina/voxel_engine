use egui::{Align, Align2, Area, Color32, FontId, RichText, Vec2};

use super::{
    BrushShape, DebugInfo, EditorState, GameSettings, LightingMode, LoadingState, UiMessage,
    WorldSelectState,
};
use crate::pathtracer::Palette;

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

                if menu_button(ui, "Worlds") {
                    msg = Some(UiMessage::OpenWorldSelect);
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

pub fn world_select(ctx: &egui::Context, state: &mut WorldSelectState) -> Option<UiMessage> {
    let mut msg = None;

    Area::new(egui::Id::new("world_select"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);

                ui.label(
                    RichText::new("WORLDS")
                        .font(FontId::proportional(48.0))
                        .color(Color32::WHITE),
                );

                ui.add_space(20.0);

                // World list in a scrollable frame
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(100))
                    .inner_margin(12.0)
                    .corner_radius(8.0)
                    .show(ui, |ui| {
                        ui.set_min_width(500.0);
                        ui.set_max_height(400.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if state.worlds.is_empty() {
                                ui.add_space(20.0);
                                ui.label(
                                    RichText::new("No worlds found. Create a new one!")
                                        .font(FontId::proportional(16.0))
                                        .color(Color32::GRAY),
                                );
                                ui.add_space(20.0);
                            }

                            for world in &state.worlds {
                                egui::Frame::new()
                                    .fill(Color32::from_rgba_unmultiplied(40, 40, 55, 200))
                                    .inner_margin(10.0)
                                    .corner_radius(6.0)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            // World info (left side)
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(&world.name)
                                                        .font(FontId::proportional(20.0))
                                                        .color(Color32::WHITE),
                                                );
                                                ui.label(
                                                    RichText::new(format!(
                                                        "{:.1} MB  |  {}",
                                                        world.size_mb, world.modified
                                                    ))
                                                    .font(FontId::proportional(13.0))
                                                    .color(Color32::GRAY),
                                                );
                                            });

                                            // Push buttons to the right
                                            ui.with_layout(
                                                egui::Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    let edit_btn = egui::Button::new(
                                                        RichText::new("Edit")
                                                            .font(FontId::proportional(16.0))
                                                            .color(Color32::WHITE),
                                                    )
                                                    .fill(Color32::from_rgb(60, 80, 60))
                                                    .corner_radius(4.0)
                                                    .min_size(Vec2::new(70.0, 32.0));

                                                    if ui.add(edit_btn).clicked() {
                                                        msg = Some(UiMessage::EditWorld(
                                                            world.path.clone(),
                                                        ));
                                                    }

                                                    ui.add_space(6.0);

                                                    let play_btn = egui::Button::new(
                                                        RichText::new("Play")
                                                            .font(FontId::proportional(16.0))
                                                            .color(Color32::WHITE),
                                                    )
                                                    .fill(Color32::from_rgb(50, 70, 100))
                                                    .corner_radius(4.0)
                                                    .min_size(Vec2::new(70.0, 32.0));

                                                    if ui.add(play_btn).clicked() {
                                                        msg = Some(UiMessage::PlayWorld(
                                                            world.path.clone(),
                                                        ));
                                                    }
                                                },
                                            );
                                        });
                                    });

                                ui.add_space(6.0);
                            }
                        });
                    });

                ui.add_space(15.0);

                // Create new map section
                if state.creating {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(100))
                        .inner_margin(12.0)
                        .corner_radius(8.0)
                        .show(ui, |ui| {
                            ui.set_min_width(500.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Map name:")
                                        .font(FontId::proportional(16.0))
                                        .color(Color32::WHITE),
                                );

                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut state.new_map_name)
                                        .desired_width(250.0)
                                        .font(FontId::proportional(16.0)),
                                );

                                // Auto-focus the text input
                                response.request_focus();

                                let name_valid = !state.new_map_name.trim().is_empty();

                                let confirm_btn = egui::Button::new(
                                    RichText::new("Create")
                                        .font(FontId::proportional(16.0))
                                        .color(Color32::WHITE),
                                )
                                .fill(if name_valid {
                                    Color32::from_rgb(50, 100, 50)
                                } else {
                                    Color32::from_rgb(60, 60, 60)
                                })
                                .corner_radius(4.0);

                                let enter_pressed = response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));

                                if (ui.add(confirm_btn).clicked() || enter_pressed) && name_valid {
                                    let name = state.new_map_name.trim().to_string();
                                    state.creating = false;
                                    state.new_map_name.clear();
                                    msg = Some(UiMessage::CreateNewMap(name));
                                }

                                let cancel_btn = egui::Button::new(
                                    RichText::new("Cancel")
                                        .font(FontId::proportional(16.0))
                                        .color(Color32::LIGHT_GRAY),
                                )
                                .fill(Color32::from_rgb(80, 50, 50))
                                .corner_radius(4.0);

                                if ui.add(cancel_btn).clicked() {
                                    state.creating = false;
                                    state.new_map_name.clear();
                                }
                            });
                        });
                } else if menu_button(ui, "Create New Map") {
                    state.creating = true;
                }

                ui.add_space(10.0);

                if menu_button(ui, "Back") {
                    msg = Some(UiMessage::Back);
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

                if menu_button(ui, "Save World") {
                    msg = Some(UiMessage::SaveBigWorld);
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

pub fn hud(ctx: &egui::Context, fps: f32, debug: Option<&DebugInfo>) {
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

    // Debug overlay (F3)
    if let Some(dbg) = debug {
        debug_overlay(ctx, fps, dbg);
    }
}

fn debug_overlay(ctx: &egui::Context, fps: f32, dbg: &DebugInfo) {
    Area::new(egui::Id::new("debug_overlay"))
        .anchor(Align2::LEFT_TOP, Vec2::new(10.0, 35.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(160))
                .inner_margin(8.0)
                .corner_radius(4.0)
                .show(ui, |ui| {
                    let mono = FontId::monospace(13.0);
                    let white = Color32::LIGHT_GRAY;
                    let green = Color32::from_rgb(100, 255, 100);
                    let cyan = Color32::from_rgb(100, 220, 255);

                    ui.label(
                        RichText::new(format!(
                            "{:.1} fps ({:.2} ms)",
                            fps,
                            1000.0 / fps.max(0.001)
                        ))
                        .font(mono.clone())
                        .color(green),
                    );

                    if let Some(cam) = &dbg.camera_pos {
                        let yellow = Color32::from_rgb(255, 255, 100);
                        ui.label(RichText::new("[FREE CAM]").font(mono.clone()).color(yellow));
                        ui.label(
                            RichText::new(format!(
                                "Cam: {:.2} / {:.2} / {:.2}",
                                cam[0], cam[1], cam[2]
                            ))
                            .font(mono.clone())
                            .color(yellow),
                        );
                    }

                    ui.label(
                        RichText::new(format!(
                            "Player: {:.2} / {:.2} / {:.2}",
                            dbg.player_pos[0], dbg.player_pos[1], dbg.player_pos[2]
                        ))
                        .font(mono.clone())
                        .color(white),
                    );

                    ui.label(
                        RichText::new(format!(
                            "Chunk: {} / {} / {}",
                            dbg.player_chunk[0], dbg.player_chunk[1], dbg.player_chunk[2]
                        ))
                        .font(mono.clone())
                        .color(white),
                    );

                    ui.add_space(4.0);

                    // Mesh counts
                    ui.label(
                        RichText::new(format!(
                            "Meshes: {} total ({} chunk, {} LOD)",
                            dbg.total_meshes, dbg.chunk_meshes, dbg.lod_meshes
                        ))
                        .font(mono.clone())
                        .color(cyan),
                    );

                    ui.label(
                        RichText::new(format!("World chunks: {}", dbg.world_chunks))
                            .font(mono.clone())
                            .color(white),
                    );

                    // Streaming info
                    if dbg.streaming_active {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("-- Streaming --")
                                .font(mono.clone())
                                .color(green),
                        );
                        ui.label(
                            RichText::new(format!(
                                "Loaded: {}  Queue: {}  LOD nodes: {}",
                                dbg.streaming_loaded, dbg.streaming_queue, dbg.streaming_lod_nodes
                            ))
                            .font(mono.clone())
                            .color(white),
                        );
                    }

                    // Octree info
                    if dbg.octree_active {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("-- Octree --")
                                .font(mono.clone())
                                .color(green),
                        );
                        ui.label(
                            RichText::new(format!(
                                "Nodes: {}  Data: {}  Depth: {}",
                                dbg.octree_nodes, dbg.octree_data_blocks, dbg.octree_depth
                            ))
                            .font(mono.clone())
                            .color(white),
                        );
                    }

                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("F3 to hide")
                            .font(FontId::monospace(11.0))
                            .color(Color32::DARK_GRAY),
                    );
                });
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

    // Bottom toolbar with color palette
    Area::new(egui::Id::new("editor_toolbar"))
        .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -20.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(180))
                .inner_margin(10.0)
                .corner_radius(8.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        // Color palette grid
                        let palette = Palette::new();
                        let cell_size = 16.0;
                        let cols = 16;
                        let rows = 8; // Show first 128 colors

                        ui.label(RichText::new("Palette:").color(Color32::WHITE));
                        ui.add_space(4.0);

                        for row in 0..rows {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
                                for col in 0..cols {
                                    let idx = (row * cols + col + 1) as u8; // Skip 0 (air)
                                    let mat = palette.get(idx);
                                    let color = Color32::from_rgb(
                                        (mat.albedo[0] * 255.0) as u8,
                                        (mat.albedo[1] * 255.0) as u8,
                                        (mat.albedo[2] * 255.0) as u8,
                                    );

                                    let selected = editor.selected_color == idx;
                                    let (rect, response) = ui.allocate_exact_size(
                                        Vec2::splat(cell_size),
                                        egui::Sense::click(),
                                    );

                                    if response.clicked() {
                                        editor.selected_color = idx;
                                    }

                                    // Draw selection border first (behind)
                                    if selected {
                                        ui.painter().rect_filled(
                                            rect.expand(2.0),
                                            3.0,
                                            Color32::WHITE,
                                        );
                                    }

                                    // Draw color cell
                                    ui.painter().rect_filled(rect, 2.0, color);
                                }
                            });
                        }

                        ui.add_space(8.0);

                        // Brush and color settings
                        ui.horizontal(|ui| {
                            // Brush shape
                            ui.label(RichText::new("Brush:").color(Color32::WHITE));
                            for shape in BrushShape::all() {
                                let selected = editor.brush_shape == *shape;
                                let btn_color = if selected {
                                    Color32::from_rgb(80, 120, 80)
                                } else {
                                    Color32::from_rgba_unmultiplied(60, 60, 80, 200)
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(shape.name())
                                                .font(FontId::proportional(12.0))
                                                .color(Color32::WHITE),
                                        )
                                        .fill(btn_color)
                                        .corner_radius(3.0),
                                    )
                                    .clicked()
                                {
                                    editor.brush_shape = *shape;
                                }
                            }

                            ui.separator();

                            // Brush size (only for sphere/cube)
                            if editor.brush_shape != BrushShape::Single {
                                ui.label(RichText::new("Size:").color(Color32::WHITE));
                                let mut size = editor.brush_size as i32;
                                if ui
                                    .add(egui::Slider::new(&mut size, 1..=8).show_value(true))
                                    .changed()
                                {
                                    editor.brush_size = size as u8;
                                }
                                ui.separator();
                            }

                            // Speed slider
                            ui.label(RichText::new("Speed:").color(Color32::WHITE));
                            ui.add(
                                egui::Slider::new(&mut editor.fly_speed, 5.0..=50.0)
                                    .show_value(false),
                            );

                            ui.separator();

                            // Show selected color
                            let mat = palette.get(editor.selected_color);
                            let color = Color32::from_rgb(
                                (mat.albedo[0] * 255.0) as u8,
                                (mat.albedo[1] * 255.0) as u8,
                                (mat.albedo[2] * 255.0) as u8,
                            );
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::new(24.0, 16.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, color);
                            ui.label(
                                RichText::new(format!("#{}", editor.selected_color))
                                    .color(Color32::WHITE),
                            );
                        });
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
                        RichText::new(
                            "LMB: Remove | RMB: Place | Tab: UI | ESC: Menu | Ctrl+S: Save",
                        )
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

                if menu_button(ui, "Save World") {
                    msg = Some(UiMessage::SaveBigWorld);
                }

                ui.add_space(10.0);

                if menu_button(ui, "Quit to Menu") {
                    msg = Some(UiMessage::EditorQuitToMenu);
                }
            });
        });

    msg
}

pub fn loading_screen(ctx: &egui::Context, state: &LoadingState) {
    // Dark background
    egui::Area::new(egui::Id::new("loading_bg"))
        .anchor(Align2::LEFT_TOP, Vec2::ZERO)
        .show(ctx, |ui| {
            let screen = ui.ctx().input(|i| i.viewport_rect());
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_rgb(20, 20, 30));
        });

    Area::new(egui::Id::new("loading_screen"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Title
                ui.label(
                    RichText::new(&state.message)
                        .font(FontId::proportional(36.0))
                        .color(Color32::WHITE),
                );

                ui.add_space(30.0);

                // Progress bar
                let bar_width = 400.0;
                let bar_height = 24.0;
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(bar_width, bar_height), egui::Sense::hover());

                // Background
                ui.painter()
                    .rect_filled(rect, 4.0, Color32::from_rgb(40, 40, 50));

                // Progress fill
                let fill_width = bar_width * state.progress;
                if fill_width > 0.0 {
                    let fill_rect =
                        egui::Rect::from_min_size(rect.min, Vec2::new(fill_width, bar_height));
                    ui.painter()
                        .rect_filled(fill_rect, 4.0, Color32::from_rgb(80, 160, 80));
                }

                // Progress border
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(2.0, Color32::from_rgb(100, 100, 120)),
                    egui::StrokeKind::Outside,
                );

                ui.add_space(15.0);

                // Chunks counter (hidden during save when total is 0)
                if state.chunks_total > 0 {
                    ui.label(
                        RichText::new(format!("{} / {}", state.chunks_loaded, state.chunks_total))
                            .font(FontId::proportional(18.0))
                            .color(Color32::LIGHT_GRAY),
                    );
                }

                // Percentage
                ui.label(
                    RichText::new(format!("{:.1}%", state.progress * 100.0))
                        .font(FontId::proportional(24.0))
                        .color(Color32::WHITE),
                );
            });
        });
}
