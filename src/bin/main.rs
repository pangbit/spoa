use std::sync::Arc;

use anyhow::Result;
use tokio::{net::TcpListener, signal};
use tracing::info;
use x23::spoa;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    info!("启动");

    let listener = TcpListener::bind("0.0.0.0:33103").await.expect("绑定失败");

    let processer = spoa::Processer::new();
    spoa::server::run(listener, Arc::new(processer), signal::ctrl_c()).await;

    info!("退出");
    Ok(())
}
