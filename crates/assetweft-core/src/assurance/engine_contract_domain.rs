use super::{canonical_f64, semantic_sha256_hex};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetEngine {
    Unity,
    Unreal,
    Godot,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetCategory {
    Static,
    RiggedAction,
    #[serde(rename = "2d_sprite")]
    Sprite2d,
    PresentationBundle,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum AdapterKind {
    UnityCsharp,
    UnrealCpp,
    UnrealPython,
    GodotGdscript,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    target_engine: TargetEngine,
    asset_category: AssetCategory,
    engine_version: String,
    plugin_version: String,
    adapter_kind: AdapterKind,
    adapter_entrypoint: String,
    scale_units: String,
    dependencies: Vec<String>,
    transform_sample: TransformSample,
    pbr_textures: Vec<Texture>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransformSample {
    source_position_m: [f64; 3],
    target_position: [f64; 3],
    source_triangle: [u32; 3],
    target_triangle: [u32; 3],
    source_tangent_w: f64,
    target_tangent_w: f64,
    source_ibm_col_major: [f64; 16],
    target_ibm_col_major: [f64; 16],
    max_tolerance: f64,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum MapRole {
    BaseColor,
    Emissive,
    Normal,
    Roughness,
    Metallic,
    Ao,
    Orm,
    MetallicSmoothness,
    MaskMap,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Texture {
    map_role: MapRole,
    srgb: bool,
    channel_order: Option<ChannelOrder>,
    smoothness_inverted_from_roughness: Option<bool>,
}

const ORM: [&str; 3] = ["ao", "roughness", "metallic"];
const METALLIC_SMOOTHNESS: [&str; 4] = ["metallic", "unused", "unused", "smoothness"];
const MASK_MAP: [&str; 4] = ["metallic", "ao", "detail_mask", "smoothness"];

#[derive(Deserialize)]
#[serde(untagged)]
enum ChannelOrder {
    Rgb([String; 3]),
    Rgba([String; 4]),
    Label(String),
}

impl ChannelOrder {
    fn channels(&self) -> Vec<&str> {
        match self {
            Self::Rgb(channels) => channels.iter().map(String::as_str).collect(),
            Self::Rgba(channels) => channels.iter().map(String::as_str).collect(),
            Self::Label(label) => match label.as_str() {
                "RGB=ORM" => ORM.to_vec(),
                "R=metallic,A=smoothness" => METALLIC_SMOOTHNESS.to_vec(),
                "RGBA=metallic,ao,detail_mask,smoothness" => MASK_MAP.to_vec(),
                _ => vec![label],
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EngineContractReport {
    pub target_engine: TargetEngine,
    pub asset_category: AssetCategory,
    pub engine_contract_sha256: String,
    pub engine_editor_runtime_status: &'static str,
    pub headless_import_is_render_pass: bool,
    pub violations: Vec<EngineContractViolation>,
}

#[derive(Debug, Serialize)]
pub struct EngineContractViolation {
    pub field: String,
    pub code: &'static str,
    pub detail: &'static str,
}

impl TargetEngine {
    // Each output axis selects one source axis with the corresponding scale.
    fn basis(self) -> ([usize; 4], [f64; 4], bool, &'static str) {
        match self {
            Self::Unity => ([0, 1, 2, 3], [-1., 1., 1., 1.], true, "m"),
            Self::Unreal => ([0, 2, 1, 3], [100., 100., 100., 1.], true, "cm"),
            Self::Godot => ([0, 1, 2, 3], [1.; 4], false, "m"),
        }
    }

    fn accepts(self, adapter: &AdapterKind) -> bool {
        matches!(
            (self, adapter),
            (Self::Unity, AdapterKind::UnityCsharp)
                | (
                    Self::Unreal,
                    AdapterKind::UnrealCpp | AdapterKind::UnrealPython
                )
                | (Self::Godot, AdapterKind::GodotGdscript)
        )
    }
}

impl Spec {
    pub(super) fn inspect(self) -> Result<EngineContractReport, String> {
        let sample = &self.transform_sample;
        if !matches!(sample.source_tangent_w, -1.0 | 1.0)
            || !matches!(sample.target_tangent_w, -1.0 | 1.0)
        {
            return Err("tangent handedness must be -1 or +1".into());
        }
        let [i0, i1, i2] = sample.source_triangle;
        if i0 == i1 || i0 == i2 || i1 == i2 {
            return Err("source_triangle indices must be distinct".into());
        }
        if canonical_f64(sample.max_tolerance).map_err(|e| e.to_string())? < 0.0 {
            return Err("max_tolerance must be non-negative".into());
        }
        if self.pbr_textures.is_empty() {
            return Err("pbr_textures must not be empty".into());
        }

        let mut violations = Vec::new();
        let mut reject = |field: String, code, detail| {
            violations.push(EngineContractViolation {
                field,
                code,
                detail,
            });
        };
        for (field, pin) in [
            ("engine_version", &self.engine_version),
            ("plugin_version", &self.plugin_version),
            ("adapter_entrypoint", &self.adapter_entrypoint),
        ] {
            if pin.trim().is_empty() {
                reject(
                    field.into(),
                    "IncompleteEnginePin",
                    "value must not be blank",
                );
            }
        }
        for (index, dependency) in self.dependencies.iter().enumerate() {
            if dependency.trim().is_empty() {
                reject(
                    format!("dependencies[{index}]"),
                    "IncompleteEnginePin",
                    "dependency must not be blank",
                );
            }
        }
        if !self.target_engine.accepts(&self.adapter_kind) {
            reject(
                "adapter_kind".into(),
                "EngineAdapterMismatch",
                "adapter must match target engine",
            );
        }

        let (axis, scale, reverses, units) = self.target_engine.basis();
        let position = std::array::from_fn(|row| scale[row] * sample.source_position_m[axis[row]]);
        if self.scale_units != units {
            reject(
                "scale_units".into(),
                "CoordinateBasisMismatch",
                "units must match target engine",
            );
        }
        if !within(sample.target_position, position, sample.max_tolerance) {
            reject(
                "transform_sample.target_position".into(),
                "CoordinateBasisMismatch",
                "position must match target basis",
            );
        }
        let (winding, tangent) = if reverses {
            ([i0, i2, i1], -sample.source_tangent_w)
        } else {
            ([i0, i1, i2], sample.source_tangent_w)
        };
        if sample.target_triangle != winding {
            reject(
                "transform_sample.target_triangle".into(),
                "WindingOrderViolation",
                "winding must follow basis determinant",
            );
        }
        if sample.target_tangent_w != tangent {
            reject(
                "transform_sample.target_tangent_w".into(),
                "TangentHandednessViolation",
                "tangent sign must follow basis determinant",
            );
        }
        // C * IBM * C^-1 reduces to a column/row permutation and a scale ratio.
        let ibm = std::array::from_fn(|index| {
            let (col, row) = (index / 4, index % 4);
            sample.source_ibm_col_major[4 * axis[col] + axis[row]] * (scale[row] / scale[col])
        });
        if !within(sample.target_ibm_col_major, ibm, sample.max_tolerance) {
            reject(
                "transform_sample.target_ibm_col_major".into(),
                "InverseBindMatrixMismatch",
                "matrix must equal C * IBM * C^-1 within tolerance",
            );
        }

        let mut textures = Vec::new();
        for (index, texture) in self.pbr_textures.iter().enumerate() {
            if texture.srgb && !matches!(texture.map_role, MapRole::BaseColor | MapRole::Emissive) {
                reject(
                    format!("pbr_textures[{index}].srgb"),
                    "DataTextureSrgbViolation",
                    "data textures require srgb=false",
                );
            }
            let channels = texture.channel_order.as_ref().map(ChannelOrder::channels);
            let packing_ok = match (self.target_engine, texture.map_role) {
                (TargetEngine::Unreal | TargetEngine::Godot, MapRole::Orm) => {
                    channels.as_deref() == Some(ORM.as_slice())
                }
                (TargetEngine::Unity, MapRole::MetallicSmoothness) => {
                    channels.as_deref() == Some(METALLIC_SMOOTHNESS.as_slice())
                        && texture.smoothness_inverted_from_roughness == Some(true)
                }
                (TargetEngine::Unity, MapRole::MaskMap) => {
                    channels.as_deref() == Some(MASK_MAP.as_slice())
                        && texture.smoothness_inverted_from_roughness == Some(true)
                }
                (_, MapRole::Orm | MapRole::MetallicSmoothness | MapRole::MaskMap) => false,
                _ => true,
            };
            if !packing_ok {
                reject(
                    format!("pbr_textures[{index}]"),
                    "PbrChannelPackingViolation",
                    "packed map must match target engine channels and smoothness convention",
                );
            }
            textures.push((
                texture.map_role,
                texture.srgb,
                channels,
                texture.smoothness_inverted_from_roughness,
            ));
        }
        textures.sort_unstable();
        let mut dependencies = self.dependencies;
        dependencies.sort_unstable();
        let metadata = serde_json::to_vec(&(
            "engine-contract:1",
            self.target_engine,
            self.asset_category,
            self.engine_version,
            self.plugin_version,
            self.adapter_kind,
            self.adapter_entrypoint,
            self.scale_units,
            dependencies,
            textures,
        ))
        .map_err(|e| e.to_string())?;
        let values: Vec<f64> = sample
            .source_position_m
            .into_iter()
            .chain(sample.target_position)
            .chain(sample.source_triangle.into_iter().map(f64::from))
            .chain(sample.target_triangle.into_iter().map(f64::from))
            .chain([
                sample.source_tangent_w,
                sample.target_tangent_w,
                sample.max_tolerance,
            ])
            .chain(sample.source_ibm_col_major)
            .chain(sample.target_ibm_col_major)
            .chain(Sha256::digest(metadata).map(f64::from))
            .collect();
        Ok(EngineContractReport {
            target_engine: self.target_engine,
            asset_category: self.asset_category,
            engine_contract_sha256: semantic_sha256_hex(&values).map_err(|e| e.to_string())?,
            engine_editor_runtime_status: "NOT_RUN",
            headless_import_is_render_pass: false,
            violations,
        })
    }
}

fn within<const N: usize>(actual: [f64; N], expected: [f64; N], tolerance: f64) -> bool {
    actual
        .into_iter()
        .zip(expected)
        .all(|(a, b)| (a - b).abs() <= tolerance)
}
