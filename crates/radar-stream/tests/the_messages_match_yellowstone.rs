// SPDX-License-Identifier: Apache-2.0
//! Every field number in `radar_stream::proto`, held to the vendored `.proto`.
//!
//! The messages are written by hand, and a wrong field number is silent: the
//! decoder skips the field as unknown, and a transaction arrives with no
//! balances. So these tests never type a field number. They read each number
//! out of `proto/geyser.proto` and `proto/solana-storage.proto` by field name,
//! write bytes with it, and check the hand-written types read them back; and
//! the other way, encode with the hand-written types and check each number
//! that comes out is the one the `.proto` gives that field's name.

use std::collections::HashMap;

use prost::Message as _;
use radar_stream::proto;

/// What kind of block a line sits in.
enum Block {
    Message(String),
    /// A oneof's fields are its message's fields.
    Oneof,
    /// An enum's `NAME = 0;` lines are not fields.
    Enum,
}

/// `message -> field -> number`, from both vendored files.
fn numbers() -> HashMap<String, HashMap<String, u32>> {
    let mut out: HashMap<String, HashMap<String, u32>> = HashMap::new();
    for file in ["geyser.proto", "solana-storage.proto"] {
        let path = format!("{}/proto/{file}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(path).expect("the vendored proto");
        let mut stack: Vec<Block> = Vec::new();
        for raw in text.lines() {
            let line = raw.split("//").next().unwrap_or_default().trim();
            if let Some(rest) = line.strip_prefix("message ") {
                stack.push(Block::Message(rest.trim_end_matches('{').trim().to_owned()));
                if line.ends_with('}') {
                    stack.pop(); // `message Empty {}` on one line
                }
                continue;
            }
            if line.starts_with("oneof ") {
                stack.push(Block::Oneof);
                continue;
            }
            if line.starts_with("enum ") {
                stack.push(Block::Enum);
                continue;
            }
            if line.starts_with('}') {
                stack.pop();
                continue;
            }
            if matches!(stack.last(), None | Some(Block::Enum)) {
                continue;
            }
            let Some((left, right)) = line.split_once('=') else {
                continue;
            };
            let Ok(number) = right.trim().trim_end_matches(';').trim().parse::<u32>() else {
                continue;
            };
            let Some(name) = left.split_whitespace().last() else {
                continue;
            };
            let Some(message) = stack.iter().rev().find_map(|b| match b {
                Block::Message(m) => Some(m.clone()),
                _ => None,
            }) else {
                continue;
            };
            out.entry(message)
                .or_default()
                .insert(name.to_owned(), number);
        }
    }
    out
}

struct Numbers(HashMap<String, HashMap<String, u32>>);

impl Numbers {
    fn of(&self, message: &str, field: &str) -> u32 {
        *self
            .0
            .get(message)
            .unwrap_or_else(|| panic!("no message {message} in the vendored proto"))
            .get(field)
            .unwrap_or_else(|| panic!("no field {message}.{field} in the vendored proto"))
    }
}

// A protobuf writer small enough to read, so these bytes owe nothing to prost.

fn varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let byte = u8::try_from(v & 0x7f).unwrap();
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn int(number: u32, v: u64, out: &mut Vec<u8>) {
    varint(u64::from(number) << 3, out);
    varint(v, out);
}

fn bytes(number: u32, payload: &[u8], out: &mut Vec<u8>) {
    varint((u64::from(number) << 3) | 2, out);
    varint(payload.len() as u64, out);
    out.extend_from_slice(payload);
}

/// Top-level `(number, wire type, varint or bytes)` of an encoded message.
fn fields(mut buf: &[u8]) -> Vec<(u32, u8, u64, Vec<u8>)> {
    fn read_varint(buf: &mut &[u8]) -> u64 {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let b = buf[0];
            *buf = &buf[1..];
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return v;
            }
            shift += 7;
        }
    }
    let mut out = Vec::new();
    while !buf.is_empty() {
        let key = read_varint(&mut buf);
        let number = u32::try_from(key >> 3).unwrap();
        let wire = u8::try_from(key & 7).unwrap();
        match wire {
            0 => out.push((number, 0, read_varint(&mut buf), Vec::new())),
            2 => {
                let len = usize::try_from(read_varint(&mut buf)).unwrap();
                out.push((number, 2, 0, buf[..len].to_vec()));
                buf = &buf[len..];
            }
            other => panic!("unexpected wire type {other}"),
        }
    }
    out
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one message tree written field by field; split up, a failure stops saying which field"
)]
fn a_transaction_update_written_from_the_proto_reads_back_whole() {
    let n = Numbers(numbers());
    let key_a = [1u8; 32];
    let key_b = [2u8; 32];
    let loaded_w = [3u8; 32];
    let loaded_r = [4u8; 32];
    let signature = [9u8; 64];

    let mut ui = Vec::new();
    int(n.of("UiTokenAmount", "decimals"), 6, &mut ui);
    bytes(n.of("UiTokenAmount", "amount"), b"123456", &mut ui);
    let mut balance = Vec::new();
    int(n.of("TokenBalance", "account_index"), 1, &mut balance);
    bytes(
        n.of("TokenBalance", "mint"),
        b"So11111111111111111111111111111111111111112",
        &mut balance,
    );
    bytes(n.of("TokenBalance", "ui_token_amount"), &ui, &mut balance);
    bytes(
        n.of("TokenBalance", "owner"),
        b"11111111111111111111111111111111",
        &mut balance,
    );

    let mut inner_ix = Vec::new();
    int(
        n.of("InnerInstruction", "program_id_index"),
        1,
        &mut inner_ix,
    );
    bytes(n.of("InnerInstruction", "accounts"), &[0, 2], &mut inner_ix);
    bytes(n.of("InnerInstruction", "data"), &[7, 7], &mut inner_ix);
    let mut inner = Vec::new();
    int(n.of("InnerInstructions", "index"), 0, &mut inner);
    bytes(
        n.of("InnerInstructions", "instructions"),
        &inner_ix,
        &mut inner,
    );

    let mut err = Vec::new();
    bytes(n.of("TransactionError", "err"), &[5], &mut err);

    let mut meta = Vec::new();
    bytes(n.of("TransactionStatusMeta", "err"), &err, &mut meta);
    int(n.of("TransactionStatusMeta", "fee"), 5_000, &mut meta);
    for v in [10u64, 20, 30, 40] {
        int(n.of("TransactionStatusMeta", "pre_balances"), v, &mut meta);
    }
    for v in [11u64, 21, 31, 41] {
        int(n.of("TransactionStatusMeta", "post_balances"), v, &mut meta);
    }
    bytes(
        n.of("TransactionStatusMeta", "inner_instructions"),
        &inner,
        &mut meta,
    );
    bytes(
        n.of("TransactionStatusMeta", "pre_token_balances"),
        &balance,
        &mut meta,
    );
    bytes(
        n.of("TransactionStatusMeta", "post_token_balances"),
        &balance,
        &mut meta,
    );
    bytes(
        n.of("TransactionStatusMeta", "loaded_writable_addresses"),
        &loaded_w,
        &mut meta,
    );
    bytes(
        n.of("TransactionStatusMeta", "loaded_readonly_addresses"),
        &loaded_r,
        &mut meta,
    );

    let mut ix = Vec::new();
    int(n.of("CompiledInstruction", "program_id_index"), 1, &mut ix);
    bytes(n.of("CompiledInstruction", "accounts"), &[0], &mut ix);
    bytes(n.of("CompiledInstruction", "data"), &[1, 2, 3], &mut ix);
    let mut message = Vec::new();
    bytes(n.of("Message", "account_keys"), &key_a, &mut message);
    bytes(n.of("Message", "account_keys"), &key_b, &mut message);
    bytes(n.of("Message", "instructions"), &ix, &mut message);
    let mut transaction = Vec::new();
    bytes(
        n.of("Transaction", "signatures"),
        &signature,
        &mut transaction,
    );
    bytes(n.of("Transaction", "message"), &message, &mut transaction);

    let mut info = Vec::new();
    bytes(
        n.of("SubscribeUpdateTransactionInfo", "signature"),
        &signature,
        &mut info,
    );
    int(
        n.of("SubscribeUpdateTransactionInfo", "is_vote"),
        1,
        &mut info,
    );
    bytes(
        n.of("SubscribeUpdateTransactionInfo", "transaction"),
        &transaction,
        &mut info,
    );
    bytes(
        n.of("SubscribeUpdateTransactionInfo", "meta"),
        &meta,
        &mut info,
    );
    int(
        n.of("SubscribeUpdateTransactionInfo", "index"),
        42,
        &mut info,
    );
    let mut tx_update = Vec::new();
    bytes(
        n.of("SubscribeUpdateTransaction", "transaction"),
        &info,
        &mut tx_update,
    );
    int(
        n.of("SubscribeUpdateTransaction", "slot"),
        777,
        &mut tx_update,
    );

    let mut update = Vec::new();
    bytes(n.of("SubscribeUpdate", "filters"), b"radar", &mut update);
    bytes(
        n.of("SubscribeUpdate", "transaction"),
        &tx_update,
        &mut update,
    );

    let read = proto::SubscribeUpdate::decode(update.as_slice()).expect("decodes");
    assert_eq!(read.filters, vec!["radar"]);
    let Some(proto::UpdateOneof::Transaction(t)) = read.update_oneof else {
        panic!("the transaction variant, got {:?}", read.update_oneof);
    };
    assert_eq!(t.slot, 777);
    let info = t.transaction.unwrap();
    assert_eq!(
        (info.signature.as_slice(), info.is_vote, info.index),
        (&signature[..], true, 42)
    );
    let transaction = info.transaction.unwrap();
    assert_eq!(transaction.signatures, vec![signature.to_vec()]);
    let message = transaction.message.unwrap();
    assert_eq!(message.account_keys, vec![key_a.to_vec(), key_b.to_vec()]);
    assert_eq!(
        message.instructions,
        vec![proto::CompiledInstruction {
            program_id_index: 1,
            accounts: vec![0],
            data: vec![1, 2, 3]
        }]
    );
    let meta = info.meta.unwrap();
    assert_eq!(meta.err, Some(proto::TransactionError { err: vec![5] }));
    assert_eq!(meta.fee, 5_000);
    assert_eq!(meta.pre_balances, vec![10, 20, 30, 40]);
    assert_eq!(meta.post_balances, vec![11, 21, 31, 41]);
    assert_eq!(
        meta.inner_instructions[0].instructions[0].accounts,
        vec![0, 2]
    );
    assert_eq!(meta.inner_instructions[0].instructions[0].data, vec![7, 7]);
    let expected_balance = proto::TokenBalance {
        account_index: 1,
        mint: "So11111111111111111111111111111111111111112".into(),
        ui_token_amount: Some(proto::UiTokenAmount {
            decimals: 6,
            amount: "123456".into(),
        }),
        owner: "11111111111111111111111111111111".into(),
    };
    assert_eq!(meta.pre_token_balances, vec![expected_balance.clone()]);
    assert_eq!(meta.post_token_balances, vec![expected_balance]);
    assert_eq!(meta.loaded_writable_addresses, vec![loaded_w.to_vec()]);
    assert_eq!(meta.loaded_readonly_addresses, vec![loaded_r.to_vec()]);
}

#[test]
fn block_time_ping_and_pong_written_from_the_proto_read_back() {
    let n = Numbers(numbers());

    let mut time = Vec::new();
    int(n.of("UnixTimestamp", "timestamp"), 1_789_305_976, &mut time);
    let mut meta = Vec::new();
    int(
        n.of("SubscribeUpdateBlockMeta", "slot"),
        446_713_943,
        &mut meta,
    );
    bytes(
        n.of("SubscribeUpdateBlockMeta", "block_time"),
        &time,
        &mut meta,
    );
    let mut update = Vec::new();
    bytes(n.of("SubscribeUpdate", "block_meta"), &meta, &mut update);
    assert_eq!(
        proto::SubscribeUpdate::decode(update.as_slice())
            .unwrap()
            .update_oneof,
        Some(proto::UpdateOneof::BlockMeta(
            proto::SubscribeUpdateBlockMeta {
                slot: 446_713_943,
                block_time: Some(proto::UnixTimestamp {
                    timestamp: 1_789_305_976
                }),
            }
        ))
    );

    let mut update = Vec::new();
    bytes(n.of("SubscribeUpdate", "ping"), &[], &mut update);
    assert_eq!(
        proto::SubscribeUpdate::decode(update.as_slice())
            .unwrap()
            .update_oneof,
        Some(proto::UpdateOneof::Ping(proto::SubscribeUpdatePing {}))
    );

    let mut pong = Vec::new();
    int(n.of("SubscribeUpdatePong", "id"), 3, &mut pong);
    let mut update = Vec::new();
    bytes(n.of("SubscribeUpdate", "pong"), &pong, &mut update);
    assert_eq!(
        proto::SubscribeUpdate::decode(update.as_slice())
            .unwrap()
            .update_oneof,
        Some(proto::UpdateOneof::Pong(proto::SubscribeUpdatePong {
            id: 3
        }))
    );
}

#[test]
fn the_subscription_request_encodes_with_the_protos_numbers() {
    let n = Numbers(numbers());
    let config = radar_stream::feed::Config {
        endpoint: "https://grpc.example.com".into(),
        token: None,
        programs: vec!["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".into()],
    };
    let mut request = radar_stream::feed::subscribe_request(&config);
    request.ping = Some(proto::SubscribeRequestPing { id: 1 });
    let top = fields(&request.encode_to_vec());

    let numbers: Vec<u32> = top.iter().map(|f| f.0).collect();
    for field in ["transactions", "blocks_meta", "commitment", "ping"] {
        assert!(
            numbers.contains(&n.of("SubscribeRequest", field)),
            "SubscribeRequest.{field} missing from {numbers:?}"
        );
    }
    let commitment = top
        .iter()
        .find(|f| f.0 == n.of("SubscribeRequest", "commitment"))
        .unwrap();
    assert_eq!(commitment.2, 1, "CONFIRMED is 1 in the proto");

    // A map entry is a message of key = 1 and value = 2; the value is the filter.
    let entry = top
        .iter()
        .find(|f| f.0 == n.of("SubscribeRequest", "transactions"))
        .unwrap();
    let entry_fields = fields(&entry.3);
    assert_eq!(entry_fields[0].3, b"radar");
    let filter = fields(&entry_fields[1].3);
    let by_number = |name: &str| {
        filter
            .iter()
            .find(|f| f.0 == n.of("SubscribeRequestFilterTransactions", name))
            .unwrap_or_else(|| panic!("filter field {name} missing from {filter:?}"))
            .clone()
    };
    assert_eq!(by_number("vote").2, 0, "vote = false");
    assert_eq!(by_number("failed").2, 0, "failed = false");
    assert_eq!(
        by_number("account_include").3,
        config.programs[0].as_bytes()
    );

    let ping = top
        .iter()
        .find(|f| f.0 == n.of("SubscribeRequest", "ping"))
        .unwrap();
    assert_eq!(fields(&ping.3)[0].0, n.of("SubscribeRequestPing", "id"));
}
