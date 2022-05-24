#![allow(dead_code)]


use std::collections::vec_deque::VecDeque;
use std::io::Read;
use std::path::Path;
use anyhow::Context;
use serde::{Deserialize};

use crate::ids::{NumId, StringId};

#[derive(Debug, thiserror::Error)]
pub enum ExternalFileLoadError {
    #[error("cannot open")]
    CannotOpen,
    #[error("cannot deserialize")]
    CannotDeserialize
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Hash)]
pub struct AffiliationEntry {
    id: NumId<AffiliationEntry>,
    name: String
}

impl AffiliationEntry {
    pub fn as_ref_id(&self) -> &NumId<AffiliationEntry> {
        &self.id
    }

    pub fn as_ref_name(&self) -> &str {
        &self.name
    }

    pub fn breach_extraction_id(&self) -> NumId<AffiliationEntry> {
        self.id.to_owned()
    }

    pub fn breach_extraction_name(&self) -> String {
        self.name.to_owned()
    }
}

impl AffiliationEntry {
    pub fn load_from<P>(path: P) -> anyhow::Result<VecDeque<AffiliationEntry>>
      where P: AsRef<Path> {
        let mut buf = String::new();
        let _ = std::fs::File::open(path.as_ref())
            .context(ExternalFileLoadError::CannotOpen)?
            .read_to_string(&mut buf);
        serde_json::from_str::<VecDeque<AffiliationEntry>>(&buf)
            .context(ExternalFileLoadError::CannotDeserialize)
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Hash)]
pub struct LiverEntry {
    id: NumId<LiverEntry>,
    name: String,
    localized_name: String,
    twitter_url: String,
    channels: Vec<Channel>
}

impl LiverEntry {
    pub fn as_ref_id(&self) -> &NumId<LiverEntry> {
        &self.id
    }

    pub fn as_ref_site(&self) -> &Vec<Channel> {
        &self.channels
    }


    pub fn breach_extraction_id(&self) -> NumId<LiverEntry> {
        self.id.to_owned()
    }

    pub fn breach_extraction_name(&self) -> String {
        self.name.to_owned()
    }
}

impl LiverEntry {
    pub fn load_from<P>(path: P) -> anyhow::Result<LiverEntry>
        where P: AsRef<Path> {
        let mut buf = String::new();
        let _ = std::fs::File::open(path.as_ref())
            .context(ExternalFileLoadError::CannotOpen)?
            .read_to_string(&mut buf);
        serde_json::from_str::<LiverEntry>(&buf)
            .context(ExternalFileLoadError::CannotDeserialize)
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Hash)]
#[serde(tag = "site_name")]
pub enum Channel {
    #[serde(rename = "Youtube")]
    Youtube { id: StringId<Channel> },

    #[serde(other)]
    Unsupported
}

impl Channel {
    pub fn as_youtube_id(&self) -> Option<StringId<Channel>> {
        match self {
            Channel::Youtube { id} => Some(id.to_owned()),
            _ => None
        }
    }
}

#[cfg(test)]
mod test {
    use crate::models::{AffiliationEntry, LiverEntry};

    #[test]
    fn affiliation_load_test() {
        let suc = AffiliationEntry::load_from(".config/affiliation.json");
        let load_fail = AffiliationEntry::load_from(".config/affiliation.js");
        let invalid = AffiliationEntry::load_from(".test/invalid_affiliation.json");

        assert!(suc.is_ok());
        assert!(load_fail.is_err());
        assert!(invalid.is_err());
    }

    #[test]
    fn liver_load_test() {
        // 私の推し！
        let suc = LiverEntry::load_from(".config/hololive/nekomata_okayu.json");
        let load_fail = LiverEntry::load_from(".config/hololive/nekomata_okayu.js");
        let invalid = LiverEntry::load_from(".test/invalid_liver.json");

        assert!(suc.is_ok());
        assert!(load_fail.is_err());
        assert!(invalid.is_err());
    }
}