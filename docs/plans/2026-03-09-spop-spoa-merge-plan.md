# spop + spoa 合并实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 spop 协议库合并进 spoa，消除双仓库维护开销，统一服务器代码

**Architecture:** spop 代码整体移入 `src/protocol/` 子模块，server.rs/uds_server.rs 通过泛型 trait 合一，错误类型统一为单一 enum

**Tech Stack:** Rust 2021 edition, tokio, nom, tokio-util codec, thiserror

**Reference:** `docs/plans/2026-03-09-spop-spoa-merge-design.md`

**Important:** 本地 spop 落后于远程 (local: `a5a08e0`, remote/spoa 使用: `a846594`)。远程版本有 `SpopCodec { max_frame_size }` 和 `AgentHelloFrame`/`AgentDisconnectFrame` 等类型。合并时以远程版本为准。

---

### Task 0: 同步本地 spop 到远程最新版本

**Files:**
- Modify: `/Users/xubochen/Workspace/spop/` (git pull)

**Step 1: 拉取远程最新代码**

Run: `cd /Users/xubochen/Workspace/spop && git pull origin main`

Expected: Fast-forward to `a846594`

**Step 2: 验证本地 spop 编译通过**

Run: `cd /Users/xubochen/Workspace/spop && cargo check`

Expected: Compiles without errors

**Step 3: 切换 spoa 到 path 依赖并验证**

Modify `/Users/xubochen/Workspace/spoa/Cargo.toml`:
```toml
# 临时改为 path 依赖，确保本地 spop 与 spoa 兼容
spop = { path = "../spop" }
```

Run: `cd /Users/xubochen/Workspace/spoa && cargo check`

Expected: Compiles without errors. 如果失败，修复不兼容问题。

---

### Task 1: 创建 protocol 模块目录并复制 spop 源文件

**Files:**
- Create: `src/protocol/mod.rs`
- Copy: 从 `/Users/xubochen/Workspace/spop/src/` 复制所有 .rs 文件到 `src/protocol/`
- Copy: 从 `/Users/xubochen/Workspace/spop/src/frames/` 复制到 `src/protocol/frames/`

**Step 1: 创建目录结构**

```bash
mkdir -p /Users/xubochen/Workspace/spoa/src/protocol/frames
```

**Step 2: 复制 spop 源文件（不含 lib.rs）**

```bash
cd /Users/xubochen/Workspace/spop/src
cp frame.rs parser.rs types.rs actions.rs codec.rs varint.rs /Users/xubochen/Workspace/spoa/src/protocol/
cp frames/ack.rs frames/agent_disconnect.rs frames/agent_hello.rs frames/capabilities.rs frames/haproxy_disconnect.rs frames/haproxy_hello.rs frames/notify.rs /Users/xubochen/Workspace/spoa/src/protocol/frames/
cp frames/mod.rs /Users/xubochen/Workspace/spoa/src/protocol/frames/mod.rs
```

**Step 3: 创建 `src/protocol/mod.rs`**

将 spop 的 `lib.rs` 内容转化为 `mod.rs`，将所有 `crate::` 引用保持不变（protocol 内部用 `crate::protocol::` 或者用相对路径 `super::`）。

```rust
//! SPOP Protocol - parsing HAProxy SPOP (Stream Processing Offload Protocol)
//!
//! <https://github.com/haproxy/haproxy/blob/master/doc/SPOE.txt>

pub mod frames;
pub mod parser;

pub mod actions;
pub use self::actions::{Action, VarScope};

pub mod frame;
pub use self::frame::{FrameFlags, FramePayload, FrameType, Metadata};

pub mod types;
pub use self::types::TypedData;

pub mod varint;
pub use self::varint::{decode_varint, encode_varint};

pub mod codec;
pub use self::codec::SpopCodec;

// SpopFrame trait 和 encode_payload 函数从 spop/lib.rs 移入此处
// （完整复制 SpopFrame trait 定义和 encode_payload 函数）
```

注意：将 spop `lib.rs` 中的 `SpopFrame` trait 和 `encode_payload()` 函数体也复制到此 `mod.rs` 中。

**Step 4: 验证文件都已就位**

Run: `find /Users/xubochen/Workspace/spoa/src/protocol -name "*.rs" | sort`

Expected:
```
src/protocol/actions.rs
src/protocol/codec.rs
src/protocol/frame.rs
src/protocol/frames/ack.rs
src/protocol/frames/agent_disconnect.rs
src/protocol/frames/agent_hello.rs
src/protocol/frames/capabilities.rs
src/protocol/frames/haproxy_disconnect.rs
src/protocol/frames/haproxy_hello.rs
src/protocol/frames/mod.rs
src/protocol/frames/notify.rs
src/protocol/mod.rs
src/protocol/parser.rs
src/protocol/types.rs
src/protocol/varint.rs
```

---

### Task 2: 修复 protocol 模块内部的 crate 路径

**Files:**
- Modify: `src/protocol/` 下所有 .rs 文件

所有 spop 源文件中使用 `crate::` 的路径都需要改为 `crate::protocol::`。

**受影响的文件和具体变更：**

`src/protocol/codec.rs`:
- `use crate::{SpopFrame, parser::parse_frame}` → `use crate::protocol::{SpopFrame, parser::parse_frame}`

`src/protocol/frame.rs`:
- `use crate::{actions::Action, types::TypedData, varint::encode_varint}` → `use crate::protocol::{actions::Action, types::TypedData, varint::encode_varint}`

`src/protocol/types.rs`:
- `use crate::varint::{decode_varint, encode_varint}` → `use crate::protocol::varint::{decode_varint, encode_varint}`

`src/protocol/frames/ack.rs`:
- `use crate::{SpopFrame, actions::..., frame::..., types::...}` → `use crate::protocol::{SpopFrame, actions::..., frame::..., types::...}`

`src/protocol/frames/agent_hello.rs`:
- `use crate::{SpopFrame, frame::..., frames::..., types::...}` → `use crate::protocol::{SpopFrame, frame::..., frames::..., types::...}`

`src/protocol/frames/agent_disconnect.rs`:
- 同上模式

`src/protocol/frames/haproxy_hello.rs`:
- 同上模式

`src/protocol/frames/haproxy_disconnect.rs`:
- 同上模式

`src/protocol/frames/notify.rs`:
- 同上模式

`src/protocol/parser.rs`:
- 所有 `crate::` → `crate::protocol::`

**Step 1: 批量替换**

对 `src/protocol/` 下所有 .rs 文件执行替换：`crate::` → `crate::protocol::` （仅对 protocol 内部文件）。

注意：`mod.rs` 中的 `self::` 引用不需要改。

**Step 2: 验证编译**

此时暂不编译（需要先完成 Task 3 更新 lib.rs）。

---

### Task 3: 更新 Cargo.toml 和 lib.rs

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`

**Step 1: 更新 Cargo.toml**

```toml
[package]
name = "spoa"
version = "0.2.0"
edition = "2021"

[dependencies]
async-trait = "0.1"
bytes = "1"
futures = "0.3"
nom = "8.0"
rand = "0.9"
semver = "1.0"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec"] }
tracing = "0.1"

[dev-dependencies]
anyhow = "1"
socket2 = "0.6"
tracing-subscriber = "0.3"

[[example]]
name = "server"
path = "examples/server.rs"
```

关键变更：
- 删除 `spop` 依赖
- 添加 `bytes`, `nom`, `rand`（来自 spop）
- 版本升到 `0.2.0`
- edition 改为 `2021`

**Step 2: 更新 `src/lib.rs`**

```rust
mod error;
pub use error::Error;
pub use error::Result;

mod shutdown;
pub(crate) use shutdown::Shutdown;

pub mod protocol;
pub mod server;

// Re-export 协议层核心类型（保持 zwwaf-spoa 的 import 不变）
pub use protocol::SpopFrame;
pub use protocol::types::TypedData;
pub use protocol::actions::{Action, VarScope};
pub use protocol::frame::{Message, FramePayload, FrameType, Metadata, FrameFlags};
pub use protocol::codec::SpopCodec;
pub use protocol::frames::{
    Ack, AgentDisconnect, AgentDisconnectFrame, AgentHello, AgentHelloFrame,
    FrameCapabilities, HaproxyDisconnect, HaproxyHello,
};
pub use protocol::varint::{decode_varint, encode_varint};

#[async_trait::async_trait]
pub trait IProcesser: Send + Sync {
    async fn handle_messages(
        &self,
        messages: &[Message],
    ) -> Result<Vec<(VarScope, String, TypedData)>>;
}

pub struct ProcesserHolder {
    processer: Box<dyn IProcesser + Sync + Send>,
}

impl ProcesserHolder {
    pub fn new(processer: Box<dyn IProcesser + Sync + Send>) -> Self {
        Self { processer }
    }

    pub fn set_processer(&mut self, new_processer: Box<dyn IProcesser + Sync + Send>) {
        self.processer = new_processer;
    }

    pub fn replace_processer(
        &mut self,
        new_processer: Box<dyn IProcesser + Sync + Send>,
    ) -> Box<dyn IProcesser + Sync + Send> {
        std::mem::replace(&mut self.processer, new_processer)
    }
}
```

关键变更：
- 删除 `mod uds_server`（将在 Task 4 合并到 server.rs）
- `pub mod protocol` 替代 `spop` 依赖
- 所有原来从 `spop` re-export 的类型改为从 `protocol` re-export
- 添加 `IProcesser` 的 `Send + Sync` bound（之前依赖 `dyn` 的隐式约束）

**Step 3: 首次编译验证**

Run: `cargo check 2>&1`

Expected: 可能有错误（server.rs 还引用 `spop::` 路径），这些会在 Task 4 修复。

**Step 4: 提交**

```bash
git add src/protocol/ Cargo.toml src/lib.rs
git commit -m "feat: merge spop protocol library into spoa as protocol module"
```

---

### Task 4: 合并 server.rs 和 uds_server.rs 为泛型实现

**Files:**
- Rewrite: `src/server.rs`
- Delete: `src/uds_server.rs`

**Step 1: 编写统一的泛型 server.rs**

```rust
use std::future::Future;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use semver::Version;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::sync::{RwLock, Semaphore, broadcast, mpsc};
use tokio::time::{self, Duration};
use tokio_util::codec::Framed;
use tracing::{debug, error, info};

use crate::protocol::frames::{Ack, AgentDisconnectFrame, AgentHelloFrame, FrameCapabilities, HaproxyHello};
use crate::protocol::{FramePayload, FrameType, SpopCodec, SpopFrame};
use crate::{Error, ProcesserHolder, Result, Shutdown};

const MAX_CONNECTIONS: usize = 100_000;

/// Trait abstracting TCP and Unix socket listeners.
pub trait SpoaListener: Send + 'static {
    type Stream: AsyncRead + AsyncWrite + Send + Unpin + 'static;

    fn accept(&self) -> impl Future<Output = std::io::Result<Self::Stream>> + Send;
}

impl SpoaListener for TcpListener {
    type Stream = TcpStream;

    async fn accept(&self) -> std::io::Result<TcpStream> {
        let (stream, _addr) = TcpListener::accept(self).await?;
        Ok(stream)
    }
}

impl SpoaListener for UnixListener {
    type Stream = UnixStream;

    async fn accept(&self) -> std::io::Result<UnixStream> {
        let (stream, _addr) = UnixListener::accept(self).await?;
        Ok(stream)
    }
}

struct Listener<L: SpoaListener> {
    listener: L,
    limit_connections: Arc<Semaphore>,
    notify_shutdown: broadcast::Sender<()>,
    shutdown_complete_tx: mpsc::Sender<()>,
    processer_holder: Arc<RwLock<ProcesserHolder>>,
}

struct Handler<S: AsyncRead + AsyncWrite + Send + Unpin> {
    socket: Framed<S, SpopCodec>,
    read_timeout: Duration,
    write_timeout: Duration,
    shutdown: Shutdown,
    _shutdown_complete: mpsc::Sender<()>,
    processer_holder: Arc<RwLock<ProcesserHolder>>,
}

pub async fn run<L: SpoaListener>(
    listener: L,
    processer: Arc<RwLock<ProcesserHolder>>,
    shutdown: impl Future,
) {
    let (notify_shutdown, _) = broadcast::channel(1);
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    let mut server = Listener {
        listener,
        limit_connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        notify_shutdown,
        shutdown_complete_tx,
        processer_holder: Arc::clone(&processer),
    };

    tokio::select! {
        res = server.run() => {
            if let Err(err) = res {
                error!(cause = %err, "failed to accept");
            }
        }
        _ = shutdown => {
            info!("shutting down");
        }
    }

    let Listener {
        shutdown_complete_tx,
        notify_shutdown,
        ..
    } = server;

    drop(notify_shutdown);
    drop(shutdown_complete_tx);

    let _ = shutdown_complete_rx.recv().await;
}

impl<L: SpoaListener> Listener<L> {
    async fn run(&mut self) -> Result<()> {
        info!("accepting inbound connections");

        loop {
            let permit = self
                .limit_connections
                .clone()
                .acquire_owned()
                .await
                .unwrap();

            let socket = self.accept_with_backoff().await?;

            let mut handler = Handler {
                socket: Framed::new(socket, SpopCodec { max_frame_size: 0 }),
                shutdown: Shutdown::new(self.notify_shutdown.subscribe()),
                _shutdown_complete: self.shutdown_complete_tx.clone(),
                processer_holder: Arc::clone(&self.processer_holder),
                read_timeout: Duration::from_secs(30),
                write_timeout: Duration::from_secs(30),
            };

            tokio::spawn(async move {
                if let Err(err) = handler.run().await {
                    debug!(cause = ?err, "connection error");
                }
                drop(permit)
            });
        }
    }

    async fn accept_with_backoff(&self) -> Result<L::Stream> {
        let mut backoff = 1;

        loop {
            match self.listener.accept().await {
                Ok(stream) => return Ok(stream),
                Err(err) => {
                    if backoff > 64 {
                        return Err(Error::IO(err));
                    }
                }
            }

            time::sleep(Duration::from_secs(backoff)).await;
            backoff *= 2;
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Send + Unpin> Handler<S> {
    async fn run(&mut self) -> Result<()> {
        while !self.shutdown.is_shutdown() {
            let maybe_frame = tokio::select! {
                res = self.socket.next() => res,
                _ = self.shutdown.recv() => {
                    return Ok(());
                }
                _ = time::sleep(self.read_timeout) => {
                    return Err(Error::ReadTimeout);
                }
            };

            let frame = match maybe_frame {
                Some(Ok(frame)) => frame,
                Some(Err(e)) => {
                    error!("read_frame failed: {}", e);
                    return Err(Error::IO(e));
                }
                None => return Ok(()),
            };

            match frame.frame_type() {
                FrameType::HaproxyHello => {
                    let hello = HaproxyHello::try_from(frame.payload())
                        .map_err(Error::InvalidHaproxyHello)?;

                    let max_frame_size = hello.max_frame_size;
                    let is_healthcheck = hello.healthcheck.unwrap_or(false);
                    let version = Version::parse("2.0.0").unwrap();

                    let agent_hello = AgentHelloFrame::new(
                        version,
                        max_frame_size,
                        vec![FrameCapabilities::Pipelining],
                    );

                    debug!("Sending AgentHello: {:?}", agent_hello.payload());

                    match time::timeout(self.write_timeout, self.socket.send(Box::new(agent_hello)))
                        .await
                    {
                        Ok(Ok(_)) => {}
                        Ok(Err(e)) => return Err(e.into()),
                        Err(_) => return Err(Error::WriteTimeout),
                    };

                    if is_healthcheck {
                        info!("Handled healthcheck. Closing socket.");
                        return Ok(());
                    }
                }

                FrameType::HaproxyDisconnect => {
                    let agent_disconnect = AgentDisconnectFrame::new(0, "Goodbye".to_string());
                    info!("Sending AgentDisconnect: {:?}", agent_disconnect.payload());

                    match time::timeout(
                        self.write_timeout,
                        self.socket.send(Box::new(agent_disconnect)),
                    )
                    .await
                    {
                        Ok(Ok(_)) => self.socket.close().await?,
                        Ok(Err(e)) => return Err(e.into()),
                        Err(_) => return Err(Error::WriteTimeout),
                    }

                    return Ok(());
                }

                FrameType::Notify => {
                    if let FramePayload::ListOfMessages(messages) = &frame.payload() {
                        let meta = frame.metadata();

                        let ack = match self
                            .processer_holder
                            .read()
                            .await
                            .processer
                            .handle_messages(messages)
                            .await
                        {
                            Ok(vars) => {
                                vars.into_iter().fold(
                                    Ack::new(meta.stream_id, meta.frame_id),
                                    |ack, (scope, name, value)| ack.set_var(scope, &name, value),
                                )
                            }
                            Err(e) => {
                                error!("processer handle_messages failed: {}", e);
                                Ack::new(meta.stream_id, meta.frame_id)
                            }
                        };

                        debug!("Sending Ack: {:?}", ack.payload());
                        match time::timeout(self.write_timeout, self.socket.send(Box::new(ack)))
                            .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => return Err(e.into()),
                            Err(_) => return Err(Error::WriteTimeout),
                        }
                    }
                }

                _ => {
                    error!("Unsupported frame type: {:?}", frame.frame_type());
                }
            }
        }

        Ok(())
    }
}
```

**Step 2: 删除 uds_server.rs**

```bash
rm /Users/xubochen/Workspace/spoa/src/uds_server.rs
```

**Step 3: 编译验证**

Run: `cargo check 2>&1`

Expected: 编译通过。如果 `AgentHelloFrame::new()` 或 `AgentDisconnectFrame::new()` 签名不匹配，按远程 spop 版本调整。

**Step 4: 提交**

```bash
git add src/server.rs
git rm src/uds_server.rs
git commit -m "refactor: unify TCP and UDS servers with generic SpoaListener trait"
```

---

### Task 5: 更新 error.rs

**Files:**
- Modify: `src/error.rs`

**Step 1: 更新错误类型**

保持现有的 4 个错误变体不变（它们已经足够），因为 spop 的协议层错误通过 `std::io::Error` 传播，已经被 `IO` 变体覆盖。

验证 `src/error.rs` 不需要改动（当前内容已足够）。

**Step 2: 编译验证**

Run: `cargo check 2>&1`

Expected: 编译通过

---

### Task 6: 运行已有测试

**Files:** 无新增

**Step 1: 运行所有测试（包含从 spop 迁移的单元测试）**

Run: `cargo test 2>&1`

Expected: 所有测试通过。spop 中 `frame.rs`、`types.rs`、`varint.rs`、`haproxy_hello.rs`、`capabilities.rs` 的单元测试应自动被包含。

**Step 2: 如果有测试失败，修复**

常见问题：
- 路径引用错误（`crate::` vs `crate::protocol::`）
- `use super::*` 在测试模块中可能需要调整

**Step 3: 提交**

```bash
git commit --allow-empty -m "test: verify migrated spop tests pass"
```

（如有修复则 `git add` 相关文件）

---

### Task 7: 更新 examples

**Files:**
- Modify: `examples/server.rs`
- Modify: `examples/client.rs`

**Step 1: 更新 server.rs example**

主要变更：`use spoa::{self, IProcesser, Message, ProcesserHolder, TypedData, VarScope}` 不变。
`spoa::server::run(listener, ...)` 调用不变（`run` 现在是泛型的，`TcpListener` 自动推导）。

验证是否需要任何改动。

**Step 2: 更新 client.rs example**

主要变更：
- `use spop::...` → `use spoa::...` 或 `use spoa::protocol::...`
- 例如：`use spoa::{FrameFlags, Metadata, SpopCodec, Message}`
- `use spoa::protocol::frames::{...}` 对于深层类型

**Step 3: 编译示例验证**

Run: `cargo check --examples 2>&1`

Expected: 编译通过

**Step 4: 提交**

```bash
git add examples/
git commit -m "chore: update examples for merged crate structure"
```

---

### Task 8: 新增端到端集成测试

**Files:**
- Create: `tests/handshake.rs`

**Step 1: 编写集成测试**

```rust
//! End-to-end SPOP handshake test

use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use futures::{SinkExt, StreamExt};
use tokio_util::codec::Framed;
use semver::Version;
use std::str::FromStr;
use std::collections::HashMap;

use spoa::{
    IProcesser, Message, ProcesserHolder, TypedData, VarScope,
    SpopCodec, FrameFlags, Metadata, FrameType,
};
use spoa::protocol::frames::{
    FrameCapabilities, HaproxyHello,
    haproxy_hello::HaproxyHelloFrame,
    haproxy_disconnect::{HaproxyDisconnect, HaproxyDisconnectFrame},
    notify::NotifyFrame,
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
            spoa::server::run(listener, holder, async { shutdown_rx.await.ok(); }).await;
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
```

**Step 2: 运行测试**

Run: `cargo test test_full_handshake -- --nocapture 2>&1`

Expected: PASS

**Step 3: 提交**

```bash
git add tests/handshake.rs
git commit -m "test: add end-to-end SPOP handshake integration test"
```

---

### Task 9: 清理并最终验证

**Files:** 无新增

**Step 1: 运行全部测试**

Run: `cargo test 2>&1`

Expected: 所有测试通过

**Step 2: 运行 clippy**

Run: `cargo clippy 2>&1`

Expected: 无 warning（或只有可接受的 warning）

**Step 3: 检查示例编译**

Run: `cargo check --examples 2>&1`

Expected: 编译通过

**Step 4: 确认不再依赖 spop**

Run: `grep -r "spop" Cargo.toml`

Expected: 无 `spop` 依赖行

**Step 5: 最终提交**

```bash
git add -A
git commit -m "chore: final cleanup after spop merge"
```

---

### Task 10: 验证 zwwaf-spoa 兼容性

**Files:**
- Modify: `/Users/xubochen/Workspace/zwwaf-spoa/Cargo.toml`（临时改为 path 依赖测试）

**Step 1: 临时切换 zwwaf-spoa 的 spoa 依赖为 path**

```toml
spoa = { path = "../spoa" }
```

**Step 2: 编译 zwwaf-spoa**

Run: `cd /Users/xubochen/Workspace/zwwaf-spoa && cargo check 2>&1`

Expected: 编译通过。如果有 import 路径错误，记录需要修改的文件。

**Step 3: 如有编译错误，修复 zwwaf-spoa 的 import**

可能的变更：
- 如果 zwwaf-spoa 直接使用了 `spop::` 路径，需改为 `spoa::` 或 `spoa::protocol::`
- 大部分情况下无需改动（已通过 spoa re-export）

**Step 4: 恢复或保持 path 依赖**

根据开发习惯决定是保持 path 依赖还是推送后改回 git 依赖。

---

## 任务依赖图

```
Task 0 (sync spop)
  → Task 1 (copy files)
    → Task 2 (fix crate paths)
      → Task 3 (update Cargo.toml + lib.rs)
        → Task 4 (unify server) + Task 5 (error.rs)
          → Task 6 (run tests)
            → Task 7 (update examples)
              → Task 8 (integration test)
                → Task 9 (cleanup)
                  → Task 10 (verify zwwaf-spoa)
```

## 预计工作量

- Task 0-3: 机械性文件操作和路径替换
- Task 4: 核心重构，需要仔细测试
- Task 5-7: 小改动
- Task 8: 新代码，但基于现有 client.rs 示例
- Task 9-10: 验证

总计约 11 个任务，每个 2-10 分钟。
