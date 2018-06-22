use disa::{X_L, Y_L, Z_L};
use registers::RegisterFile;
use sreg::SReg;


// TODO: chip-specific?

// iox128a4u.h
pub const RAMPD : u32 = 0x0038;
pub const RAMPX : u32 = 0x0039;
pub const RAMPY : u32 = 0x003A;
pub const RAMPZ : u32 = 0x003B;
pub const EIND : u32 = 0x003C;
pub const SPL : u32 = 0x003D;
pub const SPH : u32 = 0x003E;
pub const SREG : u32 = 0x003F;

pub const OSC : u32 = 0x50;

pub const USART_C0 : u32 = 0x08A0;


pub struct IOMemory {
    pub regs: RegisterFile,
    pub sreg: SReg,

    pub data_mem: Vec<u8>,

    pub usart_input: Vec<u8>,
    pub usart_output_log: Vec<u8>,

    pub rtc_cnt : u16,
}

impl IOMemory {
    pub fn new() -> IOMemory {
        IOMemory {
            regs: RegisterFile::new(),
            sreg: SReg::new(),
            data_mem: vec![0; 1 << 22],

            usart_input: vec![],
            usart_output_log: vec![],

            rtc_cnt: 0,
        }
    }

    fn _get8(&self, addr: u32) -> u8 {
        self.data_mem[addr as usize]
    }

    fn _set8(&mut self, addr: u32, val: u8) {
        self.data_mem[addr as usize] = val;
    }

    pub fn get_rampd(&self) -> u8 {
        self._get8(RAMPD)
    }

    pub fn get_rampx(&self) -> u8 {
        self._get8(RAMPX)
    }

    pub fn get_rampy(&self) -> u8 {
        self._get8(RAMPY)
    }

    pub fn get_rampz(&self) -> u8 {
        self._get8(RAMPZ)
    }

    pub fn get_eind(&self) -> u8 {
        self._get8(EIND)
    }

    pub fn set_rampx(&mut self, val: u8) {
        self._set8(RAMPX, val);
    }

    pub fn set_rampy(&mut self, val: u8) {
        self._set8(RAMPY, val);
    }

    pub fn set_rampz(&mut self, val: u8) {
        self._set8(RAMPZ, val);
    }

    pub fn get_full_x(&self) -> u32 {
        ((self.get_rampx() as u32) << 16)
            | (self.regs.get16(X_L.0) as u32)
    }

    pub fn get_full_y(&self) -> u32 {
        ((self.get_rampy() as u32) << 16)
            | (self.regs.get16(Y_L.0) as u32)
    }

    pub fn get_full_z(&self) -> u32 {
        ((self.get_rampz() as u32) << 16)
            | (self.regs.get16(Z_L.0) as u32)
    }

    pub fn get_full_ind(&self) -> u32 {
        ((self.get_eind() as u32) << 16)
            | (self.regs.get16(Z_L.0) as u32)
    }

    pub fn get_full_reg(&self, reg: u8) -> u32 {
        match reg {
            26 => self.get_full_x(),
            28 => self.get_full_y(),
            30 => self.get_full_z(),
            _ => panic!("bad register {}", reg)
        }
    }

    pub fn set_full_x(&mut self, val: u32) {
        self.regs.set16(X_L.0, (val & 0xffff) as u16);
        self.set_rampx(((val >> 16) & 0xff) as u8);
    }

    pub fn set_full_y(&mut self, val: u32) {
        self.regs.set16(Y_L.0, (val & 0xffff) as u16);
        self.set_rampy(((val >> 16) & 0xff) as u8);
    }

    pub fn set_full_z(&mut self, val: u32) {
        self.regs.set16(Z_L.0, (val & 0xffff) as u16);
        self.set_rampz(((val >> 16) & 0xff) as u8);
    }

    pub fn set_full_reg(&mut self, reg: u8, val: u32) {
        match reg {
            26 => self.set_full_x(val),
            28 => self.set_full_y(val),
            30 => self.set_full_z(val),
            _ => panic!("bad register {}", reg)
        }
    }

    pub fn get8(&mut self, addr: u32, call_stack: &str, pc: u32) -> u8 {
        match addr {
            // oscillator status = ready
            0x0051 => 0xff,

            // rtc
            0x0401 => 0,
            0x0408 => {
                self.rtc_cnt += 1000;
                (self.rtc_cnt & 0xff) as u8
            },
            0x0409 => (self.rtc_cnt >> 8) as u8,

            0x08a0 => self.usart_input.remove(0),
            0x08a1 => 0x20 | (if self.usart_input.is_empty() { 0 } else { 0x80 }),

            // simple IO regs
            0x38...0x3e => self._get8(addr),

            SREG => self.sreg.as_u8(),

            // data memory
            0x2000...0x1000000 => self._get8(addr),

            _ => {
                println!("TODO: io read from {:#x} @ {}; {:#x}",
                    addr, call_stack, pc);
                0
            }
        }
    }

    pub fn set8(&mut self, addr: u32, val: u8, call_stack: &str, pc: u32) {
        match addr {
            0x08a0 => {
                self.usart_output_log.push(val);
                if val.is_ascii_whitespace() || val.is_ascii_graphic() {
                    print!("{}", val as char);
                }
            }

            // simple IO regs
            0x38...0x3e => self._set8(addr, val),

            SREG => self.sreg.set_u8(val),

            // data memory
            0x2000...0x1000000 => self._set8(addr, val),

            _ => {
                println!("TODO: io write to {:#x} = {:#x} @ {}; {:#x}",
                    addr, val, call_stack, pc);
            }
        }
    }

    pub fn get16(&mut self, addr: u32, call_stack: &str, pc: u32) -> u16 {
        ((self.get8(addr + 1, call_stack, pc) as u16) << 8)
          | (self.get8(addr, call_stack, pc) as u16)
    }

    pub fn set16(&mut self, addr: u32, val: u16, call_stack: &str, pc: u32) {
        self.set8(addr, (val & 0xff) as u8, call_stack, pc);
        self.set8(addr + 1, ((val >> 8) & 0xff) as u8, call_stack, pc);
    }

    fn _get16(&self, addr: u32) -> u16 {
        ((self._get8(addr + 1) as u16) << 8) | (self._get8(addr) as u16)
    }

    fn _set16(&mut self, addr: u32, val: u16) {
        self._set8(addr, (val & 0xff) as u8);
        self._set8(addr + 1, ((val >> 8) & 0xff) as u8);
    }

    pub fn get_sp(&self) -> u16 {
        self._get16(SPL)
    }

    pub fn set_sp(&mut self, val: u16) {
        self._set16(SPL, val)
    }

    pub fn push8(&mut self, val: u8) {
        let old_sp = self.get_sp();
        self._set8(old_sp as u32, val);

        self.set_sp(old_sp - 1);
    }

    pub fn pop8(&mut self) -> u8 {
        let old_sp = self.get_sp();
        self.set_sp(old_sp + 1);

        self._get8(self.get_sp() as u32)
    }

    pub fn push16(&mut self, val: u16) {
        self.push8(((val >> 0) & 0xff) as u8);
        self.push8(((val >> 8) & 0xff) as u8);
    }

    pub fn pop16(&mut self) -> u16 {
        let mut val;
        val = (self.pop8() as u16) << 8;
        val |= self.pop8() as u16;
        val
    }

    pub fn push24(&mut self, val: u32) {
        self.push8(((val >> 0) & 0xff) as u8);
        self.push8(((val >> 8) & 0xff) as u8);
        self.push8(((val >> 16) & 0xff) as u8);
    }

    pub fn pop24(&mut self) -> u32 {
        let mut val;
        val = (self.pop8() as u32) << 16;
        val |= (self.pop8() as u32) << 8;
        val |= self.pop8() as u32;
        val
    }
}
