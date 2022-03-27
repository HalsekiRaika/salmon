#![allow(dead_code)]

use std::fs::File;
use std::io::Read;
use std::path::Path;
use serde::{Deserialize};
use crate::ids::{AffiliationId, ChannelId, LiverId};

#[derive(Debug, Clone, Deserialize)]
pub struct AffiliationEntry {
    id: AffiliationId,
    name: String
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiverEntry {
    id: LiverId,
    name: String,
    localized_name: String,
    twitter_url: String,
    channels: Vec<Channel>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "site_name")]
pub enum Channel {
    #[serde(rename = "Youtube")]
    Youtube { id: ChannelId },

    #[serde(other)]
    Unsupported
}

impl AffiliationEntry {
    fn load<P>(path: P) -> Vec<AffiliationEntry> where P: AsRef<Path> {
        let path = path.as_ref();
        let mut buf = String::new();
        let _ = File::open(path)
            .expect("cannot open")
            .read_to_string(&mut buf);
        serde_json::from_str::<Vec<AffiliationEntry>>(&buf)
            .expect("cannot parse")

    }
}

impl LiverEntry {
    pub fn get_id(&self) -> LiverId {
        self.id
    }

    pub fn get_site(&self) -> Vec<Channel> {
        self.channels.to_vec()
    }

    fn load<P>(path: P) -> LiverEntry where P: AsRef<Path> {
        let path = path.as_ref();
        let mut buf = String::new();
        let _ = File::open(path)
            .expect("cannot open")
            .read_to_string(&mut buf);
        serde_json::from_str::<LiverEntry>(&buf)
            .expect("cannot parse")
    }
}

impl Channel {
    pub fn as_youtube_id(&self) -> Option<ChannelId> {
        match self {
            Channel::Youtube { id} => Some(id.to_owned()),
            _ => None
        }
    }
}

#[cfg(test)]
mod test {
    use crate::entry::entry_objects::{AffiliationEntry, LiverEntry};

    #[test]
    fn load_test() {
        AffiliationEntry::load("./.config/affiliation.json");
        LiverEntry::load("./.config/hololive/akai_haato.json");
    }
}