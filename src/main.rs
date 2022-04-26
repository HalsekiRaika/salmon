#![forbid(unsafe_code, unsafe_op_in_unsafe_fn)]
extern crate serde;

mod logger;
mod repository;
mod ids;
mod entry;
mod models;

#[tokio::main]
async fn main() {
    repository::setup_config_repository();
    entry::channel_info_request_handler().await.expect("");
    entry::upcoming_live_request_handler().await.expect("");
}