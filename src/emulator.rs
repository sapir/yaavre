use std::fs::File;
use std::result::Result;
use std::io;
use std::io::{Read, BufReader, BufRead};
use std::collections::HashMap;
use regex::Regex;
use hex;
use iomem::IOMemory;


pub struct Emulator {
    pub prog_mem: Vec<u8>,
    pub pmem_asm: HashMap<usize, (Vec<u8>, String, Vec<String>)>,
    pub io_mem: IOMemory,
    pub pc: usize,

    pub call_stack: Vec<(usize, usize)>,

    pub skip_next_insn: bool,
}

impl Emulator {
    pub fn new() -> Emulator {
        Emulator {
            prog_mem: vec![0; 1 << 22],
            pmem_asm: HashMap::new(),

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
        let insn = match self.pmem_asm.get(&self.pc) {
            Some(&(_, ref opcode, ref op_strs)) =>
                format!("{} {}", opcode, op_strs.join(", ")),

            None => String::from("???")
        };

        println!("{:#06x}:  {}", self.pc, insn);
        println!();

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

    pub fn load_bin(&mut self, path: &str) -> io::Result<()> {
        let mut f = File::open(path)?;
        let mut buffer = vec![];
        f.read_to_end(&mut buffer)?;
        self.prog_mem[..buffer.len()].clone_from_slice(&buffer);
        Ok(())
    }

    pub fn load_disasm(&mut self, path: &str) -> Result<(), String> {
        let f = File::open(path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(f);

        let line_re = Regex::new(
            r"^\s*([0-9a-f]+):\s+([0-9a-f ]+)\t([^;]+)"
        ).map_err(|e| e.to_string())?;

        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            let caps = line_re.captures(&line);
            if caps.is_none() {
                continue;
            }

            let caps = caps.unwrap();
            let addr = usize::from_str_radix(&caps[1], 16)
                .map_err(|e| e.to_string())?;
            let insn_bytes = hex::decode(
                &caps[2].replace(" ", "")
            ).map_err(|e| e.to_string())?;
            let asm = &caps[3].trim();

            let asm_parts : Vec<&str> = asm.split("\t").collect();

            let opcode = asm_parts[0].to_string();

            let op_strs =
                if asm_parts.len() > 1 {
                    asm_parts[1].split(",")
                        .map(|op| op.trim().to_string()).collect()
                } else {
                    vec![]
                };

            self.pmem_asm.insert(addr, (insn_bytes, opcode, op_strs));
        }

        Ok(())
    }
}
