# GB3000

This repository contains a very minimal GameBoy emulator written in Python. It is **not** a fully functional emulator yet, but it provides a basic skeleton with CPU and memory components.

## Running

```
python main.py path/to/game.gb
```

The emulator currently supports only a couple of instructions (NOP and "LD A, n"). Running any real GameBoy ROM will stop with a `NotImplementedError` once an unimplemented opcode is encountered. This code serves as a starting point for further development.
