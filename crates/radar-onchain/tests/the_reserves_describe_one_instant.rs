// SPDX-License-Identifier: Apache-2.0
//! A pool and both its vaults come back at one slot, or they do not come back.
//!
//! # What this is really testing
//!
//! Not that the bytes parse -- `radar-pumpfun`'s
//! `the_vaults_are_what_mainnet_holds` does that against the same capture. This
//! is about **when** the three numbers were true. A base balance from one slot
//! against a quote balance from another is a ratio that never existed, and
//! nothing downstream of it could tell, because every individual figure is real
//! and checks out on an explorer.
//!
//! The vault addresses live inside the pool account, so the pool has to be read
//! once to find out what to ask for. That first read is discovery and its values
//! are thrown away; the pool that reaches the answer comes out of the same
//! `getMultipleAccounts` call as the balances. The obvious shortcut -- keep the
//! pool you already parsed -- is the bug this file exists to catch, so the two
//! responses below deliberately disagree about `lp_supply` and about the slot.
//!
//! # Where the bytes come from
//!
//! `radar-pumpfun`'s `pumpswap_reserves.json`, read across the crate boundary
//! rather than copied. A second copy would drift, and the point of the fixture
//! is that all five accounts in one entry were captured in a single call.

use std::str::FromStr as _;
use std::sync::Mutex;
use std::time::Duration;

use radar_onchain::budget::Budget;
use radar_onchain::reserves::{self, Role, Unreadable};
use radar_onchain::rpc::{MultiAccountRead, OwnedAccount, RpcClient, Transport};
use radar_pumpfun::token::TokenMalformed;
use radar_types::{Address, Asset, Slot, b64};

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../radar-pumpfun/tests/fixtures/pumpswap_reserves.json"
));

/// Token-2022 base against a classic SPL wrapped-SOL quote.
const MIXED_PROGRAMS: &str = "C4mLt6fs2dL2W1oovZAT9QpM3tpL6CA7DZ8hqHU9Ldqb";
/// Quotes in USDC, six decimals against wrapped SOL's nine.
const USDC_QUOTED: &str = "82zcJ16FYLuqbjxbdHKbD3F7YigdhBe6YHTTvsErNHB";

/// Where `lp_supply` sits in a `Pool` account, from research 0033's field order.
const LP_SUPPLY_AT: usize = 203;

/// Answers each call with the next canned response, and refuses a call the test
/// did not plan for.
struct Canned(Mutex<Vec<String>>);

impl Transport for Canned {
    fn post(&self, _: &str, _: String) -> Result<String, String> {
        self.0
            .lock()
            .map_err(|_| "poisoned".to_owned())?
            .pop()
            .ok_or_else(|| "the client asked for more than the test supplied".to_owned())
    }
}

fn client(responses: &[String]) -> RpcClient {
    RpcClient::with_transport(
        "http://test.invalid",
        Box::new(Canned(Mutex::new(
            responses.iter().rev().cloned().collect(),
        ))),
    )
}

fn budget() -> Budget {
    Budget::new(60, 3, Duration::from_secs(30))
}

fn address(text: &str) -> Address {
    Address::from_str(text).expect("the fixture carries base58 addresses")
}

/// One captured account: its address, its owning program, and its bytes.
#[derive(Clone)]
struct Captured {
    address: String,
    owner: String,
    data: Vec<u8>,
}

/// The five accounts of one captured read, in `Role::ORDER`.
fn captured(pool: &str) -> Vec<Captured> {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("valid JSON");
    let read = fixture["reads"]
        .as_array()
        .expect("reads")
        .iter()
        .find(|read| read["pool"].as_str() == Some(pool))
        .unwrap_or_else(|| panic!("{pool} is in the fixture"))
        .clone();
    let accounts = read["accounts"].as_array().expect("accounts").clone();

    // Ordered by role rather than by however the capture happened to store them,
    // because the request order is what the reader maps the response onto.
    Role::ORDER
        .iter()
        .map(|role| {
            let want = match role {
                Role::Pool => "pool",
                Role::BaseMint => "base_mint",
                Role::QuoteMint => "quote_mint",
                Role::BaseVault => "base_vault",
                Role::QuoteVault => "quote_vault",
            };
            let found = accounts
                .iter()
                .find(|a| a["role"].as_str() == Some(want))
                .unwrap_or_else(|| panic!("{pool} has a {want}"));
            Captured {
                address: found["address"].as_str().expect("an address").to_owned(),
                owner: found["owner"].as_str().expect("an owner").to_owned(),
                data: b64::decode(found["data_b64"].as_str().expect("base64")).expect("base64"),
            }
        })
        .collect()
}

fn account_json(account: &Captured) -> String {
    format!(
        r#"{{"data":["{}","base64"],"owner":"{}"}}"#,
        b64::encode(&account.data),
        account.owner,
    )
}

/// A `getAccountInfo` response for one account at one slot.
fn single(account: &Captured, slot: u64) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":{slot}}},"value":{}}}}}"#,
        account_json(account),
    )
}

/// A `getMultipleAccounts` response: several accounts, **one** slot.
fn multiple(accounts: &[Captured], slot: u64) -> String {
    let list: Vec<String> = accounts.iter().map(account_json).collect();
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":{slot}}},"value":[{}]}}}}"#,
        list.join(","),
    )
}

/// The same pool account with a different `lp_supply`, so the two reads of it
/// are distinguishable.
fn with_lp_supply(account: &Captured, supply: u64) -> Captured {
    let mut data = account.data.clone();
    data[LP_SUPPLY_AT..LP_SUPPLY_AT + 8].copy_from_slice(&supply.to_le_bytes());
    Captured {
        data,
        ..account.clone()
    }
}

#[test]
fn the_pool_and_its_vaults_come_from_one_slot() {
    // The discovery read and the atomic read are made to disagree about two
    // things: the slot, and `lp_supply`. Both come from the *pool* account, so
    // if the answer carries 100 or 111 the pool in it was the one read on its
    // own, a slot before the balances beside it.
    let five = captured(MIXED_PROGRAMS);
    let discovery = with_lp_supply(&five[0], 111);
    let atomic: Vec<Captured> = std::iter::once(with_lp_supply(&five[0], 222))
        .chain(five[1..].iter().cloned())
        .collect();

    let client = client(&[single(&discovery, 100), multiple(&atomic, 200)]);
    let read = reserves::read(&client, &mut budget(), &address(MIXED_PROGRAMS)).expect("reserves");

    assert_eq!(read.slot, Slot(200), "the slot is the atomic read's");
    assert_eq!(
        read.pool.lp_supply, 222,
        "the pool is the one read beside the balances, not the one read to find them",
    );

    // And the balances are the captured ones, so the reserves and the pool are
    // the same five accounts rather than a mixture.
    assert_eq!(read.address, address(MIXED_PROGRAMS));
    assert_eq!(read.base.raw, 533_803_274_498_380);
    assert_eq!(read.quote.raw, 15_824_934_875);
    assert_eq!(read.base.decimals, 6);
    assert_eq!(read.quote.decimals, 9);
    assert_eq!(read.quote.asset, Asset::WrappedSol);
    assert_eq!(read.base.asset, Asset::token_2022(read.pool.base_mint));
    assert_eq!(read.base.vault, address(&five[3].address));
    assert_eq!(read.quote.vault, address(&five[4].address));
}

#[test]
fn a_node_that_will_not_say_when_it_read_is_refused() {
    // Rule 9. Three balances with no slot are not three balances at an unknown
    // instant -- they may be three instants, and the difference is invisible
    // afterwards. The refusal has to happen here because nothing downstream can
    // reconstruct what was lost.
    let five = captured(USDC_QUOTED);
    let list: Vec<String> = five.iter().map(account_json).collect();
    let no_context = format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"value":[{}]}}}}"#,
        list.join(","),
    );

    let client = client(&[single(&five[0], 100), no_context]);
    let refused =
        reserves::read(&client, &mut budget(), &address(USDC_QUOTED)).expect_err("no slot");
    assert!(matches!(refused, Unreadable::NoSlot), "{refused:?}");
}

#[test]
fn a_vault_that_vanished_between_the_two_reads_is_refused_not_read_as_empty() {
    // A vault that existed at discovery and not a moment later is a pool being
    // torn down. Reported as a zero balance it would read as a pool with no
    // liquidity, which is a sentence somebody could act on.
    let five = captured(USDC_QUOTED);
    let mut list: Vec<String> = five.iter().map(account_json).collect();
    list[4] = "null".to_owned();
    let missing = format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":200}},"value":[{}]}}}}"#,
        list.join(","),
    );

    let client = client(&[single(&five[0], 100), missing]);
    let refused =
        reserves::read(&client, &mut budget(), &address(USDC_QUOTED)).expect_err("a gone vault");
    assert!(
        matches!(
            refused,
            Unreadable::Missing {
                role: Role::QuoteVault,
                ..
            }
        ),
        "{refused:?}",
    );
}

#[test]
fn a_pool_that_names_different_vaults_by_the_second_read_is_refused() {
    // The pool re-read has to still claim the accounts that were read for it.
    // Here the atomic read returns a *different* pool -- the USDC-quoted one --
    // at the address the vaults were discovered from, so the balances belong to
    // somebody else's pool.
    let five = captured(MIXED_PROGRAMS);
    let other = captured(USDC_QUOTED);
    let swapped: Vec<Captured> = std::iter::once(Captured {
        address: five[0].address.clone(),
        owner: five[0].owner.clone(),
        data: other[0].data.clone(),
    })
    .chain(five[1..].iter().cloned())
    .collect();

    let client = client(&[single(&five[0], 100), multiple(&swapped, 200)]);
    let refused = reserves::read(&client, &mut budget(), &address(MIXED_PROGRAMS))
        .expect_err("a different pool");
    assert!(
        matches!(
            refused,
            Unreadable::PoolMoved {
                role: Role::BaseMint,
                ..
            }
        ),
        "{refused:?}",
    );
}

#[test]
fn a_pool_address_owned_by_something_else_is_refused() {
    // The owner is checked on the atomic read, not only at discovery. An account
    // that changed hands between the two is not a pool any more, and its 301
    // bytes would still parse.
    let five = captured(MIXED_PROGRAMS);
    let stolen: Vec<Captured> = std::iter::once(Captured {
        owner: five[1].owner.clone(),
        ..five[0].clone()
    })
    .chain(five[1..].iter().cloned())
    .collect();

    let client = client(&[single(&five[0], 100), multiple(&stolen, 200)]);
    let refused =
        reserves::read(&client, &mut budget(), &address(MIXED_PROGRAMS)).expect_err("not a pool");
    assert!(
        matches!(refused, Unreadable::NotAPool { .. }),
        "{refused:?}"
    );
}

#[test]
fn an_unmodelled_extension_on_a_vault_stops_the_read_and_names_itself() {
    // The refusal a Token-2022 extension produces has to survive the trip out of
    // the reader with the extension still named in it, or an operator sees "the
    // quote vault could not be read" and has nothing to act on. The specimen is
    // a mint whose only extension is a transfer fee.
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("valid JSON");
    let specimen = fixture["specimens"]
        .as_array()
        .expect("specimens")
        .iter()
        .find(|s| s["address"].as_str() == Some("CKfatsPMUf8SkiURsDXs7eK6GWb4Jsd6UDbs7twMCWxo"))
        .expect("the transfer-fee mint");

    let five = captured(MIXED_PROGRAMS);
    let fee_bearing: Vec<Captured> = five
        .iter()
        .enumerate()
        .map(|(at, account)| {
            if at == 1 {
                Captured {
                    address: account.address.clone(),
                    owner: specimen["owner"].as_str().expect("an owner").to_owned(),
                    data: b64::decode(specimen["data_b64"].as_str().expect("base64"))
                        .expect("base64"),
                }
            } else {
                account.clone()
            }
        })
        .collect();

    let client = client(&[single(&five[0], 100), multiple(&fee_bearing, 200)]);
    let refused = reserves::read(&client, &mut budget(), &address(MIXED_PROGRAMS))
        .expect_err("a transfer fee");

    let Unreadable::Token {
        role: Role::BaseMint,
        source: TokenMalformed::UnmodelledExtension { extension },
    } = refused
    else {
        panic!("{refused:?} should name the extension and the account");
    };
    assert_eq!(extension.name(), Some("TransferFeeConfig"));
}

#[test]
fn a_short_list_is_refused_rather_than_mapped_onto_the_wrong_roles() {
    // Four accounts against five requested addresses would shift every account
    // one place: the base vault's balance read with the quote mint's decimals.
    // Both the client and the reader refuse it, and this checks the reader,
    // which is the one a caller could reach with a hand-built read.
    let five = captured(USDC_QUOTED);
    let asked: [Address; 5] = std::array::from_fn(|at| address(&five[at].address));
    let short = MultiAccountRead {
        slot: Some(Slot(200)),
        accounts: five[..4]
            .iter()
            .map(|account| {
                Some(OwnedAccount {
                    data: account.data.clone(),
                    owner: Some(account.owner.clone()),
                })
            })
            .collect(),
    };

    let refused = reserves::at_one_slot(&asked, &short).expect_err("four is not five");
    assert!(
        matches!(
            refused,
            Unreadable::WrongCount {
                asked: 5,
                returned: 4
            }
        ),
        "{refused:?}",
    );
}

#[test]
fn an_account_whose_owner_the_node_withheld_is_refused() {
    // Without the owner there is no way to know whether the account is classic
    // SPL or Token-2022, so there is no way to know whether an extension could
    // be changing what the balance is worth. Rule 9: that is a refusal, not a
    // guess at the commoner of the two programs.
    let five = captured(USDC_QUOTED);
    let asked: [Address; 5] = std::array::from_fn(|at| address(&five[at].address));
    let anonymous = MultiAccountRead {
        slot: Some(Slot(200)),
        accounts: five
            .iter()
            .enumerate()
            .map(|(at, account)| {
                Some(OwnedAccount {
                    data: account.data.clone(),
                    owner: (at != 4).then(|| account.owner.clone()),
                })
            })
            .collect(),
    };

    let refused = reserves::at_one_slot(&asked, &anonymous).expect_err("no owner");
    assert!(
        matches!(
            refused,
            Unreadable::NoOwner {
                role: Role::QuoteVault,
                ..
            }
        ),
        "{refused:?}",
    );
}

#[test]
fn the_usdc_quoted_pool_reads_as_usdc_and_not_as_lamports() {
    // The pool the whole "reserves carry their mint" rule exists for. Its quote
    // is USDC at six decimals; read as lamports at nine it is a thousand times
    // too small, and the resulting price is a thousand times too large.
    let five = captured(USDC_QUOTED);
    let client = client(&[single(&five[0], 100), multiple(&five, 445_773_272)]);
    let read = reserves::read(&client, &mut budget(), &address(USDC_QUOTED)).expect("reserves");

    assert_eq!(read.quote.asset, Asset::Usdc);
    assert_eq!(read.quote.decimals, 6);
    assert_eq!(read.quote.raw, 697_738);
    assert_eq!(read.base.decimals, 6);
    assert_eq!(read.base.raw, 21_244_128_555);
    assert_eq!(
        read.slot,
        Slot(445_773_272),
        "the slot the capture recorded"
    );
}

#[test]
fn each_role_names_itself_distinctly_in_a_refusal() {
    // `Role::name` only ever reaches a person, in the message saying which of
    // five accounts could not be read. That makes it plumbing rather than
    // logic -- but plumbing with one job, and it fails at that job in exactly
    // two ways: a name that is blank, and five names that are the same.
    //
    // Both leave an operator holding "could not read the account" with no way
    // to know which account, against a read that fetches five of them in one
    // call. So the property worth pinning is not the spelling, it is that the
    // five are distinguishable and non-empty. Asserting the literals instead
    // would be a test of the words, which is the redundancy AGENTS.md refuses.
    let names: Vec<&str> = Role::ORDER.iter().map(|r| r.name()).collect();
    assert_eq!(names.len(), 5, "five accounts are read together");

    for (role, name) in Role::ORDER.iter().zip(&names) {
        assert!(!name.is_empty(), "{role:?} has no name to print");
    }

    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        names.len(),
        "two roles share a name, so a refusal cannot say which account failed: {names:?}"
    );
}
