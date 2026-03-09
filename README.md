# SPOA

[中文文档](README.zh-CN.md)

Rust implementation of HAProxy's SPOP (Stream Processing Offload Protocol) agent framework. Serves as both the protocol maintainer and the production-ready Rust SDK, aiming to become the cross-language reference implementation and drive the protocol's evolution.

## Features

- Async server based on tokio
- TCP and Unix socket support via generic `SpoaListener` trait
- Configurable timeouts, connection limits, and frame sizes (`ServerConfig`)
- Builder pattern for server setup
- Hot-reload of message processors at runtime
- Graceful shutdown

## Quick Start

```rust
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use spoa::{IProcesser, Message, ProcesserHolder, TypedData, VarScope, ServerConfig};

struct MyProcesser;

#[async_trait::async_trait]
impl IProcesser for MyProcesser {
    async fn handle_messages(
        &self,
        messages: &[Message],
    ) -> spoa::Result<Vec<(VarScope, String, TypedData)>> {
        Ok(vec![(
            VarScope::Transaction,
            "my_app.result".to_string(),
            TypedData::String("hello from spoa".to_string()),
        )])
    }
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("0.0.0.0:33103").await.unwrap();
    let processer = Box::new(MyProcesser);
    let holder = Arc::new(RwLock::new(ProcesserHolder::new(processer)));

    // Using builder pattern
    spoa::Server::new(listener, holder)
        .config(ServerConfig {
            max_frame_size: 16_384,
            ..Default::default()
        })
        .run(tokio::signal::ctrl_c())
        .await;
}
```

## Configuration

`ServerConfig` provides tunable parameters:

| Field | Default | Description |
|-------|---------|-------------|
| `read_timeout` | 30s | Per-connection read timeout |
| `write_timeout` | 30s | Per-connection write timeout |
| `max_connections` | 100,000 | Maximum concurrent connections |
| `max_frame_size` | 16,384 | Maximum SPOP frame size in bytes |

## Protocol

Implements SPOP as defined in [HAProxy SPOE specification](https://github.com/haproxy/haproxy/blob/master/doc/SPOE.txt).

Supported frame types:
- HAPROXY-HELLO / AGENT-HELLO (handshake)
- NOTIFY / ACK (message processing)
- HAPROXY-DISCONNECT / AGENT-DISCONNECT (connection teardown)
- Health check detection

## License

MIT
