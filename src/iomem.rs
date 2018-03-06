use registers;
use registers::RegisterFile;
use sreg::SReg;


// TODO: chip-specific?

// iox128a4u.h
pub const RAMPD : usize = 0x0038;
pub const RAMPX : usize = 0x0039;
pub const RAMPY : usize = 0x003A;
pub const RAMPZ : usize = 0x003B;
pub const EIND : usize = 0x003C;
pub const SPL : usize = 0x003D;
pub const SPH : usize = 0x003E;
pub const SREG : usize = 0x003F;

pub const OSC : usize = 0x50;

pub const USART_C0 : usize = 0x08A0;


pub struct IOMemory {
    pub regs: RegisterFile,
    pub sreg: SReg,
    pub sp: u16,
    pub rampd: u8,
    pub rampx: u8,
    pub rampy: u8,
    pub rampz: u8,
    pub eind: u8,

    pub data_mem: Vec<u8>,
}

impl IOMemory {
    pub fn new() -> IOMemory {
        IOMemory {
            regs: RegisterFile::new(),
            sreg: SReg::new(),
            sp: 0,
            rampd: 0,
            rampx: 0,
            rampy: 0,
            rampz: 0,
            eind: 0,
            data_mem: vec![0; 1 << 22],
        }
    }

    pub fn get_full_x(&self) -> u32 {
        ((self.rampx as u32) << 16) | (self.regs.get16(registers::X) as u32)
    }

    pub fn get_full_y(&self) -> u32 {
        ((self.rampy as u32) << 16) | (self.regs.get16(registers::Y) as u32)
    }

    pub fn get_full_z(&self) -> u32 {
        ((self.rampz as u32) << 16) | (self.regs.get16(registers::Z) as u32)
    }
}
