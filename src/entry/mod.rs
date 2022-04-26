mod cache;
mod request;
pub(crate) mod transport;

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;
use async_std::task::block_on;
use futures::StreamExt;
use once_cell::sync::OnceCell;
use regex::Regex;
use walkdir::{DirEntry, WalkDir};
use crate::entry::request::{channel_info_request, RequestEntry, VideoInfo};
use crate::entry::transport::{Applier, salmon};
use crate::entry::transport::salmon::{Affiliation, Liver};
use crate::logger::Logger;
use crate::models::{AffiliationEntry, Channel, LiverEntry};

fn get_regex_for_ignored() -> &'static Regex {
    static REGEX: OnceCell<Regex> = OnceCell::new();
    REGEX.get_or_init(|| {
        Regex::new("(([Ff])([Rr])(([Ee])+)([Cc])([Hh])([Aa])([Tt]))|(([Ff])([Rr])(([Ee])+)\\s([Cc])([Hh])([Aa])([Tt]))|(([ふフﾌ])([りリﾘ])((ー)+)([ちチﾁ])([ゃャｬ])([っッｯ])([とトﾄ]))").unwrap()
    })
}

fn is_ignored_file(entry: &DirEntry, ignore: impl Into<String>) -> bool {
    entry.file_name().to_str()
        .map(|name| name.ends_with(&ignore.into()))
        .unwrap_or(false)
}

fn is_json(entry: &DirEntry) -> bool {
    entry.file_name().to_str()
        .map(|name| name.ends_with(".json"))
        .unwrap_or(false)
}

pub fn get_or_init_config() -> &'static HashMap<AffiliationEntry, HashSet<LiverEntry>> {
    static LOCKED: OnceCell<HashMap<AffiliationEntry, HashSet<LiverEntry>>> = OnceCell::new();
    LOCKED.get_or_init(|| {
        let logger = Logger::new(Some("Init Lock"));
        logger.debug("Initialize >>>");
        let total = Instant::now();
        let path = dotenv::var("CONFIG_PATH")
            .unwrap_or_else(|_| String::from("./.config"));
        let mut maps: HashMap<AffiliationEntry, HashSet<LiverEntry>> = HashMap::new();
        AffiliationEntry::load_from(format!("{}/affiliation.json", &path))
            .expect("not found affiliation config").iter()
            .for_each(|affiliation| {
                logger.debug(format!("Loading << {}", affiliation.as_ref_name()));
                let timer = Instant::now();
                let mut lives = HashSet::new();
                WalkDir::new(format!("{}/{}", path, affiliation.as_ref_name())).into_iter()
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| is_json(entry) && !is_ignored_file(entry, affiliation.as_ref_name()))
                    .for_each(|liver| {
                        let item = LiverEntry::load_from(liver.path())
                            .expect("failed load liver config");
                        logger.debug(format!(" + {}", item.as_ref_id().as_ref()));
                        lives.insert(item);
                    });
                logger.debug(format!("Loaded affiliation:{}/livers:{}", affiliation.as_ref_name(), lives.len()));
                maps.insert(affiliation.to_owned(), lives);
                logger.debug(format!("Finished >> {}ms", timer.elapsed().as_millis()));
            });
        logger.debug("Start send base data to API Server >>");
        let timer = Instant::now();
        let mut client = block_on(transport::build_client())
            .expect("build_grpc_client");
        let client = &mut client;
        logger.debug("client built");
        match block_on(client.insert_req_affiliation(tonic::Request::new(futures::stream::iter(maps.iter()
            .map(|(aff, _)| Affiliation::from(aff.to_owned())).collect::<Vec<_>>())))) {
            Ok(_) => logger.debug("affiliation base info finished."),
            Err(reason) => logger.error(format!("failed task: {}", reason))
        };

        match block_on(client.insert_req_v_tuber(tonic::Request::new(futures::stream::iter(maps.iter()
            .flat_map(|(aff, livers)| livers.iter().map(|base| Liver::from(base.to_owned()).apply(aff))).collect::<Vec<_>>())))) {
            Ok(_) => logger.debug("liver base info finished."),
            Err(reason) => logger.error(format!("failed task: {}", reason))
        };
        logger.debug(format!("finished << {}sec", timer.elapsed().as_secs_f32()));
        logger.debug(format!("Total elapsed <<< {}sec", total.elapsed().as_secs_f32()));
        maps
    })
}

pub async fn channel_info_request_handler() -> anyhow::Result<()> {
    let logger = Logger::new(Some("Request"));
    let total = Instant::now();

    futures::stream::iter(get_or_init_config().iter()).for_each(|(aff, liver)| async move {
        let logger = Logger::new(Some("Request"));
        let mut client = transport::build_client().await
            .expect("build_grpc_client");
        let client = &mut client;

        logger.info(format!("Liver Info Retrieve {}", aff.as_ref_name()));
        let infos = channel_info_request(liver).await
            .expect("channel_info_request");

        for info in &infos {
            println!("{}", info.as_ref_snippet().as_ref_title());
        }

        let send = infos.iter()
            .map(|info| salmon::Channel::from(info.to_owned()))
            .collect::<VecDeque<_>>();
        let applied = send.iter().map(|trans| trans.to_owned().apply(
            liver.iter()
                .find(|person| person.as_ref_site().iter()
                    .flat_map(Channel::as_youtube_id)
                    .any(|id| id.as_ref() == trans.channel_id))
                .unwrap()))
            .collect::<Vec<_>>();

        let stream_req = tonic::Request::new(futures::stream::iter(applied));
        match client.clone().insert_req_channel(stream_req).await {
            Ok(_) => (),
            Err(reason) => println!("{}", reason)
        };
    }).await;
    logger.info(format!("Total elapsed >>> {}sec", total.elapsed().as_secs_f32()));
    Ok(())
}

pub async fn upcoming_live_request_handler() -> anyhow::Result<()> {
    let logger = Logger::new(Some("Request"));
    let total = Instant::now();

    futures::stream::iter(get_or_init_config().iter()).for_each(|(aff, liver)| async move {
        let logger = Logger::new(Some("Request"));
        let mut client = transport::build_client().await
            .expect("build_grpc_client");
        let client = &mut client;
        let request = Instant::now();
        logger.info(format!("Request << {}", aff.as_ref_name()));
        let video_infos = RequestEntry::new(liver).request_video_info_concurrency().await
            .expect("failed req.").iter()
            .filter(|video| !video.is_live_finished())
            .filter(|video| !video.is_too_long_span_live())
            .filter(|video| !get_regex_for_ignored().is_match(video.as_ref_title()))
            .map(|video| video.to_owned())
            .collect::<VecDeque<VideoInfo>>();
        logger.info(format!("Finished {} >> {}sec", aff.as_ref_name(), request.elapsed().as_secs_f32()));
        let send = video_infos.iter().map(|info| salmon::Live::from(info.to_owned())).collect::<Vec<_>>();
        let stream_req = tonic::Request::new(futures::stream::iter(send));
        match client.clone().insert_req_live(stream_req).await {
            Ok(_) => (),
            Err(reason) => println!("{}", reason)
        };
    }).await;
    logger.info(format!("Total elapsed >>> {}sec", total.elapsed().as_secs_f32()));
    Ok(())
}

#[allow(dead_code)]
pub async fn send_handler() -> anyhow::Result<()> {

    Ok(())
}
