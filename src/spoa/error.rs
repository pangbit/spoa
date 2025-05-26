use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("invalid haproxy hello, {}", .0)]
    InvalidHaproxyHello(String),

    #[error("invalid spop version, {}", .0)]
    InvalidSPOPVersion(String),
}
