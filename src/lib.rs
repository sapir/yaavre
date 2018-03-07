#![feature(advanced_slice_patterns, slice_patterns)]

extern crate regex;
extern crate hex;

#[macro_use]
extern crate lazy_static;


pub mod registers;
pub mod emulator;
pub mod sreg;
pub mod iomem;


pub use emulator::Emulator;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
