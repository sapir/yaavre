// AVR Status Register

pub struct SReg {
    pub c : bool,
    pub z : bool,
    pub n : bool,
    pub v : bool,
    pub s : bool,
    pub h : bool,
    pub t : bool,
    pub i : bool,    
}

impl SReg {
    pub fn new() -> SReg {
        SReg {
            c: false,
            z: false,
            n: false,
            v: false,
            s: false,
            h: false,
            t: false,
            i: false,
        }
    }

    pub fn as_u8(&self) -> u8 {
        (if self.c { 1 << 0 } else { 0 })
        | (if self.z { 1 << 1 } else { 0 })
        | (if self.n { 1 << 2 } else { 0 })
        | (if self.v { 1 << 3 } else { 0 })
        | (if self.s { 1 << 4 } else { 0 })
        | (if self.h { 1 << 5 } else { 0 })
        | (if self.t { 1 << 6 } else { 0 })
        | (if self.i { 1 << 7 } else { 0 })
    }

    pub fn set_u8(&mut self, val : u8) {
        self.c = (val & (1 << 0)) != 0;
        self.z = (val & (1 << 1)) != 0;
        self.n = (val & (1 << 2)) != 0;
        self.v = (val & (1 << 3)) != 0;
        self.s = (val & (1 << 4)) != 0;
        self.h = (val & (1 << 5)) != 0;
        self.t = (val & (1 << 6)) != 0;
        self.i = (val & (1 << 7)) != 0;
    }
}
