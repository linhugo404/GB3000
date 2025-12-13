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

The emulator renders the classic Game Boy screen at 160x144 resolution with the
iconic green palette.

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

## Architecture

The emulator is organized into several modules:

- **`cpu.rs`**: Sharp LR35902 CPU with all opcodes and flag handling
- **`memory.rs`**: Memory management with MBC support and I/O registers
- **`ppu.rs`**: Picture Processing Unit for graphics rendering
- **`apu.rs`**: Audio Processing Unit for sound generation
- **`timer.rs`**: Timer subsystem with DIV and TIMA counters
- **`main.rs`**: Main emulator loop with window/input handling

## Compatibility

The emulator passes basic test ROMs and can run many commercial games. However,
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
