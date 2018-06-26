pub struct RegisterFile {
    pub r: [u8; 32],
}

impl RegisterFile {
    pub fn new() -> RegisterFile {
        RegisterFile {
            r: [0; 32],
        }
    }

    pub fn get8(&self, i: u8) -> u8 {
        self.r[i as usize]
    }

    pub fn set8(&mut self, i: u8, val: u8) {
        self.r[i as usize] = val
    }

    pub fn get16(&self, i: u8) -> u16 {
        (self.get8(i) as u16) | ((self.get8(i + 1) as u16) << 8)
    }

    pub fn set16(&mut self, i: u8, val: u16) {
        self.set8(i, (val & 0xff) as u8);
        self.set8(i + 1, ((val >> 8) & 0xff) as u8)
    }
}
