use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use socket2::{Domain, Socket, Type};
use tokio::{net::TcpListener, signal, sync::RwLock, time};
use tracing::info;

use spoa::{self, IProcesser, Message, ProcesserHolder, TypedData, VarScope};

struct MyProcesser {
    id: u32,
}

#[async_trait::async_trait]
impl IProcesser for MyProcesser {
    async fn handle_messages(
        &self,
        messages: &[Message],
    ) -> spoa::Result<Vec<(VarScope, String, TypedData)>> {
        for msg in messages {
            info!("id: {}, msg: {}", self.id, msg.name);

            // msg.args.iter().for_each(|(k, v)| match v {
            //     TypedData::Binary(b) => {
            //         info!(
            //             "{}: {}",
            //             k,
            //             String::from_utf8_lossy(&b[0..b.len().min(1024)])
            //         )
            //     }
            //     TypedData::String(s) => {
            //         info!("{}: {}", k, s)
            //     }
            //     _ => {
            //         info!("{}", k)
            //     }
            // })
        }

        Ok(vec![
            (
                VarScope::Transaction,
                "spoa.v1".to_string(),
                TypedData::String("hello".to_string()),
            ),
            (
                VarScope::Session,
                "spoa.v2".to_string(),
                TypedData::String("world".to_string()),
            ),
            (
                VarScope::Request,
                "spoa.v3".to_string(),
                TypedData::String("!!!".to_string()),
            ),
        ])
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
// #[tokio::main]
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

    let mut count = 0;

    let processer = Box::new(MyProcesser { id: count });
    let processer_holder = Arc::new(RwLock::new(ProcesserHolder::new(processer)));

    tokio::spawn({
        let process_holder_ = Arc::clone(&processer_holder);

        async move {
            let mut tick = time::interval(time::Duration::from_secs(5));

            loop {
                tick.tick().await;
                count += 1;

                let processer = Box::new(MyProcesser { id: count });

                info!("RELOAD, {}", count);
                process_holder_.write().await.set_processer(processer);
            }
        }
    });

    spoa::server::run(listener, processer_holder, signal::ctrl_c()).await;

    info!("退出");
    Ok(())
}
