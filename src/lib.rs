mod error;
pub use error::Error;
pub use error::Result;

mod shutdown;
pub(crate) use shutdown::Shutdown;

pub mod protocol;
pub mod server;
pub use server::{Server, ServerConfig};

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

#[async_trait::async_trait]
pub trait IProcesser: Send + Sync {
    async fn handle_messages(
        &self,
        messages: &[Message],
    ) -> Result<Vec<(VarScope, String, TypedData)>>;
}

pub struct ProcesserHolder {
    processer: Box<dyn IProcesser + Sync + Send>,
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
