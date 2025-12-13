# GB3000

GB3000 is an experimental Game Boy (DMG) emulator written in Rust. The goal of this
project is to provide an accurate and well-documented emulator while keeping the
code as simple and approachable as possible.

## Features

- **Full CPU emulation**: All 256 base opcodes and 256 CB-prefixed opcodes
- **Accurate timing**: Cycle-accurate CPU with proper instruction timing
- **Graphics (PPU)**: Background, window, and sprite rendering
- **Memory Bank Controllers**: Support for MBC1, MBC2, MBC3, and MBC5
- **Timer**: DIV, TIMA, TMA, TAC with proper interrupt generation
- **Interrupts**: VBlank, LCD STAT, Timer, Serial, and Joypad interrupts
- **Audio (APU)**: 4 sound channels (pulse, wave, noise)
- **Input**: Full joypad support with keyboard mapping

## Screenshots

The emulator renders the Game Boy screen at 160x144 resolution (scaled 4x) with
a grayscale palette for clear visibility.

## Building

Requires Rust 1.70 or later.

```sh
cargo build --release
```

## Running

```sh
cargo run --release -- path/to/rom.gb
```

## Controls

| Key         | Game Boy Button |
|-------------|-----------------|
| Arrow Keys  | D-Pad           |
| Z           | A               |
| X           | B               |
| Enter       | Start           |
| Space       | Select          |
| Escape      | Quit            |

## Testing

```sh
cargo test
```

### Automated Test ROM Suite

Run the test runner against Blargg and Mooneye test ROMs:

```sh
cargo run --release -- --test test_roms/blargg/cpu_instrs/individual
cargo run --release -- --test test_roms/mooneye-test-suite/acceptance
```

### Test Results

| Test Suite | Pass Rate | Notes |
|------------|-----------|-------|
| Blargg CPU Instructions | **11/11** ✓ | All CPU opcodes correct |
| Blargg Instruction Timing | **1/1** ✓ | Instruction timing accurate |
| Mooneye MBC1 | **12/13** | 64KB RAM, 8MB/16MB ROM support |
| Mooneye Timer | **11/13** | Accurate falling-edge detection |
| Mooneye Acceptance | **18/41** | Multi-model support |

**Passing Acceptance Tests (18):**
- Boot registers (DMG-ABC, DMG-0, MGB, SGB, SGB2)
- Interrupt timing (di_timing, ei_timing, ei_sequence, intr_timing)
- HALT behavior (halt_ime0_*, halt_ime1_*)
- RETI timing, div_timing, if_ie_registers, pop_timing

**Hardware Model Support:**
The emulator automatically detects and emulates different Game Boy models:
- DMG-0 (early Game Boy)
- DMG-ABC (standard Game Boy) 
- MGB (Game Boy Pocket)
- SGB/SGB2 (Super Game Boy)
- CGB (Game Boy Color)

**M-cycle Accurate Execution:**
The CPU executes with M-cycle (4 T-cycle) granularity:
- Timer, PPU, and DMA updated between memory accesses
- Enables accurate testing of instruction timing

The remaining failures are primarily:
- Complex instruction timing edge cases (PUSH, CALL, RET)
- PPU mode transition timing

## Architecture

The emulator is organized into several modules:

- **`cpu.rs`**: Sharp LR35902 CPU with all opcodes and flag handling
- **`memory.rs`**: Memory management with MBC support and I/O registers
- **`ppu.rs`**: Picture Processing Unit for graphics rendering
- **`apu.rs`**: Audio Processing Unit for sound generation
- **`timer.rs`**: Timer subsystem with DIV and TIMA counters
- **`main.rs`**: Main emulator loop with window/input handling

## Compatibility

The emulator can run commercial games including Pokemon Yellow. However,
some features may not be perfectly accurate:

- Cycle-exact PPU timing (simplified for now)
- Some MBC edge cases
- Audio mixing may not be perfect

## Future Improvements

- [ ] Game Boy Color (CGB) support
- [ ] Save state support
- [ ] Serial link emulation
- [ ] Debugger/disassembler
- [ ] More accurate PPU timing
- [ ] Better audio quality

## Resources

- [Pan Docs](https://gbdev.io/pandocs/) - Comprehensive Game Boy documentation
- [Game Boy CPU Manual](http://marc.rawer.de/Gameboy/Docs/GBCPUman.pdf)
- [Blargg's test ROMs](https://github.com/retrio/gb-test-roms)

## License

This project is open source and available under the MIT License.
