#![allow(dead_code)]

use std::io::Write;
use std::path::Path;
use anyhow::{Result, Context};
use git2::{AnnotatedCommit, AutotagOption, FetchOptions, RemoteCallbacks, Repository, ResetType};
use crate::logger::Logger;

const DEFAULT_URL: &str = "https://github.com/ReiRokusanami0010/NekomataLibrary";
const DEFAULT_PATH: &str = "./.config";

fn get_open_or_clone() -> Repository {
    let repository = dotenv::var("CONFIG_REPO")
        .ok()
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let download = dotenv::var("CONFIG_PATH")
        .ok()
        .unwrap_or_else(|| DEFAULT_PATH.to_string());
    if Path::new(DEFAULT_PATH).exists() && !Path::new(&format!("{}{}", DEFAULT_PATH, "/.git")).exists() {
        match Repository::clone(&repository, &download) {
            Ok(repo) => repo,
            Err(reason) => panic!("{}", reason)
        }
    } else {
        match Repository::open(&download) {
            Ok(repo) => repo,
            Err(reason) => panic!("{}", reason)
        }
    }
}

fn hard_reset(repo: &Repository) -> Result<()> {
    repo.reset(&repo.revparse_single("HEAD").context(RepositoryManagementError::RevisionParse)?, ResetType::Hard, None)
        .context(RepositoryManagementError::HardReset)
}

const REMOTE_NAME: &str = "origin";
const REMOTE_BRANCH: &str = "master";
const REFERENCE_NAME: &str = "FETCH_HEAD";

fn fetch_latest_contents(config_repo: &Repository) -> Result<AnnotatedCommit> {
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
        .context(RepositoryManagementError::CommitFind("remote"))?;

    let logger = Logger::new(Some("fetch"));

    logger.info(format!("Fetching {}", remote.name().unwrap()));

    remote.fetch(&[REMOTE_BRANCH], Some(&mut fetch_option), None)
        .context(RepositoryManagementError::Fetch)?;

    let status = remote.stats();
    if status.local_objects() > 0 {
        logger.info(format!("Received {}/{} objects in {} bytes (used {} local \\ objects)",
            status.indexed_objects(), status.total_objects(), status.received_bytes(), status.local_objects()));
    } else {
        logger.info(format!("Received {}/{} objects in {} bytes",
            status.indexed_objects(), status.total_objects(), status.received_bytes()));
    }

    let remote_head = config_repo.find_reference(REFERENCE_NAME)
        .context(RepositoryManagementError::ReferenceFind)?;

    config_repo.reference_to_annotated_commit(&remote_head)
        .context(RepositoryManagementError::Sublimate("head_reference"))
}

const HEAD: &str = "HEAD";

fn merge(config_repo: &Repository, local: &AnnotatedCommit, remote: &AnnotatedCommit) -> Result<()> {
    let logger = Logger::new(Some("merge"));

    let local_tree = config_repo.find_commit(local.id())
        .context(RepositoryManagementError::CommitFind("local"))?.tree()
        .context(RepositoryManagementError::TreeGet("local"))?;
    let remote_tree = config_repo.find_commit(remote.id())
        .context(RepositoryManagementError::CommitFind("remote"))?.tree()
        .context(RepositoryManagementError::TreeGet("remote"))?;
    let predecessor = config_repo.find_commit(config_repo.merge_base(local.id(), remote.id())
        .context(RepositoryManagementError::Merge(98))?)
        .context(RepositoryManagementError::CommitFind("predecessor"))?.tree()
        .context(RepositoryManagementError::TreeGet("predecessor"))?;

    let mut index = config_repo.merge_trees(&predecessor, &local_tree, &remote_tree, None)
        .context(RepositoryManagementError::Merge(103))?;

    if index.has_conflicts() {
        logger.caut("conflict detected.");
        #[allow(unused_must_use)]
        config_repo.checkout_index(Some(&mut index), None)
            .context(RepositoryManagementError::Checkout)?;
        return Ok(());
    }

    let generated = config_repo.find_tree(index.write_tree_to(config_repo).expect("cannot write tree."))
        .context(RepositoryManagementError::TreeGenerate)?;

    let msg = format!("Merge: {} into {}", remote.id(), local.id());
    let signature = config_repo.signature()
        .context(RepositoryManagementError::SignatureGet)?;
    let local_commit = config_repo.find_commit(local.id())
        .context(RepositoryManagementError::CommitFind("local"))?;
    let remote_commit = config_repo.find_commit(remote.id())
        .context(RepositoryManagementError::CommitFind("remote"))?;

    let _merge = config_repo.commit(Some(HEAD), &signature, &signature, &msg, &generated, &[&local_commit, &remote_commit])
        .context(RepositoryManagementError::Merge(123))?;

    config_repo.checkout_head(None)
        .context(RepositoryManagementError::Checkout)?;

    logger.info("Merge pull successful.");

    Ok(())
}

fn update(config_repo: &Repository, remote_head: AnnotatedCommit) -> Result<()> {
    let analysis = config_repo.merge_analysis(&[&remote_head])
        .context(RepositoryManagementError::Analysis)?;
    let logger = Logger::new(Some("update"));
    if analysis.0.is_fast_forward() {
        logger.error("fast forward does not impl... X/");
        logger.error("please reset or remake dir config directory.");
        unimplemented!()
    } else if analysis.0.is_normal() {
        let head = config_repo.head().expect("cannot get local head");
        let local_head = config_repo.reference_to_annotated_commit(&head)
            .context(RepositoryManagementError::HeadGetFail("local"))?;
        merge(config_repo, &local_head, &remote_head)
            .context(RepositoryManagementError::Merge(145))?;
    } else {
        logger.info("no-op ;3");
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryManagementError {
    #[error("failed hard reset.")]
    HardReset,
    #[error("cannot find {} commit.", .0)]
    CommitFind(&'static str),
    #[error("cannot get {} tree.", .0)]
    TreeWrite(&'static str),
    #[error("cannot get {} tree.", .0)]
    TreeGet(&'static str),
    #[error("cannot generate tree.")]
    TreeGenerate,
    #[error("cannot analysis repository.")]
    Analysis,
    #[error("cannot merge. from line: {}", .0)]
    Merge(usize),
    #[error("cannot get")]
    SignatureGet,
    #[error("cannot check out.")]
    Checkout,
    #[error("cannot fetch.")]
    Fetch,
    #[error("cannot find reference")]
    ReferenceFind,
    #[error("{} cannot sublimate annotated commit.", .0)]
    Sublimate(&'static str),
    #[error("cannot get {} head", .0)]
    HeadGetFail(&'static str),
    #[error("cannot rev parse")]
    RevisionParse,
}

pub fn setup_config_repository() {
    let config_repo = get_open_or_clone();
    hard_reset(&config_repo)
        .expect("Failed hard reset to repository.");
    let latest = fetch_latest_contents(&config_repo)
        .expect("Failed fetch latest.");
    update(&config_repo, latest)
        .expect("Failed update repository.");
}