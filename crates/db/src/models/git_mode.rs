use serde::{Deserialize, Serialize};
use sqlx::Type;
use strum_macros::{Display, EnumString};
use ts_rs::TS;

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    Display,
    EnumString,
    Type,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    TS,
)]
#[sqlx(type_name = "git_mode", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum GitMode {
    #[default]
    Managed,
    PreserveHistory,
}

impl GitMode {
    pub fn default_merge_strategy(self) -> MergeStrategy {
        match self {
            Self::Managed => MergeStrategy::Squash,
            Self::PreserveHistory => MergeStrategy::FastForwardTarget,
        }
    }

    pub fn should_auto_commit(self) -> bool {
        matches!(self, Self::Managed)
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    Display,
    EnumString,
    Type,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    TS,
)]
#[sqlx(type_name = "merge_strategy", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum MergeStrategy {
    #[default]
    Squash,
    FastForwardTarget,
}
