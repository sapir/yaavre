use std::fs::File;
use std::io;
use std::io::{Read, Cursor};
use hex;
use iomem::IOMemory;
use std::sync::mpsc;
use signal_notify::{notify, Signal};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use disa::{AvrInsn, Reg, RegPair, MemAccess, MemRegUpdate};


pub struct Emulator {
    pub prog_mem: Vec<u16>,
    pub io_mem: IOMemory,
    pub pc: u32,

    pub call_stack: Vec<(u16, u32, u32)>,

    pub skip_next_insn: bool,

    pub insn_count: u64,
    // TODO: cycle_count

    pub halted: bool,

    sig_chan: mpsc::Receiver<Signal>,
}

impl Emulator {
    pub fn new() -> Emulator {
        let sig_chan = notify(&[Signal::USR1]);

        Emulator {
            prog_mem: vec![0; 1 << (22 - 1)],

            io_mem: IOMemory::new(),
            pc: 0,

            call_stack: vec![],

            skip_next_insn: false,

            insn_count: 0,

            halted: false,

            sig_chan: sig_chan,
        }
    }

    pub fn reset(&mut self) {
        self.pc = 0;
        self.io_mem = IOMemory::new();
        self.call_stack = vec![];
        self.skip_next_insn = false;
        self.insn_count = 0;
        self.halted = false;
    }

    pub fn fmt_call_stack(&self) -> String {
        let frame_strings : Vec<String> =
            self.call_stack
                .iter()
                .map(|&(_, from, to)| format!("{:#x}->{:#x}", from, to))
                .collect();

        format!("[{}]", frame_strings.join(", "))
    }

    fn get_prog_mem_byte(&self, addr: u32) -> u8 {
        let pmem_index = (addr / 2) as usize;

        if pmem_index >= self.prog_mem.len() {
            println!(
                "WARNING: replacing pmem read from {:#x} @ {}; {:#x} with 0",
                addr, self.fmt_call_stack(), self.pc);
            return 0;
        }

        let word = self.prog_mem[pmem_index];

        let mut bytes: [u8; 2] = [0; 2];
        (&mut bytes[..]).write_u16::<LittleEndian>(word).unwrap();

        bytes[(addr & 1) as usize]
    }

    fn get_insn_at(&self, addr: u32) -> Option<AvrInsn> {
        let pmem_index = (addr / 2) as usize;
        let decode_input = &self.prog_mem[pmem_index..];
        AvrInsn::decode(decode_input).map(|(_, insn)| insn)
    }

    fn get_cur_insn(&self) -> Option<AvrInsn> {
        self.get_insn_at(self.pc)
    }

    pub fn print_state(&self) {
        let insn = self.get_cur_insn();

        println!("{:#06x}:  {:?}", self.pc, insn);
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

        self.prog_mem = vec![0; buffer.len() / 2];

        let mut rdr = Cursor::new(buffer);
        rdr.read_u16_into::<LittleEndian>(&mut self.prog_mem)?;

        Ok(())
    }

    pub fn run(&mut self) {
        self.halted = false;
        while !self.halted {
            self._step();
        }

        self.print_state();
    }

    pub fn until(&mut self, pc: u32) {
        self.halted = false;
        while !self.halted {
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

    pub fn get_reg8(&self, r: u8) -> u8 {
        self.io_mem.regs.get8(r)
    }

    pub fn set_reg8(&mut self, r: u8, val: u8) {
        self.io_mem.regs.set8(r, val);
    }

    pub fn get_reg16(&self, r: u8) -> u16 {
        self.io_mem.regs.get16(r)
    }

    pub fn set_reg16(&mut self, r: u8, val: u16) {
        self.io_mem.regs.set16(r, val);
    }

    fn _step(&mut self) {
        match self.sig_chan.try_recv() {
            Ok(_) => self.print_state(),
            _ => (),
        }

        let insn = self.get_cur_insn().unwrap();
        let mut next_pc = self.pc + (insn.byte_size() as u32);

        if self.skip_next_insn {
            self.skip_next_insn = false;
        } else {
            self.do_opcode(&insn, &mut next_pc);
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

    fn push_ret_addr(&mut self, ret_addr: u32, call_tgt: u32) {
        self.call_stack.push((self.io_mem.get_sp(), self.pc, call_tgt));

        let ret_addr = ret_addr >> 1;

        // TODO: if !has_22bit_addrs, push16
        self.io_mem.push24(ret_addr);
    }

    fn pop_ret_addr(&mut self) -> u32 {
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

        ret_addr
    }

    fn do_call(&mut self, next_pc: &mut u32, call_tgt: u32) {
        let ret_addr = *next_pc;
        self.push_ret_addr(ret_addr, call_tgt);
        *next_pc = call_tgt;
    }

    // does the pre-update and returns the address
    fn do_pre_mem_access(&mut self, mema: MemAccess, full_reg: bool) -> u32 {
        let MemAccess { reg_pair, ofs, update } = mema;

        let base_addr =
            if full_reg {
                let mut val = self.io_mem.get_full_reg(reg_pair.0);

                if update == MemRegUpdate::PreDec {
                    // TODO: incorrect overflow handling
                    val -= 1;
                    self.io_mem.set_full_reg(reg_pair.0, val);
                }

                val
            } else {
                let mut val = self.get_reg16(reg_pair.0);

                if update == MemRegUpdate::PreDec {
                    // TODO: incorrect overflow handling
                    val -= 1;
                    self.set_reg16(reg_pair.0, val);
                }

                val as u32
            };

        // TODO: incorrect overflow handling
        base_addr + (ofs as u32)
    }

    fn do_post_mem_access(&mut self, mema: MemAccess, full_reg: bool) {
        let MemAccess { reg_pair, ofs: _, update } = mema;

        if full_reg {
            if update == MemRegUpdate::PostInc {
                let val = self.get_reg16(reg_pair.0);
                self.set_reg16(reg_pair.0, val + 1);
            }
        } else {
            if update == MemRegUpdate::PostInc {
                let val = self.io_mem.get_full_reg(reg_pair.0);
                self.io_mem.set_full_reg(reg_pair.0, val + 1);
            }
        }
    }

    fn get_rel_jmp_target(&self, next_pc: u32, ofs: i16) -> u32 {
        next_pc.wrapping_add(ofs as i32 as u32)
    }

    fn do_opcode(&mut self, insn: &AvrInsn, next_pc: &mut u32) {
        match insn {
            &AvrInsn::Nop => {},

            &AvrInsn::Jmp(tgt) => *next_pc = tgt,

            &AvrInsn::Rjmp(ofs) => {
                // catch "__stop_program"
                if ofs == -2 && !self.io_mem.sreg.i {
                    self.halted = true;
                }

                *next_pc = self.get_rel_jmp_target(*next_pc, ofs);
            }

            &AvrInsn::Eijmp => *next_pc = self.io_mem.get_full_ind() << 1,

            &AvrInsn::Call(tgt) =>
                self.do_call(next_pc, tgt),

            &AvrInsn::Rcall(ofs) => {
                let tgt = self.get_rel_jmp_target(*next_pc, ofs);
                self.do_call(next_pc, tgt);
            },

            &AvrInsn::Eicall => {
                let tgt = self.io_mem.get_full_ind() << 1;
                self.do_call(next_pc, tgt);
            },

            &AvrInsn::Ret => *next_pc = self.pop_ret_addr(),

            &AvrInsn::Reti => {
                self.io_mem.sreg.i = true;
                *next_pc = self.pop_ret_addr();
            },

            &AvrInsn::Push(Reg(rr)) => {
                let val = self.get_reg8(rr);
                self.io_mem.push8(val);
            }

            &AvrInsn::Pop(Reg(rd)) => {
                let val = self.io_mem.pop8();
                self.set_reg8(rd, val);
            }

            &AvrInsn::Breq(ofs) =>
                if self.io_mem.sreg.z {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brne(ofs) =>
                if !self.io_mem.sreg.z {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brcc(ofs) =>
                if !self.io_mem.sreg.c {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brcs(ofs) =>
                if self.io_mem.sreg.c {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brge(ofs) =>
                if !(self.io_mem.sreg.n ^ self.io_mem.sreg.v) {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brlt(ofs) =>
                if self.io_mem.sreg.n ^ self.io_mem.sreg.v {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brmi(ofs) =>
                if self.io_mem.sreg.n {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brpl(ofs) =>
                if !self.io_mem.sreg.n {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brtc(ofs) =>
                if !self.io_mem.sreg.t {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Brts(ofs) =>
                if self.io_mem.sreg.t {
                    *next_pc = self.get_rel_jmp_target(*next_pc, ofs.into());
                },

            &AvrInsn::Sbrc(Reg(rr), bit) => {
                let rr_val = self.get_reg8(rr);
                self.skip_next_insn = (rr_val & (1 << bit)) == 0;
            },

            &AvrInsn::Sbrs(Reg(rr), bit) => {
                let rr_val = self.get_reg8(rr);
                self.skip_next_insn = (rr_val & (1 << bit)) != 0;
            },

            &AvrInsn::Cpse(Reg(rd), Reg(rr)) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                self.skip_next_insn = rd_val == rr_val;
            },

            &AvrInsn::Clc => self.io_mem.sreg.c = false,

            &AvrInsn::Clh => self.io_mem.sreg.h = false,

            &AvrInsn::Cli => self.io_mem.sreg.i = false,

            &AvrInsn::Cln => self.io_mem.sreg.n = false,

            &AvrInsn::Cls => self.io_mem.sreg.s = false,

            &AvrInsn::Clt => self.io_mem.sreg.t = false,

            &AvrInsn::Clv => self.io_mem.sreg.v = false,

            &AvrInsn::Clz => self.io_mem.sreg.z = false,

            &AvrInsn::Sec => self.io_mem.sreg.c = true,

            &AvrInsn::Seh => self.io_mem.sreg.h = true,

            &AvrInsn::Sei => self.io_mem.sreg.i = true,

            &AvrInsn::Sen => self.io_mem.sreg.n = true,

            &AvrInsn::Ses => self.io_mem.sreg.s = true,

            &AvrInsn::Set => self.io_mem.sreg.t = true,

            &AvrInsn::Sev => self.io_mem.sreg.v = true,

            &AvrInsn::Sez => self.io_mem.sreg.z = true,

            &AvrInsn::Bst(Reg(rr), bit) => {
                let bit_val = (1 << bit) as u8;
                let rr_val = self.get_reg8(rr);
                self.io_mem.sreg.t = (rr_val & bit_val) != 0;
            },

            &AvrInsn::Bld(Reg(rd), bit) => {
                let bit_val = (1 << bit) as u8;

                let mut val = self.get_reg8(rd);
                if self.io_mem.sreg.t {
                    val |= bit_val;
                } else {
                    val &= !bit_val;
                }

                self.set_reg8(rd, val);
            },

            // TODO:
            // &AvrInsn::Clr(Reg(rd)) => self.set_reg8(rd, 0),

            // &AvrInsn::Ser(Reg(rd)) => self.set_reg8(rd, 0xff),

            &AvrInsn::Ldi(Reg(rd), k) => {
                self.set_reg8(rd, k as u8);
            },

            &AvrInsn::Mov(Reg(rd), Reg(rr)) => {
                let val = self.get_reg8(rr);
                self.set_reg8(rd, val);
            },

            &AvrInsn::Movw(RegPair(rd), RegPair(rr)) => {
                let val = self.get_reg16(rr);
                self.set_reg16(rd, val);
            },

            &AvrInsn::Andi(Reg(rd), k) => {
                let r_val = self.get_reg8(rd) & (k as u8);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            &AvrInsn::Ori(Reg(rd), k) => {
                let r_val = self.get_reg8(rd) | (k as u8);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            &AvrInsn::And(Reg(rd), Reg(rr)) => {
                let r_val = self.get_reg8(rd) & self.get_reg8(rr);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            &AvrInsn::Or(Reg(rd), Reg(rr)) => {
                let r_val = self.get_reg8(rd) | self.get_reg8(rr);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            &AvrInsn::Eor(Reg(rd), Reg(rr)) => {
                let r_val = self.get_reg8(rd) ^ self.get_reg8(rr);
                self.set_reg8(rd, r_val);
                self.set_sreg_for_bits(r_val);
            },

            &AvrInsn::Lsr(Reg(rd)) => {
                let val_before = self.get_reg8(rd);
                let val_after = val_before >> 1;
                self.set_sreg_for_shift(val_before, val_after);
                self.set_reg8(rd, val_after);
            },

            &AvrInsn::Asr(Reg(rd)) => {
                let val_before = self.get_reg8(rd);
                let val_after = (val_before & 0x80) | val_before >> 1;
                self.set_sreg_for_shift(val_before, val_after);
                self.set_reg8(rd, val_after);
            },

            &AvrInsn::Ror(Reg(rd)) => {
                let val_before = self.get_reg8(rd);

                let mut val_after = val_before >> 1;
                if self.io_mem.sreg.c {
                    val_after |= 0x80;
                }

                self.set_sreg_for_shift(val_before, val_after);
                self.set_reg8(rd, val_after);
            },

            &AvrInsn::Swap(Reg(rd)) => {
                let n = self.get_reg8(rd);
                self.set_reg8(rd, ((n & 0xf0) >> 4) | ((n & 0x0f) << 4));
            },

            &AvrInsn::Add(Reg(rd), Reg(rr)) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_add(rr_val);
                self.set_sreg_for_add(rd_val, rr_val, r_val);
                self.set_reg8(rd, r_val);
            },

            &AvrInsn::Adc(Reg(rd), Reg(rr)) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_add(rr_val)
                                    .wrapping_add(self.get_carry());
                self.set_sreg_for_add(rd_val, rr_val, r_val);
                self.set_reg8(rd, r_val);
            },

            &AvrInsn::Cp(Reg(rd), Reg(rr)) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_sub(rr_val);
                self.set_sreg_for_sub(rd_val, rr_val, r_val, false);
            },

            &AvrInsn::Cpi(Reg(rd), k) => {
                let rd_val = self.get_reg8(rd);
                let k = k as u8;
                let r_val = rd_val.wrapping_sub(k);
                self.set_sreg_for_sub(rd_val, k, r_val, false);
            },

            &AvrInsn::Cpc(Reg(rd), Reg(rr)) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_sub(rr_val)
                                    .wrapping_sub(self.get_carry());
                self.set_sreg_for_sub(rd_val, rr_val, r_val, true);
            },

            &AvrInsn::Sub(Reg(rd), Reg(rr)) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_sub(rr_val);
                self.set_sreg_for_sub(rd_val, rr_val, r_val, false);
                self.set_reg8(rd, r_val);
            },

            &AvrInsn::Subi(Reg(rd), k) => {
                let rd_val = self.get_reg8(rd);
                let k = k as u8;
                let r_val = rd_val.wrapping_sub(k);
                self.set_sreg_for_sub(rd_val, k, r_val, false);
                self.set_reg8(rd, r_val);
            },

            &AvrInsn::Sbc(Reg(rd), Reg(rr)) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = rd_val.wrapping_sub(rr_val)
                // TODO: long
                                    .wrapping_sub(if self.io_mem.sreg.c { 1 } else { 0 });
                self.set_sreg_for_sub(rd_val, rr_val, r_val, true);
                self.set_reg8(rd, r_val);
            },

            &AvrInsn::Sbci(Reg(rd), k) => {
                let rd_val = self.get_reg8(rd);
                let k = k as u8;
                let r_val = rd_val.wrapping_sub(k)
                                    .wrapping_sub(self.get_carry());
                self.set_sreg_for_sub(rd_val, k, r_val, true);
                self.set_reg8(rd, r_val);
            },

            &AvrInsn::Adiw(RegPair(rd), k) => {
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

            &AvrInsn::Sbiw(RegPair(rd), k) => {
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

            &AvrInsn::Inc(Reg(rd)) => {
                let rd_val = self.get_reg8(rd);
                let r_val = rd_val.wrapping_add(1);

                self.set_reg8(rd, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.v = rd_val == 0x7f;
                sreg.n = (r_val & 0x80) != 0;
                sreg.z = r_val == 0;
                sreg.s = sreg.n ^ sreg.v;
            },

            &AvrInsn::Dec(Reg(rd)) => {
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
            &AvrInsn::Com(Reg(rd)) => {
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
            &AvrInsn::Neg(Reg(rd)) => {
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

            &AvrInsn::Mul(Reg(rd), Reg(rr)) => {
                let rd_val = self.get_reg8(rd);
                let rr_val = self.get_reg8(rr);
                let r_val = (rd_val as u16) * (rr_val as u16);
                self.set_reg16(0, r_val);

                let sreg = &mut self.io_mem.sreg;
                sreg.c = (r_val & 0x8000) != 0;
                sreg.z = r_val == 0;
            },

            &AvrInsn::In(Reg(rd), port) => {
                let call_stack = self.fmt_call_stack();
                let val = self.io_mem.get8(port as u32, &call_stack, self.pc);
                self.set_reg8(rd, val);
            },

            &AvrInsn::Out(port, Reg(rr)) => {
                let val = self.get_reg8(rr);
                let call_stack = self.fmt_call_stack();
                self.io_mem.set8(port as u32, val, &call_stack, self.pc);
            },

            &AvrInsn::LpmZ(Reg(rd), mema) => {

                let addr = self.do_pre_mem_access(mema, false);

                let val = self.get_prog_mem_byte(addr);
                self.set_reg8(rd, val);

                self.do_post_mem_access(mema, false);
            },

            &AvrInsn::ElpmZ(Reg(rd), mema) => {
                let addr = self.do_pre_mem_access(mema, true);

                let val = self.get_prog_mem_byte(addr);
                self.set_reg8(rd, val);

                self.do_post_mem_access(mema, true);
            },

            &AvrInsn::Ld(Reg(rd), mema) | &AvrInsn::Ldd(Reg(rd), mema) => {
                let addr = self.do_pre_mem_access(mema, true);

                let call_stack = self.fmt_call_stack();
                let val = self.io_mem.get8(addr, &call_stack, self.pc);
                self.set_reg8(rd, val);

                self.do_post_mem_access(mema, true);
            },

            &AvrInsn::St(mema, Reg(rr)) | &AvrInsn::Std(mema, Reg(rr)) => {
                let addr = self.do_pre_mem_access(mema, true);

                let val = self.get_reg8(rr);
                let call_stack = self.fmt_call_stack();
                self.io_mem.set8(addr, val, &call_stack, self.pc);

                self.do_post_mem_access(mema, true);
            },

            &AvrInsn::Lds(Reg(rd), k) => {
                let call_stack = self.fmt_call_stack();
                let val = self.io_mem.get8(k as u32, &call_stack, self.pc);
                self.set_reg8(rd, val);
            },

            &AvrInsn::Sts(k, Reg(rr)) => {
                let val = self.get_reg8(rr);
                let call_stack = self.fmt_call_stack();
                self.io_mem.set8(k as u32, val, &call_stack, self.pc);
            },

            _ => {
                self.print_state();
                panic!(
                    "unimplemented instruction {:?} @ {:#x} after {} instructions",
                    insn, self.pc, self.insn_count);
            }
        }
    }
}
