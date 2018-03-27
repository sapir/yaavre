extern crate clap;
extern crate yaavre;
extern crate hex;

use clap::{Arg, App};


fn main() {
    let matches = App::new("yaavre")
                    .arg(Arg::with_name("BIN").index(1))
                    .arg(Arg::with_name("ASM").index(2))
                    .get_matches();

    let mut emu = yaavre::Emulator::new();
    emu.load_bin(matches.value_of("BIN").unwrap()).unwrap();
    emu.load_disasm(matches.value_of("ASM").unwrap()).unwrap();
    emu.run();
}
