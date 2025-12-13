//! GB3000 Desktop UI
//!
//! A graphical frontend for the GB3000 Game Boy emulator.
//! This binary uses the gb3000 library for emulation.

mod test_runner;
mod ui;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gb3000::{palettes, Button, Emulator, SCREEN_HEIGHT, SCREEN_WIDTH};
use minifb::{Key, Window, WindowOptions};
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use ui::{EmulatorState, PauseMenuAction, Ui};

/// Target frame time in nanoseconds (~16.74ms for 60 FPS)
const FRAME_TIME_NS: u64 = 1_000_000_000 / 60;

/// UI window dimensions (Game Boy screen scaled up)
const UI_WIDTH: usize = 640;
const UI_HEIGHT: usize = 576;

/// Audio buffer size
const AUDIO_BUFFER_SIZE: usize = 4096;

/// Set up audio output stream using cpal
fn setup_audio(
    audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            eprintln!("Warning: No audio output device found");
            return None;
        }
    };

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let mut last_sample = 0.0f32;

    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut buffer = audio_buffer.lock().unwrap();

                for sample in data.iter_mut() {
                    if let Some(s) = buffer.pop_front() {
                        *sample = s;
                        last_sample = s;
                    } else {
                        // Fade to silence on underrun
                        last_sample *= 0.9;
                        *sample = last_sample;
                    }
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        )
        .ok()?;

    stream.play().ok()?;
    Some(stream)
}

/// Scale the Game Boy framebuffer to UI size
fn scale_framebuffer(src: &[u8], dst: &mut [u32], palette: &[u32; 4]) {
    let scale_x = UI_WIDTH / SCREEN_WIDTH;
    let scale_y = UI_HEIGHT / SCREEN_HEIGHT;

    for y in 0..UI_HEIGHT {
        for x in 0..UI_WIDTH {
            let src_x = x / scale_x;
            let src_y = y / scale_y;
            let src_idx = src_y * SCREEN_WIDTH + src_x;
            let dst_idx = y * UI_WIDTH + x;
            if src_idx < src.len() && dst_idx < dst.len() {
                let color_idx = src[src_idx] as usize & 0x03;
                dst[dst_idx] = palette[color_idx];
            }
        }
    }
}

/// Load a ROM file
fn load_rom_file(path: &PathBuf) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("Failed to read ROM: {}", e))
}

/// Update emulator input from window keys
fn update_input(emulator: &mut Emulator, window: &Window) {
    emulator.set_button(Button::Right, window.is_key_down(Key::Right));
    emulator.set_button(Button::Left, window.is_key_down(Key::Left));
    emulator.set_button(Button::Up, window.is_key_down(Key::Up));
    emulator.set_button(Button::Down, window.is_key_down(Key::Down));
    emulator.set_button(Button::A, window.is_key_down(Key::Z));
    emulator.set_button(Button::B, window.is_key_down(Key::X));
    emulator.set_button(Button::Select, window.is_key_down(Key::Space));
    emulator.set_button(Button::Start, window.is_key_down(Key::Enter));
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Check for test mode
    if args.len() > 1 && args[1] == "--test" {
        run_test_mode(&args);
        return;
    }

    // Check for direct ROM argument
    let initial_rom: Option<PathBuf> = if args.len() > 1 {
        Some(PathBuf::from(&args[1]))
    } else {
        None
    };

    // Create window
    let mut window = Window::new(
        "GB3000 - Game Boy Emulator",
        UI_WIDTH,
        UI_HEIGHT,
        WindowOptions {
            resize: false,
            ..WindowOptions::default()
        },
    )
    .expect("Failed to create window");

    window.set_target_fps(60);

    // Create egui context with dark theme
    let mut egui_ctx = egui::Context::default();
    let mut visuals = egui::Visuals::dark();
    visuals.window_rounding = egui::Rounding::same(8.0);
    egui_ctx.set_visuals(visuals);

    // Create UI state
    let mut ui = Ui::new();

    // Create emulator
    let mut emulator = Emulator::new();

    // Selected palette
    let palette = palettes::GRAYSCALE;

    // Set up audio
    let audio_buffer: Arc<Mutex<VecDeque<f32>>> =
        Arc::new(Mutex::new(VecDeque::with_capacity(AUDIO_BUFFER_SIZE)));
    let _audio_stream = setup_audio(Arc::clone(&audio_buffer), emulator.audio_sample_rate());

    // Framebuffers
    let mut scaled_buffer = vec![0u32; UI_WIDTH * UI_HEIGHT];
    let mut egui_buffer = vec![0u32; UI_WIDTH * UI_HEIGHT];

    // FPS tracking
    let mut frame_count = 0u64;
    let mut last_fps_time = Instant::now();
    let start_time = Instant::now();

    // Handle initial ROM if provided
    if let Some(path) = initial_rom {
        match load_rom_file(&path) {
            Ok(rom) => {
                if let Some(info) = Emulator::parse_rom_info(&rom) {
                    ui.add_recent_rom(path.clone(), info.title.clone());
                    ui.rom_info = Some(ui::RomInfo {
                        title: info.title,
                        cart_type: info.cart_type,
                        rom_size: info.rom_size,
                        ram_size: info.ram_size,
                    });
                }
                emulator.load_rom(&rom);
                emulator.reset();
                ui.current_rom = Some(path);
                ui.state = EmulatorState::Running;
            }
            Err(e) => {
                ui.error_message = Some(e);
            }
        }
    }

    // Main loop
    while window.is_open() {
        let frame_start = Instant::now();

        // Handle Escape key for pause/quit
        if window.is_key_pressed(Key::Escape, minifb::KeyRepeat::No) {
            match ui.state {
                EmulatorState::StartScreen => break,
                EmulatorState::Running => ui.state = EmulatorState::Paused,
                EmulatorState::Paused => ui.state = EmulatorState::Running,
            }
        }

        // Gather input for egui
        let raw_input = gather_egui_input(&window, &egui_ctx);
        egui_ctx.begin_frame(raw_input);

        match ui.state {
            EmulatorState::StartScreen => {
                // Fill with dark background
                for pixel in scaled_buffer.iter_mut() {
                    *pixel = 0xFF12121B;
                }

                // Render start screen UI
                if let Some(path) = ui.render_start_screen(&egui_ctx) {
                    match load_rom_file(&path) {
                        Ok(rom) => {
                            if let Some(info) = Emulator::parse_rom_info(&rom) {
                                ui.add_recent_rom(path.clone(), info.title.clone());
                                ui.rom_info = Some(ui::RomInfo {
                                    title: info.title,
                                    cart_type: info.cart_type,
                                    rom_size: info.rom_size,
                                    ram_size: info.ram_size,
                                });
                            }
                            emulator = Emulator::new();
                            emulator.load_rom(&rom);
                            emulator.reset();
                            ui.current_rom = Some(path);
                            ui.state = EmulatorState::Running;
                            ui.error_message = None;
                        }
                        Err(e) => {
                            ui.error_message = Some(e);
                        }
                    }
                }
            }

            EmulatorState::Running => {
                // Update input
                update_input(&mut emulator, &window);

                // Run emulation
                emulator.run_frame();

                // Get and scale framebuffer
                scale_framebuffer(emulator.framebuffer(), &mut scaled_buffer, &palette);

                // Send audio samples
                let samples = emulator.audio_samples();
                if !samples.is_empty() {
                    if let Ok(mut buffer) = audio_buffer.lock() {
                        for sample in samples {
                            buffer.push_back(sample);
                        }
                        while buffer.len() > AUDIO_BUFFER_SIZE {
                            buffer.pop_front();
                        }
                    }
                }

                // Render FPS overlay
                ui.render_fps_overlay(&egui_ctx);
            }

            EmulatorState::Paused => {
                // Show game in background
                scale_framebuffer(emulator.framebuffer(), &mut scaled_buffer, &palette);

                // Render pause menu
                match ui.render_pause_menu(&egui_ctx) {
                    PauseMenuAction::Resume => {
                        ui.state = EmulatorState::Running;
                    }
                    PauseMenuAction::Reset => {
                        emulator.reset();
                        ui.state = EmulatorState::Running;
                    }
                    PauseMenuAction::LoadRom(path) => match load_rom_file(&path) {
                        Ok(rom) => {
                            if let Some(info) = Emulator::parse_rom_info(&rom) {
                                ui.add_recent_rom(path.clone(), info.title.clone());
                                ui.rom_info = Some(ui::RomInfo {
                                    title: info.title,
                                    cart_type: info.cart_type,
                                    rom_size: info.rom_size,
                                    ram_size: info.ram_size,
                                });
                            }
                            emulator = Emulator::new();
                            emulator.load_rom(&rom);
                            emulator.reset();
                            ui.current_rom = Some(path);
                            ui.state = EmulatorState::Running;
                        }
                        Err(e) => {
                            ui.error_message = Some(e);
                        }
                    },
                    PauseMenuAction::Quit => break,
                    PauseMenuAction::None => {}
                }
            }
        }

        // End egui frame and render
        let full_output = egui_ctx.end_frame();

        // Paint egui on top of game
        egui_buffer.copy_from_slice(&scaled_buffer);
        paint_egui(
            &egui_ctx,
            &full_output,
            &mut egui_buffer,
            UI_WIDTH,
            UI_HEIGHT,
        );

        // Update window
        window
            .update_with_buffer(&egui_buffer, UI_WIDTH, UI_HEIGHT)
            .expect("Failed to update window");

        // Update FPS
        frame_count += 1;
        if last_fps_time.elapsed() >= Duration::from_secs(1) {
            ui.fps = frame_count as f64 / start_time.elapsed().as_secs_f64();
            last_fps_time = Instant::now();
        }

        // Frame timing
        let frame_time = frame_start.elapsed();
        let target_time = Duration::from_nanos(FRAME_TIME_NS);
        if frame_time < target_time {
            spin_sleep::sleep(target_time - frame_time);
        }
    }
}

/// Gather input for egui from minifb window
fn gather_egui_input(window: &Window, _ctx: &egui::Context) -> egui::RawInput {
    let mut raw_input = egui::RawInput::default();

    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(UI_WIDTH as f32, UI_HEIGHT as f32),
    ));

    if let Some((x, y)) = window.get_mouse_pos(minifb::MouseMode::Clamp) {
        raw_input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(x, y)));
    }

    if window.get_mouse_down(minifb::MouseButton::Left) {
        if let Some((x, y)) = window.get_mouse_pos(minifb::MouseMode::Clamp) {
            raw_input.events.push(egui::Event::PointerButton {
                pos: egui::pos2(x, y),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            });
        }
    }

    raw_input
}

/// Paint egui output onto buffer
fn paint_egui(
    ctx: &egui::Context,
    full_output: &egui::FullOutput,
    buffer: &mut [u32],
    width: usize,
    height: usize,
) {
    let shapes = ctx.tessellate(full_output.shapes.clone(), ctx.pixels_per_point());

    for clipped in shapes {
        let mesh = match &clipped.primitive {
            egui::epaint::Primitive::Mesh(mesh) => mesh,
            _ => continue,
        };

        for triangle in mesh.indices.chunks(3) {
            if triangle.len() < 3 {
                continue;
            }

            let v0 = &mesh.vertices[triangle[0] as usize];
            let v1 = &mesh.vertices[triangle[1] as usize];
            let v2 = &mesh.vertices[triangle[2] as usize];

            let min_x = v0.pos.x.min(v1.pos.x).min(v2.pos.x).max(0.0) as usize;
            let max_x = v0.pos.x.max(v1.pos.x).max(v2.pos.x).min(width as f32) as usize;
            let min_y = v0.pos.y.min(v1.pos.y).min(v2.pos.y).max(0.0) as usize;
            let max_y = v0.pos.y.max(v1.pos.y).max(v2.pos.y).min(height as f32) as usize;

            for y in min_y..max_y {
                for x in min_x..max_x {
                    let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);

                    if point_in_triangle(p, v0.pos, v1.pos, v2.pos) {
                        let (w0, w1, w2) = barycentric(p, v0.pos, v1.pos, v2.pos);
                        let color = interpolate_color(v0.color, v1.color, v2.color, w0, w1, w2);

                        let idx = y * width + x;
                        if idx < buffer.len() && color.a() > 0 {
                            buffer[idx] = blend_colors(buffer[idx], color);
                        }
                    }
                }
            }
        }
    }
}

fn point_in_triangle(p: egui::Pos2, v0: egui::Pos2, v1: egui::Pos2, v2: egui::Pos2) -> bool {
    let area = 0.5 * (-v1.y * v2.x + v0.y * (-v1.x + v2.x) + v0.x * (v1.y - v2.y) + v1.x * v2.y);
    let s = 1.0 / (2.0 * area)
        * (v0.y * v2.x - v0.x * v2.y + (v2.y - v0.y) * p.x + (v0.x - v2.x) * p.y);
    let t = 1.0 / (2.0 * area)
        * (v0.x * v1.y - v0.y * v1.x + (v0.y - v1.y) * p.x + (v1.x - v0.x) * p.y);

    s >= 0.0 && t >= 0.0 && (1.0 - s - t) >= 0.0
}

fn barycentric(p: egui::Pos2, v0: egui::Pos2, v1: egui::Pos2, v2: egui::Pos2) -> (f32, f32, f32) {
    let d = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
    if d.abs() < 0.0001 {
        return (0.33, 0.33, 0.34);
    }
    let w0 = ((v1.y - v2.y) * (p.x - v2.x) + (v2.x - v1.x) * (p.y - v2.y)) / d;
    let w1 = ((v2.y - v0.y) * (p.x - v2.x) + (v0.x - v2.x) * (p.y - v2.y)) / d;
    let w2 = 1.0 - w0 - w1;
    (w0.max(0.0), w1.max(0.0), w2.max(0.0))
}

fn interpolate_color(
    c0: egui::Color32,
    c1: egui::Color32,
    c2: egui::Color32,
    w0: f32,
    w1: f32,
    w2: f32,
) -> egui::Color32 {
    let r = (c0.r() as f32 * w0 + c1.r() as f32 * w1 + c2.r() as f32 * w2) as u8;
    let g = (c0.g() as f32 * w0 + c1.g() as f32 * w1 + c2.g() as f32 * w2) as u8;
    let b = (c0.b() as f32 * w0 + c1.b() as f32 * w1 + c2.b() as f32 * w2) as u8;
    let a = (c0.a() as f32 * w0 + c1.a() as f32 * w1 + c2.a() as f32 * w2) as u8;
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn blend_colors(dst: u32, src: egui::Color32) -> u32 {
    let sa = src.a() as f32 / 255.0;
    let da = 1.0 - sa;

    let dr = ((dst >> 16) & 0xFF) as f32;
    let dg = ((dst >> 8) & 0xFF) as f32;
    let db = (dst & 0xFF) as f32;

    let r = (src.r() as f32 * sa + dr * da) as u32;
    let g = (src.g() as f32 * sa + dg * da) as u32;
    let b = (src.b() as f32 * sa + db * da) as u32;

    0xFF000000 | (r << 16) | (g << 8) | b
}

fn run_test_mode(args: &[String]) {
    let test_dir = args
        .get(2)
        .map(|s| s.as_str())
        .unwrap_or("test_roms/blargg/cpu_instrs/individual");

    println!("╔══════════════════════════════════════╗");
    println!("║      GB3000 Test Runner              ║");
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("Running tests from: {}", test_dir);
    println!();

    let results = test_runner::run_all_tests(test_dir);

    println!();
    println!("════════════════════════════════════════");
    println!("                SUMMARY                 ");
    println!("════════════════════════════════════════");

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();

    for result in &results {
        let status = if result.passed { "✓ PASS" } else { "✗ FAIL" };
        println!("{} {} ({} cycles)", status, result.name, result.cycles);
        if !result.passed {
            if let Some(ref err) = result.error {
                println!("  Error: {}", err);
            }
            if !result.output.is_empty() {
                println!(
                    "  Output: {}",
                    result.output.chars().take(100).collect::<String>()
                );
            }
        }
    }

    println!();
    println!("Passed: {}/{}", passed, results.len());
    println!("Failed: {}/{}", failed, results.len());

    if failed > 0 {
        std::process::exit(1);
    }
}
