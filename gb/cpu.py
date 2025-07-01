class CPU:
    """A very simplified LR35902 CPU emulator skeleton."""

    def __init__(self, memory):
        # Registers (8-bit unless combined)
        self.a = 0
        self.f = 0  # Flags
        self.b = 0
        self.c = 0
        self.d = 0
        self.e = 0
        self.h = 0
        self.l = 0
        self.sp = 0  # Stack pointer
        self.pc = 0  # Program counter
        self.memory = memory

    def step(self):
        """Fetch, decode and execute one instruction."""
        opcode = self.memory.read_byte(self.pc)
        self.pc = (self.pc + 1) & 0xFFFF

        # Only implement a tiny subset for demo purposes
        if opcode == 0x00:  # NOP
            pass
        elif opcode == 0x3E:  # LD A, n
            value = self.memory.read_byte(self.pc)
            self.pc = (self.pc + 1) & 0xFFFF
            self.a = value
        else:
            raise NotImplementedError(f"Opcode {opcode:02X} not implemented")
