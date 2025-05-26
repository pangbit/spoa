use spop::{TypedData, VarScope, frame::Message};
use tracing::info;

use super::Result;

pub struct Processer {}

impl Processer {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn handle_frame(
        &self,
        messages: &Vec<Message>,
    ) -> Result<Vec<(VarScope, String, TypedData)>> {
        info!("{:?}", messages);

        Ok(Vec::new())
    }
}
