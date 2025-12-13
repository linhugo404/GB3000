//! GB3000 Desktop UI
//!
//! A graphical frontend for the GB3000 Game Boy emulator.

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
use ui::{EmulatorState, RomInfo, Ui, UiAction};

/// Target frame time (~60 FPS)
const FRAME_TIME_NS: u64 = 1_000_000_000 / 60;

/// UI window dimensions
const UI_WIDTH: usize = 640;
const UI_HEIGHT: usize = 576;

/// Audio buffer size
const AUDIO_BUFFER_SIZE: usize = 4096;

fn setup_audio(
    audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let device = host.default_output_device()?;

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
                        last_sample *= 0.9;
                        *sample = last_sample;
                    }
                }
            },
            |err| eprintln!("Audio error: {}", err),
            None,
        )
        .ok()?;

    stream.play().ok()?;
    Some(stream)
}

/// Scale Game Boy framebuffer to UI size
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
                dst[dst_idx] = palette[(src[src_idx] & 0x03) as usize];
            }
        }
    }
}

fn load_rom_file(path: &PathBuf) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("Failed to read ROM: {}", e))
}

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

    // Test mode
    if args.len() > 1 && args[1] == "--test" {
        run_test_mode(&args);
        return;
    }

    // Initial ROM from command line
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
        WindowOptions::default(),
    )
    .expect("Failed to create window");

    window.set_target_fps(60);

    // Create UI and emulator
    let mut ui = Ui::new();
    let mut emulator = Emulator::new();
    let palette = palettes::GRAYSCALE;

    // Audio setup
    let audio_buffer: Arc<Mutex<VecDeque<f32>>> =
        Arc::new(Mutex::new(VecDeque::with_capacity(AUDIO_BUFFER_SIZE)));
    let _audio_stream = setup_audio(Arc::clone(&audio_buffer), emulator.audio_sample_rate());

    // Framebuffer
    let mut buffer = vec![0u32; UI_WIDTH * UI_HEIGHT];

    // FPS tracking
    let mut frame_count = 0u64;
    let mut last_fps_time = Instant::now();
    let start_time = Instant::now();

    // Load initial ROM if provided
    if let Some(path) = initial_rom {
        if let Ok(rom) = load_rom_file(&path) {
            if let Some(info) = Emulator::parse_rom_info(&rom) {
                ui.add_recent_rom(path.clone(), info.title.clone());
                ui.rom_info = Some(RomInfo {
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
    }

    // Main loop
    while window.is_open() {
        let frame_start = Instant::now();

        // Update mouse state
        if let Some((mx, my)) = window.get_mouse_pos(minifb::MouseMode::Clamp) {
            let mouse_down = window.get_mouse_down(minifb::MouseButton::Left);
            ui.update_mouse(mx, my, mouse_down);
        }

        // Handle escape key
        if window.is_key_pressed(Key::Escape, minifb::KeyRepeat::No) {
            match ui.state {
                EmulatorState::StartScreen => break,
                EmulatorState::Running => ui.state = EmulatorState::Paused,
                EmulatorState::Paused => ui.state = EmulatorState::Running,
            }
        }

        // Process UI state
        let action = match ui.state {
            EmulatorState::StartScreen => {
                ui.render_start_screen(&mut buffer, UI_WIDTH, UI_HEIGHT)
            }

            EmulatorState::Running => {
                update_input(&mut emulator, &window);
                emulator.run_frame();
                scale_framebuffer(emulator.framebuffer(), &mut buffer, &palette);
                
                // Audio
                let samples = emulator.audio_samples();
                if !samples.is_empty() {
                    if let Ok(mut ab) = audio_buffer.lock() {
                        ab.extend(samples);
                        while ab.len() > AUDIO_BUFFER_SIZE {
                            ab.pop_front();
                        }
                    }
                }

                // FPS overlay
                ui.render_fps(&mut buffer, UI_WIDTH);
                
                UiAction::None
            }

            EmulatorState::Paused => {
                scale_framebuffer(emulator.framebuffer(), &mut buffer, &palette);
                ui.render_pause_menu(&mut buffer, UI_WIDTH, UI_HEIGHT)
            }
        };

        // Handle UI actions
        match action {
            UiAction::OpenFile => {
                if let Some(path) = Ui::open_file_dialog() {
                    if let Ok(rom) = load_rom_file(&path) {
                        if let Some(info) = Emulator::parse_rom_info(&rom) {
                            ui.add_recent_rom(path.clone(), info.title.clone());
                            ui.rom_info = Some(RomInfo {
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
                    } else {
                        ui.error_message = Some("Failed to load ROM".to_string());
                    }
                }
            }
            UiAction::LoadRom(path) => {
                if let Ok(rom) = load_rom_file(&path) {
                    if let Some(info) = Emulator::parse_rom_info(&rom) {
                        ui.rom_info = Some(RomInfo {
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
            }
            UiAction::Resume => ui.state = EmulatorState::Running,
            UiAction::Reset => {
                emulator.reset();
                ui.state = EmulatorState::Running;
            }
            UiAction::Quit => break,
            UiAction::None => {}
        }

        // Update window
        window
            .update_with_buffer(&buffer, UI_WIDTH, UI_HEIGHT)
            .expect("Failed to update window");

        // FPS tracking
        frame_count += 1;
        if last_fps_time.elapsed() >= Duration::from_secs(1) {
            ui.fps = frame_count as f64 / start_time.elapsed().as_secs_f64();
            last_fps_time = Instant::now();
        }

        // Frame timing
        let elapsed = frame_start.elapsed();
        let target = Duration::from_nanos(FRAME_TIME_NS);
        if elapsed < target {
            spin_sleep::sleep(target - elapsed);
        }
    }
}

fn run_test_mode(args: &[String]) {
    let test_dir = args
        .get(2)
        .map(|s| s.as_str())
        .unwrap_or("test_roms/blargg/cpu_instrs/individual");

    println!("╔══════════════════════════════════════╗");
    println!("║      GB3000 Test Runner              ║");
    println!("╚══════════════════════════════════════╝");
    println!("\nRunning tests from: {}\n", test_dir);

    let results = test_runner::run_all_tests(test_dir);

    println!("\n════════════════════════════════════════");
    println!("                SUMMARY                 ");
    println!("════════════════════════════════════════\n");

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();

    for result in &results {
        let status = if result.passed { "✓ PASS" } else { "✗ FAIL" };
        println!("{} {} ({} cycles)", status, result.name, result.cycles);
        if !result.passed {
            if let Some(ref err) = result.error {
                println!("  Error: {}", err);
            }
        }
    }

    println!("\nPassed: {}/{}", passed, results.len());
    println!("Failed: {}/{}", failed, results.len());

    if failed > 0 {
        std::process::exit(1);
    }
}
