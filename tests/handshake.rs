//! End-to-end SPOP handshake test

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use semver::Version;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio_util::codec::Framed;

use spoa::protocol::frame::Message;
use spoa::protocol::frames::{
    FrameCapabilities, HaproxyDisconnect,
    haproxy_disconnect::HaproxyDisconnectFrame,
    haproxy_hello::{HaproxyHello, HaproxyHelloFrame},
    notify::NotifyFrame,
};
use spoa::{
    FrameFlags, FrameType, IProcesser, Metadata, ProcesserHolder, SpopCodec, TypedData, VarScope,
};

struct TestProcesser;

#[async_trait::async_trait]
impl IProcesser for TestProcesser {
    async fn handle_messages(
        &self,
        _messages: &[Message],
    ) -> spoa::Result<Vec<(VarScope, String, TypedData)>> {
        Ok(vec![(
            VarScope::Transaction,
            "test.var".to_string(),
            TypedData::String("ok".to_string()),
        )])
    }
}

#[tokio::test]
async fn test_full_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let processer = Box::new(TestProcesser);
    let holder = Arc::new(RwLock::new(ProcesserHolder::new(processer)));

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Start server
    let server_handle = tokio::spawn({
        let holder = Arc::clone(&holder);
        async move {
            spoa::server::run(listener, holder, async {
                shutdown_rx.await.ok();
            })
            .await;
        }
    });

    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Client: connect and perform handshake
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, SpopCodec { max_frame_size: 0 });

    // Send HaproxyHello
    let hello = HaproxyHelloFrame {
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
    };
    framed.send(Box::new(hello)).await.unwrap();

    // Receive AgentHello
    let frame = framed.next().await.unwrap().unwrap();
    assert_eq!(*frame.frame_type(), FrameType::AgentHello);

    // Send Notify
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

    // Receive Ack
    let frame = framed.next().await.unwrap().unwrap();
    assert_eq!(*frame.frame_type(), FrameType::Ack);

    // Send HaproxyDisconnect
    let disconnect = HaproxyDisconnectFrame {
        metadata: Metadata {
            flags: FrameFlags::new(true, false),
            stream_id: 0,
            frame_id: 0,
        },
        payload: HaproxyDisconnect {
            status_code: 0,
            message: "done".to_string(),
        },
    };
    framed.send(Box::new(disconnect)).await.unwrap();

    // Receive AgentDisconnect
    let frame = framed.next().await.unwrap().unwrap();
    assert_eq!(*frame.frame_type(), FrameType::AgentDisconnect);

    // Shutdown server
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
