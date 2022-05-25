#![allow(dead_code)]

use std::collections::HashSet;
use std::collections::vec_deque::VecDeque;
use std::fmt::Formatter;
use std::marker::PhantomData;
use std::str::FromStr;
use anyhow::{Result, Context};
use chrono::{Datelike, DateTime, Local};
use futures::StreamExt;
use misery_rs::{CacheWrapper, MiseryHandler};
use once_cell::sync::OnceCell;
use reqwest::{Client, StatusCode};
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Serialize, Deserialize, Deserializer};
use serde::de::{Error, Visitor};
use crate::ids::StringId;
use crate::logger::Logger;
use crate::models::{Channel, LiverEntry};

fn get_api_key_param() -> &'static str {
    static API_KEY: OnceCell<String> = OnceCell::new();
    API_KEY.get_or_init(|| {
        dotenv::var("API_KEY")
            .expect("API_KEY is not set.")
    })
}

fn get_process_concurrency() -> &'static usize {
    static THREAD_NUM: OnceCell<usize> = OnceCell::new();
    THREAD_NUM.get_or_init(|| {
        dotenv::var("REQUEST_CONCURRENT")
            .ok()
            .and_then(|f| f.parse().ok())
            .unwrap_or(16)
    })
}

fn get_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceCell<reqwest::Client> = OnceCell::new();
    CLIENT.get_or_init(|| {
        Client::new()
    })
}

pub(super) async fn request_video_info_concurrency(queue: &HashSet<LiverEntry>) -> Result<HashSet<VideoInfo>> {
    let logger = Logger::new(Some("search api"));
    let caching: MiseryHandler<StringId<Channel>, Etag> = MiseryHandler::load_from_blocking("./.cache/video_search_cache.json");
    let client = get_http_client();

    let youtube_ext = queue.iter()
        .flat_map(|entity| entity.as_ref_site().iter()
            .flat_map(Channel::as_youtube_id)
            .collect::<Vec<StringId<Channel>>>())
        .collect::<Vec<StringId<Channel>>>();

    // SIDE EFFECT IN ITER MAP !
    // This is incorrect because side effects are prohibited in Monad's fmap.
    // But I couldn't figure out any other way to do it well, so here...
    let responses = futures::stream::iter(youtube_ext)
        .map(|id| {
            let client = client;
            let caching = &caching;
            async move {
                let etag = caching.find_value(&id).await.unwrap_or_default();
                let res = client.get("https://www.googleapis.com/youtube/v3/search")
                    .header(HeaderName::from_static("if-none-match"), HeaderValue::from_str(etag.as_ref()).expect(""))
                    .header(HeaderName::from_static("user-agent"), HeaderValue::from_static("Nekomata-salmon (retrieve for scheduled live of virtual liver. [https://github.com/ReiRokusanami0010/salmon])"))
                    .query(&[("channelId", id.as_ref()), ("part", "snippet"), ("type", "video"),
                        ("eventType", "upcoming"), ("fields", "(etag, items(id(videoId)))"), ("key", get_api_key_param())])
                    .send().await
                    .context(RequestError::HttpGet)
                    .expect("http_get");
                (res, id)
            }
        }).buffer_unordered(*get_process_concurrency())
        .inspect(|res| logger.info(format!("req >> {}", res.1.as_ref())))
        .collect::<Vec<(reqwest::Response, StringId<Channel>)>>().await;

    let mut id_queue = VecDeque::new();
    for response in responses {
        let parsed: Option<SearchedObjects> = match response.0.status() {
            StatusCode::OK => {
                let parsed = response.0.json::<SearchedObjects>().await
                    .expect("failed parse");
                logger.info(format!("rec <- {}", response.1.as_ref()));
                caching.abs(CacheWrapper::new(response.1, Etag::new(&parsed.etag))).await;
                Some(parsed)
            },
            StatusCode::NOT_MODIFIED => {
                logger.info(format!("___ -- {}", response.1.as_ref()));
                None
            },
            StatusCode::TOO_MANY_REQUESTS => {
                logger.error("Resource Exhausted, Quota Limit exceeded!");
                panic!()
            },
            _ => {
                println!("{}", response.0.text().await.expect(""));
                unimplemented!("unknown error code.")
            }
        };
        id_queue.push_back(parsed);
    }

    logger.info("finished id search.");

    let response = id_queue.into_iter().flatten()
        .flat_map(|raw_object| raw_object.items.into_iter()
            .map(|item| item.id.video_id)
            .collect::<Vec<StringId<VideoInfo>>>())
        .collect::<VecDeque<StringId<VideoInfo>>>();

    for picked in 0..((response.len() / 5) + (if (response.len() % 5) > 0 { 1 } else { 0 } )) {
        let aggregate = response.iter()
            .skip(5 * picked).take(5)
            .map(|id| id.as_ref())
            .collect::<Vec<&str>>()
            .join(", ");
        logger.info(format!("({:<2}):: {}", picked + 1, aggregate));
    }

    let mut queue: VecDeque<String> = VecDeque::new();

    for picked in 0..(response.len() / 50 + (if (response.len() % 50) > 0 { 1 } else { 0 } )) {
        queue.push_back(response.clone().into_iter()
            .skip(50 * picked).take(50)
            .map(|id| id.breach_inner())
            .collect::<Vec<String>>()
            .join(","));
    }

    let mut response = VecDeque::new();

    logger.info("search details");
    for video_id in queue {
        let external = client.get("https://www.googleapis.com/youtube/v3/videos")
            .header(HeaderName::from_static("user-agent"), HeaderValue::from_static("Nekomata-salmon (retrieve for scheduled live of virtual liver. [https://github.com/ReiRokusanami0010/salmon])"))
            .query(&[("id", video_id.as_str()), ("part", "liveStreamingDetails, statistics, snippet"),
                ("fields", "(etag, items(id, snippet(title, description, channelTitle, channelId, publishedAt), statistics, liveStreamingDetails))"),
                ("key", get_api_key_param())])
            .send()
            .await
            .context(RequestError::HttpGet)?;
        let parsed: Option<SearchedVideoInfoObjects> = match external.status() {
            StatusCode::OK => {
                let parsed = external.json::<SearchedVideoInfoObjects>().await
                    .context(RequestError::DataParse)?;
                Some(parsed)
            },
            StatusCode::NOT_MODIFIED => None,
            StatusCode::TOO_MANY_REQUESTS => {
                logger.error("Resource Exhausted, Quota Limit exceeded!");
                panic!()
            },
            _ => {
                println!("{}", external.text().await.expect(""));
                unimplemented!("unknown error code.")
            }
        };
        response.push_back(parsed)
    }
    logger.info("finished detail search.");

    let aggregates = response.into_iter().flatten()
        .flat_map(|searched| searched.items)
        .collect::<HashSet<VideoInfo>>();

    Ok(aggregates)

}

pub(super) async fn channel_info_request(entry: &HashSet<LiverEntry>) -> anyhow::Result<HashSet<ChannelInfo>> {
    let logger = Logger::new(Some("search api"));
    let client = get_http_client();
    let caching: MiseryHandler<StringId<Channel>, Etag> = MiseryHandler::load_from_blocking("./.cache/ch_search_cache.json");
    let youtube_ext = entry.iter()
        .flat_map(|entity| entity.as_ref_site().iter()
            .flat_map(Channel::as_youtube_id)
            .collect::<Vec<StringId<Channel>>>())
        .collect::<Vec<StringId<Channel>>>();

    // notify: Line 63-65
    let responses = futures::stream::iter(youtube_ext)
        .map(|id| {
            let client = client;
            let caching = &caching;
            async move {
                let etag = caching.find_value(&id).await.unwrap_or_default();
                let res = client.get("https://www.googleapis.com/youtube/v3/channels")
                    .header(HeaderName::from_static("if-none-match"), HeaderValue::from_str(etag.as_ref()).expect(""))
                    .header(HeaderName::from_static("user-agent"), HeaderValue::from_static("Nekomata-salmon (retrieve for scheduled live of virtual liver. [https://github.com/ReiRokusanami0010/salmon])"))
                    .query(&[("id", id.as_ref()), ("part", "snippet,statistics"), ("fields", "(etag, items(id, (snippet(title, description, publishedAt, thumbnails(high(url))))))"), ("key", get_api_key_param())])
                    .send().await
                    .context(RequestError::HttpGet)
                    .expect("http_get");
                (res, id)
            }
        }).buffer_unordered(*get_process_concurrency())
        .collect::<VecDeque<_>>().await;

    let mut id_queue = VecDeque::new();
    for response in responses {
        let parsed: Option<ChannelInfoWithEtag> = match response.0.status() {
            StatusCode::OK => {
                let parsed = response.0.json::<ChannelInfoWithEtag>().await
                    .expect("failed parse");
                logger.info(format!("rec <- {}", response.1.as_ref()));
                caching.abs(CacheWrapper::new(response.1, Etag::new(&parsed.etag))).await;
                Some(parsed)
            },
            StatusCode::NOT_MODIFIED => {
                logger.info(format!("___ -- {}", response.1.as_ref()));
                None
            },
            StatusCode::TOO_MANY_REQUESTS => {
                logger.error("Resource Exhausted, Quota Limit exceeded!");
                panic!()
            },
            _ => {
                println!("{}", response.0.text().await.expect(""));
                unimplemented!("unknown error code.")
            }
        };
        id_queue.push_back(parsed);
    }

    let response = id_queue.into_iter().flatten()
        .flat_map(|raw_object| raw_object.separate_etag().1)
        .collect::<HashSet<ChannelInfo>>();

    Ok(response)
}

#[derive(Debug, thiserror::Error)]
enum RequestError {
    #[error("failed get http request.")]
    HttpGet,
    #[error("failed to load etag from cache.")]
    ETagLoad,
    #[error("cannot parse. this data structure is wrong.")]
    DataParse
}

impl VideoInfo {
    pub fn as_ref_id(&self) -> &StringId<VideoInfo> {
        &self.id
    }

    pub fn as_ref_snippet(&self) -> &VideoInfoSnippet {
        &self.snippet
    }

    pub fn as_ref_live_streaming_details(&self) -> &LiveStreamingDetails {
        &self.details
    }

    pub fn as_ref_title(&self) -> &str {
        &self.snippet.title.0
    }

    pub fn as_ref_description(&self) -> &str {
        &self.snippet.description.0
    }

    pub fn is_live_finished(&self) -> bool {
        self.details.actual_end_time.is_some()
            || self.details.scheduled_start_time.unwrap().timestamp() <= Local::now().timestamp()
    }

    pub fn is_too_long_span_live(&self) -> bool {
        if self.details.scheduled_start_time.is_none() {
            return false
        }
        (self.details.scheduled_start_time.unwrap().year() - self.snippet.published_at.year()) >= 1
    }
}

impl VideoInfoSnippet {
    pub fn as_ref_dependency_channel_id(&self) -> &StringId<Channel> {
        &self.channel_id
    }

    pub fn as_ref_published_at(&self) -> &DateTime<Local> {
        &self.published_at
    }
}

impl LiveStreamingDetails {
    pub fn as_ref_scheduled_start_time_optional(&self) -> &Option<DateTime<Local>> {
        &self.scheduled_start_time
    }

    pub fn as_ref_actual_start_time_optional(&self) -> &Option<DateTime<Local>> {
        &self.actual_start_time
    }

    pub fn as_ref_actual_end_time_optional(&self) -> &Option<DateTime<Local>> {
        &self.actual_end_time
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(transparent)]
pub struct Etag(String);

impl Etag {
    fn new<S>(etag: S) -> Etag where S: Into<String> {
        Self(etag.into())
    }

    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
struct SearchedObjects {
    etag: String,
    items: Vec<IdentifierWrapper>
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
struct IdentifierWrapper {
    id: Identifier,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
struct Identifier {
    #[serde(rename = "videoId")]
    video_id: StringId<VideoInfo>
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
struct SearchedVideoInfoObjects {
    etag: String,
    items: Vec<VideoInfo>
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct VideoInfo {
    id: StringId<VideoInfo>,
    snippet: VideoInfoSnippet,
    statistics: Statistics,
    #[serde(rename = "liveStreamingDetails")]
    details: LiveStreamingDetails
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct VideoInfoSnippet {
    #[serde(rename = "publishedAt")]
    published_at: DateTime<Local>,
    #[serde(rename = "channelId")]
    channel_id: StringId<Channel>,
    #[serde(deserialize_with = "must_string_contents")]
    title: TextComponent,
    #[serde(deserialize_with = "must_string_contents")]
    description: TextComponent,
    #[serde(rename = "channelTitle")]
    channel_title: String
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct Statistics {
    #[serde(default)]
    #[serde(with = "from_string")]
    #[serde(rename = "viewCount")]
    view_count: i64,
    #[serde(default)]
    #[serde(with = "from_string")]
    #[serde(rename = "likeCount")]
    like_count: i64,
    #[serde(default)]
    #[serde(with = "from_string")]
    #[serde(rename = "favoriteCount")]
    favorite_count: i64,
    #[serde(default)]
    #[serde(with = "from_string")]
    #[serde(rename = "commentCount")]
    comment_count: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct LiveStreamingDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "actualStartTime")]
    actual_start_time: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "actualEndTime")]
    actual_end_time: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "scheduledStartTime")]
    scheduled_start_time: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "activeLiveChatId")]
    active_live_chat_id: Option<String>
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct TextComponent(pub String);

impl FromStr for TextComponent {
    type Err = void::Void;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

pub(crate) fn must_string_contents<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where T: Deserialize<'de> + FromStr<Err = void::Void>,
          D: Deserializer<'de>
{
    struct MustStringContents<T>(PhantomData<fn() -> T>);
    impl<'de, T> Visitor<'de> for MustStringContents<T>
        where T: Deserialize<'de> + FromStr<Err = void::Void>
    {
        type Value = T;
        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("must string contents")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: Error
        {
            Ok(FromStr::from_str(v).unwrap())
        }
    }
    deserializer.deserialize_any(MustStringContents(PhantomData))
}

mod from_string {
    use std::fmt::Display;
    use std::str::FromStr;

    use serde::{de, Serializer, Deserialize, Deserializer};

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
        where T: Display,
              S: Serializer
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
        where T: FromStr,
              T::Err: Display,
              D: Deserializer<'de>
    {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
struct ChannelInfoWithEtag {
    etag: String,
    items: Vec<ChannelInfo>
}

#[allow(dead_code)]
impl ChannelInfoWithEtag {
    fn separate_etag(self) -> (String, Vec<ChannelInfo>) {
        (self.etag, self.items)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct ChannelInfo {
    id: StringId<ChannelInfo>,
    snippet: ChannelInfoSnippet
}

impl ChannelInfo {
    pub fn as_ref_id(&self) -> &StringId<ChannelInfo> {
        &self.id
    }

    pub fn as_ref_snippet(&self) -> &ChannelInfoSnippet {
        &self.snippet
    }

    pub fn breach_extraction_id(&self) -> StringId<ChannelInfo> {
        self.id.to_owned()
    }

    pub fn breach_extraction_snippet(&self) -> ChannelInfoSnippet {
        self.snippet.to_owned()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct ChannelInfoSnippet {
    title: String,
    description: String,
    #[serde(rename = "publishedAt")]
    published_at: DateTime<Local>,
    thumbnails: ChannelInfoThumbnail
}

impl ChannelInfoSnippet {
    pub fn as_ref_title(&self) -> &str {
        &self.title
    }

    pub fn as_ref_description(&self) -> &str {
        &self.description
    }

    pub fn as_ref_published_at(&self) -> &DateTime<Local> {
        &self.published_at
    }

    pub fn as_ref_thumbnail(&self) -> &str {
        &self.thumbnails.high.url
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
struct ChannelInfoThumbnail {
    high: HighRes
}

#[allow(dead_code)]
impl ChannelInfoThumbnail {
    fn remove_unnecessary_wrap(self) -> String {
        self.high.url
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
struct HighRes {
    url: String
}