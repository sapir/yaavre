extern crate clap;
extern crate yaavre;
extern crate hex;

use clap::{Arg, App};


fn main() {
    let matches = App::new("yaavre")
                    .arg(Arg::with_name("BIN").index(1))
                    .get_matches();

    let mut emu = yaavre::Emulator::new();
    emu.load_bin(matches.value_of("BIN").unwrap()).unwrap();
    emu.run();
}
