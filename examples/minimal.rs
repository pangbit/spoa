//! Minimal SPOP server example using the Server builder.

use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::RwLock;

use spoa::{IProcesser, Message, ProcesserHolder, Server, TypedData, VarScope};

struct EchoProcesser;

#[async_trait::async_trait]
impl IProcesser for EchoProcesser {
    async fn handle_messages(
        &self,
        _messages: &[Message],
    ) -> spoa::Result<Vec<(VarScope, String, TypedData)>> {
        Ok(vec![(
            VarScope::Transaction,
            "echo.ok".to_string(),
            TypedData::Bool(true),
        )])
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let listener = TcpListener::bind("0.0.0.0:33103").await.unwrap();
    let holder = Arc::new(RwLock::new(ProcesserHolder::new(Box::new(EchoProcesser))));

    Server::new(listener, holder)
        .run(tokio::signal::ctrl_c())
        .await;
}
