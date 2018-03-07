use std::fs::File;
use std::result::Result;
use std::io;
use std::io::{Read, BufReader, BufRead};
use std::collections::HashMap;
use regex::Regex;
use hex;
use iomem::IOMemory;
use registers;


#[derive(Debug, Copy, Clone)]
pub enum Operand {
    Reg(usize),
    Imm(usize),
    MemAccess { reg: usize, ofs: isize, postinc: bool, predec: bool },
}

impl Operand {
    pub fn to_string(&self) -> String {
        match *self {
            Operand::Reg(num) => format!("r{}", num),
            Operand::Imm(x) => format!("{:#x}", x),
            Operand::MemAccess{ reg, ofs, postinc, predec } =>
                format!("{}{}{}{}",
                    if predec { "-" } else { "" },
                    match reg {
                        registers::X => String::from("X"),
                        registers::Y => String::from("Y"),
                        registers::Z => String::from("Z"),
                        _ => format!("r{}", reg)
                    },
                    if ofs != 0 {
                        format!("{:+}", ofs)
                    } else {
                        String::from("")
                    },
                    if postinc { "+" } else { "" },
                ),
        }
    }
}


lazy_static! {
    static ref MEM_ACCESS_REGEX : Regex =
        Regex::new(r"^(-)?([XYZ])(?:(\+)|([-+]\d+)|$)$").unwrap();
}


pub struct Emulator {
    pub prog_mem: Vec<u8>,
    pub pmem_asm: HashMap<usize, (Vec<u8>, String, Vec<Operand>)>,
    pub io_mem: IOMemory,
    pub pc: usize,

    pub call_stack: Vec<(u16, usize, usize)>,

    pub skip_next_insn: bool,

    pub insn_count: u64,
    // TODO: cycle_count
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

            insn_count: 0,
        }
    }

    pub fn reset(&mut self) {
        self.pc = 0;
        self.io_mem = IOMemory::new();
        self.call_stack = vec![];
        self.skip_next_insn = false;
        self.insn_count = 0;
    }

    pub fn fmt_call_stack(&self) -> String {
        let frame_strings : Vec<String> =
            self.call_stack
                .iter()
                .map(|&(_, from, to)| format!("{:#x}->{:#x}", from, to))
                .collect();

        format!("[{}]", frame_strings.join(", "))
    }

    pub fn print_state(&self) {
        let insn = match self.pmem_asm.get(&self.pc) {
            Some(&(_, ref opcode, ref operands)) =>
                format!("{} {}", opcode, {
                    let op_strs : Vec<String> = operands
                        .into_iter()
                        .map(|op| op.to_string())
                        .collect();
                    op_strs.join(", ")
                }),

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

        println!("sp={:#06x}, sreg: {}", self.io_mem.get_sp(), sreg_str);
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

        let sp = self.io_mem.get_sp() as usize;
        println!("some stack bytes: {}",
            hex::encode(&self.io_mem.data_mem[sp..sp + 16]));
    }

    pub fn load_bin(&mut self, path: &str) -> io::Result<()> {
        let mut f = File::open(path)?;
        let mut buffer = vec![];
        f.read_to_end(&mut buffer)?;
        self.prog_mem[..buffer.len()].clone_from_slice(&buffer);
        Ok(())
    }

    fn parse_operand(op_str: &str, next_pc: usize) -> Operand {
        let mut cs = op_str.chars();
        let first = cs.nth(0).unwrap();
        let rest = &op_str[1..];

        match first {
            'r' => Operand::Reg(rest.parse::<usize>().unwrap()),

            '.' => {
                let ofs = rest.parse::<isize>().unwrap();
                Operand::Imm(next_pc.wrapping_add(ofs as usize))
            },

            '0'...'9' => Operand::Imm(
                if op_str.starts_with("0x") {
                    usize::from_str_radix(&op_str[2..], 16).unwrap()
                } else {
                    usize::from_str_radix(op_str, 10).unwrap()
                }
            ),

            '-' | 'X' ... 'Z' => {
                let caps = MEM_ACCESS_REGEX.captures(op_str).unwrap();

                let predec = caps.get(1).is_some();

                let reg = match &caps[2] {
                    "X" => registers::X,
                    "Y" => registers::Y,
                    "Z" => registers::Z,
                    _ => unreachable!()
                };

                let postinc = caps.get(3).is_some();

                let ofs = match caps.get(4) {
                    Some(s) => s.as_str().parse::<isize>().unwrap(),
                    None => 0
                };

                Operand::MemAccess { reg, ofs, postinc, predec }
            }

            _ => panic!(format!("bad operand! {}", op_str))
        }
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

            let next_pc = addr + insn_bytes.len();

            let op_strs =
                if asm_parts.len() > 1 {
                    asm_parts[1].split(",")
                        .map(|op| Emulator::parse_operand(op.trim(), next_pc))
                        .collect()
                } else {
                    vec![]
                };

            self.pmem_asm.insert(addr, (insn_bytes, opcode, op_strs));
        }

        Ok(())
    }

    pub fn run(&mut self) {
        loop {
            self._step();
        }
    }

    pub fn until(&mut self, pc: usize) {
        loop {
            self._step();
            if self.pc == pc {
                break;
            }
        }

        self.print_state();
    }

    pub fn step(&mut self) {
        self._step();
        self.print_state();
    }

    pub fn get_reg8(&self, r: usize) -> u8 {
        self.io_mem.regs.get8(r)
    }

    pub fn set_reg8(&mut self, r: usize, val: u8) {
        self.io_mem.regs.set8(r, val);
    }

    pub fn get_reg16(&self, r: usize) -> u16 {
        self.io_mem.regs.get16(r)
    }

    pub fn set_reg16(&mut self, r: usize, val: u16) {
        self.io_mem.regs.set16(r, val);
    }

    fn _step(&mut self) {
        let mut next_pc;
        let opcode;
        let operands;

        {
            let (ref insn_bytes, ref opcode_, ref operands_) =
                self.pmem_asm[&self.pc];

            next_pc = self.pc + insn_bytes.len();
            opcode = opcode_.clone();
            operands = operands_.clone();
        }

        if self.skip_next_insn {
            self.skip_next_insn = false;
        } else {
            self.do_opcode(&*opcode, operands, &mut next_pc);
        }

        self.pc = next_pc;
        // TODO
        self.insn_count += 1;
    }

    /// set SReg for logical bit operations
    fn set_sreg_for_bits(&mut self, r_val: u8)
    {
        let sreg = &mut self.io_mem.sreg;
        sreg.v = false;
        sreg.n = (r_val & 0x80) != 0;
        sreg.z = r_val == 0;
        sreg.s = sreg.n ^ sreg.v;
    }

    /// set SReg for bit shift operations
    fn set_sreg_for_shift(&mut self, val_before: u8, val_after: u8)
    {
        let sreg = &mut self.io_mem.sreg;
        sreg.c = (val_before & 1) != 0;
        sreg.n = (val_after & 0x80) != 0;
        sreg.z = val_after == 0;
        sreg.v = sreg.n ^ sreg.c;
        sreg.s = sreg.n ^ sreg.v;
    }

    /// set SReg for addition operations
    fn set_sreg_for_add(&mut self, rd_val: u8, rr_val: u8, r_val: u8)
    {
        let sreg = &mut self.io_mem.sreg;
        let rd3 = (rd_val & (1 << 3)) != 0;
        let rr3 = (rr_val & (1 << 3)) != 0;
        let r3 = (r_val & (1 << 3)) != 0;
        sreg.h = (rd3 && rr3) || (rr3 && !r3) || (!r3 && rd3);

        let rd7 = (rd_val & (1 << 7)) != 0;
        let rr7 = (rr_val & (1 << 7)) != 0;
        let r7 = (r_val & (1 << 7)) != 0;
        sreg.v = (rd7 && rr7 && !r7) || (!rd7 && !rr7 && r7);

        sreg.n = r7;

        sreg.z = r_val == 0;

        sreg.c = (rd7 && rr7) || (rr7 && !r7) || (!r7 && rd7);

        sreg.s = sreg.n ^ sreg.v;
    }

    /// set SReg for subtraction operations
    fn set_sreg_for_sub(&mut self, rd_val: u8, rr_val: u8, r_val: u8,
            use_prev: bool)
    {
        let sreg = &mut self.io_mem.sreg;
        let rd3 = (rd_val & (1 << 3)) != 0;
        let rr3 = (rr_val & (1 << 3)) != 0;
        let r3 = (r_val & (1 << 3)) != 0;
        sreg.h = (!rd3 && rr3) || (rr3 && r3) || (r3 && !rd3);

        let rd7 = (rd_val & (1 << 7)) != 0;
        let rr7 = (rr_val & (1 << 7)) != 0;
        let r7 = (r_val & (1 << 7)) != 0;
        sreg.v = (rd7 && !rr7 && !r7) || (!rd7 && rr7 && r7);

        sreg.n = r7;

        if use_prev {
            sreg.z = (r_val == 0) && sreg.z;
        } else {
            sreg.z = r_val == 0;
        }

        sreg.c = (!rd7 && rr7) || (rr7 && r7) || (r7 && !rd7);

        sreg.s = sreg.n ^ sreg.v;
    }

    fn get_carry(&self) -> u8 {
        if self.io_mem.sreg.c { 1 } else { 0 }
    }

    fn push_ret_addr(&mut self, ret_addr: usize, call_tgt: usize) {
        self.call_stack.push((self.io_mem.get_sp(), self.pc, call_tgt));

        let ret_addr = ret_addr >> 1;

        // TODO: if !has_22bit_addrs, push16
        self.io_mem.push24(ret_addr as u32);
    }

    fn pop_ret_addr(&mut self) -> usize {
        // TODO: if !has_22bit_addrs, pop16
        let mut ret_addr = self.io_mem.pop24();

        ret_addr <<= 1;

        // remove return address from call stack but also any extra "return
        // addresses" pushed by "rcall .+0" instructions just to get the
        // current address or allocate stack space
        while !self.call_stack.is_empty() &&
               self.call_stack.last().unwrap().0 <= self.io_mem.get_sp() {

            self.call_stack.pop();
        }

        ret_addr as usize
    }

    fn do_call(&mut self, next_pc: &mut usize, call_tgt: usize) {
        let ret_addr = *next_pc;
        self.push_ret_addr(ret_addr, call_tgt);
        *next_pc = call_tgt;
    }

    fn do_opcode(&mut self, opcode: &str, operands: Vec<Operand>,
            next_pc: &mut usize)
    {
        match (opcode, operands.as_slice()) {
            ("nop", &[]) => {},

            ("jmp", &[Operand::Imm(tgt)]) => *next_pc = tgt,

            ("rjmp", &[Operand::Imm(tgt)]) => *next_pc = tgt,

            ("eijmp", &[]) => *next_pc = self.io_mem.get_full_ind() << 1,

            ("call", &[Operand::Imm(tgt)]) =>
                self.do_call(next_pc, tgt),

            ("rcall", &[Operand::Imm(tgt)]) =>
                self.do_call(next_pc, tgt),

            ("eicall", &[]) => {
                let tgt = self.io_mem.get_full_ind() << 1;
                self.do_call(next_pc, tgt);
            },

            ("ret", &[]) => *next_pc = self.pop_ret_addr(),

            ("reti", &[]) => {
                self.io_mem.sreg.i = true;
                *next_pc = self.pop_ret_addr();
            },

            ("push", &[Operand::Reg(rr)]) => {
                let val = self.get_reg8(rr);
                self.io_mem.push8(val);
            }

            ("pop", &[Operand::Reg(rd)]) => {
                let val = self.io_mem.pop8();
                self.set_reg8(rd, val);
            }

            ("breq", &[Operand::Imm(tgt)]) =>
                if self.io_mem.sreg.z {
                    *next_pc = tgt;
                },

            ("brne", &[Operand::Imm(tgt)]) =>
                if !self.io_mem.sreg.z {
                    *next_pc = tgt;
                },

            ("brcc", &[Operand::Imm(tgt)]) =>
                if !self.io_mem.sreg.c {
                    *next_pc = tgt;
                },

            ("brcs", &[Operand::Imm(tgt)]) =>
                if self.io_mem.sreg.c {
                    *next_pc = tgt;
                },

            ("brge", &[Operand::Imm(tgt)]) =>
                if !(self.io_mem.sreg.n ^ self.io_mem.sreg.v) {
                    *next_pc = tgt;
                },

            ("brlt", &[Operand::Imm(tgt)]) =>
                if self.io_mem.sreg.n ^ self.io_mem.sreg.v {
                    *next_pc = tgt;
                },

            ("brmi", &[Operand::Imm(tgt)]) =>
                if self.io_mem.sreg.n {
                    *next_pc = tgt;
                },

            ("brpl", &[Operand::Imm(tgt)]) =>
                if !self.io_mem.sreg.n {
                    *next_pc = tgt;
                },

            ("brtc", &[Operand::Imm(tgt)]) =>
                if !self.io_mem.sreg.t {
                    *next_pc = tgt;
                },

            ("brts", &[Operand::Imm(tgt)]) =>
                if self.io_mem.sreg.t {
                    *next_pc = tgt;
                },

            ("sbrc", &[Operand::Reg(rr), Operand::Imm(bit)]) => {
                let rr_val = self.get_reg8(rr);
                self.skip_next_insn = (rr_val & (1 << bit)) == 0;
            },

            ("sbrs", &[Operand::Reg(rr), Operand::Imm(bit)]) => {
                let rr_val = self.get_reg8(rr);
                self.skip_next_insn = (rr_val & (1 << bit)) != 0;
            },

            ("cpse", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                self.skip_next_insn = rd_val == rr_val;
            },

            ("clc", &[]) => self.io_mem.sreg.c = false,

            ("clh", &[]) => self.io_mem.sreg.h = false,

            ("cli", &[]) => self.io_mem.sreg.i = false,

            ("cln", &[]) => self.io_mem.sreg.n = false,

            ("cls", &[]) => self.io_mem.sreg.s = false,

            ("clt", &[]) => self.io_mem.sreg.t = false,

            ("clv", &[]) => self.io_mem.sreg.v = false,

            ("clz", &[]) => self.io_mem.sreg.z = false,

            ("sec", &[]) => self.io_mem.sreg.c = true,

            ("seh", &[]) => self.io_mem.sreg.h = true,

            ("sei", &[]) => self.io_mem.sreg.i = true,

            ("sen", &[]) => self.io_mem.sreg.n = true,

            ("ses", &[]) => self.io_mem.sreg.s = true,

            ("set", &[]) => self.io_mem.sreg.t = true,

            ("sev", &[]) => self.io_mem.sreg.v = true,

            ("sez", &[]) => self.io_mem.sreg.z = true,

            ("bst", &[Operand::Reg(rr), Operand::Imm(bit)]) => {
                let bit_val = (1 << bit) as u8;
                let rr_val = self.get_reg8(rr);
                self.io_mem.sreg.t = (rr_val & bit_val) != 0;
            },

            ("bld", &[Operand::Reg(rd), Operand::Imm(bit)]) => {
                let bit_val = (1 << bit) as u8;

                let mut val = self.get_reg8(rd);
                if self.io_mem.sreg.t {
                    val |= bit_val;
                } else {
                    val &= !bit_val;
                }

                self.set_reg8(rd, val);
            },

            ("clr", &[Operand::Reg(rd)]) => self.set_reg8(rd, 0),

            ("ser", &[Operand::Reg(rd)]) => self.set_reg8(rd, 0xff),

            ("ldi", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                self.set_reg8(rd, k as u8);
            },

            ("mov", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let val = self.get_reg8(rr);
                self.set_reg8(rd, val);
            },

            ("movw", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let val = self.get_reg16(rr);
                self.set_reg16(rd, val);
            },

            ("andi", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                let r_val = self.get_reg8(rd) & (k as u8);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            ("ori", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                let r_val = self.get_reg8(rd) | (k as u8);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            ("and", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let r_val = self.get_reg8(rd) & self.get_reg8(rr);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            ("or", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let r_val = self.get_reg8(rd) | self.get_reg8(rr);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            ("eor", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let r_val = self.get_reg8(rd) ^ self.get_reg8(rr);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            ("lsr", &[Operand::Reg(rd)]) => {
                let val_before = self.get_reg8(rd);
                let val_after = val_before >> 1;
                self.set_sreg_for_shift(val_before, val_after);
                self.set_reg8(rd, val_after);
            },

            ("asr", &[Operand::Reg(rd)]) => {
                let val_before = self.get_reg8(rd);
                let val_after = (val_before & 0x80) | val_before >> 1;
                self.set_sreg_for_shift(val_before, val_after);
                self.set_reg8(rd, val_after);
            },

            ("ror", &[Operand::Reg(rd)]) => {
                let val_before = self.get_reg8(rd);

                let mut val_after = val_before >> 1;
                if self.io_mem.sreg.c {
                    val_after |= 0x80;
                }

                self.set_sreg_for_shift(val_before, val_after);
                self.set_reg8(rd, val_after);
            },

            ("swap", &[Operand::Reg(rd)]) => {
                let n = self.get_reg8(rd);
                self.set_reg8(rd, ((n & 0xf0) >> 4) | ((n & 0x0f) << 4));
            },

            ("add", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_add(rr_val);
                self.set_sreg_for_add(rd_val, rr_val, r_val);
                self.set_reg8(rd, r_val);
            },

            ("adc", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_add(rr_val)
                                    .wrapping_add(self.get_carry());
                self.set_sreg_for_add(rd_val, rr_val, r_val);
                self.set_reg8(rd, r_val);
            },

            ("cp", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_sub(rr_val);
                self.set_sreg_for_sub(rd_val, rr_val, r_val, false);
            },

            ("cpi", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                let rd_val = self.get_reg8(rd);
                let k = k as u8;
                let r_val = rd_val.wrapping_sub(k);
                self.set_sreg_for_sub(rd_val, k, r_val, false);
            },

            ("cpc", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_sub(rr_val)
                                    .wrapping_sub(self.get_carry());
                self.set_sreg_for_sub(rd_val, rr_val, r_val, true);
            },

            ("sub", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_sub(rr_val);
                self.set_sreg_for_sub(rd_val, rr_val, r_val, false);
                self.set_reg8(rd, r_val);
            },

            ("subi", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                let rd_val = self.get_reg8(rd);
                let k = k as u8;
                let r_val = rd_val.wrapping_sub(k);
                self.set_sreg_for_sub(rd_val, k, r_val, false);
                self.set_reg8(rd, r_val);
            },

            ("sbc", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_sub(rr_val)
                // TODO: long
                                    .wrapping_sub(if self.io_mem.sreg.c { 1 } else { 0 });
                self.set_sreg_for_sub(rd_val, rr_val, r_val, true);
                self.set_reg8(rd, r_val);
            },

            ("sbci", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                let rd_val = self.get_reg8(rd);
                let k = k as u8;
                let r_val = rd_val.wrapping_sub(k)
                                    .wrapping_sub(self.get_carry());
                self.set_sreg_for_sub(rd_val, k, r_val, true);
                self.set_reg8(rd, r_val);
            },

            ("adiw", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                let rdw_val = self.get_reg16(rd);
                let kw_val = k as u16;
                let r_val = rdw_val.wrapping_add(kw_val);
                self.set_reg16(rd, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.v = ((rdw_val & 0x8000) == 0) && ((r_val & 0x8000) != 0);
                sreg.n = (r_val & 0x8000) != 0;
                sreg.z = r_val == 0;
                sreg.c = ((r_val & 0x8000) == 0) && ((rdw_val & 0x8000) != 0);
                sreg.s = sreg.n ^ sreg.v;
            },

            ("sbiw", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                let rdw_val = self.get_reg16(rd);
                let kw_val = k as u16;
                let r_val = rdw_val.wrapping_sub(kw_val);
                self.set_reg16(rd, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.v = ((rdw_val & 0x8000) != 0) && ((r_val & 0x8000) == 0);
                sreg.n = (r_val & 0x8000) != 0;
                sreg.z = r_val == 0;
                sreg.c = ((r_val & 0x8000) != 0) && ((rdw_val & 0x8000) == 0);
                sreg.s = sreg.n ^ sreg.v;
            },

            ("inc", &[Operand::Reg(rd)]) => {
                let rd_val = self.get_reg8(rd);
                let r_val = rd_val.wrapping_add(1);

                self.set_reg8(rd, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.v = rd_val == 0x7f;
                sreg.n = (r_val & 0x80) != 0;
                sreg.z = r_val == 0;
                sreg.s = sreg.n ^ sreg.v;
            },

            ("dec", &[Operand::Reg(rd)]) => {
                let rd_val = self.get_reg8(rd);
                let r_val = rd_val.wrapping_sub(1);

                self.set_reg8(rd, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.v = rd_val == 0x80;
                sreg.n = (r_val & 0x80) != 0;
                sreg.z = r_val == 0;
                sreg.s = sreg.n ^ sreg.v;
            },

            // TODO: verify sreg
            ("com", &[Operand::Reg(rd)]) => {
                let rd_val = self.get_reg8(rd);
                let r_val = 0xff - rd_val;

                self.set_reg8(rd, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.v = false;
                sreg.n = (r_val & 0x80) != 0;
                sreg.z = r_val == 0;
                sreg.c = true;
                sreg.s = sreg.n ^ sreg.v;
            },

            // TODO: verify sreg
            ("neg", &[Operand::Reg(rd)]) => {
                let rd_val = self.get_reg8(rd);
                let r_val = (-(rd_val as i8)) as u8;

                self.set_reg8(rd, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.h = ((r_val & 0x40) != 0) && ((rd_val & 0x40) == 0);
                sreg.v = r_val == 0x80;
                sreg.n = (r_val & 0x80) != 0;
                sreg.z = r_val == 0;
                sreg.c = r_val != 0;
                sreg.s = sreg.n ^ sreg.v;
            },

            ("mul", &[Operand::Reg(rd), Operand::Reg(rr)]) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = (rd_val as u16) * (rr_val as u16);
                self.set_reg16(0, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.c = (r_val & 0x8000) != 0;
                sreg.z = r_val == 0;
            },

            ("in", &[Operand::Reg(rd), Operand::Imm(port)]) => {
                let val = self.io_mem.get8(port);
                self.set_reg8(rd, val);
            },

            ("out", &[Operand::Imm(port), Operand::Reg(rr)]) => {
                let val = self.get_reg8(rr);
                self.io_mem.set8(port, val);
            },

            // TODO: default is lpm r0, Z. possibly disassembler shows
            // operands anyway.
            ("lpm", &[Operand::Reg(rd),
                      Operand::MemAccess { reg: registers::Z, ofs: 0, postinc, predec: false }
                      ]) => {

                let addr = self.get_reg16(registers::Z);
                let val = self.prog_mem[addr as usize];
                self.set_reg8(rd, val);

                if postinc {
                    self.set_reg16(registers::Z, addr + 1);
                }
            },

            ("elpm", &[Operand::Reg(rd),
                       Operand::MemAccess { reg: registers::Z, ofs: 0, postinc, predec: false }
                       ]) => {

                let addr = self.io_mem.get_full_z();
                let val = self.prog_mem[addr as usize];
                self.set_reg8(rd, val);

                if postinc {
                    self.io_mem.set_full_z(addr + 1);
                }
            },

            ("ld", &[Operand::Reg(rd),
                     Operand::MemAccess { reg, ofs, postinc, predec }])
            | ("ldd", &[Operand::Reg(rd),
                     Operand::MemAccess { reg, ofs, postinc, predec }])
            => {

                let mut base_addr = self.io_mem.get_full_reg(reg);

                if predec {
                    base_addr -= 1;
                }

                // TODO: usize is the wrong size!
                let addr = (base_addr as usize).wrapping_add(ofs as usize);
                let val = self.io_mem.get8(addr);
                self.set_reg8(rd, val);

                if postinc {
                    base_addr += 1;
                }

                if predec || postinc {
                    self.io_mem.set_full_reg(reg, base_addr);
                }
            },

            ("st", &[Operand::MemAccess { reg, ofs, postinc, predec },
                     Operand::Reg(rr)])
            | ("std", &[Operand::MemAccess { reg, ofs, postinc, predec },
                     Operand::Reg(rr)])
            => {

                let mut base_addr = self.io_mem.get_full_reg(reg);

                if predec {
                    base_addr -= 1;
                }

                // TODO: usize is the wrong size!
                let addr = (base_addr as usize).wrapping_add(ofs as usize);
                let val = self.get_reg8(rr);
                self.io_mem.set8(addr, val);

                if postinc {
                    base_addr += 1;
                }

                if predec || postinc {
                    self.io_mem.set_full_reg(reg, base_addr);
                }
            },

            ("lds", &[Operand::Reg(rd), Operand::Imm(k)]) => {
                let val = self.io_mem.get8(k as usize);
                self.set_reg8(rd, val);
            },

            ("sts", &[Operand::Imm(k), Operand::Reg(rr)]) => {
                let val = self.get_reg8(rr);
                self.io_mem.set8(k as usize, val);
            },

            _ => {
                self.print_state();
                panic!(
                    "unimplemented opcode {} @ {:#x} after {} instructions",
                    opcode, self.pc, self.insn_count);
            }
        }
    }
}
