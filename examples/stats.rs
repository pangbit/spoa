//! SPOP server with live stats reporting.

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::info;

use spoa::{IProcesser, Message, ProcesserHolder, Server, TypedData, VarScope};

struct NoopProcesser;

#[async_trait::async_trait]
impl IProcesser for NoopProcesser {
    async fn handle_messages(
        &self,
        _messages: &[Message],
    ) -> spoa::Result<Vec<(VarScope, String, TypedData)>> {
        Ok(vec![])
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let listener = TcpListener::bind("0.0.0.0:33103").await.unwrap();
    let holder = Arc::new(RwLock::new(ProcesserHolder::new(Box::new(NoopProcesser))));

    let server = Server::new(listener, holder);
    let stats = server.stats();

    // Periodically log stats
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let s = stats.snapshot();
            info!(
                "stats: total_conn={}, active_conn={}, messages={}, errors={}",
                s.total_connections, s.active_connections, s.total_messages, s.total_errors
            );
        }
    });

    server.run(tokio::signal::ctrl_c()).await;
}
