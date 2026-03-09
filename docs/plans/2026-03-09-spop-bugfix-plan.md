# SPOP 协议 Bug 修复实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 修复 SPOP 协议实现中的三个 bug：Int32 sign-extend 序列化、BOOL 编码可读性、多消息解析。

**Architecture:** 所有变更集中在 `src/protocol/types.rs` 和 `src/protocol/parser.rs` 两个文件，通过已有 test 基础设施验证。每个 bug 独立修复并提交。

**Tech Stack:** Rust 2021, nom 8.0, tokio-util codec

---

## Task 1: 修复 Int32 序列化 sign-extend bug

**Files:**
- Modify: `src/protocol/types.rs:78`
- Modify: `src/protocol/types.rs:173-276` (测试模块)

**Step 1: 添加 Int32 负数 round-trip 测试**

在 `src/protocol/types.rs` 的 `test_cases()` 函数中，在现有 `Int32` 测试用例之后添加负数测试：

```rust
// Int32 negative: -1 should encode as u32 bit pattern (0xFFFFFFFF varint)
(
    "Int32 negative -1",
    {
        let mut v = vec![0x02]; // TYPE_INT32
        v.extend(encode_varint((-1i32 as u32) as u64));
        v
    },
    TypedData::Int32(-1),
),
// Int32 negative: i32::MIN
(
    "Int32 min",
    {
        let mut v = vec![0x02];
        v.extend(encode_varint((i32::MIN as u32) as u64));
        v
    },
    TypedData::Int32(i32::MIN),
),
```

注意：需要在 tests 模块顶部添加 `use super::encode_varint;`。

**Step 2: 运行测试，确认失败**

Run: `cargo test --lib protocol::types::tests::test_to_bytes -- --nocapture 2>&1`
Expected: FAIL — `Int32(-1)` 的 `to_bytes` 输出与期望不匹配（sign-extend 产生更长的 varint）

**Step 3: 修复 Int32 序列化**

将 `src/protocol/types.rs:78` 从：
```rust
Self::Int32(val) => {
    buf.push(TYPE_INT32);
    buf.extend(encode_varint(*val as u64));
}
```
改为：
```rust
Self::Int32(val) => {
    buf.push(TYPE_INT32);
    buf.extend(encode_varint(*val as u32 as u64));
}
```

**Step 4: 运行测试，确认通过**

Run: `cargo test --lib protocol::types -- --nocapture 2>&1`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add src/protocol/types.rs
git commit -m "fix: Int32 serialization sign-extend bug causing varint bloat for negative values"
```

---

## Task 2: 改善 BOOL 编码可读性

**Files:**
- Modify: `src/protocol/types.rs:73`

**Step 1: 添加括号明确运算优先级**

将 `src/protocol/types.rs:73` 从：
```rust
let flags = if *val { 0x01 } else { 0x00 } << 4;
```
改为：
```rust
let flags = (if *val { 0x01u8 } else { 0x00u8 }) << 4;
```

**Step 2: 运行测试确认无回归**

Run: `cargo test --lib protocol::types -- --nocapture 2>&1`
Expected: ALL PASS（行为不变，仅可读性改善）

**Step 3: Commit**

```bash
git add src/protocol/types.rs
git commit -m "style: clarify BOOL encoding operator precedence with explicit parentheses"
```

---

## Task 3: 修复多消息解析

**Files:**
- Modify: `src/protocol/parser.rs:266-293`
- Modify: `src/protocol/parser.rs:335` (测试模块)

**Step 1: 添加多消息 round-trip 测试**

在 `src/protocol/parser.rs` 的 `#[cfg(test)] mod tests` 中添加：

```rust
#[test]
fn test_parse_notify_multiple_messages() {
    use crate::protocol::frames::notify::NotifyFrame;
    use crate::protocol::frame::{FrameFlags, Metadata};

    // Build a NotifyFrame with 2 messages
    let messages = vec![
        Message {
            name: "msg1".to_string(),
            args: {
                let mut m = HashMap::new();
                m.insert("key1".to_string(), TypedData::String("val1".to_string()));
                m
            },
        },
        Message {
            name: "msg2".to_string(),
            args: {
                let mut m = HashMap::new();
                m.insert("key2".to_string(), TypedData::UInt32(42));
                m
            },
        },
    ];

    let frame = NotifyFrame::new(1, 1, messages);

    // Serialize and parse back
    let serialized = frame.serialize().unwrap();
    let (_, parsed) = parse_frame(&serialized).unwrap();

    assert_eq!(parsed.frame_type(), &FrameType::Notify);
    match parsed.payload() {
        FramePayload::ListOfMessages(msgs) => {
            assert_eq!(msgs.len(), 2, "Expected 2 messages, got {}", msgs.len());
            assert_eq!(msgs[0].name, "msg1");
            assert_eq!(msgs[1].name, "msg2");
            assert_eq!(
                msgs[0].args.get("key1"),
                Some(&TypedData::String("val1".to_string()))
            );
            assert_eq!(
                msgs[1].args.get("key2"),
                Some(&TypedData::UInt32(42))
            );
        }
        _ => panic!("Expected ListOfMessages payload"),
    }
}
```

**Step 2: 运行测试，确认失败**

Run: `cargo test --lib protocol::parser::tests::test_parse_notify_multiple_messages -- --nocapture 2>&1`
Expected: FAIL — 当前 `all_consuming` 内层 parser 在第一条消息后遇到第二条消息的数据会报错

**Step 3: 重构 parse_list_of_messages**

将 `src/protocol/parser.rs:262-293` 替换为：

```rust
/// Parse entire list of messages payload
///
/// LIST-OF-MESSAGES : [ <MESSAGE-NAME> <NB-ARGS:1 byte> <KV-LIST> ... ]
/// MESSAGE-NAME     : <STRING>
fn parse_list_of_messages(input: &[u8]) -> IResult<&[u8], Vec<Message>> {
    all_consuming(many0(complete(parse_single_message))).parse(input)
}

/// Parse a single message: name + nb_args + KV pairs
fn parse_single_message(input: &[u8]) -> IResult<&[u8], Message> {
    let (remaining, name) = parse_string(input)?;

    let (remaining, nb_args) = be_u8(remaining)?;
    let nb_args = nb_args as usize;

    let (remaining, kv_list) = many_m_n(nb_args, nb_args, parse_key_value_pair)(remaining)?;

    let mut args = HashMap::new();
    for (key, value) in kv_list {
        if args.contains_key(&key) {
            return Err(nom::Err::Failure(Error::new(input, ErrorKind::Tag)));
        }
        args.insert(key, value);
    }

    Ok((remaining, Message { name, args }))
}
```

关键变更：
- 拆分为 `parse_list_of_messages` + `parse_single_message`
- 移除内层 `all_consuming`，改为外层 `all_consuming(many0(...))`
- 用 `be_u8` 替代 `take(1usize)` 读 nb_args，更直接

**Step 4: 运行测试，确认全部通过**

Run: `cargo test --lib protocol::parser -- --nocapture 2>&1`
Expected: ALL PASS（包括新测试和原有 `test_parse_haproxy_hello`）

**Step 5: Commit**

```bash
git add src/protocol/parser.rs
git commit -m "fix: parse_list_of_messages now handles multiple messages in NOTIFY frames"
```

---

## Task 4: 最终验证

**Step 1: 全量测试**

Run: `cargo test 2>&1`
Expected: 所有测试通过（包括 tests/ 目录下的集成测试）

**Step 2: Clippy 检查**

Run: `cargo clippy -- -D warnings 2>&1`
Expected: 零警告

**Step 3: 版本号更新**

将 `Cargo.toml:3` 的 `version` 从 `"0.3.0"` 改为 `"0.3.1"`。

**Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to 0.3.1 for protocol bug fixes"
```

---

## 任务依赖

```
Task 1 (Int32 fix) ──┐
Task 2 (BOOL style) ─┼─→ Task 4 (最终验证)
Task 3 (多消息 fix) ──┘
```

Task 1-3 互不依赖，可并行执行。Task 4 在全部完成后执行。
