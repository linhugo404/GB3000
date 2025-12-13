//! Simple UI module for GB3000 emulator
//!
//! Uses software rendering with a built-in bitmap font.

use rfd::FileDialog;
use std::path::PathBuf;

/// UI state
#[derive(Debug, Clone, PartialEq)]
pub enum EmulatorState {
    StartScreen,
    Running,
    Paused,
}

/// Recent ROM entry
#[derive(Debug, Clone)]
pub struct RecentRom {
    pub path: PathBuf,
    pub title: String,
}

/// ROM information
#[derive(Debug, Clone)]
pub struct RomInfo {
    pub title: String,
    pub cart_type: String,
    pub rom_size: String,
    pub ram_size: String,
}

/// Main UI controller
pub struct Ui {
    pub state: EmulatorState,
    pub recent_roms: Vec<RecentRom>,
    pub current_rom: Option<PathBuf>,
    pub rom_info: Option<RomInfo>,
    pub show_fps: bool,
    pub fps: f64,
    pub error_message: Option<String>,
    /// Mouse position
    mouse_x: f32,
    mouse_y: f32,
    /// Mouse button state
    mouse_down: bool,
    mouse_clicked: bool,
}

/// Actions from UI
#[derive(Debug, Clone)]
pub enum UiAction {
    None,
    OpenFile,
    LoadRom(PathBuf),
    Resume,
    Reset,
    Quit,
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
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_down: false,
            mouse_clicked: false,
        }
    }

    /// Update mouse state
    pub fn update_mouse(&mut self, x: f32, y: f32, down: bool) {
        self.mouse_x = x;
        self.mouse_y = y;
        // Detect click (transition from not pressed to pressed)
        self.mouse_clicked = down && !self.mouse_down;
        self.mouse_down = down;
    }

    /// Open file dialog
    pub fn open_file_dialog() -> Option<PathBuf> {
        FileDialog::new()
            .add_filter("Game Boy ROMs", &["gb", "gbc", "GB", "GBC"])
            .add_filter("All files", &["*"])
            .set_title("Select a Game Boy ROM")
            .pick_file()
    }

    /// Add ROM to recent list
    pub fn add_recent_rom(&mut self, path: PathBuf, title: String) {
        self.recent_roms.retain(|r| r.path != path);
        self.recent_roms.insert(0, RecentRom { path, title });
        self.recent_roms.truncate(5);
    }

    /// Render start screen and return action
    pub fn render_start_screen(&mut self, buffer: &mut [u32], width: usize, height: usize) -> UiAction {
        // Fill background
        fill_rect(buffer, width, 0, 0, width, height, 0xFF1a1a2e);

        // Title
        let title = "GB3000";
        let title_x = (width - title.len() * 24) / 2;
        draw_text_large(buffer, width, title_x, 80, title, 0xFF4ade80);

        // Subtitle
        let subtitle = "Game Boy Emulator";
        let sub_x = (width - subtitle.len() * 8) / 2;
        draw_text(buffer, width, sub_x, 140, subtitle, 0xFF9ca3af);

        // Open ROM button
        let btn_w = 200;
        let btn_h = 50;
        let btn_x = (width - btn_w) / 2;
        let btn_y = 200;
        
        let btn_hover = self.is_mouse_in_rect(btn_x, btn_y, btn_w, btn_h);
        let btn_color = if btn_hover { 0xFF22c55e } else { 0xFF16a34a };
        
        fill_rect(buffer, width, btn_x, btn_y, btn_w, btn_h, btn_color);
        draw_rect(buffer, width, btn_x, btn_y, btn_w, btn_h, 0xFF4ade80);
        
        let text = "Open ROM";
        let text_x = btn_x + (btn_w - text.len() * 8) / 2;
        let text_y = btn_y + (btn_h - 8) / 2;
        draw_text(buffer, width, text_x, text_y, text, 0xFFffffff);

        if btn_hover && self.mouse_clicked {
            return UiAction::OpenFile;
        }

        // Recent ROMs
        if !self.recent_roms.is_empty() {
            draw_text(buffer, width, (width - 11 * 8) / 2, 280, "Recent ROMs", 0xFF6b7280);
            
            for (i, recent) in self.recent_roms.iter().enumerate() {
                let y = 310 + i * 35;
                let item_w = 300;
                let item_x = (width - item_w) / 2;
                
                let hover = self.is_mouse_in_rect(item_x, y, item_w, 30);
                let bg_color = if hover { 0xFF374151 } else { 0xFF1f2937 };
                
                fill_rect(buffer, width, item_x, y, item_w, 30, bg_color);
                
                let display_title = if recent.title.len() > 30 {
                    format!("{}...", &recent.title[..27])
                } else {
                    recent.title.clone()
                };
                let tx = item_x + 10;
                let ty = y + 11;
                draw_text(buffer, width, tx, ty, &display_title, 0xFFd1d5db);
                
                if hover && self.mouse_clicked {
                    return UiAction::LoadRom(recent.path.clone());
                }
            }
        }

        // Controls hint
        let controls = "Arrow Keys = D-Pad | Z = A | X = B | Enter = Start | Space = Select | Esc = Menu";
        let cx = (width.saturating_sub(controls.len() * 6)) / 2;
        draw_text_small(buffer, width, cx, height - 40, controls, 0xFF4b5563);

        // Error message
        if let Some(ref error) = self.error_message {
            let ex = (width.saturating_sub(error.len() * 8)) / 2;
            draw_text(buffer, width, ex, height - 80, error, 0xFFef4444);
        }

        UiAction::None
    }

    /// Render pause menu overlay
    pub fn render_pause_menu(&mut self, buffer: &mut [u32], width: usize, height: usize) -> UiAction {
        // Darken background
        for pixel in buffer.iter_mut() {
            let r = ((*pixel >> 16) & 0xFF) / 3;
            let g = ((*pixel >> 8) & 0xFF) / 3;
            let b = (*pixel & 0xFF) / 3;
            *pixel = 0xFF000000 | (r << 16) | (g << 8) | b;
        }

        // Title
        let title = "PAUSED";
        let tx = (width - title.len() * 16) / 2;
        draw_text_large(buffer, width, tx, 100, title, 0xFFffffff);

        // Buttons
        let buttons = [
            ("Resume", UiAction::Resume, 0xFF22c55e),
            ("Reset", UiAction::Reset, 0xFF3b82f6),
            ("Open ROM", UiAction::OpenFile, 0xFF6366f1),
            ("Quit", UiAction::Quit, 0xFFef4444),
        ];

        let btn_w = 180;
        let btn_h = 45;
        let btn_x = (width - btn_w) / 2;
        let start_y = 180;

        for (i, (text, action, color)) in buttons.iter().enumerate() {
            let btn_y = start_y + i * 55;
            
            let hover = self.is_mouse_in_rect(btn_x, btn_y, btn_w, btn_h);
            let bg = if hover { lighten_color(*color) } else { *color };
            
            fill_rect(buffer, width, btn_x, btn_y, btn_w, btn_h, bg);
            draw_rect(buffer, width, btn_x, btn_y, btn_w, btn_h, lighten_color(*color));
            
            let text_x = btn_x + (btn_w - text.len() * 8) / 2;
            let text_y = btn_y + (btn_h - 8) / 2;
            draw_text(buffer, width, text_x, text_y, text, 0xFFffffff);
            
            if hover && self.mouse_clicked {
                return action.clone();
            }
        }

        // ROM info
        if let Some(ref info) = self.rom_info {
            let info_text = format!("Playing: {}", info.title);
            let ix = (width.saturating_sub(info_text.len() * 6)) / 2;
            draw_text_small(buffer, width, ix, height - 50, &info_text, 0xFF9ca3af);
        }

        UiAction::None
    }

    /// Render FPS overlay
    pub fn render_fps(&self, buffer: &mut [u32], width: usize) {
        if !self.show_fps {
            return;
        }
        let fps_text = format!("FPS: {:.0}", self.fps);
        // Background
        fill_rect(buffer, width, 5, 5, fps_text.len() * 6 + 8, 14, 0x80000000);
        draw_text_small(buffer, width, 9, 8, &fps_text, 0xFF4ade80);
    }

    fn is_mouse_in_rect(&self, x: usize, y: usize, w: usize, h: usize) -> bool {
        let mx = self.mouse_x as usize;
        let my = self.mouse_y as usize;
        mx >= x && mx < x + w && my >= y && my < y + h
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Drawing primitives
// ============================================================================

fn fill_rect(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < buf_width && py < buffer.len() / buf_width {
                let idx = py * buf_width + px;
                if idx < buffer.len() {
                    buffer[idx] = blend(buffer[idx], color);
                }
            }
        }
    }
}

fn draw_rect(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    // Top and bottom
    for dx in 0..w {
        set_pixel(buffer, buf_width, x + dx, y, color);
        set_pixel(buffer, buf_width, x + dx, y + h - 1, color);
    }
    // Left and right
    for dy in 0..h {
        set_pixel(buffer, buf_width, x, y + dy, color);
        set_pixel(buffer, buf_width, x + w - 1, y + dy, color);
    }
}

fn set_pixel(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, color: u32) {
    if x < buf_width {
        let idx = y * buf_width + x;
        if idx < buffer.len() {
            buffer[idx] = color;
        }
    }
}

fn blend(dst: u32, src: u32) -> u32 {
    let sa = ((src >> 24) & 0xFF) as f32 / 255.0;
    if sa >= 1.0 {
        return src;
    }
    if sa <= 0.0 {
        return dst;
    }
    
    let sr = ((src >> 16) & 0xFF) as f32;
    let sg = ((src >> 8) & 0xFF) as f32;
    let sb = (src & 0xFF) as f32;
    
    let dr = ((dst >> 16) & 0xFF) as f32;
    let dg = ((dst >> 8) & 0xFF) as f32;
    let db = (dst & 0xFF) as f32;
    
    let r = (sr * sa + dr * (1.0 - sa)) as u32;
    let g = (sg * sa + dg * (1.0 - sa)) as u32;
    let b = (sb * sa + db * (1.0 - sa)) as u32;
    
    0xFF000000 | (r << 16) | (g << 8) | b
}

fn lighten_color(color: u32) -> u32 {
    let r = ((color >> 16) & 0xFF).min(200) + 40;
    let g = ((color >> 8) & 0xFF).min(200) + 40;
    let b = (color & 0xFF).min(200) + 40;
    0xFF000000 | (r << 16) | (g << 8) | b
}

// ============================================================================
// Bitmap font (5x7 characters)
// ============================================================================

/// Draw text with 8x8 character size
fn draw_text(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, text: &str, color: u32) {
    for (i, ch) in text.chars().enumerate() {
        draw_char(buffer, buf_width, x + i * 8, y, ch, color, 1);
    }
}

/// Draw text with 6x6 character size (small)
fn draw_text_small(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, text: &str, color: u32) {
    for (i, ch) in text.chars().enumerate() {
        draw_char_small(buffer, buf_width, x + i * 6, y, ch, color);
    }
}

/// Draw text with 16x14 character size (large)
fn draw_text_large(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, text: &str, color: u32) {
    for (i, ch) in text.chars().enumerate() {
        draw_char(buffer, buf_width, x + i * 24, y, ch, color, 3);
    }
}

/// Draw a single character at scale
fn draw_char(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, ch: char, color: u32, scale: usize) {
    let bitmap = get_char_bitmap(ch);
    for (row, &bits) in bitmap.iter().enumerate() {
        for col in 0..5 {
            if (bits >> (4 - col)) & 1 == 1 {
                for sy in 0..scale {
                    for sx in 0..scale {
                        set_pixel(buffer, buf_width, x + col * scale + sx, y + row * scale + sy, color);
                    }
                }
            }
        }
    }
}

/// Draw a small character (no scaling, just the 5x7 bitmap)
fn draw_char_small(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, ch: char, color: u32) {
    let bitmap = get_char_bitmap(ch);
    for (row, &bits) in bitmap.iter().enumerate() {
        for col in 0..5 {
            if (bits >> (4 - col)) & 1 == 1 {
                set_pixel(buffer, buf_width, x + col, y + row, color);
            }
        }
    }
}

/// Get 5x7 bitmap for a character (each byte is one row, bits 4-0 are pixels)
fn get_char_bitmap(ch: char) -> [u8; 7] {
    match ch {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        'a' => [0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111],
        'b' => [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b11110],
        'c' => [0b00000, 0b00000, 0b01110, 0b10000, 0b10000, 0b10001, 0b01110],
        'd' => [0b00001, 0b00001, 0b01101, 0b10011, 0b10001, 0b10001, 0b01111],
        'e' => [0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110],
        'f' => [0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000],
        'g' => [0b00000, 0b01111, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110],
        'h' => [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001],
        'i' => [0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110],
        'j' => [0b00010, 0b00000, 0b00110, 0b00010, 0b00010, 0b10010, 0b01100],
        'k' => [0b10000, 0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010],
        'l' => [0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'm' => [0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10001, 0b10001],
        'n' => [0b00000, 0b00000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001],
        'o' => [0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        'p' => [0b00000, 0b00000, 0b11110, 0b10001, 0b11110, 0b10000, 0b10000],
        'q' => [0b00000, 0b00000, 0b01101, 0b10011, 0b01111, 0b00001, 0b00001],
        'r' => [0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000],
        's' => [0b00000, 0b00000, 0b01110, 0b10000, 0b01110, 0b00001, 0b11110],
        't' => [0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110],
        'u' => [0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101],
        'v' => [0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'w' => [0b00000, 0b00000, 0b10001, 0b10001, 0b10101, 0b10101, 0b01010],
        'x' => [0b00000, 0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001],
        'y' => [0b00000, 0b00000, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110],
        'z' => [0b00000, 0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        ' ' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        ',' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00100, 0b01000],
        ':' => [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000],
        ';' => [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b00100, 0b01000],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '+' => [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000],
        '=' => [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        '\\' => [0b10000, 0b01000, 0b01000, 0b00100, 0b00010, 0b00010, 0b00001],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        '[' => [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110],
        ']' => [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110],
        '<' => [0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010],
        '>' => [0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000],
        '|' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        '_' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111],
        '\'' => [0b00100, 0b00100, 0b01000, 0b00000, 0b00000, 0b00000, 0b00000],
        '"' => [0b01010, 0b01010, 0b10100, 0b00000, 0b00000, 0b00000, 0b00000],
        _ => [0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111], // Box for unknown
    }
}
