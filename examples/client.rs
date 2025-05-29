use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, atomic},
    time::Duration,
};

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use semver::Version;
use spop::{
    FrameFlags, Metadata, SpopCodec,
    frame::Message,
    frames::{
        FrameCapabilities, HaproxyDisconnect, HaproxyHello,
        haproxy_disconnect::HaproxyDisconnectFrame, haproxy_hello::HaproxyHelloFrame,
        notify::NotifyFrame,
    },
};
use tokio::{net::TcpStream, signal, time};
use tokio_util::codec::Framed;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("启动");

    let count = Arc::new(atomic::AtomicU32::new(0));

    for _ in 0..10 {
        tokio::spawn({
            let count = Arc::clone(&count);

            async move {
                loop {
                    let socket = TcpStream::connect("192.168.12.150:33103").await.unwrap();
                    let mut socket = Framed::new(socket, SpopCodec);

                    //haproxy hello
                    let payload = HaproxyHello {
                        supported_versions: vec![Version::new(2, 0, 0)],
                        max_frame_size: 1024,
                        capabilities: vec![FrameCapabilities::from_str("pipelining").unwrap()],
                        healthcheck: Some(false),
                        engine_id: Some("engine-123".to_string()),
                    };

                    let frame = HaproxyHelloFrame {
                        metadata: Metadata {
                            flags: FrameFlags::new(true, false),
                            frame_id: 0,
                            stream_id: 0,
                        },
                        payload,
                    };

                    socket.send(Box::new(frame)).await.unwrap();
                    let _frame = socket.next().await;

                    //notify
                    let payload = NotifyFrame {
                        metadata: Metadata {
                            flags: FrameFlags::new(true, false),
                            frame_id: 0,
                            stream_id: 0,
                        },
                        messages: vec![Message {
                            name: "notify".to_string(),
                            args: HashMap::new(),
                        }],
                    };
                    socket.send(Box::new(payload)).await.unwrap();
                    let _frame = socket.next().await;

                    //haproxy disconnect
                    let payload = HaproxyDisconnect {
                        status_code: 200,
                        message: "ok".to_string(),
                    };

                    let frame = HaproxyDisconnectFrame {
                        metadata: Metadata {
                            flags: FrameFlags::new(true, false),
                            frame_id: 0,
                            stream_id: 0,
                        },
                        payload,
                    };

                    socket.send(Box::new(frame)).await.unwrap();
                    let _frame = socket.next().await;

                    count.fetch_add(1, atomic::Ordering::Relaxed);
                }
            }
        });
    }

    tokio::spawn(async move {
        let mut tick = time::interval(Duration::from_secs(10));

        loop {
            tick.tick().await;

            let sum = count.swap(0, atomic::Ordering::Relaxed);
            info!("qps: {:.2}", sum as f32 / 10.);
        }
    });

    signal::ctrl_c().await.unwrap();
    info!("结束");
    Ok(())
}
