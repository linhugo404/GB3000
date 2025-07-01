# GB3000

GB3000 is an experimental Game Boy emulator written in Rust. The goal of this
project is to eventually provide an accurate and well-documented emulator while
keeping the code as simple and approachable as possible.

At this early stage the emulator only defines the CPU and memory structures and
includes a minimal `main` that resets the CPU. More functionality will be added
incrementally.

## Building

```sh
cargo build
```

## Running

```sh
cargo run
```

## Testing

```sh
cargo test
```
