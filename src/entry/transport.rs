#![allow(dead_code)]

use std::time::SystemTime;
use anyhow::Context;
use serde::{Serialize, Deserialize};
use chrono::Local;
use once_cell::sync::OnceCell;
use tonic::transport::Channel as GrpcChannel;

use crate::entry::request::{ChannelInfo, VideoInfo};
use crate::entry::transport::salmon::{Affiliation, Liver, Channel, Live};
use crate::entry::transport::salmon::salmon_api_client::SalmonApiClient;
use crate::models::{AffiliationEntry, LiverEntry};

pub mod salmon {
    tonic::include_proto!("salmon");
}

fn get_server_ip() -> &'static str {
    static ADDRESS: OnceCell<String> = OnceCell::new();
    ADDRESS.get_or_init(|| {
        dotenv::var("MATATABI_SERVER")
            .unwrap_or_else(|_| String::from("http://[::1]:50051"))
    })
}

pub async fn build_client() -> anyhow::Result<SalmonApiClient<GrpcChannel>> {
    SalmonApiClient::connect(get_server_ip()).await
        .context(GrpcError::ConnectionEstablished(get_server_ip()))
}

#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    #[error("cannot establish connection to {}", .0)]
    ConnectionEstablished(&'static str)
}

impl From<AffiliationEntry> for Affiliation {
    fn from(base: AffiliationEntry) -> Self {
        Affiliation {
            affiliation_id: base.as_ref_id().breach_extract(),
            name: base.breach_extraction_name(),
            override_at: UpdateSignature::default().as_i64()
        }
    }
}

impl From<LiverEntry> for Liver {
    fn from(base: LiverEntry) -> Self {
        Liver {
            liver_id: base.breach_extraction_id().breach_extract(),
            name: base.breach_extraction_name(),
            affiliation_id: None,
            override_at: UpdateSignature::default().as_i64()
        }
    }
}

impl From<crate::entry::request::ChannelInfo> for Channel {
    fn from(base: ChannelInfo) -> Self {
        Self {
            channel_id: base.as_ref_id().as_ref().to_string(),
            liver_id: None,
            published_at: Some(::prost_types::Timestamp::from(SystemTime::from(*base.as_ref_snippet().as_ref_published_at()))),
            description: base.as_ref_snippet().as_ref_description().to_string(),
            logo_url: base.as_ref_snippet().as_ref_thumbnail().to_string(),
            override_at: UpdateSignature::default().as_i64()
        }
    }
}

impl From<crate::entry::request::VideoInfo> for Live {
    fn from(base: VideoInfo) -> Self {
        Self {
            video_id: base.as_ref_id().to_owned().as_ref().to_string(),
            channel_id: Some(base.as_ref_snippet().as_ref_dependency_channel_id().as_ref().to_string()),
            title: base.as_ref_title().to_string(),
            description: base.as_ref_description().to_string(),
            published_at: Some(::prost_types::Timestamp::from(SystemTime::from(*base.as_ref_snippet().as_ref_published_at()))),
            updated_at: Some(::prost_types::Timestamp::from(SystemTime::now())),
            will_start_at: base.as_ref_live_streaming_details().as_ref_scheduled_start_time_optional().map(|b| ::prost_types::Timestamp::from(SystemTime::from(b))),
            started_at: base.as_ref_live_streaming_details().as_ref_actual_start_time_optional().map(|b| ::prost_types::Timestamp::from(SystemTime::from(b))),
            override_at: UpdateSignature::default().as_i64()
        }
    }
}

impl Applier<AffiliationEntry> for Liver {
    fn apply(mut self, apply: &AffiliationEntry) -> Self {
        self.affiliation_id = Some(apply.breach_extraction_id().breach_extract());
        self
    }
}

impl Applier<LiverEntry> for Channel {
    fn apply(mut self, apply: &LiverEntry) -> Self {
        self.liver_id = Some(apply.breach_extraction_id().breach_extract());
        self
    }
}

pub trait Applier<A> {
    fn apply(self, apply: &A) -> Self;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateSignature(pub i64);

impl UpdateSignature {
    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl Default for UpdateSignature {
    fn default() -> Self {
        let date = Local::now();
        let a: i64 = date.format("%Y%m%d%H%M")
            .to_string()
            .parse()
            .unwrap();
        Self(a)
    }
}

impl Affiliation {
    pub fn del_sign(mut self) -> Self {
        self.override_at = -1;
        self
    }
}

impl Liver {
    pub fn del_sign(mut self) -> Self {
        self.override_at = -1;
        self
    }
}

impl Channel {
    pub fn del_sign(mut self) -> Self {
        self.override_at = -1;
        self
    }
}

impl Live {
    pub fn del_sign(mut self) -> Self {
        self.override_at = -1;
        self
    }
}