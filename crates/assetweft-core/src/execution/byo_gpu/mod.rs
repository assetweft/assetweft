mod policy;
pub(crate) mod process;

use crate::budget::{Receipt, Resolution, Unit, Usage};
use std::path::PathBuf;

pub use policy::{Profile, ProfileDescription, profiles};
pub use process::validate_kaggle_bundle;

pub struct Run {
    pub budget_id: String,
    pub intent_id: String,
    pub profile: Profile,
    pub model: String,
    pub out_dir: PathBuf,
    pub image: Option<PathBuf>,
    pub prompt: Option<String>,
    pub bundle: Option<PathBuf>,
    pub adapter_bin: Option<PathBuf>,
    pub allow_remote_upload: bool,
    pub max_bytes: u64,
}

#[derive(Debug)]
pub struct Invalid(pub &'static str, pub &'static str);

pub struct KaggleValidation {
    pub credential_present: bool,
    pub accelerator: &'static str,
}

pub enum RunError<E> {
    Invalid(Invalid),
    Credentials,
    Ledger(E),
}

pub enum Outcome {
    Succeeded { sha256: String },
    Unknown { diagnostic: &'static str },
    Released,
}

pub struct Report {
    pub attempted: bool,
    pub artifact: &'static str,
    pub outcome: Outcome,
}

pub(crate) const LEDGER_SOURCE: &str = "byo-gpu-v1";
const RECEIPT_ID: &str = "byo-gpu-result-v1";
const SHA_SOURCE_PREFIX: &str = "artifact-sha256:";

pub(crate) fn receipt(sha256: &str) -> Receipt {
    Receipt {
        id: RECEIPT_ID.into(),
        source: LEDGER_SOURCE.into(),
        resolution: Resolution::Confirmed { charge: 1 },
        usage: vec![Usage {
            unit: Unit::Task,
            amount: Some(1),
            source: format!("{SHA_SOURCE_PREFIX}{sha256}"),
        }],
    }
}

pub(crate) fn receipt_hash(receipt: &Receipt) -> Option<&str> {
    let [usage] = receipt.usage.as_slice() else {
        return None;
    };
    if receipt.id != RECEIPT_ID
        || receipt.source != LEDGER_SOURCE
        || receipt.resolution != (Resolution::Confirmed { charge: 1 })
        || usage.unit != Unit::Task
        || usage.amount != Some(1)
    {
        return None;
    }
    usage.source.strip_prefix(SHA_SOURCE_PREFIX).filter(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
