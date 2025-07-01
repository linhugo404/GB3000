from .cpu import CPU
from .memory import Memory

class Emulator:
    """A basic GameBoy emulator skeleton."""

    def __init__(self, rom_bytes):
        self.memory = Memory(rom_bytes)
        self.cpu = CPU(self.memory)

    def run(self, cycles=1_000_000):
        """Run the emulator for a number of CPU cycles."""
        for _ in range(cycles):
            self.cpu.step()
