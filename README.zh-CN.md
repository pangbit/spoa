# SPOA

[English](README.md)

HAProxy SPOP（Stream Processing Offload Protocol）协议的 Rust agent framework 实现。本项目同时承担协议维护和 Rust SDK 两个角色，目标是成为 SPOP 的跨语言参考实现，并推动协议本身的演进。

## 特性

- 基于 tokio 的异步服务器
- 通过 `SpoaListener` trait 支持 TCP 和 Unix socket
- 可配置超时、连接上限和帧大小（`ServerConfig`）
- Builder 模式构建服务器
- 运行时热替换消息处理器
- 优雅关闭

## 快速开始

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

    // 使用 builder 模式
    spoa::Server::new(listener, holder)
        .config(ServerConfig {
            max_frame_size: 16_384,
            ..Default::default()
        })
        .run(tokio::signal::ctrl_c())
        .await;
}
```

## 配置

`ServerConfig` 提供以下可调参数：

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `read_timeout` | 30s | 单连接读超时 |
| `write_timeout` | 30s | 单连接写超时 |
| `max_connections` | 100,000 | 最大并发连接数 |
| `max_frame_size` | 16,384 | SPOP 帧最大字节数 |

## 协议

基于 [HAProxy SPOE 规范](https://github.com/haproxy/haproxy/blob/master/doc/SPOE.txt) 实现 SPOP 协议。

支持的帧类型：
- HAPROXY-HELLO / AGENT-HELLO（握手）
- NOTIFY / ACK（消息处理）
- HAPROXY-DISCONNECT / AGENT-DISCONNECT（连接关闭）
- 健康检查检测

## 许可证

MIT
