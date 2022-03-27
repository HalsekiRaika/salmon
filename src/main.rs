extern crate serde;

mod logger;
mod repository;
mod ids;
mod entry;

fn main() {
    repository::setup_config_repository();
}