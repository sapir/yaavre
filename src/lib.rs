extern crate regex;
extern crate hex;


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
