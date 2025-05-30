mod error;
pub use error::Error;
pub use error::Result;

mod shutdown;
use shutdown::Shutdown;

pub mod server;
pub mod uds_server;

pub use spop::{TypedData, VarScope, frame::Message};

#[async_trait::async_trait]
pub trait IProcesser {
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
