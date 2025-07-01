class Memory:
    """Simplified memory bus for the GameBoy."""

    def __init__(self, rom_bytes):
        self.rom = rom_bytes
        self.ram = bytearray(0x10000)

    def read_byte(self, address):
        if 0x0000 <= address < len(self.rom):
            return self.rom[address]
        return self.ram[address]

    def write_byte(self, address, value):
        if 0x0000 <= address < len(self.rom):
            # ROM is read-only
            return
        self.ram[address] = value & 0xFF
