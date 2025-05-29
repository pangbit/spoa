use std::str::FromStr;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use semver::Version;
use spop::{
    FrameFlags, Metadata, SpopCodec,
    frames::{
        FrameCapabilities, HaproxyDisconnect, HaproxyHello,
        haproxy_disconnect::HaproxyDisconnectFrame, haproxy_hello::HaproxyHelloFrame,
    },
};
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("启动");

    let socket = TcpStream::connect("192.168.12.150:33103").await?;

    // let (r, w) = socket.split();
    let mut socket = Framed::new(socket, SpopCodec);

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

    socket.send(Box::new(frame)).await?;
    let frame = socket.next().await;
    info!("{:#?}", frame);

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

    socket.send(Box::new(frame)).await?;
    let frame = socket.next().await;
    info!("{:#?}", frame);

    info!("结束");
    Ok(())
}
