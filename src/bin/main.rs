use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use socket2::{Domain, Socket, Type};
use tokio::{net::TcpListener, signal};
use tracing::info;

use x23::spoa;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    info!("启动");

    // let listener = TcpListener::bind("0.0.0.0:33103").await.expect("绑定失败");

    let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
    let addr: SocketAddr = "0.0.0.0:33103".parse()?;

    socket.set_nonblocking(true)?;
    socket.set_keepalive(true)?;

    socket.bind(&addr.into())?;
    socket.listen(128)?;

    let listener = TcpListener::from_std(socket.into())?;
    let processer = spoa::Processer::new();

    spoa::server::run(listener, Arc::new(processer), signal::ctrl_c()).await;

    info!("退出");
    Ok(())
}
