use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid address bytes: {:?}", .0)]
    InvalidAddressBytes(Vec<u8>),
    #[error("invalid address hex: {0}")]
    InvalidAddressHex(String),
}
