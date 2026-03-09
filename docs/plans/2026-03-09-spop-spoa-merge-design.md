# spop + spoa 合并设计

日期: 2026-03-09

## 背景

当前 SPOA 生态由三层组成：

```
zwwaf-spoa (生产 WAF 应用, 52K+ 行)
    ↓
spoa (服务器框架, 612 行)
    ↓
spop (协议解析库, 2184 行)
```

spop 和 spoa 都由同一团队维护，已完全独立于上游（Nicolas Embriz），且唯一消费者是 zwwaf-spoa。两个独立 crate 带来的维护开销大于其分离带来的收益。

## 决策

将 spop 代码完整合并进 spoa，形成单一 crate。

## 合并后模块结构

```
spoa/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # 统一入口，re-export 核心类型
│   │
│   ├── protocol/               # 原 spop 代码，协议层
│   │   ├── mod.rs
│   │   ├── frame.rs            # FrameType, Metadata, FramePayload, Message, FrameFlags
│   │   ├── parser.rs           # parse_frame() — nom 解析器
│   │   ├── types.rs            # TypedData 枚举
│   │   ├── actions.rs          # Action, VarScope
│   │   ├── codec.rs            # SpopCodec (Encoder/Decoder)
│   │   ├── varint.rs           # encode_varint/decode_varint
│   │   └── frames/             # 各帧类型实现
│   │       ├── mod.rs
│   │       ├── haproxy_hello.rs
│   │       ├── agent_hello.rs
│   │       ├── haproxy_disconnect.rs
│   │       ├── agent_disconnect.rs
│   │       ├── notify.rs
│   │       ├── ack.rs
│   │       └── capabilities.rs
│   │
│   ├── server.rs               # 统一泛型服务器（TCP + UDS 合一）
│   ├── shutdown.rs             # 优雅关闭
│   └── error.rs                # 统一错误类型
│
└── examples/
    ├── server.rs
    └── client.rs
```

## 消除 server.rs / uds_server.rs 重复

用泛型 trait 统一 TCP 和 UDS 服务器（当前 261 + 261 = 522 行近乎重复代码）：

```rust
pub trait SpoaListener: Send + 'static {
    type Stream: AsyncRead + AsyncWrite + Send + Unpin + 'static;
    async fn accept(&self) -> io::Result<Self::Stream>;
}
```

- `TcpListener` 和 `UnixListener` 各自实现此 trait
- `Listener<L: SpoaListener>` 和 `Handler<S>` 使用泛型，一套代码处理两种传输
- 预计 522 行 → ~280 行（减少 ~46%）
- 未来加 TLS 只需实现 `SpoaListener`

## 统一错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // 协议层（原 spop）
    #[error("frame parse error: {0}")]
    FrameParse(String),
    #[error("frame encode error: {0}")]
    FrameEncode(String),

    // 服务器层（原 spoa）
    #[error("read timeout")]
    ReadTimeout,
    #[error("write timeout")]
    WriteTimeout,
    #[error("invalid haproxy hello")]
    InvalidHaproxyHello,
    #[error("invalid SPOP version: {0}")]
    InvalidSPOPVersion(String),

    // 通用
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

## 依赖合并

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
```

去掉 `anyhow`（协议层改用自定义 Error），加入 `nom`、`bytes`、`rand`（来自 spop）。

## 公共 API

```rust
// 顶层 re-export — 保持 zwwaf-spoa 使用习惯不变
pub use protocol::types::TypedData;
pub use protocol::actions::{Action, VarScope};
pub use protocol::frame::{Message, FramePayload, FrameType, Metadata, FrameFlags};
pub use protocol::codec::SpopCodec;
pub use protocol::SpopFrame;
pub use server::{Listener, SpoaListener};
pub use shutdown::Shutdown;
```

## 对 zwwaf-spoa 的影响

- Cargo.toml：无变化（仍然只依赖 spoa）
- import 路径：无变化（已经通过 spoa re-export 使用）
- 调用方式：`Listener::new()` 泛型自动推导，改动极小

## 测试策略

1. 迁移 spop 原有单元测试（varint、TypedData、capabilities）
2. 新增端到端集成测试：模拟完整 SPOP 握手流程
3. 保留 examples/ 作为冒烟测试

## 版本

`0.2.0` — 标记 breaking change（模块路径变更、错误类型统一）。
