# GB3000

GB3000 is an experimental Game Boy emulator written in Rust. The goal of this
project is to eventually provide an accurate and well-documented emulator while
keeping the code as simple and approachable as possible.

At this early stage the emulator provides basic CPU and memory implementations
and executes only a small set of instructions (like NOP, immediate loads, and
simple increment/decrement operations).
A ROM file can be provided on the command line and the emulator will execute a
few instructions from it as a placeholder loop.

## Building

```sh
cargo build
```

## Running

```sh
cargo run -- path/to/rom.gb
```

## Testing

```sh
cargo test
```
