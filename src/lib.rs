//! # spoa — HAProxy SPOP Agent Framework
//!
//! Async server framework for building HAProxy SPOP (Stream Processing Offload Protocol) agents.
//!
//! ## Usage
//!
//! 1. Implement [`IProcesser`] to handle incoming messages
//! 2. Wrap it in [`ProcesserHolder`] for thread-safe access
//! 3. Use [`Server`] builder or [`server::run`] to start the server
//!
//! ```no_run
//! use std::sync::Arc;
//! use tokio::net::TcpListener;
//! use tokio::sync::RwLock;
//! use spoa::{IProcesser, Message, ProcesserHolder, TypedData, VarScope};
//!
//! struct MyProcesser;
//!
//! #[async_trait::async_trait]
//! impl IProcesser for MyProcesser {
//!     async fn handle_messages(
//!         &self,
//!         messages: &[Message],
//!     ) -> spoa::Result<Vec<(VarScope, String, TypedData)>> {
//!         Ok(vec![(VarScope::Transaction, "k".into(), TypedData::String("v".into()))])
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let listener = TcpListener::bind("0.0.0.0:33103").await.unwrap();
//!     let holder = Arc::new(RwLock::new(ProcesserHolder::new(Box::new(MyProcesser))));
//!     spoa::Server::new(listener, holder).run(tokio::signal::ctrl_c()).await;
//! }
//! ```

mod error;
pub use error::Error;
pub use error::Result;

mod shutdown;
pub(crate) use shutdown::Shutdown;

pub mod protocol;
pub mod server;
pub use server::{Server, ServerConfig, ServerStats, StatsSnapshot};

// Re-export protocol types for convenience
pub use protocol::SpopFrame;
pub use protocol::types::TypedData;
pub use protocol::actions::{Action, VarScope};
pub use protocol::frame::{Message, FramePayload, FrameType, Metadata, FrameFlags};
pub use protocol::codec::SpopCodec;
pub use protocol::frames::{
    Ack, AgentDisconnect, AgentDisconnectFrame, AgentHello, AgentHelloFrame,
    FrameCapabilities, HaproxyDisconnect, HaproxyHello,
};
pub use protocol::varint::{decode_varint, encode_varint};

/// Trait for processing incoming SPOP messages from HAProxy.
///
/// Implement this trait to define your agent's message handling logic.
/// Return a list of `(scope, name, value)` tuples to set variables in HAProxy.
#[async_trait::async_trait]
pub trait IProcesser: Send + Sync {
    async fn handle_messages(
        &self,
        messages: &[Message],
    ) -> Result<Vec<(VarScope, String, TypedData)>>;
}

/// Thread-safe wrapper for dynamic processor replacement at runtime.
///
/// Wrap in `Arc<RwLock<ProcesserHolder>>` to share across connections
/// and enable hot-reload via [`set_processer`](ProcesserHolder::set_processer).
pub struct ProcesserHolder {
    pub(crate) processer: Box<dyn IProcesser + Sync + Send>,
}

impl ProcesserHolder {
    pub fn new(processer: Box<dyn IProcesser + Sync + Send>) -> Self {
        Self { processer }
    }

    pub fn set_processer(&mut self, new_processer: Box<dyn IProcesser + Sync + Send>) {
        self.processer = new_processer;
    }

    pub fn replace_processer(
        &mut self,
        new_processer: Box<dyn IProcesser + Sync + Send>,
    ) -> Box<dyn IProcesser + Sync + Send> {
        std::mem::replace(&mut self.processer, new_processer)
    }
}
