//! PandaOS Bootloader placeholder
//!
//! This crate is a placeholder for future custom bootloader implementation.
//! Currently, the kernel uses the external `bootloader` crate.

#![no_std]

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_test() {
        assert_eq!(2 + 2, 4);
    }
}
