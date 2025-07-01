from pathlib import Path
from gb.emulator import Emulator
import sys


def load_rom(path):
    return Path(path).read_bytes()


def main():
    if len(sys.argv) < 2:
        print("Usage: python main.py <rom.gb>")
        return

    rom_path = sys.argv[1]
    rom = load_rom(rom_path)
    emulator = Emulator(rom)
    try:
        emulator.run()
    except NotImplementedError as e:
        print("Emulation stopped:", e)


if __name__ == "__main__":
    main()
