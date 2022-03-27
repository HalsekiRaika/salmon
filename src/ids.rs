use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct AffiliationId(pub i64);

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(transparent)]
pub struct LiverId(pub i64);

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ChannelId(pub String);

impl From<ChannelId> for String {
    fn from(id: ChannelId) -> Self {
        id.0
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct VideoId(pub String);