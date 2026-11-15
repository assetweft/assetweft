//! Responsibility: Consumption CLI incoming adapter for local budget approve/balance.
//! Dependencies: shared transport, existing Application and public budget value types.
//! Callers: main command dispatch; strict wire DTOs and map visitors are private here.
//! Invariants: exact object/duplicate/type/integer admission and one facade operation; core alone owns ledger policy, with no SQL, payment or replay here.

use crate::protocol::{envelope, error, existing_database, library_outcome, read_json};
use assetweft_core::{
    application::Application,
    assurance as val,
    budget::{Allowance, Approval, Unit},
};
use clap::ArgMatches;
use serde_json::{Value, json};
use std::path::PathBuf;

/// Wire shape only; Consumption alone owns text, currency, integer and approval policy.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AllowanceInput {
    id: String,
    unit: UnitInput,
    capacity: u64,
    approval: Approval,
    source: String,
}
#[derive(serde::Deserialize)]
enum UnitInput {
    Token,
    Task,
    Second,
    CurrencyMinor(CurrencyObject),
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrencyInput {
    currency: String,
    decimals: u8,
}
/// A map-only boundary retains serde's duplicate/missing/type checks and rejects arrays.
struct CurrencyObject(CurrencyInput);
impl<'de> serde::Deserialize<'de> for CurrencyObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CurrencyVisitor;
        impl<'de> serde::de::Visitor<'de> for CurrencyVisitor {
            type Value = CurrencyObject;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a currency object with currency and decimals")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                <CurrencyInput as serde::Deserialize>::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
                .map(CurrencyObject)
            }
        }
        deserializer.deserialize_map(CurrencyVisitor)
    }
}
struct AllowanceObject(Allowance);
impl<'de> serde::Deserialize<'de> for AllowanceObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AllowanceVisitor;
        impl<'de> serde::de::Visitor<'de> for AllowanceVisitor {
            type Value = AllowanceObject;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an allowance object with id, unit, capacity, approval and source")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                let input = <AllowanceInput as serde::Deserialize>::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                let unit = match input.unit {
                    UnitInput::Token => Unit::Token,
                    UnitInput::Task => Unit::Task,
                    UnitInput::Second => Unit::Second,
                    UnitInput::CurrencyMinor(CurrencyObject(c)) => Unit::CurrencyMinor {
                        currency: c.currency,
                        decimals: c.decimals,
                    },
                };
                Ok(AllowanceObject(Allowance {
                    id: input.id,
                    unit,
                    capacity: input.capacity,
                    approval: input.approval,
                    source: input.source,
                }))
            }
        }
        deserializer.deserialize_map(AllowanceVisitor)
    }
}
/// One facade operation only: no retry, top-up/reset, extra read after write or provider call.
/// Balance is a local projection, not a filesystem read-only or external fee guarantee.
pub(super) fn budget_response(
    wire: &str,
    call_id: Option<&String>,
    action: &str,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let finish =
        |code, status, errors, results| (code, envelope(wire, call_id, status, errors, results));
    if let Err(e) = existing_database(db) {
        return finish(2, "INVALID_INPUT", json!([e]), Value::Null);
    }
    let mut app = match Application::open(db) {
        Ok(app) => app,
        Err(e) => {
            return finish(
                2,
                "INVALID_INPUT",
                json!([error("db", "INVALID_INPUT", e)]),
                Value::Null,
            );
        }
    };
    let result = if action == "approve" {
        let path = args.get_one::<PathBuf>("allowance").expect("required");
        let AllowanceObject(allowance) = match read_json(path, limits, "allowance") {
            Ok(input) => input,
            Err(e) => return finish(2, "INVALID_INPUT", json!([e]), Value::Null),
        };
        app.approve_budget(&allowance).map(|()| {
            json!({
                "scope":"stored_local_allowance_declaration","allowance":allowance,
            })
        })
    } else {
        // Verbatim, including empty IDs: preserve the actual query's error category.
        let id = args.get_one::<String>("budget-id").expect("required");
        app.budget_balance(id).map(|balance| {
            json!({
                "scope":"local_budget_balance","allowance":balance.allowance,
                "reserved":balance.reserved,"spent":balance.spent,
            })
        })
    };
    match result {
        Ok(value) => finish(0, "PASS", json!([]), value),
        Err(e) => {
            let (code, status) = library_outcome(&e);
            finish(
                code,
                status,
                json!([error("budget", status, e)]),
                Value::Null,
            )
        }
    }
}
