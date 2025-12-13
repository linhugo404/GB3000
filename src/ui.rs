//! UI module for GB3000 emulator
//!
//! Provides a modern user interface including:
//! - Start screen with ROM selection
//! - In-game menu overlay
//! - Settings and configuration

use egui::{Color32, FontId, RichText, Rounding, Stroke, Vec2};
use rfd::FileDialog;
use std::path::PathBuf;

/// UI state and configuration
#[derive(Debug, Clone, PartialEq)]
pub enum EmulatorState {
    /// Start screen - no ROM loaded
    StartScreen,
    /// ROM is loaded and running
    Running,
    /// Paused with menu overlay
    Paused,
}

/// Recent ROM entry
#[derive(Debug, Clone)]
pub struct RecentRom {
    pub path: PathBuf,
    pub title: String,
}

/// Main UI controller
pub struct Ui {
    /// Current emulator state
    pub state: EmulatorState,
    /// Recently opened ROMs
    pub recent_roms: Vec<RecentRom>,
    /// Currently loaded ROM path
    pub current_rom: Option<PathBuf>,
    /// ROM info for display
    pub rom_info: Option<RomInfo>,
    /// Show FPS counter
    pub show_fps: bool,
    /// Current FPS
    pub fps: f64,
    /// Error message to display
    pub error_message: Option<String>,
    /// Selected palette index
    pub palette_index: usize,
}

/// ROM information for display
#[derive(Debug, Clone)]
pub struct RomInfo {
    pub title: String,
    pub cart_type: String,
    pub rom_size: String,
    pub ram_size: String,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            state: EmulatorState::StartScreen,
            recent_roms: Vec::new(),
            current_rom: None,
            rom_info: None,
            show_fps: true,
            fps: 0.0,
            error_message: None,
            palette_index: 0,
        }
    }

    /// Open file dialog to select a ROM
    pub fn open_file_dialog() -> Option<PathBuf> {
        FileDialog::new()
            .add_filter("Game Boy ROMs", &["gb", "gbc", "GB", "GBC"])
            .add_filter("All files", &["*"])
            .set_title("Select a Game Boy ROM")
            .pick_file()
    }

    /// Parse ROM info from ROM data
    pub fn parse_rom_info(rom: &[u8]) -> Option<RomInfo> {
        if rom.len() < 0x150 {
            return None;
        }

        // Title (0x0134-0x0143)
        let title: String = rom[0x0134..0x0144]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' })
            .collect();

        // Cartridge type
        let cart_type = match rom[0x0147] {
            0x00 => "ROM ONLY",
            0x01 => "MBC1",
            0x02 => "MBC1+RAM",
            0x03 => "MBC1+RAM+BATTERY",
            0x05 => "MBC2",
            0x06 => "MBC2+BATTERY",
            0x0F..=0x13 => "MBC3",
            0x19..=0x1E => "MBC5",
            _ => "Unknown",
        }
        .to_string();

        // ROM size
        let rom_size = match rom[0x0148] {
            0x00 => "32 KB",
            0x01 => "64 KB",
            0x02 => "128 KB",
            0x03 => "256 KB",
            0x04 => "512 KB",
            0x05 => "1 MB",
            0x06 => "2 MB",
            0x07 => "4 MB",
            0x08 => "8 MB",
            _ => "Unknown",
        }
        .to_string();

        // RAM size
        let ram_size = match rom[0x0149] {
            0x00 => "None",
            0x01 => "2 KB",
            0x02 => "8 KB",
            0x03 => "32 KB",
            0x04 => "128 KB",
            0x05 => "64 KB",
            _ => "Unknown",
        }
        .to_string();

        Some(RomInfo {
            title: title.trim().to_string(),
            cart_type,
            rom_size,
            ram_size,
        })
    }

    /// Render the start screen
    pub fn render_start_screen(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        let mut selected_rom: Option<PathBuf> = None;

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(18, 18, 24)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);

                    // Logo/Title
                    ui.label(
                        RichText::new("🎮 GB3000")
                            .font(FontId::proportional(48.0))
                            .color(Color32::from_rgb(100, 200, 100)),
                    );

                    ui.add_space(8.0);

                    ui.label(
                        RichText::new("Game Boy Emulator")
                            .font(FontId::proportional(18.0))
                            .color(Color32::from_rgb(150, 150, 160)),
                    );

                    ui.add_space(40.0);

                    // Open ROM button
                    let button = egui::Button::new(
                        RichText::new("📂  Open ROM")
                            .font(FontId::proportional(20.0))
                            .color(Color32::WHITE),
                    )
                    .min_size(Vec2::new(200.0, 50.0))
                    .fill(Color32::from_rgb(60, 120, 80))
                    .stroke(Stroke::new(2.0, Color32::from_rgb(80, 160, 100)))
                    .rounding(Rounding::same(8.0));

                    if ui.add(button).clicked() {
                        if let Some(path) = Self::open_file_dialog() {
                            selected_rom = Some(path);
                        }
                    }

                    ui.add_space(30.0);

                    // Recent ROMs section
                    if !self.recent_roms.is_empty() {
                        ui.label(
                            RichText::new("Recent ROMs")
                                .font(FontId::proportional(16.0))
                                .color(Color32::from_rgb(120, 120, 130)),
                        );

                        ui.add_space(10.0);

                        for recent in &self.recent_roms.clone() {
                            let recent_button = egui::Button::new(
                                RichText::new(format!("🕹️  {}", recent.title))
                                    .font(FontId::proportional(14.0))
                                    .color(Color32::from_rgb(200, 200, 210)),
                            )
                            .min_size(Vec2::new(250.0, 35.0))
                            .fill(Color32::from_rgb(40, 40, 50))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(60, 60, 70)))
                            .rounding(Rounding::same(6.0));

                            if ui.add(recent_button).clicked() {
                                if recent.path.exists() {
                                    selected_rom = Some(recent.path.clone());
                                }
                            }
                        }
                    }

                    ui.add_space(40.0);

                    // Controls info
                    ui.label(
                        RichText::new("Controls")
                            .font(FontId::proportional(14.0))
                            .color(Color32::from_rgb(100, 100, 110)),
                    );

                    ui.add_space(8.0);

                    let controls_text = "Arrow Keys = D-Pad  |  Z = A  |  X = B  |  Enter = Start  |  Space = Select";
                    ui.label(
                        RichText::new(controls_text)
                            .font(FontId::proportional(12.0))
                            .color(Color32::from_rgb(80, 80, 90)),
                    );

                    // Error message
                    if let Some(ref error) = self.error_message {
                        ui.add_space(20.0);
                        ui.label(
                            RichText::new(format!("⚠️ {}", error))
                                .font(FontId::proportional(14.0))
                                .color(Color32::from_rgb(255, 100, 100)),
                        );
                    }
                });
            });

        selected_rom
    }

    /// Render the pause menu overlay
    pub fn render_pause_menu(&mut self, ctx: &egui::Context) -> PauseMenuAction {
        let mut action = PauseMenuAction::None;

        // Darken background
        egui::Area::new(egui::Id::new("pause_overlay"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    Rounding::ZERO,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 180),
                );
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);

                    ui.label(
                        RichText::new("⏸️  PAUSED")
                            .font(FontId::proportional(32.0))
                            .color(Color32::WHITE),
                    );

                    ui.add_space(30.0);

                    // Resume button
                    if ui
                        .add(self.menu_button("▶️  Resume", Color32::from_rgb(60, 120, 80)))
                        .clicked()
                    {
                        action = PauseMenuAction::Resume;
                    }

                    ui.add_space(10.0);

                    // Reset button
                    if ui
                        .add(self.menu_button("🔄  Reset", Color32::from_rgb(80, 80, 120)))
                        .clicked()
                    {
                        action = PauseMenuAction::Reset;
                    }

                    ui.add_space(10.0);

                    // Open ROM button
                    if ui
                        .add(self.menu_button("📂  Open ROM", Color32::from_rgb(80, 80, 120)))
                        .clicked()
                    {
                        if let Some(path) = Self::open_file_dialog() {
                            action = PauseMenuAction::LoadRom(path);
                        }
                    }

                    ui.add_space(10.0);

                    // Quit button
                    if ui
                        .add(self.menu_button("❌  Quit", Color32::from_rgb(120, 60, 60)))
                        .clicked()
                    {
                        action = PauseMenuAction::Quit;
                    }

                    ui.add_space(30.0);

                    // ROM info
                    if let Some(ref info) = self.rom_info {
                        ui.label(
                            RichText::new(format!("Playing: {}", info.title))
                                .font(FontId::proportional(14.0))
                                .color(Color32::from_rgb(150, 150, 160)),
                        );
                    }
                });
            });

        action
    }

    /// Render FPS overlay
    pub fn render_fps_overlay(&self, ctx: &egui::Context) {
        if !self.show_fps {
            return;
        }

        egui::Area::new(egui::Id::new("fps_overlay"))
            .fixed_pos(egui::pos2(5.0, 5.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!("FPS: {:.0}", self.fps))
                        .font(FontId::proportional(12.0))
                        .color(Color32::from_rgb(100, 200, 100))
                        .background_color(Color32::from_rgba_unmultiplied(0, 0, 0, 150)),
                );
            });
    }

    /// Create a styled menu button
    fn menu_button(&self, text: &str, color: Color32) -> egui::Button<'_> {
        egui::Button::new(
            RichText::new(text)
                .font(FontId::proportional(18.0))
                .color(Color32::WHITE),
        )
        .min_size(Vec2::new(180.0, 45.0))
        .fill(color)
        .stroke(Stroke::new(1.0, color.linear_multiply(1.3)))
        .rounding(Rounding::same(8.0))
    }

    /// Add a ROM to recent list
    pub fn add_recent_rom(&mut self, path: PathBuf, title: String) {
        // Remove if already in list
        self.recent_roms.retain(|r| r.path != path);

        // Add to front
        self.recent_roms.insert(
            0,
            RecentRom {
                path,
                title,
            },
        );

        // Keep only last 5
        self.recent_roms.truncate(5);
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

/// Actions from pause menu
#[derive(Debug, Clone)]
pub enum PauseMenuAction {
    None,
    Resume,
    Reset,
    LoadRom(PathBuf),
    Quit,
}

