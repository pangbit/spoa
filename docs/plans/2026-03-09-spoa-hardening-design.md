# SPOA 打磨内核 — 设计文档

日期：2026-03-09
状态：已批准

## 背景

spoa v0.2.0 刚完成 spop 协议库合并，项目尚未正式投产。趁此窗口期打磨库的成熟度，为后续投产和开源做准备。

## 总体策略

分三个阶段推进：**可配置性+健壮性 → 文档 → 功能扩展**。

---

## 阶段 1：可配置性 + 健壮性（优先）

### 1.1 ServerConfig + Builder 模式

引入 `ServerConfig` 结构体，将硬编码参数提取为可配置项：

```rust
pub struct ServerConfig {
    pub read_timeout: Duration,      // 默认 30s
    pub write_timeout: Duration,     // 默认 30s
    pub max_connections: usize,      // 默认 100_000
    pub max_frame_size: usize,       // 默认 16_384
}

impl Default for ServerConfig { /* 当前硬编码值 */ }
```

重构 `server::run()` 为 builder 模式：

```rust
Server::new(listener, processer)
    .config(ServerConfig::default())
    .shutdown(signal)
    .run()
    .await;
```

理由：参数可选、后续扩展不破坏已有调用、符合 Rust 生态习惯。

### 1.2 Error 类型细化

从 5 个变体扩展到 ~10 个：

```rust
pub enum Error {
    // I/O
    IO(io::Error),
    ReadTimeout,
    WriteTimeout,

    // 协议层（新增）
    InvalidFrameType(u8),
    FrameParseFailed(String),
    InvalidPayload(String),
    FrameTooLarge { size: usize, max: usize },

    // 握手层（重命名）
    HandshakeFailed(String),        // 原 InvalidHaproxyHello
    UnsupportedVersion(String),     // 原 InvalidSPOPVersion

    // 处理层（新增）
    ProcesserError(String),
}
```

要点：
- 协议解析错误从 IO 中分离
- 保持 thiserror derive
- 旧变体语义化重命名

### 1.3 测试增强

**协议层单元测试：**
- 畸形帧解析（截断数据、非法帧类型、错误 payload 格式）
- FrameTooLarge — 超过 max_frame_size 的帧
- varint 边界值（u64::MAX、溢出场景）

**集成测试：**
- 超时测试 — 客户端连接后不发数据，验证 read_timeout 生效
- 并发连接测试 — 多连接同时握手 + 消息处理
- 连接数上限测试 — 达到 max_connections 后新连接的行为
- 异常断连 — 客户端在握手/消息中途断开

### 1.4 代码清理

- 清理 examples/server.rs 中注释掉的调试代码
- examples/client.rs 中硬编码 IP 改为命令行参数或 localhost 默认值
- 确保 clippy 零警告

---

## 阶段 2：文档 + API doc

- **README.md** 重写：项目简介、特性列表、快速上手、配置说明
- **模块级 `//!` 文档**：lib.rs、protocol/mod.rs、server.rs 顶部加模块概述
- **关键 trait 文档**：IProcesser、SpoaListener 加用法示例
- **cargo doc 零警告**

---

## 阶段 3：功能扩展

- **连接指标**：活跃连接数、总处理消息数、错误计数（通过回调或 metrics trait 暴露）
- **更丰富的 examples**：最小化 server、带日志配置的 server、Unix socket server
- **按需评估**：健康检查接口、连接空闲超时等，视实际使用需求决定

---

## 版本规划

- 阶段 1 完成后发布 v0.3.0（含 breaking change：API 重构）
- 阶段 2 完成后发布 v0.3.1（文档补全，无 API 变更）
- 阶段 3 视具体功能决定版本号
