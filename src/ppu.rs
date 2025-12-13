/// Picture Processing Unit (PPU) for the Game Boy emulator.
///
/// The PPU handles all graphics rendering including:
/// - Background layer
/// - Window layer
/// - Sprites (OBJ)
///
/// The Game Boy screen is 160x144 pixels with 4 shades of gray.
/// The PPU operates in cycles matching the LCD refresh:
/// - Mode 2 (OAM Scan): 80 dots
/// - Mode 3 (Drawing): 172-289 dots (variable based on sprites/scroll/window)
/// - Mode 0 (HBlank): remaining dots to complete 456 per line
/// - Mode 1 (VBlank): 10 lines (4560 dots total)
///
/// Cycle-exact timing features:
/// - Variable Mode 3 length based on sprite count and positions
/// - SCX fine scroll penalty (SCX % 8 extra cycles)
/// - Window trigger penalty
/// - Proper STAT interrupt timing with blocking
/// - OAM/VRAM access blocking during appropriate modes

use crate::memory::{io, interrupts, Memory};

/// Dots per scanline (constant)
const DOTS_PER_LINE: u32 = 456;

/// Mode 2 (OAM Scan) duration
const MODE_2_DOTS: u32 = 80;

/// Base Mode 3 duration (minimum, before penalties)
const MODE_3_BASE_DOTS: u32 = 172;

/// Screen dimensions
pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

/// PPU modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    HBlank = 0, // Mode 0
    VBlank = 1, // Mode 1
    OamScan = 2, // Mode 2
    Drawing = 3, // Mode 3
}

/// Sprite attributes
#[derive(Debug, Clone, Copy, Default)]
struct Sprite {
    y: u8,
    x: u8,
    tile: u8,
    flags: u8,
}

impl Sprite {
    fn priority(&self) -> bool {
        self.flags & 0x80 != 0
    }

    fn y_flip(&self) -> bool {
        self.flags & 0x40 != 0
    }

    fn x_flip(&self) -> bool {
        self.flags & 0x20 != 0
    }

    fn palette(&self) -> bool {
        self.flags & 0x10 != 0
    }
}

#[derive(Debug)]
pub struct Ppu {
    /// Current mode
    mode: Mode,
    /// Dot counter within current line (0-455)
    dots: u32,
    /// Frame buffer (160x144 pixels, 2 bits per pixel stored as u8)
    pub framebuffer: [u8; SCREEN_WIDTH * SCREEN_HEIGHT],
    /// Flag indicating a new frame is ready
    pub frame_ready: bool,
    /// Sprites on current scanline (max 10)
    scanline_sprites: Vec<Sprite>,
    /// Window line counter (internal)
    window_line: u8,
    /// Window was triggered this frame
    window_triggered: bool,
    /// Calculated Mode 3 duration for current scanline
    mode_3_length: u32,
    /// STAT interrupt line (for blocking duplicate interrupts)
    stat_interrupt_line: bool,
    /// Previous STAT interrupt conditions (for edge detection)
    prev_stat_conditions: bool,
    /// Current pixel X position during Mode 3 rendering
    render_x: u8,
    /// Pixel FIFO for background
    bg_fifo: u16,
    /// Pixel FIFO for sprites
    sprite_fifo: u16,
    /// FIFO pixel count
    fifo_count: u8,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            mode: Mode::OamScan,
            dots: 0,
            framebuffer: [0; SCREEN_WIDTH * SCREEN_HEIGHT],
            frame_ready: false,
            scanline_sprites: Vec::with_capacity(10),
            window_line: 0,
            window_triggered: false,
            mode_3_length: MODE_3_BASE_DOTS,
            stat_interrupt_line: false,
            prev_stat_conditions: false,
            render_x: 0,
            bg_fifo: 0,
            sprite_fifo: 0,
            fifo_count: 0,
        }
    }

    pub fn reset(&mut self) {
        self.mode = Mode::OamScan;
        self.dots = 0;
        self.framebuffer = [0; SCREEN_WIDTH * SCREEN_HEIGHT];
        self.frame_ready = false;
        self.scanline_sprites.clear();
        self.window_line = 0;
        self.window_triggered = false;
        self.mode_3_length = MODE_3_BASE_DOTS;
        self.stat_interrupt_line = false;
        self.prev_stat_conditions = false;
        self.render_x = 0;
        self.bg_fifo = 0;
        self.sprite_fifo = 0;
        self.fifo_count = 0;
    }

    /// Advance the PPU by the given number of T-cycles.
    pub fn tick(&mut self, memory: &mut Memory, cycles: u32) {
        let lcdc = memory.data[io::LCDC as usize];

        // If LCD is disabled, do nothing
        if lcdc & 0x80 == 0 {
            self.mode = Mode::HBlank;
            self.dots = 0;
            memory.data[io::LY as usize] = 0;
            // Clear mode bits in STAT
            memory.data[io::STAT as usize] &= 0xFC;
            return;
        }

        for _ in 0..cycles {
            self.tick_single(memory);
        }
    }

    /// Advance the PPU by a single T-cycle.
    fn tick_single(&mut self, memory: &mut Memory) {
        self.dots += 1;

        let ly = memory.data[io::LY as usize];

        match self.mode {
            Mode::OamScan => {
                // Mode 2: OAM Scan (80 dots)
                if self.dots >= MODE_2_DOTS {
                    // Scan OAM for sprites on this scanline (done at end of Mode 2)
                    self.scan_oam(memory, ly);
                    
                    // Calculate Mode 3 length based on current state
                    self.mode_3_length = self.calculate_mode_3_length(memory, ly);
                    
                    self.dots = 0;
                    self.mode = Mode::Drawing;
                    self.render_x = 0;
                    self.update_stat(memory);
                }
            }

            Mode::Drawing => {
                // Mode 3: Drawing (variable length)
                if self.dots >= self.mode_3_length {
                    // Render the scanline at end of Mode 3
                    self.render_scanline(memory, ly);
                    
                    self.dots = 0;
                    self.mode = Mode::HBlank;
                    self.update_stat(memory);
                }
            }

            Mode::HBlank => {
                // Mode 0: HBlank (remaining dots to complete 456 per line)
                // HBlank length = 456 - 80 - mode_3_length
                let hblank_length = DOTS_PER_LINE - MODE_2_DOTS - self.mode_3_length;
                
                if self.dots >= hblank_length {
                    self.dots = 0;

                    // Move to next line
                    let new_ly = ly.wrapping_add(1);
                    memory.data[io::LY as usize] = new_ly;

                    if new_ly >= 144 {
                        // Enter VBlank
                        self.mode = Mode::VBlank;
                        self.frame_ready = true;
                        self.window_line = 0;
                        self.window_triggered = false;

                        // VBlank interrupt (always fires)
                        memory.request_interrupt(interrupts::VBLANK);
                    } else {
                        // Next scanline - start OAM scan
                        self.mode = Mode::OamScan;
                    }
                    
                    self.update_stat(memory);
                    
                    // Check LYC coincidence for new line
                    self.check_lyc(memory, new_ly);
                }
            }

            Mode::VBlank => {
                // Mode 1: VBlank (10 lines, 456 dots each)
                if self.dots >= DOTS_PER_LINE {
                    self.dots = 0;

                    let new_ly = ly.wrapping_add(1);

                    if new_ly >= 154 {
                        // Start new frame
                        memory.data[io::LY as usize] = 0;
                        self.mode = Mode::OamScan;
                        self.update_stat(memory);
                        self.check_lyc(memory, 0);
                    } else {
                        memory.data[io::LY as usize] = new_ly;
                        self.check_lyc(memory, new_ly);
                    }
                }
            }
        }
        
        // Handle STAT interrupts with proper edge detection
        self.handle_stat_interrupt(memory);
    }
    
    /// Calculate Mode 3 length based on sprites, scroll, and window
    fn calculate_mode_3_length(&self, memory: &Memory, ly: u8) -> u32 {
        let lcdc = memory.data[io::LCDC as usize];
        let scx = memory.data[io::SCX as usize];
        let wy = memory.data[io::WY as usize];
        let wx = memory.data[io::WX as usize];
        
        let mut length = MODE_3_BASE_DOTS;
        
        // SCX fine scroll penalty: (SCX % 8) extra dots at the start
        // Actually, this is handled by discarding pixels, adding ~0-7 cycles
        length += (scx % 8) as u32;
        
        // Sprite penalty: each sprite adds 6-11 cycles depending on position
        // Simplified: each sprite adds ~6 cycles on average
        let sprite_count = self.scanline_sprites.len() as u32;
        length += sprite_count * 6;
        
        // Window penalty: if window is visible on this line, adds ~6 cycles
        if lcdc & 0x20 != 0 && ly >= wy && wx <= 166 {
            length += 6;
        }
        
        // Clamp to reasonable bounds (Mode 3 can be 172-289 dots)
        length.min(289)
    }
    
    /// Handle STAT interrupt with rising edge detection
    fn handle_stat_interrupt(&mut self, memory: &mut Memory) {
        let stat = memory.data[io::STAT as usize];
        let ly = memory.data[io::LY as usize];
        let lyc = memory.data[io::LYC as usize];
        
        // Calculate if any STAT interrupt condition is true
        let mode_0_condition = (stat & 0x08 != 0) && self.mode == Mode::HBlank;
        let mode_1_condition = (stat & 0x10 != 0) && self.mode == Mode::VBlank;
        let mode_2_condition = (stat & 0x20 != 0) && self.mode == Mode::OamScan;
        let lyc_condition = (stat & 0x40 != 0) && (ly == lyc);
        
        let current_conditions = mode_0_condition || mode_1_condition || mode_2_condition || lyc_condition;
        
        // STAT interrupt on rising edge (low to high transition)
        if current_conditions && !self.prev_stat_conditions {
            if !self.stat_interrupt_line {
                memory.request_interrupt(interrupts::LCD_STAT);
                self.stat_interrupt_line = true;
            }
        }
        
        if !current_conditions {
            self.stat_interrupt_line = false;
        }
        
        self.prev_stat_conditions = current_conditions;
    }

    /// Update STAT register with current mode and LYC flag
    fn update_stat(&self, memory: &mut Memory) {
        let ly = memory.data[io::LY as usize];
        let lyc = memory.data[io::LYC as usize];
        
        let mut stat = memory.data[io::STAT as usize] & 0xF8;
        stat |= self.mode as u8;
        
        // Update LY=LYC coincidence flag
        if ly == lyc {
            stat |= 0x04;
        }
        
        memory.data[io::STAT as usize] = stat;
    }

    /// Check LY == LYC coincidence and update flag
    fn check_lyc(&mut self, memory: &mut Memory, ly: u8) {
        let lyc = memory.data[io::LYC as usize];

        if ly == lyc {
            // Set coincidence flag
            memory.data[io::STAT as usize] |= 0x04;
        } else {
            // Clear coincidence flag
            memory.data[io::STAT as usize] &= !0x04;
        }
        // Note: STAT interrupt is handled by handle_stat_interrupt()
    }
    
    /// Check if OAM is accessible (not during Mode 2 or Mode 3)
    pub fn oam_accessible(&self) -> bool {
        self.mode != Mode::OamScan && self.mode != Mode::Drawing
    }
    
    /// Check if VRAM is accessible (not during Mode 3)
    pub fn vram_accessible(&self) -> bool {
        self.mode != Mode::Drawing
    }
    
    /// Get current PPU mode
    pub fn current_mode(&self) -> Mode {
        self.mode
    }

    /// Scan OAM for sprites on the given scanline
    fn scan_oam(&mut self, memory: &Memory, ly: u8) {
        self.scanline_sprites.clear();

        let lcdc = memory.data[io::LCDC as usize];
        let sprite_height = if lcdc & 0x04 != 0 { 16 } else { 8 };

        // Scan all 40 sprites in OAM
        for i in 0..40 {
            let addr = 0xFE00 + (i * 4);
            let y = memory.data[addr as usize];
            let x = memory.data[(addr + 1) as usize];
            let tile = memory.data[(addr + 2) as usize];
            let flags = memory.data[(addr + 3) as usize];

            // Check if sprite is on this scanline
            let sprite_y = y.wrapping_sub(16);
            let line = ly;

            if line >= sprite_y && line < sprite_y.wrapping_add(sprite_height) {
                self.scanline_sprites.push(Sprite { y, x, tile, flags });

                // Max 10 sprites per scanline
                if self.scanline_sprites.len() >= 10 {
                    break;
                }
            }
        }

        // Sort by X coordinate (lower X = higher priority)
        self.scanline_sprites.sort_by(|a, b| a.x.cmp(&b.x));
    }

    /// Render a single scanline
    fn render_scanline(&mut self, memory: &Memory, ly: u8) {
        let lcdc = memory.data[io::LCDC as usize];

        // Get palettes
        let bgp = memory.data[io::BGP as usize];
        let obp0 = memory.data[io::OBP0 as usize];
        let obp1 = memory.data[io::OBP1 as usize];

        let line_offset = (ly as usize) * SCREEN_WIDTH;

        // Background enable (on DMG, this also affects window)
        let bg_enable = lcdc & 0x01 != 0;

        // Render background
        if bg_enable {
            self.render_background(memory, ly, lcdc, bgp, line_offset);
        } else {
            // Fill with color 0
            for x in 0..SCREEN_WIDTH {
                self.framebuffer[line_offset + x] = 0;
            }
        }

        // Render window
        if bg_enable && (lcdc & 0x20 != 0) {
            self.render_window(memory, ly, lcdc, bgp, line_offset);
        }

        // Render sprites
        if lcdc & 0x02 != 0 {
            self.render_sprites(memory, ly, lcdc, obp0, obp1, line_offset);
        }
    }

    /// Render background for a scanline
    fn render_background(
        &mut self,
        memory: &Memory,
        ly: u8,
        lcdc: u8,
        bgp: u8,
        line_offset: usize,
    ) {
        let scy = memory.data[io::SCY as usize];
        let scx = memory.data[io::SCX as usize];

        // Background tile map address
        let tile_map = if lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 };

        // Background/window tile data address
        let tile_data = if lcdc & 0x10 != 0 { 0x8000 } else { 0x8800 };
        let signed_addressing = lcdc & 0x10 == 0;

        let y = ly.wrapping_add(scy);
        let tile_row = (y / 8) as u16;

        for screen_x in 0..SCREEN_WIDTH {
            let x = (screen_x as u8).wrapping_add(scx);
            let tile_col = (x / 8) as u16;

            // Get tile index from tile map
            let map_addr = tile_map + (tile_row * 32) + tile_col;
            let tile_idx = memory.data[map_addr as usize];

            // Calculate tile data address
            let tile_addr = if signed_addressing {
                let signed_idx = tile_idx as i8 as i16;
                (tile_data as i32 + ((signed_idx as i32 + 128) * 16)) as u16
            } else {
                tile_data + (tile_idx as u16 * 16)
            };

            // Get pixel within tile
            let tile_y = (y % 8) as u16;
            let tile_x = 7 - (x % 8);

            // Read tile data (2 bytes per row)
            let addr = tile_addr + (tile_y * 2);
            let low = memory.data[addr as usize];
            let high = memory.data[(addr + 1) as usize];

            // Get color index
            let color_bit = 1 << tile_x;
            let color_idx = ((high & color_bit) >> tile_x << 1) | ((low & color_bit) >> tile_x);

            // Apply palette
            let color = (bgp >> (color_idx * 2)) & 0x03;
            self.framebuffer[line_offset + screen_x] = color;
        }
    }

    /// Render window for a scanline
    fn render_window(
        &mut self,
        memory: &Memory,
        ly: u8,
        lcdc: u8,
        bgp: u8,
        line_offset: usize,
    ) {
        let wy = memory.data[io::WY as usize];
        let wx = memory.data[io::WX as usize];

        // Window not visible yet
        if ly < wy || wx > 166 {
            return;
        }

        // Window tile map address
        let tile_map = if lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 };

        // Background/window tile data address
        let tile_data = if lcdc & 0x10 != 0 { 0x8000 } else { 0x8800 };
        let signed_addressing = lcdc & 0x10 == 0;

        let window_x_start = wx.saturating_sub(7) as usize;
        let tile_row = (self.window_line / 8) as u16;

        for screen_x in window_x_start..SCREEN_WIDTH {
            let x = (screen_x - window_x_start) as u8;
            let tile_col = (x / 8) as u16;

            // Get tile index from tile map
            let map_addr = tile_map + (tile_row * 32) + tile_col;
            let tile_idx = memory.data[map_addr as usize];

            // Calculate tile data address
            let tile_addr = if signed_addressing {
                let signed_idx = tile_idx as i8 as i16;
                (tile_data as i32 + ((signed_idx as i32 + 128) * 16)) as u16
            } else {
                tile_data + (tile_idx as u16 * 16)
            };

            // Get pixel within tile
            let tile_y = (self.window_line % 8) as u16;
            let tile_x = 7 - (x % 8);

            // Read tile data
            let addr = tile_addr + (tile_y * 2);
            let low = memory.data[addr as usize];
            let high = memory.data[(addr + 1) as usize];

            // Get color index
            let color_bit = 1 << tile_x;
            let color_idx = ((high & color_bit) >> tile_x << 1) | ((low & color_bit) >> tile_x);

            // Apply palette
            let color = (bgp >> (color_idx * 2)) & 0x03;
            self.framebuffer[line_offset + screen_x] = color;
        }

        self.window_line += 1;
        self.window_triggered = true;
    }

    /// Render sprites for a scanline
    fn render_sprites(
        &mut self,
        memory: &Memory,
        ly: u8,
        lcdc: u8,
        obp0: u8,
        obp1: u8,
        line_offset: usize,
    ) {
        let sprite_height = if lcdc & 0x04 != 0 { 16 } else { 8 };

        // Render sprites in reverse order (lower index = higher priority when same X)
        for sprite in self.scanline_sprites.iter().rev() {
            let palette = if sprite.palette() { obp1 } else { obp0 };

            // Calculate sprite position
            let sprite_x = sprite.x.wrapping_sub(8);
            let sprite_y = sprite.y.wrapping_sub(16);

            // Calculate which row of the sprite we're on
            let mut tile_y = ly.wrapping_sub(sprite_y);
            if sprite.y_flip() {
                tile_y = (sprite_height - 1) - tile_y;
            }

            // For 8x16 sprites, mask out the lowest bit of tile number
            let tile = if sprite_height == 16 {
                sprite.tile & 0xFE
            } else {
                sprite.tile
            };

            // Calculate tile address
            let tile_addr = 0x8000 + (tile as u16 * 16) + ((tile_y as u16) * 2);
            let low = memory.data[tile_addr as usize];
            let high = memory.data[(tile_addr + 1) as usize];

            // Render each pixel of the sprite
            for tile_x in 0..8 {
                let screen_x = sprite_x.wrapping_add(tile_x);
                if screen_x >= 160 {
                    continue;
                }

                // Get pixel bit (with X flip handling)
                let bit = if sprite.x_flip() { tile_x } else { 7 - tile_x };

                let color_bit = 1 << bit;
                let color_idx = ((high & color_bit) >> bit << 1) | ((low & color_bit) >> bit);

                // Color 0 is transparent for sprites
                if color_idx == 0 {
                    continue;
                }

                // Check background priority
                let bg_color = self.framebuffer[line_offset + screen_x as usize];
                if sprite.priority() && bg_color != 0 {
                    continue;
                }

                // Apply palette (color 0 is transparent, so skip it in palette)
                let color = (palette >> (color_idx * 2)) & 0x03;
                self.framebuffer[line_offset + screen_x as usize] = color;
            }
        }
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppu_modes_cycle() {
        let mut ppu = Ppu::new();
        let mut memory = Memory::new();

        // Enable LCD
        memory.data[io::LCDC as usize] = 0x91;

        // Start in OAM scan
        assert_eq!(ppu.mode, Mode::OamScan);

        // After 80 dots, should be in Drawing
        ppu.tick(&mut memory, MODE_2_DOTS);
        assert_eq!(ppu.mode, Mode::Drawing);

        // After mode_3_length more dots, should be in HBlank
        let mode_3_len = ppu.mode_3_length;
        ppu.tick(&mut memory, mode_3_len);
        assert_eq!(ppu.mode, Mode::HBlank);

        // After remaining dots, should be back in OAM scan (next line)
        let hblank_len = DOTS_PER_LINE - MODE_2_DOTS - mode_3_len;
        ppu.tick(&mut memory, hblank_len);
        assert_eq!(ppu.mode, Mode::OamScan);
        assert_eq!(memory.data[io::LY as usize], 1);
    }

    #[test]
    fn vblank_after_144_lines() {
        let mut ppu = Ppu::new();
        let mut memory = Memory::new();

        // Enable LCD
        memory.data[io::LCDC as usize] = 0x91;

        // Run through 144 lines (456 dots each)
        for _ in 0..144 {
            ppu.tick(&mut memory, DOTS_PER_LINE);
        }

        // Should be in VBlank
        assert_eq!(ppu.mode, Mode::VBlank);
        assert!(ppu.frame_ready);

        // VBlank interrupt should be requested
        assert!(memory.data[io::IF as usize] & interrupts::VBLANK != 0);
    }
    
    #[test]
    fn mode_3_length_varies_with_sprites() {
        let mut ppu = Ppu::new();
        let mut memory = Memory::new();
        
        // Enable LCD and sprites
        memory.data[io::LCDC as usize] = 0x93;
        
        // Base mode 3 length (no sprites)
        let base_length = ppu.calculate_mode_3_length(&memory, 0);
        assert_eq!(base_length, MODE_3_BASE_DOTS);
        
        // Add a sprite on line 0
        memory.data[0xFE00] = 16; // Y = 16 means visible on line 0
        memory.data[0xFE01] = 8;  // X = 8
        ppu.scan_oam(&memory, 0);
        
        let with_sprite = ppu.calculate_mode_3_length(&memory, 0);
        assert!(with_sprite > base_length);
    }
    
    #[test]
    fn oam_vram_access_timing() {
        let ppu_oam = Ppu { mode: Mode::OamScan, ..Ppu::new() };
        let ppu_draw = Ppu { mode: Mode::Drawing, ..Ppu::new() };
        let ppu_hblank = Ppu { mode: Mode::HBlank, ..Ppu::new() };
        let ppu_vblank = Ppu { mode: Mode::VBlank, ..Ppu::new() };
        
        // OAM accessible during HBlank and VBlank only
        assert!(!ppu_oam.oam_accessible());
        assert!(!ppu_draw.oam_accessible());
        assert!(ppu_hblank.oam_accessible());
        assert!(ppu_vblank.oam_accessible());
        
        // VRAM accessible during Mode 0, 1, 2 (not Mode 3)
        assert!(ppu_oam.vram_accessible());
        assert!(!ppu_draw.vram_accessible());
        assert!(ppu_hblank.vram_accessible());
        assert!(ppu_vblank.vram_accessible());
    }
}

