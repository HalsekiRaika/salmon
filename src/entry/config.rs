#![allow(dead_code)]

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use chrono::{DateTime, Local};
use once_cell::sync::OnceCell;
use reqwest::{Client, StatusCode};
use reqwest::header::HeaderName;
use serde::{Deserialize, Serialize};
use crate::entry::entry_objects::{Channel, LiverEntry};
use crate::ids;
use crate::ids::ChannelId;
use crate::logger::Logger;

pub struct RequestEntry(VecDeque<LiverEntry>);

pub(crate) fn get_api_key_param() -> &'static str {
    static API_KEY: OnceCell<String> = OnceCell::new();
    API_KEY.get_or_init(|| {
        dotenv::var("api_key")
            .expect("api_key is not set.")
    })
}

// fix me: 後は頼んだ。
// reply you: よっしゃ任せろ
impl RequestEntry {
    async fn request_external(&self) -> Result<VecDeque<VideoInfoItem>, reqwest::Error> {
        const SEARCH_API: &str = "https://www.googleapis.com/youtube/v3/search";
        let logger = Logger::new(Some("search api"));
        let http_client = Client::new();

        let youtube_ext = self.0.iter()
            .flat_map(|entity| entity.get_site().iter()
                .flat_map(Channel::as_youtube_id)
                .collect::<Vec<ChannelId>>())
            .collect::<Vec<ChannelId>>();

        let mut response = VecDeque::new();

        for channel_id in youtube_ext {
            let external = http_client.get(SEARCH_API)
                .header(HeaderName::from_static("If-None-Match"), ETagCache::get_etag(channel_id.clone()))
                .form(&[("id", channel_id.0.as_str()), ("part", "snippet"), ("type", "video"),
                        ("eventType", "upcoming"), ("fields", "(etag, items(id(videoId)))"), ("key", get_api_key_param())])
                .send()
                .await?;
            let parsed = match external.status() {
                StatusCode::OK => {
                    let parsed = external.json::<SearchedObjects>().await
                        .expect("cannot parse. this data structure is wrong.");
                    ETagCache::new(channel_id.clone(), parsed.etag.clone()).cached();
                    Some(parsed)
                },
                StatusCode::NOT_MODIFIED => None,
                StatusCode::TOO_MANY_REQUESTS => {
                    logger.error("Resource Exhausted, Quota Limit exceeded!");
                    panic!()
                },
                _ => unimplemented!("unknown error code.")
            };
            response.push_back(parsed);
        }

        let response = response.iter().flatten()
            .flat_map(|raw_object| raw_object.items.iter()
                .map(|item| item.id.video_id.clone())
                .collect::<Vec<ids::VideoId>>())
            .collect::<VecDeque<ids::VideoId>>();

        let mut queue: VecDeque<String> = VecDeque::new();
        for picked in 0..=(response.len() / 50 + (if (response.len() % 50) > 0 { 1 } else { 0 } )) {
            queue.push_back(response.iter().by_ref()
                .take(50).skip(50 * picked)
                .map(|id| id.0.to_string())
                .collect::<Vec<String>>()
                .join(","));
        }

        const VIDEO_DESCRIPTION_API: &str = "https://www.googleapis.com/youtube/v3/videos";

        let mut response = VecDeque::new();

        for video_id in queue {
            let external = http_client.get(VIDEO_DESCRIPTION_API)
                .header(HeaderName::from_static("If-None-Match"), "")
                .form(&[("id", video_id.as_str()), ("part", "liveStreamingDetails, statistics, snippet"),
                    ("fields", "(etag, items(id, snippet(title, description, channelTitle, channelId, publishedAt), statistics, liveStreamingDetails))"),
                    ("key", get_api_key_param())])
                .send()
                .await?;
            let parsed = match external.status() {
                StatusCode::OK => {
                    let parsed = external.json::<SearchedVideoInfoObjects>().await
                        .expect("cannot parse. this data structure is wrong.");
                    //ETagCache::new();
                    Some(parsed)
                },
                StatusCode::NOT_MODIFIED => None,
                StatusCode::TOO_MANY_REQUESTS => {
                    logger.error("Resource Exhausted, Quota Limit exceeded!");
                    panic!()
                },
                _ => unimplemented!("unknown error code.")
            };
            response.push_back(parsed)
        }

        let aggregates = response.iter().flatten()
            .flat_map(|searched| searched.items.clone())
            .collect::<VecDeque<VideoInfoItem>>();

        Ok(aggregates)
    }
}


#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
struct ETagCache {
    key: String,
    etag: String
}

impl ETagCache {
    fn new(key: impl Into<String>, etag: impl Into<String>) -> ETagCache {
        Self { key: key.into(), etag: etag.into() }
    }

    fn cached(&self) {
        let mut queue = ETagCache::load()
            .unwrap_or_default();
        if queue.contains(self) { return; }
        queue.push_back(self.clone());
        ETagCache::save(&queue)
    }

    fn get_etag(key: impl Into<String> + Clone) -> String {
        let queue = ETagCache::load()
            .unwrap_or_default();
        queue.iter().filter(|q| q.key == key.clone().into())
            .map(|q| q.etag.clone())
            .collect()
    }

    fn remove_etag(etag: String) -> String {
        let mut queue = ETagCache::load()
            .unwrap_or_default();
        queue.retain(|q| q.etag != etag);
        ETagCache::save(&queue);
        etag
    }

    fn save(queue: &VecDeque<ETagCache>) {
        let cache = ETagCache::get_context();
        let _ = cache.set_len(0);
        let _ = serde_json::to_writer(cache, queue)
            .expect("cannot write to cache file.");
    }

    fn load() -> Option<VecDeque<ETagCache>> {
        let cache = ETagCache::get_context();
        let cache_queue: Option<VecDeque<ETagCache>> = serde_json::from_reader(cache)
            .unwrap_or(None);
        cache_queue
    }

    fn get_context() -> File {
        const CACHE_PATH: &str = "./.cache.json";
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(CACHE_PATH)
            .unwrap_or_else(|_| {
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .read(true)
                    .open(CACHE_PATH)
                    .expect("failed to open cache.json")
            })
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SearchedObjects {
    etag: String,
    items: Vec<IdentifierWrapper>
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct IdentifierWrapper {
    id: Identifier,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Identifier {
    #[serde(rename = "videoId")]
    video_id: ids::VideoId
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SearchedVideoInfoObjects {
    etag: String,
    items: Vec<VideoInfoItem>
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct VideoInfoItem {
    id: ids::VideoId,
    snippet: Snippet,
    statistics: Statistics,
    #[serde(rename = "liveStreamingDetails")]
    details: LiveStreamingDetails
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Snippet {
    #[serde(rename = "publishedAt")]
    published_at: DateTime<Local>,
    #[serde(rename = "channelId")]
    channel_id: ChannelId,
    title: String,
    description: String,
    #[serde(rename = "channelTitle")]
    channel_title: String
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Statistics {
    #[serde(rename = "viewCount")]
    view_count: i64,
    #[serde(rename = "likeCount")]
    like_count: i64,
    #[serde(rename = "favoriteCount")]
    favorite_count: i64,
    #[serde(rename = "commentCount")]
    comment_count: i64
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct LiveStreamingDetails {
    actual_start_time: DateTime<Local>,
    actual_end_time: DateTime<Local>,
    scheduled_start_time: DateTime<Local>
}

#[cfg(test)]
mod test_deserialize {
    use crate::entry::config::SearchedObjects;

    #[test]
    fn deserialize_response() {
        static JSON_STRUCT: &str = r#"
{
    "etag": "_NtfKFaDSQPi_nKlGipTi6U25gk",
    "items": [
        { "id": { "videoId": "KXAsNwifn0o" } }
    ]
}
        "#;

        let parsed: SearchedObjects = serde_json::from_str(JSON_STRUCT)
            .expect("cannot parse.");
    }
}

#[cfg(test)]
mod test_cache {
    use crate::entry::config::ETagCache;
    use crate::ids::ChannelId;

    #[test]
    fn caching_save_test() {
        ETagCache {
            key: ChannelId("UCX7YkU9nEeaoZbkVLVajcMg".to_string()).0,
            etag: "tmW7oPByL2oIL29fzgRAoKJwozU".to_string()
        }.cached()
    }
    #[test]
    fn caching_load_test() {
        ETagCache {
            key: ChannelId("UChAnqc_AY5_I3Px5dig3X1Q".to_string()).0,
            etag: "aicCgxcWpojNQk4EnAlpsAZzTy0".to_string()
        }.cached();

        let etag = ETagCache::get_etag(&ChannelId("UChAnqc_AY5_I3Px5dig3X1Q".to_string()));
        assert_eq!(etag, "aicCgxcWpojNQk4EnAlpsAZzTy0".to_string())
    }

    #[test]
    fn cashing_remove_test() {
        ETagCache {
            key: ChannelId("UCvaTdHTWBGv3MKj3KVqJVCw".to_string()).0,
            etag: "Coo_4NE2_7KoYHEGOsf2dvyzGpE".to_string()
        }.cached();

        let etag = ETagCache::get_etag(&ChannelId("UCvaTdHTWBGv3MKj3KVqJVCw".to_string()));
        ETagCache::remove_etag(etag);

        let etag = ETagCache::get_etag(&ChannelId("UCvaTdHTWBGv3MKj3KVqJVCw".to_string()));
        assert_eq!(etag, "".to_string())
    }
}