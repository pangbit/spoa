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
        for msg in messages {
            info!("msg: {}", msg.name);

            msg.args.iter().for_each(|(k, v)| match v {
                TypedData::Binary(b) => {
                    info!(
                        "{}: {}",
                        k,
                        String::from_utf8_lossy(&b[0..b.len().min(1024)])
                    )
                }
                TypedData::String(s) => {
                    info!("{}: {}", k, s)
                }
                _ => {
                    info!("{}", k)
                }
            })
        }

        Ok(Vec::new())
    }
}
