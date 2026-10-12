// SPDX-License-Identifier: Apache-2.0
//! Paginated paper fills and cash events. Observations never release reservations.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PAPER_API: &str = "https://paper-api.alpaca.markets";
const ACTIVITIES: &str = "/v2/account/activities?direction=asc&page_size=100";
const PAGE_SIZE: usize = 100;

#[derive(Deserialize, Serialize)]
pub(super) struct Activity {
    pub(super) id: String,
    #[serde(rename = "activity_type")]
    pub(super) category: String,
    account_id: Option<String>,
    pub(super) order_id: Option<String>,
    transaction_time: Option<String>,
    date: Option<String>,
    #[serde(rename = "type")]
    pub(super) kind: Option<String>,
    pub(super) side: Option<String>,
    pub(super) symbol: Option<String>,
    pub(super) qty: Option<String>,
    pub(super) price: Option<String>,
    pub(super) cum_qty: Option<String>,
    pub(super) leaves_qty: Option<String>,
    net_amount: Option<String>,
    status: Option<String>,
}

fn encoded(token: &str) -> String {
    let mut result = String::new();
    for byte in token.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(char::from(byte));
        } else {
            // String writes are infallible. Encode bytes, including UTF-8,
            // instead of letting an activity identity add query parameters.
            write!(result, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    result
}

pub(super) fn collect(
    expected: &str,
    max_pages: u16,
    mut read: impl FnMut(&str) -> Result<String, String>,
) -> Result<Value, String> {
    if expected.trim().is_empty() || max_pages == 0 {
        return Err("paper account identity and positive page limit are required".into());
    }
    let started = super::wallet_read::now()?;
    let before = super::alpaca_reconcile::account(&read("/v2/account")?, expected)?;
    let mut activities = Vec::new();
    let mut seen = BTreeSet::new();
    let mut path = ACTIVITIES.to_owned();
    let mut exhausted = false;
    let mut pages_read = 0;
    for _ in 0..max_pages {
        let page: Vec<Activity> = serde_json::from_str(&read(&path)?)
            .map_err(|_| "invalid paper account activities response")?;
        pages_read += 1;
        if page.len() > PAGE_SIZE {
            return Err("paper activities response exceeds requested page size".into());
        }
        for event in &page {
            if event.id.trim().is_empty()
                || event.category.trim().is_empty()
                || !seen.insert(event.id.clone())
                || event.account_id.as_deref().is_some_and(|id| id != expected)
            {
                return Err(
                    "paper activities have missing, repeated or mismatched identities".into(),
                );
            }
        }
        let short = page.len() < PAGE_SIZE;
        if let Some(last) = page.last() {
            path = format!("{ACTIVITIES}&page_token={}", encoded(&last.id));
        }
        activities.extend(page);
        if short {
            exhausted = true;
            break;
        }
    }
    if !exhausted {
        return Err("paper activities reached page limit; history unresolved".into());
    }
    let after = super::alpaca_reconcile::account(&read("/v2/account")?, expected)?;
    Ok(
        json!({"version":1,"venue":"alpaca","environment":"paper","source":PAPER_API,
        "expected_account_id":expected,"read_started_unix_secs":started,
        "read_completed_unix_secs":super::wallet_read::now()?,"pages_read":pages_read,
        "pagination_exhausted":true,"selected_account_fields_unchanged":before == after,
        "account_before":before,"account_after":after,"activities":activities,
        "atomic_snapshot":false,"economic_history_complete":false,
        "fills_reconciled":false,"fees_final":false,"cash_reconciled":false,
        "claim_release_authorized":false,"execution_authority":false}),
    )
}

pub(super) fn page_limit(pages: &str) -> Result<u16, String> {
    pages
        .parse()
        .ok()
        .filter(|pages| *pages > 0)
        .ok_or_else(|| "activity page limit must be a positive u16".into())
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-activities-read",
        "--account-id",
        expected,
        "--max-pages",
        pages,
    ] = values.as_slice()
    else {
        return Err("usage: radar alpaca-activities-read --account-id <paper-id> --max-pages <positive-u16>".into());
    };
    let max_pages = page_limit(pages)?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = collect(expected, max_pages, |path| {
        super::alpaca_read::get(&agent, &format!("{PAPER_API}{path}"), &key, &secret)
    })?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> String {
        json!({"id":"paper-id","status":"ACTIVE","cash":"50","private":"omit"}).to_string()
    }

    fn events(count: usize) -> Value {
        json!(
            (0..count)
                .map(
                    |id| json!({"id":format!("{id}::activity"),"activity_type":"FILL",
            "order_id":"broker-id","transaction_time":"2026-10-21T15:00:00Z","type":"partial_fill",
            "side":"buy","symbol":"SPY","qty":"0.000000001","price":"100.000001",
            "cum_qty":"0.000000002","leaves_qty":"0.000000003","private":"omit"})
                )
                .collect::<Vec<_>>()
        )
    }

    fn flags(report: &Value) {
        for flag in [
            "atomic_snapshot",
            "economic_history_complete",
            "fills_reconciled",
            "fees_final",
            "cash_reconciled",
            "claim_release_authorized",
            "execution_authority",
        ] {
            assert_eq!(report[flag], false, "{flag}");
        }
    }

    #[test]
    fn pagination_uses_encoded_last_identity_and_preserves_fills_and_cash_without_finality() {
        let token = "99::a&x=1/?#%é";
        assert_eq!(encoded(token), "99%3A%3Aa%26x%3D1%2F%3F%23%25%C3%A9");
        assert_eq!(encoded("ABC-_.~09"), "ABC-_.~09");
        let mut paths = Vec::new();
        let report = collect("paper-id", 2, |path| {
            paths.push(path.to_owned());
            Ok(if path == "/v2/account" {
                account()
            } else if path == ACTIVITIES {
                let mut page = events(100);
                page[99]["id"] = json!(token);
                page.to_string()
            } else {
                json!([{"id":"fee-1","activity_type":"FEE","net_amount":"-0.000001",
                    "date":"2026-10-22","status":"executed","account_id":"paper-id"},
                    {"id":"cash-1","activity_type":"CSD","net_amount":"50.00"},
                    {"id":"unknown-1","activity_type":"FUTURE_TYPE"}])
                .to_string()
            })
        })
        .unwrap();
        assert_eq!(
            paths,
            [
                "/v2/account".to_owned(),
                ACTIVITIES.to_owned(),
                format!("{ACTIVITIES}&page_token=99%3A%3Aa%26x%3D1%2F%3F%23%25%C3%A9"),
                "/v2/account".to_owned()
            ]
        );
        assert_eq!(report["pages_read"], 2);
        assert_eq!(report["pagination_exhausted"], true);
        assert_eq!(report["selected_account_fields_unchanged"], true);
        assert_eq!(report["activities"].as_array().unwrap().len(), 103);
        for field in [
            "order_id",
            "transaction_time",
            "type",
            "side",
            "symbol",
            "qty",
            "price",
            "cum_qty",
            "leaves_qty",
        ] {
            assert_eq!(
                report["activities"][0][field],
                events(1)[0][field],
                "{field}"
            );
        }
        assert_eq!(report["activities"][100]["net_amount"], "-0.000001");
        assert_eq!(report["activities"][101]["net_amount"], "50.00");
        assert!(report["activities"][102]["net_amount"].is_null());
        assert!(report["activities"][0]["private"].is_null());
        assert!(report["account_before"]["private"].is_null());
        flags(&report);
    }

    #[test]
    fn short_empty_exact_full_and_oversized_pages_have_distinct_outcomes() {
        for count in [0, 99, 100, 101] {
            let result = collect("paper-id", 1, |path| {
                Ok(if path == "/v2/account" {
                    account()
                } else {
                    events(count).to_string()
                })
            });
            assert_eq!(result.is_ok(), count < 100);
            if let Ok(report) = result {
                assert_eq!(report["activities"].as_array().unwrap().len(), count);
                flags(&report);
            }
        }
        let mut page_reads = 0;
        let report = collect("paper-id", 2, |path| {
            Ok(if path == "/v2/account" {
                account()
            } else {
                page_reads += 1;
                if page_reads == 1 {
                    events(100).to_string()
                } else {
                    "[]".into()
                }
            })
        })
        .unwrap();
        assert_eq!(page_reads, 2);
        assert_eq!(report["activities"].as_array().unwrap().len(), 100);
        for (account, pages) in [(" ", 1), ("paper-id", 0)] {
            assert!(collect(account, pages, |_| panic!("invalid request must not read")).is_err());
        }
    }

    #[test]
    fn repeated_missing_mismatched_or_malformed_activity_identities_refuse() {
        for body in [
            "null".to_owned(),
            json!([{"id":"a"}]).to_string(),
            json!([{"id":"","activity_type":"FILL"}]).to_string(),
            json!([{"id":"a","activity_type":" "}]).to_string(),
            json!([{"id":"a","activity_type":"FILL","account_id":"other"}]).to_string(),
            json!([{"id":"a","activity_type":"FILL","qty":0.1}]).to_string(),
            json!([{"id":"a","activity_type":"FILL"},{"id":"a","activity_type":"FEE"}]).to_string(),
        ] {
            assert!(
                collect("paper-id", 1, |path| Ok(if path == "/v2/account" {
                    account()
                } else {
                    body.clone()
                }))
                .is_err()
            );
        }
        let mut reads = 0;
        assert!(
            collect("paper-id", 2, |path| {
                Ok(if path == "/v2/account" {
                    account()
                } else {
                    reads += 1;
                    if reads == 1 {
                        events(100).to_string()
                    } else {
                        events(1).to_string()
                    }
                })
            })
            .is_err()
        );
    }

    #[test]
    fn account_changes_stay_visible_and_each_failed_read_refuses_partial_output() {
        let mut accounts = 0;
        let report = collect("paper-id", 1, |path| {
            Ok(if path == "/v2/account" {
                accounts += 1;
                if accounts == 2 {
                    account().replace("50", "49")
                } else {
                    account()
                }
            } else {
                "[]".into()
            })
        })
        .unwrap();
        assert_eq!(report["selected_account_fields_unchanged"], false);
        flags(&report);
        for stage in 0..3 {
            for malformed in [false, true] {
                let mut index = 0;
                assert!(
                    collect("paper-id", 1, |path| {
                        let current = index;
                        index += 1;
                        if current == stage {
                            if malformed {
                                Ok("null".into())
                            } else {
                                Err("transport failed".into())
                            }
                        } else {
                            Ok(if path == "/v2/account" {
                                account()
                            } else {
                                "[]".into()
                            })
                        }
                    })
                    .is_err()
                );
            }
        }
        for stage in [0, 2] {
            let mut index = 0;
            assert!(
                collect("paper-id", 1, |path| {
                    let current = index;
                    index += 1;
                    Ok(if current == stage {
                        account().replace("paper-id", "other")
                    } else if path == "/v2/account" {
                        account()
                    } else {
                        "[]".into()
                    })
                })
                .is_err()
            );
        }
    }
}
