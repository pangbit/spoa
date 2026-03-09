# SPOP 协议 Bug 修复 — 设计文档

日期：2026-03-09
状态：已批准

## 背景

spoa v0.3.0 的 SPOP 协议实现存在三个已知 bug，需要在后续功能扩展（分片重组等）之前修复。

## 修复项

### 1. Int32 序列化 sign-extend bug

**文件：** `src/protocol/types.rs:78`

**问题：** `*val as u64` 对负数进行 sign-extend，`-1i32` 变为 `0xFFFFFFFFFFFFFFFF`，varint 编码需要 10 字节。HAProxy 期望的是 bit-pattern preserve：`-1i32 as u32 as u64` = `0xFFFFFFFF`，varint 仅需 5 字节。

**修复：**
```rust
// 修复前
Self::Int32(val) => buf.extend(encode_varint(*val as u64));
// 修复后
Self::Int32(val) => buf.extend(encode_varint(*val as u32 as u64));
```

Int64 不受影响（i64 → u64 位宽相同，bit pattern 不变）。

### 2. BOOL 编码可读性

**文件：** `src/protocol/types.rs:73`

**问题：** 当前代码依赖隐式运算符优先级，逻辑正确但难以阅读。

**修复：**
```rust
// 修复前
let flags = if *val { 0x01 } else { 0x00 } << 4;
// 修复后
let flags = (if *val { 0x01u8 } else { 0x00u8 }) << 4;
```

### 3. 多消息解析

**文件：** `src/protocol/parser.rs:266-293`

**问题：** `parse_list_of_messages` 只解析一条消息就返回 `vec![msg]`。SPOP 规范的 NOTIFY 帧可以携带多条消息（LIST-OF-MESSAGES）。

**修复：** 拆分为 `parse_single_message` + `many0` 循环：
```rust
fn parse_list_of_messages(input: &[u8]) -> IResult<&[u8], Vec<Message>> {
    all_consuming(many0(complete(parse_single_message))).parse(input)
}

fn parse_single_message(input: &[u8]) -> IResult<&[u8], Message> {
    // 解析 message name + nb_args + kv pairs
}
```

## 测试

- Int32 负数 round-trip 测试（序列化 → 反序列化）
- 多消息 NOTIFY payload 解析测试
- `cargo test` 全量通过，无回归

## 版本

修复后发布 v0.3.1（无 API breaking change）。
