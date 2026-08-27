use crate::config::{ConfigValue, ProviderConfigEnvelope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

pub const VOLCENGINE_PROVIDER_ID: &str = "volcengine";
pub const VOLCENGINE_SCHEMA_VERSION: u32 = 1;

const PUNCTUATION: &str = "punctuation";
const TEXT_NORMALIZATION: &str = "text_normalization";
const SEMANTIC_SMOOTHING: &str = "semantic_smoothing";
const FAST_FIRST_RESULT: &str = "fast_first_result";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OptionControlKind {
    Toggle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OptionSpec {
    pub id: String,
    pub control_kind: OptionControlKind,
    pub label: String,
    pub description: String,
    pub default_value: ConfigValue,
    pub group: String,
    pub order: u32,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AsrOptionPool {
    pub provider_id: String,
    pub provider_display_name: String,
    pub schema_version: u32,
    pub revision: u64,
    pub options: Vec<OptionSpec>,
    pub values: BTreeMap<String, ConfigValue>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderModelError {
    #[error("当前供应商不接受配置项“{0}”")]
    UnknownOption(String),
    #[error("配置项“{0}”需要布尔值")]
    InvalidType(String),
    #[error("配置封包属于供应商“{actual}”，当前供应商为“{expected}”")]
    ProviderMismatch { expected: String, actual: String },
    #[error("配置封包版本 {actual} 高于当前支持的版本 {supported}")]
    UnsupportedSchema { supported: u32, actual: u32 },
}

pub trait ProviderModel: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn schema_version(&self) -> u32;
    fn option_specs(&self) -> Vec<OptionSpec>;
    fn normalize(&self, envelope: &mut ProviderConfigEnvelope) -> Result<(), ProviderModelError>;
    fn set_option(
        &self,
        envelope: &mut ProviderConfigEnvelope,
        option_id: &str,
        value: ConfigValue,
    ) -> Result<(), ProviderModelError>;

    fn option_pool(
        &self,
        envelope: &ProviderConfigEnvelope,
    ) -> Result<AsrOptionPool, ProviderModelError> {
        let mut normalized = envelope.clone();
        self.normalize(&mut normalized)?;
        Ok(AsrOptionPool {
            provider_id: self.provider_id().to_string(),
            provider_display_name: self.display_name().to_string(),
            schema_version: self.schema_version(),
            revision: normalized.revision,
            options: self.option_specs(),
            values: normalized.values,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct VolcengineProviderModel;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VolcengineRequestOptions {
    pub punctuation: bool,
    pub text_normalization: bool,
    pub semantic_smoothing: bool,
    pub fast_first_result: bool,
}

impl VolcengineProviderModel {
    pub fn default_envelope() -> ProviderConfigEnvelope {
        let model = Self;
        let mut envelope = ProviderConfigEnvelope {
            provider_id: VOLCENGINE_PROVIDER_ID.to_string(),
            schema_version: VOLCENGINE_SCHEMA_VERSION,
            revision: 0,
            values: BTreeMap::new(),
        };
        model
            .normalize(&mut envelope)
            .expect("built-in Volcengine defaults must be valid");
        envelope
    }

    pub fn migrate_legacy_envelope(raw: &serde_json::Value, envelope: &mut ProviderConfigEnvelope) {
        let Some(behavior) = raw.get("recognition_behavior") else {
            return;
        };
        for (legacy_key, option_id, default_value) in [
            ("enable_punc", PUNCTUATION, true),
            ("enable_itn", TEXT_NORMALIZATION, true),
            ("enable_ddc", SEMANTIC_SMOOTHING, false),
            ("enable_accelerate_text", FAST_FIRST_RESULT, true),
        ] {
            let value = behavior
                .get(legacy_key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(default_value);
            envelope
                .values
                .insert(option_id.to_string(), ConfigValue::Boolean(value));
        }
    }
    pub fn request_options(
        &self,
        envelope: &ProviderConfigEnvelope,
    ) -> Result<VolcengineRequestOptions, ProviderModelError> {
        let mut envelope = envelope.clone();
        self.normalize(&mut envelope)?;
        Ok(VolcengineRequestOptions {
            punctuation: boolean(&envelope, PUNCTUATION)?,
            text_normalization: boolean(&envelope, TEXT_NORMALIZATION)?,
            semantic_smoothing: boolean(&envelope, SEMANTIC_SMOOTHING)?,
            fast_first_result: boolean(&envelope, FAST_FIRST_RESULT)?,
        })
    }

    fn defaults() -> [(&'static str, bool); 4] {
        [
            (PUNCTUATION, true),
            (TEXT_NORMALIZATION, true),
            (SEMANTIC_SMOOTHING, false),
            (FAST_FIRST_RESULT, true),
        ]
    }
}

impl ProviderModel for VolcengineProviderModel {
    fn provider_id(&self) -> &'static str {
        VOLCENGINE_PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "火山引擎"
    }

    fn schema_version(&self) -> u32 {
        VOLCENGINE_SCHEMA_VERSION
    }

    fn option_specs(&self) -> Vec<OptionSpec> {
        [
            (PUNCTUATION, "自动标点", "自动补全逗号、句号等标点", true),
            (
                TEXT_NORMALIZATION,
                "数字与日期格式化",
                "将日期、数字等口语表达转为书面形式",
                true,
            ),
            (
                SEMANTIC_SMOOTHING,
                "优化口语表达",
                "减少口语停顿词和重复表达",
                false,
            ),
            (
                FAST_FIRST_RESULT,
                "快速显示文字",
                "更快显示开头文字，可能略微影响开头识别准确率",
                true,
            ),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(index, (id, label, description, default_value))| OptionSpec {
                id: id.to_string(),
                control_kind: OptionControlKind::Toggle,
                label: label.to_string(),
                description: description.to_string(),
                default_value: ConfigValue::Boolean(default_value),
                group: "recognition_behavior".to_string(),
                order: index as u32,
                enabled: true,
                disabled_reason: None,
            },
        )
        .collect()
    }

    fn normalize(&self, envelope: &mut ProviderConfigEnvelope) -> Result<(), ProviderModelError> {
        if envelope.provider_id != self.provider_id() {
            return Err(ProviderModelError::ProviderMismatch {
                expected: self.provider_id().to_string(),
                actual: envelope.provider_id.clone(),
            });
        }
        if envelope.schema_version > self.schema_version() {
            return Err(ProviderModelError::UnsupportedSchema {
                supported: self.schema_version(),
                actual: envelope.schema_version,
            });
        }
        for key in envelope.values.keys() {
            if !Self::defaults().iter().any(|(id, _)| id == key) {
                return Err(ProviderModelError::UnknownOption(key.clone()));
            }
        }
        for (id, default_value) in Self::defaults() {
            match envelope.values.get(id) {
                Some(ConfigValue::Boolean(_)) => {}
                None => {
                    envelope
                        .values
                        .insert(id.to_string(), ConfigValue::Boolean(default_value));
                }
            }
        }
        envelope.schema_version = self.schema_version();
        Ok(())
    }

    fn set_option(
        &self,
        envelope: &mut ProviderConfigEnvelope,
        option_id: &str,
        value: ConfigValue,
    ) -> Result<(), ProviderModelError> {
        self.normalize(envelope)?;
        if !Self::defaults().iter().any(|(id, _)| *id == option_id) {
            return Err(ProviderModelError::UnknownOption(option_id.to_string()));
        }
        if !matches!(value, ConfigValue::Boolean(_)) {
            return Err(ProviderModelError::InvalidType(option_id.to_string()));
        }
        envelope.values.insert(option_id.to_string(), value);
        Ok(())
    }
}

fn boolean(envelope: &ProviderConfigEnvelope, option_id: &str) -> Result<bool, ProviderModelError> {
    match envelope.values.get(option_id) {
        Some(ConfigValue::Boolean(value)) => Ok(*value),
        _ => Err(ProviderModelError::InvalidType(option_id.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_specs_have_stable_friendly_defaults() {
        let specs = VolcengineProviderModel.option_specs();
        assert_eq!(specs.len(), 4);
        assert_eq!(specs[0].id, PUNCTUATION);
        assert_eq!(specs[0].label, "自动标点");
        assert_eq!(specs[0].default_value, ConfigValue::Boolean(true));
        assert_eq!(specs[2].default_value, ConfigValue::Boolean(false));
    }

    #[test]
    fn unknown_options_are_rejected() {
        let model = VolcengineProviderModel;
        let mut envelope = VolcengineProviderModel::default_envelope();
        assert!(matches!(
            model.set_option(&mut envelope, "unknown", ConfigValue::Boolean(true)),
            Err(ProviderModelError::UnknownOption(_))
        ));
    }

    #[test]
    fn incompatible_provider_envelopes_are_rejected() {
        let model = VolcengineProviderModel;
        let mut wrong_provider = VolcengineProviderModel::default_envelope();
        wrong_provider.provider_id = "other-provider".to_string();
        assert_eq!(
            model.normalize(&mut wrong_provider),
            Err(ProviderModelError::ProviderMismatch {
                expected: VOLCENGINE_PROVIDER_ID.to_string(),
                actual: "other-provider".to_string(),
            })
        );

        let mut future_schema = VolcengineProviderModel::default_envelope();
        future_schema.schema_version = VOLCENGINE_SCHEMA_VERSION + 1;
        assert_eq!(
            model.normalize(&mut future_schema),
            Err(ProviderModelError::UnsupportedSchema {
                supported: VOLCENGINE_SCHEMA_VERSION,
                actual: VOLCENGINE_SCHEMA_VERSION + 1,
            })
        );
    }

    #[test]
    fn option_pool_normalizes_defaults_without_mutating_source_envelope() {
        let model = VolcengineProviderModel;
        let envelope = ProviderConfigEnvelope {
            provider_id: VOLCENGINE_PROVIDER_ID.to_string(),
            schema_version: 0,
            revision: 7,
            values: BTreeMap::new(),
        };

        let pool = model.option_pool(&envelope).unwrap();

        assert_eq!(pool.schema_version, VOLCENGINE_SCHEMA_VERSION);
        assert_eq!(pool.revision, 7);
        assert_eq!(pool.values.len(), 4);
        assert!(envelope.values.is_empty());
        assert_eq!(envelope.schema_version, 0);
    }

    #[test]
    fn envelope_maps_to_strongly_typed_request_options() {
        let model = VolcengineProviderModel;
        let mut envelope = VolcengineProviderModel::default_envelope();
        model
            .set_option(
                &mut envelope,
                SEMANTIC_SMOOTHING,
                ConfigValue::Boolean(true),
            )
            .unwrap();
        let request = model.request_options(&envelope).unwrap();
        assert!(request.punctuation);
        assert!(request.text_normalization);
        assert!(request.semantic_smoothing);
        assert!(request.fast_first_result);
    }
    #[test]
    fn v2_behavior_is_migrated_inside_the_supplier_model() {
        let raw = serde_json::json!({
            "recognition_behavior": {
                "enable_punc": false,
                "enable_itn": true,
                "enable_ddc": true,
                "enable_accelerate_text": false
            }
        });
        let mut envelope = VolcengineProviderModel::default_envelope();
        VolcengineProviderModel::migrate_legacy_envelope(&raw, &mut envelope);
        let options = VolcengineProviderModel.request_options(&envelope).unwrap();
        assert!(!options.punctuation);
        assert!(options.text_normalization);
        assert!(options.semantic_smoothing);
        assert!(!options.fast_first_result);
    }
}
