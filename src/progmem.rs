use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Result};
use disa::AvrInsn;


pub struct ProgramMemory {
    words: Vec<u16>,
}

impl ProgramMemory {
    pub fn new() -> ProgramMemory {
        ProgramMemory { words: vec!() }
    }

    pub fn set_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.words = vec![0; bytes.len() / 2];

        let mut rdr = Cursor::new(bytes);
        rdr.read_u16_into::<LittleEndian>(&mut self.words)
    }

    pub fn get_prog_mem_byte(&self, addr: u32, call_stack: &str, pc: u32)
            -> u8 {

        let pmem_index = (addr / 2) as usize;

        if pmem_index >= self.words.len() {
            println!(
                "WARNING: replacing pmem read from {:#x} @ {}; {:#x} with 0",
                addr, call_stack, pc);
            return 0;
        }

        let word = self.words[pmem_index];

        let mut bytes: [u8; 2] = [0; 2];
        (&mut bytes[..]).write_u16::<LittleEndian>(word).unwrap();

        bytes[(addr & 1) as usize]
    }

    pub fn get_insn_at(&self, addr: u32) -> Option<AvrInsn> {
        let pmem_index = (addr / 2) as usize;
        let decode_input = &self.words[pmem_index..];
        AvrInsn::decode(decode_input).map(|(_, insn)| insn)
    }
}
