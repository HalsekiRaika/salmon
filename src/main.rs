extern crate serde;

mod logger;
mod repository;
mod ids;
mod entry;

#[tokio::main]
async fn main() {
    repository::setup_config_repository();
    entry::resolve().await.expect("")
}