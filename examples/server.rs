use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use socket2::{Domain, Socket, Type};
use spop::{TypedData, VarScope, frame::Message};
use tokio::{net::TcpListener, signal};
use tracing::info;

use spoa::{self, server::IProcesser};

struct MyProcesser {}

#[async_trait::async_trait]
impl IProcesser for MyProcesser {
    async fn handle_messages(
        &self,
        messages: &[Message],
    ) -> spoa::Result<Vec<(VarScope, String, TypedData)>> {
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    info!("启动");

    // let listener = TcpListener::bind("0.0.0.0:33103").await.expect("绑定失败");

    let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
    let addr: SocketAddr = "0.0.0.0:33103".parse()?;

    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.set_keepalive(true)?;

    socket.bind(&addr.into())?;
    socket.listen(128)?;

    let listener = TcpListener::from_std(socket.into())?;
    let processer = Box::new(MyProcesser {});

    spoa::server::run(listener, Arc::new(processer), signal::ctrl_c()).await;

    info!("退出");
    Ok(())
}
