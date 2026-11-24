use crate::protocol::{
    database_open_error, envelope, error, existing_database, library_outcome, read_json,
};
use assetweft_core::{
    application::Application,
    assurance as val,
    budget::{Allowance, Approval, Intent, Request, State, Unit},
};
use clap::ArgMatches;
use serde::{
    Deserialize,
    de::{self, IntoDeserializer},
};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AllowanceInput {
    id: String,
    #[serde(deserialize_with = "deserialize_unit")]
    unit: Unit,
    capacity: u64,
    #[serde(deserialize_with = "deserialize_approval")]
    approval: Approval,
    source: String,
}
fn deserialize_approval<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Approval, D::Error> {
    Approval::deserialize(String::deserialize(deserializer)?.into_deserializer())
}

fn deserialize_unit<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Unit, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct CurrencyUnit {
        #[serde(rename = "CurrencyMinor")]
        currency_minor: CurrencyObject,
    }

    struct UnitVisitor;
    impl<'de> de::Visitor<'de> for UnitVisitor {
        type Value = Unit;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Token, Task or Second as a string, or a CurrencyMinor object")
        }

        fn visit_str<E: de::Error>(self, unit: &str) -> Result<Unit, E> {
            match unit {
                "Token" => Ok(Unit::Token),
                "Task" => Ok(Unit::Task),
                "Second" => Ok(Unit::Second),
                _ => Err(E::unknown_variant(unit, &["Token", "Task", "Second"])),
            }
        }

        fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<Unit, M::Error> {
            let CurrencyUnit {
                currency_minor: CurrencyObject(input),
            } = CurrencyUnit::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(Unit::CurrencyMinor {
                currency: input.currency,
                decimals: input.decimals,
            })
        }
    }

    deserializer.deserialize_any(UnitVisitor)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrencyInput {
    currency: String,
    decimals: u8,
}
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
                Ok(AllowanceObject(Allowance {
                    id: input.id,
                    unit: input.unit,
                    capacity: input.capacity,
                    approval: input.approval,
                    source: input.source,
                }))
            }
        }
        deserializer.deserialize_map(AllowanceVisitor)
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestInput {
    id: String,
    account: String,
    input_fingerprint: String,
    #[serde(deserialize_with = "deserialize_unit")]
    unit: Unit,
    quote: Option<u64>,
    reservation: u64,
    source: String,
}
struct RequestObject(Request);
impl<'de> serde::Deserialize<'de> for RequestObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RequestVisitor;
        impl<'de> serde::de::Visitor<'de> for RequestVisitor {
            type Value = RequestObject;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a call request object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                let input = <RequestInput as serde::Deserialize>::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(RequestObject(Request {
                    id: input.id,
                    account: input.account,
                    input_fingerprint: input.input_fingerprint,
                    unit: input.unit,
                    quote: input.quote,
                    reservation: input.reservation,
                    source: input.source,
                }))
            }
        }
        deserializer.deserialize_map(RequestVisitor)
    }
}
fn intent_result(intent: Option<Intent>) -> Value {
    let intent = intent.map(|intent| {
        let state = match intent.state {
            State::Preparing => "PREPARING",
            State::Attempting => "ATTEMPTING",
            State::Unknown => "UNKNOWN",
            State::Confirmed => "CONFIRMED",
            State::Released => "RELEASED",
        };
        json!({"request":intent.request,"state":state,
            "cancelled":intent.cancelled,"charge":intent.charge})
    });
    json!({"scope":"local_call_intent","intent":intent})
}
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
    let read_only = action == "receipts";
    let opened = if read_only {
        Application::open_read_only(db)
    } else {
        Application::open_existing(db)
    };
    let mut app = match opened {
        Ok(app) => app,
        Err(e) if read_only => {
            return finish(
                2,
                "INVALID_INPUT",
                json!([error("db", "INVALID_INPUT", e)]),
                Value::Null,
            );
        }
        Err(e) => return database_open_error(wire, call_id, e),
    };
    let result = match action {
        "approve" => {
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
        }
        "balance" => {
            let id = args.get_one::<String>("budget-id").expect("required");
            app.budget_balance(id).map(|balance| {
                json!({
                    "scope":"local_budget_balance","allowance":balance.allowance,
                    "reserved":balance.reserved,"spent":balance.spent,
                })
            })
        }
        "reserve" => {
            let path = args.get_one::<PathBuf>("request").expect("required");
            let RequestObject(request) = match read_json(path, limits, "request") {
                Ok(input) => input,
                Err(e) => return finish(2, "INVALID_INPUT", json!([e]), Value::Null),
            };
            app.reserve_call(&request)
                .map(|intent| intent_result(Some(intent)))
        }
        "intent" => {
            let id = args.get_one::<String>("intent-id").expect("required");
            app.call_intent(id).map(intent_result)
        }
        "receipts" => {
            let id = args.get_one::<String>("intent-id").expect("required");
            app.call_receipts(id)
                .map(|receipts| json!({"scope":"local_call_receipts","receipts":receipts}))
        }
        "cancel" => {
            let id = args.get_one::<String>("intent-id").expect("required");
            app.cancel_call(id).map(|()| {
                json!({
                    "scope":"stored_local_call_cancellation_request","intent_id":id,
                })
            })
        }
        _ => unreachable!("clap grammar"),
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
