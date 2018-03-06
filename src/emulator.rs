use iomem::IOMemory;


pub struct Emulator {
    pub prog_mem: Vec<u8>,
    pub io_mem: IOMemory,
    pub pc: usize,

    pub call_stack: Vec<(usize, usize)>,

    pub skip_next_insn: bool,
}

impl Emulator {
    pub fn new() -> Emulator {
        Emulator {
            prog_mem: vec![0; 1 << 22],
            io_mem: IOMemory::new(),
            pc: 0,

            call_stack: vec![],

            skip_next_insn: false,
        }
    }

    pub fn reset(&mut self) {
        self.pc = 0;
        self.io_mem = IOMemory::new();
        self.call_stack = vec![];
        self.skip_next_insn = false;
    }

    pub fn fmt_call_stack(&self) -> String {
        let frame_strings : Vec<String> =
            self.call_stack
                .iter()
                .map(|&(from, to)| format!("{:#x}->{:#x}", from, to))
                .collect();

        format!("[{}]", frame_strings.join(", "))
    }

    pub fn print_state(&self) {
        // try:
        //     insn_bytes, opcode, op_strs = self.pmem_asm[self.pc]
        // except Exception:
        //     insn = f"???"
        // else:
        //     insn = f"{opcode} {', '.join(op_strs)}"

        // println!("{self.pc:#06x}:  {insn}")
        // println!()

        let sreg_chars = [
            if self.io_mem.sreg.c { "C" } else { "." },
            if self.io_mem.sreg.z { "Z" } else { "." },
            if self.io_mem.sreg.n { "N" } else { "." },
            if self.io_mem.sreg.v { "V" } else { "." },
            if self.io_mem.sreg.s { "S" } else { "." },
            if self.io_mem.sreg.h { "H" } else { "." },
            if self.io_mem.sreg.t { "T" } else { "." },
            if self.io_mem.sreg.i { "I" } else { "." },
        ];
        let sreg_str = sreg_chars.join("");

        println!("sp={:#06x}, sreg: {}", self.io_mem.sp, sreg_str);
        println!();

        for line_num in 0..32 / 8 {
            let i = line_num * 8;
            print!("{:>3}:", format!("r{}", i));

            for j in i..i + 8 {
                if j % 2 == 0 {
                    print!(" ");
                } else {
                    print!(":");
                }

                print!("{:02x}", self.io_mem.regs.get8(j));
            }

            println!();
        }

        println!();
        println!(
            "X: {:06x} Y: {:06x} Z: {:06x}",
            self.io_mem.get_full_x(),
            self.io_mem.get_full_y(),
            self.io_mem.get_full_z());

        println!();
        println!("call stack: {}", self.fmt_call_stack());
    }
}
