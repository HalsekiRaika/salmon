use std::io::Write;
use std::path::Path;
use git2::{AnnotatedCommit, AutotagOption, FetchOptions, RemoteCallbacks, Repository, ResetType};
use crate::logger::Logger;

const DEFAULT_URL: &str = "https://github.com/ReiRokusanami0010/NekomataLibrary";
const DEFAULT_PATH: &str = "./.config";

fn checked_clone_or_open() -> Option<Repository> {
    let repository = dotenv::var("CONFIG_REPO")
        .ok()
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let download = dotenv::var("CONFIG_PATH")
        .ok()
        .unwrap_or_else(|| DEFAULT_PATH.to_string());
    if Path::new(DEFAULT_PATH).exists() && !Path::new(&format!("{}{}", DEFAULT_PATH, "/.git")).exists() {
        match Repository::clone(&repository, &download) {
            Ok(repo) => Some(repo),
            Err(reason) => panic!("{}", reason)
        }
    } else {
        match Repository::open(&download) {
            Ok(repo) => Some(repo),
            Err(reason) => panic!("{}", reason)
        }
    }
}

fn hard_reset(repo: &Repository) {
    repo.reset(&repo.revparse_single("HEAD").expect("cannot rev parse"), ResetType::Hard, None)
        .expect("cannot hard reset")
}

const REMOTE_NAME: &str = "origin";
const REMOTE_BRANCH: &str = "master";
const REFERENCE_NAME: &str = "FETCH_HEAD";

fn fetch_latest_contents(config_repo: &Repository) -> AnnotatedCommit {
    let mut fetch_callback = RemoteCallbacks::new();
    fetch_callback.transfer_progress(|status| {
        let logger = Logger::new(Some("transfer"));
        if status.received_objects() == status.total_objects() {
            logger.info(format!("Resolving deltas {}/{}", status.indexed_deltas(), status.total_deltas()));
        } else if status.total_objects() > 0 {
            logger.info(format!("Received {}/{} objects ({}) in {} bytes",
                status.received_objects(), status.total_objects(), status.indexed_objects(), status.received_bytes()));
        }
        std::io::stdout().flush().unwrap();
        true
    });

    let mut fetch_option = FetchOptions::new();
    fetch_option.remote_callbacks(fetch_callback);
    fetch_option.download_tags(AutotagOption::All);

    let mut remote = config_repo.find_remote(REMOTE_NAME)
        .expect("Cannot find remote.");
    let logger = Logger::new(Some("fetch"));
    logger.info(format!("Fetching {}", remote.name().unwrap()));
    remote.fetch(&[REMOTE_BRANCH], Some(&mut fetch_option), None)
        .expect("Cannot fetch from remote.");

    let status = remote.stats();
    if status.local_objects() > 0 {
        logger.info(format!("Received {}/{} objects in {} bytes (used {} local \\ objects)",
            status.indexed_objects(), status.total_objects(), status.received_bytes(), status.local_objects()));
    } else {
        logger.info(format!("Received {}/{} objects in {} bytes",
            status.indexed_objects(), status.total_objects(), status.received_bytes()));
    }

    let remote_head = config_repo.find_reference(REFERENCE_NAME)
        .expect("Cannot find reference.");

    config_repo.reference_to_annotated_commit(&remote_head)
        .expect("head_reference cannot sublimate annotated_commit.")
}

const HEAD: &str = "HEAD";

#[allow(unused_must_use)]
fn merge(config_repo: &Repository, local: &AnnotatedCommit, remote: &AnnotatedCommit) {
    let logger = Logger::new(Some("merge"));

    let local_tree = config_repo.find_commit(local.id())
        .expect("cannot find local commit.").tree()
        .expect("cannot get local tree.");
    let remote_tree = config_repo.find_commit(remote.id())
        .expect("cannot find remote commit.").tree()
        .expect("cannot get remote tree");
    let predecessor = config_repo.find_commit(config_repo.merge_base(local.id(), remote.id())
        .expect("cannot merge")).expect("cannot find predecessor commit.").tree()
        .expect("cannot get predecessor tree");

    let mut index = config_repo.merge_trees(&predecessor, &local_tree, &remote_tree, None)
        .expect("cannot merge tree.");

    if index.has_conflicts() {
        logger.caut("conflict detected.");
        config_repo.checkout_index(Some(&mut index), None);
        return;
    }

    let generated = config_repo.find_tree(index.write_tree_to(config_repo).expect("cannot write tree."))
        .expect("not found generated tree.");

    let msg = format!("Merge: {} into {}", remote.id(), local.id());
    let signature = config_repo.signature()
        .expect("cannot get repository signature.");
    let local_commit = config_repo.find_commit(local.id())
        .expect("not found local commit.");
    let remote_commit = config_repo.find_commit(remote.id())
        .expect("not found remote commit.");

    let _merge = config_repo.commit(Some(HEAD), &signature, &signature, &msg, &generated, &[&local_commit, &remote_commit])
        .expect("cannot merge commit.");

    config_repo.checkout_head(None)
        .expect("cannot checkout head.");

    logger.info("Merge pull successful.")
}

fn update(config_repo: &Repository, remote_head: AnnotatedCommit) {
    let analysis = config_repo.merge_analysis(&[&remote_head])
        .expect("cannot analysis");
    let logger = Logger::new(Some("update"));
    if analysis.0.is_fast_forward() {
        logger.info("fast forward does not impl... X/");
    } else if analysis.0.is_normal() {
        let head = config_repo.head().expect("cannot get local head");
        let local_head = config_repo.reference_to_annotated_commit(&head)
            .expect("cannot get local head");
        merge(config_repo, &local_head, &remote_head);
    } else {
        logger.info("no-op ;3");
    }
}

pub fn setup_config_repository() {
    let config_repo = checked_clone_or_open()
        .expect("cannot open.");
    hard_reset(&config_repo);
    let latest = fetch_latest_contents(&config_repo);
    update(&config_repo, latest)
}