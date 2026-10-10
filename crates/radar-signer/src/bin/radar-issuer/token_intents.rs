// SPDX-License-Identifier: Apache-2.0
//! Signed top-level instruction data, never execution or account ownership.

use radar_signer::tx::Message;
use radar_types::Address;
use serde_json::{Value, json};

pub(super) fn review(message: &Message) -> Vec<Value> {
    let token: Address = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .expect("fixed SPL Token program");
    message.instructions.iter().enumerate().map(|(index, instruction)| {
        let accounts = &instruction.accounts;
        let data = &instruction.data;
        // Exact canonical single-authority TransferChecked only. No CPI or
        // Token-2022 extension semantics are inferred from this instruction.
        if instruction.program_id == token.to_bytes()
            && accounts.len() == 4
            && data.len() == 10
            && data[0] == 12
            && message.accounts[..usize::from(message.required_signatures)].contains(&accounts[3])
        {
            let amount = u64::from_le_bytes(data[1..9].try_into().expect("checked extent"));
            json!({"instruction_index":index,"kind":"spl_token_transfer_checked_intent",
                "program":token,"source_account":Address::new(accounts[0]),"mint":Address::new(accounts[1]),
                "destination_account":Address::new(accounts[2]),"authority":Address::new(accounts[3]),
                "requested_raw_amount":amount.to_string(),"requested_decimals":data[9],
                "execution_effects_verified":false,"account_ownership_verified":false})
        } else {
            json!({"instruction_index":index,"kind":"unresolved","program":Address::new(instruction.program_id),
                "execution_effects_verified":false,"account_ownership_verified":false})
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_signer::tx::Instruction;

    fn message() -> Message {
        let program: Address = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let mut data = vec![12];
        data.extend(0x0102_0304_0506_0708_u64.to_le_bytes());
        data.push(9);
        Message {
            required_signatures: 1,
            accounts: vec![[1; 32], [2; 32], [3; 32], [4; 32], program.to_bytes()],
            recent_blockhash: [0; 32],
            versioned: false,
            message_offset: 65,
            instructions: vec![Instruction {
                program_id: program.to_bytes(),
                accounts: vec![[2; 32], [3; 32], [4; 32], [1; 32]],
                data,
            }],
        }
    }

    #[test]
    fn checked_transfer_intent_preserves_exact_order_units_and_unknowns() {
        let mut message = message();
        let report = review(&message);
        let row = &report[0];
        assert_eq!(row["kind"], "spl_token_transfer_checked_intent");
        for (field, seed) in [
            ("source_account", 2),
            ("mint", 3),
            ("destination_account", 4),
            ("authority", 1),
        ] {
            assert_eq!(row[field], json!(Address::new([seed; 32])));
        }
        assert_eq!(row["requested_raw_amount"], "72623859790382856");
        assert_eq!(row["requested_decimals"], 9);
        assert_eq!(row["execution_effects_verified"], false);
        assert_eq!(row["account_ownership_verified"], false);
        message.instructions[0].data[1..9].copy_from_slice(&u64::MAX.to_le_bytes());
        message.instructions[0].data[9] = 0;
        assert_eq!(
            review(&message)[0]["requested_raw_amount"],
            u64::MAX.to_string()
        );
        assert_eq!(review(&message)[0]["requested_decimals"], 0);
        message.instructions.push(message.instructions[0].clone());
        message.instructions.push(Instruction {
            program_id: [8; 32],
            accounts: vec![],
            data: vec![],
        });
        let mixed = review(&message);
        assert_eq!(mixed.len(), 3);
        assert_eq!(mixed[1]["instruction_index"], 1);
        assert_eq!(mixed[1]["kind"], "spl_token_transfer_checked_intent");
        assert_eq!(mixed[2]["instruction_index"], 2);
        assert_eq!(mixed[2]["kind"], "unresolved");
        assert!(mixed[2].get("requested_raw_amount").is_none());
        assert_eq!(mixed[2]["program"], json!(Address::new([8; 32])));
    }

    #[test]
    fn unsupported_program_extent_opcode_and_authority_remain_unresolved() {
        let original = message();
        for case in 0..7 {
            let mut message = original.clone();
            let instruction = &mut message.instructions[0];
            match case {
                0 => {
                    instruction.program_id = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
                        .parse::<Address>()
                        .unwrap()
                        .to_bytes();
                }
                1 => {
                    instruction.accounts.pop();
                }
                2 => instruction.accounts.push([5; 32]),
                3 => {
                    instruction.data.pop();
                }
                4 => instruction.data.push(0),
                5 => instruction.data[0] = 13,
                _ => instruction.accounts[3] = [4; 32],
            }
            let row = &review(&message)[0];
            assert_eq!(row["kind"], "unresolved", "{case}");
            assert!(row.get("requested_raw_amount").is_none());
        }
        assert_eq!(
            review(&Message {
                instructions: vec![],
                ..original
            }),
            Vec::<Value>::new()
        );
    }
}
