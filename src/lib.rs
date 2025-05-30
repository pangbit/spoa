mod error;
pub use error::Error;
pub use error::Result;

mod shutdown;
use shutdown::Shutdown;

pub mod server;
pub use server::{IProcesser, ProcesserHolder};

pub use spop::{TypedData, VarScope, frame::Message};
