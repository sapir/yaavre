pub const X: usize = 26;
pub const Y: usize = 28;
pub const Z: usize = 30;


pub struct RegisterFile {
    pub r: Vec<u8>,
}

impl RegisterFile {
    pub fn new() -> RegisterFile {
        RegisterFile {
            r: vec![0; 32],
        }
    }

    pub fn get8(&self, i: usize) -> u8 {
        self.r[i]
    }

    pub fn set8(&mut self, i: usize, val: u8) {
        self.r[i] = val
    }

    pub fn get16(&self, i: usize) -> u16 {
        (self.get8(i) as u16) | ((self.get8(i + 1) as u16) << 8)
    }

    pub fn set16(&mut self, i: usize, val: u16) {
        self.set8(i, (val & 0xff) as u8);
        self.set8(i + 1, ((val >> 8) & 0xff) as u8)
    }
}
