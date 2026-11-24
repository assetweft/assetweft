use serde::{Deserialize, Deserializer, Serialize, de};
use std::path::Path;

use super::{Invalid, Run};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    HfZeroGpu,
    KaggleCli,
}

impl Profile {
    pub const fn id(self) -> &'static str {
        match self {
            Self::HfZeroGpu => "hf-zerogpu",
            Self::KaggleCli => "kaggle-cli",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::HfZeroGpu, Self::KaggleCli]
            .into_iter()
            .find(|p| p.id() == id)
    }

    fn models(self) -> &'static [Model] {
        match self {
            Self::HfZeroGpu => &[
                Z_IMAGE,
                Model {
                    id: "flux2-klein",
                    upstream: "black-forest-labs/FLUX.2-klein-4B",
                    license: "Apache-2.0",
                    artifact: "artifact.webp",
                    notes: "",
                },
                Model {
                    id: "trellis2",
                    upstream: "microsoft/TRELLIS.2",
                    license: "MIT",
                    artifact: "artifact.glb",
                    notes: "DINOv3 has a separate Meta license. The Space emits GLB with WebP textures.",
                },
                Model {
                    id: "hi3dgen",
                    upstream: "Stable-X/Hi3DGen",
                    license: "MIT",
                    artifact: "artifact.glb",
                    notes: "",
                },
            ],
            Self::KaggleCli => &[
                Model {
                    id: "trellis2",
                    upstream: "microsoft/TRELLIS.2@75fbf0183001",
                    license: "MIT",
                    artifact: "artifact.glb",
                    notes: "DINOv3 has a separate Meta license. Set extension_webp=False for PNG textures.",
                },
                Model {
                    id: "hi3dgen",
                    upstream: "bytedance/Hi3DGen@2b12d057e82b",
                    license: "MIT",
                    artifact: "artifact.glb",
                    notes: "",
                },
                Z_IMAGE,
            ],
        }
    }
}

#[derive(Serialize)]
pub struct Model {
    pub id: &'static str,
    upstream: &'static str,
    license: &'static str,
    pub artifact: &'static str,
    notes: &'static str,
}

const Z_IMAGE: Model = Model {
    id: "z-image-turbo",
    upstream: "Tongyi-MAI/Z-Image-Turbo",
    license: "Apache-2.0",
    artifact: "artifact.png",
    notes: "",
};
pub(super) const GPU_ACCELERATORS: [&str; 2] = ["NvidiaTeslaT4", "NvidiaL4"];

#[derive(Serialize)]
pub struct ProfileDescription {
    id: &'static str,
    models: &'static [Model],
    external_runtime: &'static str,
    auth_env_vars: &'static [&'static str],
    auth_config: &'static [&'static str],
    cash_cost: Option<u64>,
    cost_unknown: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_private: Option<bool>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    accelerators: &'static [&'static str],
}

pub fn profiles() -> [ProfileDescription; 2] {
    [Profile::HfZeroGpu, Profile::KaggleCli].map(|profile| {
        let kaggle = profile == Profile::KaggleCli;
        let (runtime, env, config): (_, &[_], &[_]) = if kaggle {
            (
                "Operator-supplied runner with Kaggle CLI push/status/output. PyPI kaggle==2.2.5 requires --with six when using uvx. NvidiaTeslaT4 is T4 x2.",
                &["KAGGLE_API_TOKEN", "KAGGLE_USERNAME", "KAGGLE_KEY"],
                &["~/.kaggle/access_token", "~/.kaggle/kaggle.json", "~/.config/kaggle/kaggle.json", "$KAGGLE_CONFIG_DIR/kaggle.json"],
            )
        } else {
            (
                "Operator-supplied runner with an HF Spaces client. ZeroGPU availability and quota vary by account and Space.",
                &["HF_TOKEN"],
                &[],
            )
        };
        ProfileDescription {
            id: profile.id(),
            models: profile.models(),
            external_runtime: runtime,
            auth_env_vars: env,
            auth_config: config,
            cash_cost: None,
            cost_unknown: true,
            is_private: kaggle.then_some(true),
            accelerators: if kaggle { &GPU_ACCELERATORS } else { &[] },
        }
    })
}

pub(super) fn check_run(run: &Run) -> Result<&'static Model, Invalid> {
    if (run.image.is_some() || run.bundle.is_some()) && !run.allow_remote_upload {
        return Err(Invalid(
            "allow-remote-upload",
            "sending a local image or bundle requires --allow-remote-upload",
        ));
    }
    let model = run
        .profile
        .models()
        .iter()
        .find(|m| m.id == run.model)
        .ok_or(Invalid("model", "model is unavailable for this profile"))?;
    if run
        .prompt
        .as_ref()
        .is_some_and(|p| p.trim().is_empty() || p.len() > 16_384 || p.contains('\0'))
    {
        return Err(Invalid(
            "prompt",
            "prompt must contain 1 to 16384 bytes of text",
        ));
    }
    match run.profile {
        Profile::KaggleCli if run.bundle.is_none() => {
            return Err(Invalid("bundle", "Kaggle requires a private kernel bundle"));
        }
        Profile::HfZeroGpu if run.bundle.is_some() => {
            return Err(Invalid(
                "bundle",
                "kernel bundles are only accepted for Kaggle",
            ));
        }
        Profile::HfZeroGpu if model.artifact == "artifact.glb" && run.image.is_none() => {
            return Err(Invalid("image", "this model requires a local image"));
        }
        Profile::HfZeroGpu if model.artifact != "artifact.glb" && run.prompt.is_none() => {
            return Err(Invalid("prompt", "this model requires a prompt"));
        }
        _ => {}
    }
    if !(1..=16_777_216).contains(&run.max_bytes) {
        return Err(Invalid("max-bytes", "byte limit must be in 1..=16777216"));
    }
    Ok(model)
}

#[derive(Deserialize)]
struct KernelMetadata {
    id: String,
    #[serde(default)]
    id_no: Option<u64>,
    title: String,
    code_file: String,
    language: String,
    kernel_type: String,
    #[serde(deserialize_with = "kernel_flag")]
    is_private: bool,
    #[serde(deserialize_with = "kernel_flag")]
    enable_gpu: bool,
    #[serde(default, deserialize_with = "kernel_flag")]
    enable_tpu: bool,
    #[serde(default)]
    machine_shape: Option<String>,
}

fn kernel_flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    struct FlagVisitor;

    impl de::Visitor<'_> for FlagVisitor {
        type Value = bool;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a boolean or the Kaggle template string true or false")
        }

        fn visit_bool<E: de::Error>(self, value: bool) -> Result<bool, E> {
            Ok(value)
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<bool, E> {
            match value {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(E::invalid_value(de::Unexpected::Str(value), &self)),
            }
        }
    }

    deserializer.deserialize_any(FlagVisitor)
}

pub(super) fn kernel_metadata(bytes: &[u8]) -> Result<(String, &'static str), Invalid> {
    struct MetadataVisitor;

    impl<'de> de::Visitor<'de> for MetadataVisitor {
        type Value = KernelMetadata;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a Kaggle kernel metadata object")
        }

        fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<KernelMetadata, A::Error> {
            KernelMetadata::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    let invalid_metadata = |_| {
        Invalid(
            "bundle",
            "kernel-metadata.json must contain typed kernel metadata",
        )
    };
    let mut json = serde_json::Deserializer::from_slice(bytes);
    let metadata = json
        .deserialize_map(MetadataVisitor)
        .map_err(invalid_metadata)?;
    json.end().map_err(invalid_metadata)?;
    if !metadata.is_private {
        return Err(Invalid(
            "bundle.is_private",
            "Kaggle kernel must be private",
        ));
    }
    if !metadata.enable_gpu || metadata.enable_tpu {
        return Err(Invalid(
            "bundle.enable_gpu",
            "Kaggle kernel must use GPU without TPU",
        ));
    }
    if metadata.id_no.is_some() {
        return Err(Invalid(
            "bundle.id",
            "use the kernel slug without a numeric ID override",
        ));
    }
    if metadata.id.contains("<YOUR_KAGGLE_USERNAME>") {
        return Err(Invalid(
            "bundle.id",
            "replace the Kaggle username placeholder",
        ));
    }
    let slug = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    };
    if !metadata
        .id
        .split_once('/')
        .is_some_and(|(owner, name)| slug(owner) && slug(name))
    {
        return Err(Invalid(
            "bundle.id",
            "Kaggle kernel ID must be owner/kernel-slug",
        ));
    }
    if metadata.title.trim().is_empty() {
        return Err(Invalid(
            "bundle.title",
            "Kaggle kernel title must be nonempty",
        ));
    }
    let extension = match (metadata.language.as_str(), metadata.kernel_type.as_str()) {
        ("python", "notebook") => "ipynb",
        ("python", "script") => "py",
        _ => {
            return Err(Invalid(
                "bundle.language",
                "unsupported Kaggle kernel language or type",
            ));
        }
    };
    if metadata.code_file.contains(['\\', ':', '\0'])
        || metadata
            .code_file
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !Path::new(&metadata.code_file)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
    {
        return Err(Invalid(
            "bundle.code_file",
            "kernel code must be a relative file matching its language and type",
        ));
    }
    let accelerator = GPU_ACCELERATORS
        .into_iter()
        .find(|&gpu| {
            gpu == metadata
                .machine_shape
                .as_deref()
                .filter(|shape| !shape.is_empty())
                .unwrap_or(GPU_ACCELERATORS[0])
        })
        .ok_or(Invalid(
            "bundle.machine_shape",
            "Kaggle accelerator must be NvidiaTeslaT4 or NvidiaL4",
        ))?;
    Ok((metadata.code_file, accelerator))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn private_kernel_safety_depends_on_typed_metadata() {
        let metadata = json!({
            "id": "someone/my-kernel", "title": "Private test", "code_file": "src/main.py",
            "language": "python", "kernel_type": "script", "is_private": true, "enable_gpu": true,
        });
        let check =
            |value: &serde_json::Value| kernel_metadata(&serde_json::to_vec(value).unwrap());
        assert_eq!(
            check(&metadata).unwrap(),
            ("src/main.py".into(), "NvidiaTeslaT4")
        );
        for (field, value, rejected) in [
            ("id_no", json!(7), "bundle.id"),
            ("is_private", json!("True"), "bundle"),
            ("enable_gpu", json!(false), "bundle.enable_gpu"),
            ("enable_tpu", json!(true), "bundle.enable_gpu"),
            (
                "machine_shape",
                json!("NvidiaTeslaA100"),
                "bundle.machine_shape",
            ),
            ("code_file", json!("src/../main.py"), "bundle.code_file"),
            ("code_file", json!("src\\main.py"), "bundle.code_file"),
        ] {
            let mut invalid = metadata.clone();
            invalid[field] = value;
            assert_eq!(check(&invalid).unwrap_err().0, rejected);
        }
        let duplicate = format!(
            r#"{},"is_private":true}}"#,
            metadata.to_string().strip_suffix('}').unwrap()
        );
        assert_eq!(
            kernel_metadata(duplicate.as_bytes()).unwrap_err().0,
            "bundle"
        );
    }
}
