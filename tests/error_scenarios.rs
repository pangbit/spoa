//! Tests for error scenarios: timeouts, malformed frames, connection limits

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use semver::Version;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio_util::codec::Framed;

use spoa::protocol::frame::Message;
use spoa::protocol::frames::{
    FrameCapabilities,
    haproxy_hello::{HaproxyHello, HaproxyHelloFrame},
    notify::NotifyFrame,
};
use spoa::server::ServerConfig;
use spoa::{FrameFlags, FrameType, IProcesser, Metadata, ProcesserHolder, SpopCodec, TypedData, VarScope};

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

/// Helper: start a server with given config, return (addr, shutdown_tx, server_handle)
async fn start_server(
    config: ServerConfig,
) -> (
    std::net::SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let holder = Arc::new(RwLock::new(ProcesserHolder::new(Box::new(NoopProcesser))));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server_handle = tokio::spawn({
        let holder = Arc::clone(&holder);
        async move {
            spoa::server::run(
                listener,
                holder,
                async { shutdown_rx.await.ok(); },
                config,
                None,
            )
            .await;
        }
    });

    // Give server time to start accepting
    tokio::time::sleep(Duration::from_millis(50)).await;

    (addr, shutdown_tx, server_handle)
}

/// Helper: create a standard HaproxyHelloFrame
fn make_hello() -> HaproxyHelloFrame {
    HaproxyHelloFrame {
        metadata: Metadata {
            flags: FrameFlags::new(true, false),
            stream_id: 0,
            frame_id: 0,
        },
        payload: HaproxyHello {
            supported_versions: vec![Version::new(2, 0, 0)],
            max_frame_size: 1024,
            capabilities: vec![FrameCapabilities::from_str("pipelining").unwrap()],
            healthcheck: Some(false),
            engine_id: None,
        },
    }
}

// --- Task 6: Read timeout test ---

#[tokio::test]
async fn test_read_timeout() {
    let config = ServerConfig {
        read_timeout: Duration::from_millis(200),
        ..Default::default()
    };
    let (addr, shutdown_tx, server_handle) = start_server(config).await;

    // Connect but send nothing — should trigger read timeout
    let _stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Wait longer than read_timeout
    tokio::time::sleep(Duration::from_millis(500)).await;

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// --- Task 7: Malformed frame test ---

#[tokio::test]
async fn test_malformed_frame() {
    let config = ServerConfig {
        read_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    let (addr, shutdown_tx, server_handle) = start_server(config).await;

    // Send garbage data as a "frame"
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    // Write a frame length header (4 bytes) followed by invalid frame data
    let garbage: [u8; 8] = [0x00, 0x00, 0x00, 0x04, 0xFF, 0xFF, 0xFF, 0xFF];
    stream.write_all(&garbage).await.unwrap();

    // Wait for server to process and close connection
    tokio::time::sleep(Duration::from_millis(200)).await;

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// --- Task 8: Concurrent connections test ---

#[tokio::test]
async fn test_concurrent_connections() {
    let (addr, shutdown_tx, server_handle) = start_server(ServerConfig::default()).await;

    // Spawn 10 concurrent clients doing full handshake + notify
    let mut handles = vec![];
    for _ in 0..10 {
        handles.push(tokio::spawn(async move {
            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let mut framed = Framed::new(stream, SpopCodec { max_frame_size: 0 });

            // Handshake
            framed.send(Box::new(make_hello())).await.unwrap();
            let frame = framed.next().await.unwrap().unwrap();
            assert_eq!(*frame.frame_type(), FrameType::AgentHello);

            // Notify
            let notify = NotifyFrame {
                metadata: Metadata {
                    flags: FrameFlags::new(true, false),
                    stream_id: 1,
                    frame_id: 1,
                },
                messages: vec![Message {
                    name: "test".to_string(),
                    args: HashMap::new(),
                }],
            };
            framed.send(Box::new(notify)).await.unwrap();
            let frame = framed.next().await.unwrap().unwrap();
            assert_eq!(*frame.frame_type(), FrameType::Ack);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}

// --- Task 9: Client disconnect mid-session test ---

#[tokio::test]
async fn test_client_disconnect_mid_session() {
    let (addr, shutdown_tx, server_handle) = start_server(ServerConfig::default()).await;

    // Connect, send hello, then drop connection without disconnect
    {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut framed = Framed::new(stream, SpopCodec { max_frame_size: 0 });

        framed.send(Box::new(make_hello())).await.unwrap();
        let _frame = framed.next().await.unwrap().unwrap();
        // Drop framed/stream here — simulates abrupt disconnect
    }

    // Server should handle disconnect gracefully
    tokio::time::sleep(Duration::from_millis(200)).await;

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
