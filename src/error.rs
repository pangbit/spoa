use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("read timeout")]
    ReadTimeout,

    #[error("write timeout")]
    WriteTimeout,

    // 协议层
    #[error("invalid frame type: {0}")]
    InvalidFrameType(u8),

    #[error("frame parse failed: {0}")]
    FrameParseFailed(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("frame too large: {size} bytes, max {max} bytes")]
    FrameTooLarge { size: usize, max: usize },

    // 握手层
    #[error("handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("unsupported spop version: {0}")]
    UnsupportedVersion(String),

    // 处理层
    #[error("processer error: {0}")]
    ProcesserError(String),
}
