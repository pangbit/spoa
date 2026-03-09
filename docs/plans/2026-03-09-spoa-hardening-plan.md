# SPOA 阶段1 打磨内核 — 实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 spoa 从原型状态打磨到生产可用，引入可配置的 ServerConfig（builder 模式）、细化 Error 类型、增加异常场景测试、清理示例代码。

**Architecture:** 在现有 server.rs 基础上引入 `ServerConfig` 结构体和 `Server` builder，替换 `server::run()` 自由函数。Error 枚举扩展为覆盖协议层、握手层、处理层的细分错误。codec.rs 中 frame 大小检查使用新 Error 类型。

**Tech Stack:** Rust 2021, tokio, tokio-util codec, nom, thiserror, tracing

---

## Task 1: 扩展 Error 枚举

**Files:**
- Modify: `src/error.rs`

**Step 1: 修改 Error 枚举**

将 `src/error.rs` 替换为：

```rust
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("read timeout")]
    ReadTimeout,

    #[error("write timeout")]
    WriteTimeout,

    // 协议层
    #[error("invalid frame type: {0}")]
    InvalidFrameType(u8),

    #[error("frame parse failed: {0}")]
    FrameParseFailed(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("frame too large: {size} bytes, max {max} bytes")]
    FrameTooLarge { size: usize, max: usize },

    // 握手层
    #[error("handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("unsupported spop version: {0}")]
    UnsupportedVersion(String),

    // 处理层
    #[error("processer error: {0}")]
    ProcesserError(String),
}
```

**Step 2: 更新 server.rs 中的引用**

将 `server.rs:176` 的 `Error::InvalidHaproxyHello` 改为 `Error::HandshakeFailed`。

**Step 3: 全局搜索并替换旧变体名**

检查所有引用 `InvalidHaproxyHello` 和 `InvalidSPOPVersion` 的地方，替换为新名称。可能涉及的文件：
- `src/server.rs` — `Error::InvalidHaproxyHello` → `Error::HandshakeFailed`
- `src/protocol/frames/haproxy_hello.rs` — 如果有引用 `InvalidSPOPVersion`

**Step 4: 编译验证**

Run: `cargo build 2>&1`
Expected: 编译通过，无错误

**Step 5: 运行现有测试**

Run: `cargo test 2>&1`
Expected: 所有现有测试通过

**Step 6: Commit**

```bash
git add src/error.rs src/server.rs src/protocol/
git commit -m "refactor: expand Error enum with protocol/handshake/processer variants"
```

---

## Task 2: 引入 ServerConfig 结构体

**Files:**
- Modify: `src/server.rs`
- Modify: `src/lib.rs`

**Step 1: 在 server.rs 中添加 ServerConfig**

在 `server.rs` 的 `use` 语句之后、`const MAX_CONNECTIONS` 之前添加：

```rust
/// Server configuration with sensible defaults.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Read timeout per connection. Default: 30s.
    pub read_timeout: Duration,
    /// Write timeout per connection. Default: 30s.
    pub write_timeout: Duration,
    /// Maximum concurrent connections. Default: 100_000.
    pub max_connections: usize,
    /// Maximum SPOP frame size in bytes. Default: 16_384.
    pub max_frame_size: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            read_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(30),
            max_connections: 100_000,
            max_frame_size: 16_384,
        }
    }
}
```

删除 `const MAX_CONNECTIONS: usize = 100_000;`。

**Step 2: 在 Listener 中存储 config**

将 `Listener` 结构体添加 `config: ServerConfig` 字段：

```rust
struct Listener<L: SpoaListener> {
    listener: L,
    config: ServerConfig,
    limit_connections: Arc<Semaphore>,
    notify_shutdown: broadcast::Sender<()>,
    shutdown_complete_tx: mpsc::Sender<()>,
    processer_holder: Arc<RwLock<ProcesserHolder>>,
}
```

**Step 3: 更新 run() 签名**

将 `run()` 函数签名改为接收 `ServerConfig`：

```rust
pub async fn run<L: SpoaListener>(
    listener: L,
    processer: Arc<RwLock<ProcesserHolder>>,
    shutdown: impl Future,
    config: ServerConfig,
)
```

更新函数体中的 `Listener` 初始化：

```rust
let mut server = Listener {
    listener,
    config: config.clone(),
    limit_connections: Arc::new(Semaphore::new(config.max_connections)),
    notify_shutdown,
    shutdown_complete_tx,
    processer_holder: Arc::clone(&processer),
};
```

**Step 4: 更新 Listener::run() 中的 Handler 创建**

将 `Handler` 初始化中的硬编码值替换为 config：

```rust
let mut handler = Handler {
    socket: Framed::new(socket, SpopCodec { max_frame_size: self.config.max_frame_size }),
    shutdown: Shutdown::new(self.notify_shutdown.subscribe()),
    _shutdown_complete: self.shutdown_complete_tx.clone(),
    processer_holder: Arc::clone(&self.processer_holder),
    read_timeout: self.config.read_timeout,
    write_timeout: self.config.write_timeout,
};
```

**Step 5: 在 lib.rs 中导出 ServerConfig**

在 `src/lib.rs` 中添加：

```rust
pub use server::ServerConfig;
```

**Step 6: 编译验证**

Run: `cargo build 2>&1`
Expected: examples 和 tests 编译失败（调用签名变了），这是预期的

---

## Task 3: 更新调用方适配新 API

**Files:**
- Modify: `examples/server.rs`
- Modify: `examples/client.rs`
- Modify: `tests/handshake.rs`

**Step 1: 更新 examples/server.rs**

将第 103 行：
```rust
spoa::server::run(listener, processer_holder, signal::ctrl_c()).await;
```
改为：
```rust
spoa::server::run(listener, processer_holder, signal::ctrl_c(), spoa::server::ServerConfig::default()).await;
```

**Step 2: 更新 tests/handshake.rs**

将第 54 行：
```rust
spoa::server::run(listener, holder, async {
    shutdown_rx.await.ok();
})
.await;
```
改为：
```rust
spoa::server::run(listener, holder, async {
    shutdown_rx.await.ok();
}, spoa::server::ServerConfig::default())
.await;
```

**Step 3: 编译并测试**

Run: `cargo test 2>&1`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/server.rs src/lib.rs examples/server.rs tests/handshake.rs
git commit -m "feat: introduce ServerConfig with configurable timeouts, max_connections, max_frame_size"
```

---

## Task 4: 重构为 Builder 模式

**Files:**
- Modify: `src/server.rs`
- Modify: `src/lib.rs`

**Step 1: 添加 Server builder 结构体**

在 `ServerConfig` 之后添加：

```rust
/// Builder for configuring and running a SPOA server.
pub struct Server<L: SpoaListener> {
    listener: L,
    processer: Arc<RwLock<ProcesserHolder>>,
    config: ServerConfig,
}

impl<L: SpoaListener> Server<L> {
    pub fn new(listener: L, processer: Arc<RwLock<ProcesserHolder>>) -> Self {
        Self {
            listener,
            processer,
            config: ServerConfig::default(),
        }
    }

    pub fn config(mut self, config: ServerConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn run(self, shutdown: impl Future) {
        run(self.listener, self.processer, shutdown, self.config).await;
    }
}
```

**Step 2: 在 lib.rs 导出 Server**

```rust
pub use server::Server;
```

**Step 3: 编译验证**

Run: `cargo build 2>&1`
Expected: 编译通过

**Step 4: Commit**

```bash
git add src/server.rs src/lib.rs
git commit -m "feat: add Server builder for fluent API configuration"
```

注意：保留原有 `run()` 函数作为底层实现，`Server` builder 是更友好的上层 API。两种方式都可用。

---

## Task 5: codec 中使用新 Error 类型

**Files:**
- Modify: `src/protocol/codec.rs`

**Step 1: 更新 Encoder 中的帧大小检查**

当前 codec.rs 的 encode 方法在帧过大时静默丢弃。改为返回错误：

将第 49-52 行：
```rust
if self.max_frame_size > 0 && serialized.len() > self.max_frame_size {
    warn!("frame too large ({} bytes), dropped", serialized.len());
    return Ok(());
}
```
改为：
```rust
if self.max_frame_size > 0 && serialized.len() > self.max_frame_size {
    return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("frame too large: {} bytes, max {} bytes", serialized.len(), self.max_frame_size),
    ));
}
```

**Step 2: 编译并测试**

Run: `cargo test 2>&1`
Expected: 所有测试通过

**Step 3: Commit**

```bash
git add src/protocol/codec.rs
git commit -m "fix: return error instead of silently dropping oversized frames in codec"
```

---

## Task 6: 异常场景测试 — 超时

**Files:**
- Create: `tests/error_scenarios.rs`

**Step 1: 编写 read_timeout 测试**

```rust
//! Tests for error scenarios: timeouts, malformed frames, connection limits

use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use spoa::server::ServerConfig;
use spoa::{IProcesser, Message, ProcesserHolder, TypedData, VarScope};

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

#[tokio::test]
async fn test_read_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let holder = Arc::new(RwLock::new(ProcesserHolder::new(Box::new(NoopProcesser))));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let config = ServerConfig {
        read_timeout: Duration::from_millis(200),
        ..Default::default()
    };

    let server_handle = tokio::spawn({
        let holder = Arc::clone(&holder);
        async move {
            spoa::server::run(listener, holder, async { shutdown_rx.await.ok(); }, config).await;
        }
    });

    // Connect but send nothing — should trigger read timeout
    let _stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Wait longer than read_timeout
    tokio::time::sleep(Duration::from_millis(500)).await;

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
    // If we get here without hanging, read timeout works correctly
}
```

**Step 2: 运行测试**

Run: `cargo test test_read_timeout -- --nocapture 2>&1`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/error_scenarios.rs
git commit -m "test: add read timeout integration test"
```

---

## Task 7: 异常场景测试 — 畸形帧

**Files:**
- Modify: `tests/error_scenarios.rs`

**Step 1: 添加畸形帧测试**

在 `tests/error_scenarios.rs` 末尾添加：

```rust
#[tokio::test]
async fn test_malformed_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let holder = Arc::new(RwLock::new(ProcesserHolder::new(Box::new(NoopProcesser))));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let config = ServerConfig {
        read_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let server_handle = tokio::spawn({
        let holder = Arc::clone(&holder);
        async move {
            spoa::server::run(listener, holder, async { shutdown_rx.await.ok(); }, config).await;
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

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
```

**Step 2: 运行测试**

Run: `cargo test test_malformed_frame -- --nocapture 2>&1`
Expected: PASS（服务器应优雅地处理错误并关闭连接，不 panic）

**Step 3: Commit**

```bash
git add tests/error_scenarios.rs
git commit -m "test: add malformed frame integration test"
```

---

## Task 8: 异常场景测试 — 并发连接

**Files:**
- Modify: `tests/error_scenarios.rs`

**Step 1: 添加并发握手测试**

```rust
use futures::{SinkExt, StreamExt};
use semver::Version;
use std::str::FromStr;
use std::collections::HashMap;
use spoa::{FrameFlags, FrameType, Metadata, SpopCodec};
use spoa::protocol::frame::Message;
use spoa::protocol::frames::{FrameCapabilities, haproxy_hello::{HaproxyHello, HaproxyHelloFrame}, notify::NotifyFrame};
use tokio_util::codec::Framed;

#[tokio::test]
async fn test_concurrent_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let holder = Arc::new(RwLock::new(ProcesserHolder::new(Box::new(NoopProcesser))));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server_handle = tokio::spawn({
        let holder = Arc::clone(&holder);
        async move {
            spoa::server::run(
                listener, holder,
                async { shutdown_rx.await.ok(); },
                ServerConfig::default(),
            ).await;
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Spawn 10 concurrent clients doing full handshake + notify
    let mut handles = vec![];
    for _ in 0..10 {
        handles.push(tokio::spawn(async move {
            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let mut framed = Framed::new(stream, SpopCodec { max_frame_size: 0 });

            // Handshake
            let hello = HaproxyHelloFrame {
                metadata: Metadata { flags: FrameFlags::new(true, false), stream_id: 0, frame_id: 0 },
                payload: HaproxyHello {
                    supported_versions: vec![Version::new(2, 0, 0)],
                    max_frame_size: 1024,
                    capabilities: vec![FrameCapabilities::from_str("pipelining").unwrap()],
                    healthcheck: Some(false),
                    engine_id: None,
                },
            };
            framed.send(Box::new(hello)).await.unwrap();
            let frame = framed.next().await.unwrap().unwrap();
            assert_eq!(*frame.frame_type(), FrameType::AgentHello);

            // Notify
            let notify = NotifyFrame {
                metadata: Metadata { flags: FrameFlags::new(true, false), stream_id: 1, frame_id: 1 },
                messages: vec![Message { name: "test".to_string(), args: HashMap::new() }],
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
```

注意：需要将 `use` 语句移到文件顶部，与 Task 6 的 imports 合并。

**Step 2: 运行测试**

Run: `cargo test test_concurrent_connections -- --nocapture 2>&1`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/error_scenarios.rs
git commit -m "test: add concurrent connections integration test"
```

---

## Task 9: 异常场景测试 — 客户端中途断连

**Files:**
- Modify: `tests/error_scenarios.rs`

**Step 1: 添加断连测试**

```rust
#[tokio::test]
async fn test_client_disconnect_mid_session() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let holder = Arc::new(RwLock::new(ProcesserHolder::new(Box::new(NoopProcesser))));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server_handle = tokio::spawn({
        let holder = Arc::clone(&holder);
        async move {
            spoa::server::run(
                listener, holder,
                async { shutdown_rx.await.ok(); },
                ServerConfig::default(),
            ).await;
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect, send hello, then drop connection without disconnect
    {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut framed = Framed::new(stream, SpopCodec { max_frame_size: 0 });

        let hello = HaproxyHelloFrame {
            metadata: Metadata { flags: FrameFlags::new(true, false), stream_id: 0, frame_id: 0 },
            payload: HaproxyHello {
                supported_versions: vec![Version::new(2, 0, 0)],
                max_frame_size: 1024,
                capabilities: vec![FrameCapabilities::from_str("pipelining").unwrap()],
                healthcheck: Some(false),
                engine_id: None,
            },
        };
        framed.send(Box::new(hello)).await.unwrap();
        let _frame = framed.next().await.unwrap().unwrap();
        // Drop framed/stream here — simulates abrupt disconnect
    }

    // Server should handle disconnect gracefully
    tokio::time::sleep(Duration::from_millis(200)).await;

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
```

**Step 2: 运行测试**

Run: `cargo test test_client_disconnect -- --nocapture 2>&1`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/error_scenarios.rs
git commit -m "test: add client mid-session disconnect test"
```

---

## Task 10: 代码清理

**Files:**
- Modify: `examples/server.rs`
- Modify: `examples/client.rs`

**Step 1: 清理 examples/server.rs**

- 删除第 23-37 行注释掉的调试代码块
- 删除第 61 行注释掉的 `// #[tokio::main]`
- 删除第 66 行注释掉的 `// let listener = TcpListener::bind(...)`

**Step 2: 清理 examples/client.rs**

将第 38 行硬编码 IP：
```rust
let socket = TcpStream::connect("192.168.12.123:33103").await.unwrap();
```
改为使用环境变量或默认 localhost：
```rust
let addr = std::env::var("SPOA_SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:33103".to_string());
let socket = TcpStream::connect(&addr).await.unwrap();
```

在 `main()` 开头添加地址打印：
```rust
let addr = std::env::var("SPOA_SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:33103".to_string());
info!("connecting to {}", addr);
```

并在 spawn 闭包中传入 addr clone。

**Step 3: 运行 clippy**

Run: `cargo clippy -- -D warnings 2>&1`
Expected: 零警告

**Step 4: 修复 clippy 发现的问题（如有）**

**Step 5: Commit**

```bash
git add examples/
git commit -m "chore: clean up examples, remove debug code, use env var for client addr"
```

---

## Task 11: 最终验证

**Step 1: 完整测试**

Run: `cargo test 2>&1`
Expected: 所有测试通过

**Step 2: clippy 检查**

Run: `cargo clippy -- -D warnings 2>&1`
Expected: 零警告

**Step 3: 版本号更新**

将 `Cargo.toml` 中 `version` 从 `"0.2.0"` 改为 `"0.3.0"`。

**Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "chore: bump version to 0.3.0 for ServerConfig breaking change"
```

---

## 任务依赖关系

```
Task 1 (Error 枚举) → Task 2 (ServerConfig) → Task 3 (更新调用方)
                                              → Task 4 (Builder 模式)
                       Task 5 (codec 错误)
Task 3 完成后 → Task 6-9 (测试)
              → Task 10 (代码清理)
全部完成 → Task 11 (最终验证)
```

## 预估工作量

- Task 1-5: 核心重构（约 5 个 commit）
- Task 6-9: 测试增强（约 4 个 commit）
- Task 10-11: 清理验证（约 2 个 commit）
