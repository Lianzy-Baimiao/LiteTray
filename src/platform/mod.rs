pub mod adapter;
pub mod clock_preview;
pub mod credentials;
pub mod diagnostics;
pub mod displays;
pub mod lunar;
pub mod probe;
pub mod startup;
pub mod symbols;
pub mod taskbar_clock;
pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
