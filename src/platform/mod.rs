pub mod credentials;
pub mod diagnostics;
pub mod lunar;
pub mod startup;
pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
