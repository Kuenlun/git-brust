/*!
git-brust - Rust CLI tool to visualize git branch flows
Copyright (C) 2025  Juan Luis Leal Contreras (Kuenlun)

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use git2::{Branch, Repository};
use log::info;
use std::path::Path;

use crate::git;
use crate::models::GitBrustError;

/// Assert that two paths are equal after canonicalization.
fn assert_same_path(p1: &Path, p2: &Path) {
    let err_msg = "failed to canonicalize";
    let c1 = p1.canonicalize().expect(err_msg);
    let c2 = p2.canonicalize().expect(err_msg);
    assert_eq!(c1, c2);
}

/// Initialize a repository and assert its working_dir matches the given path
fn init_repo_assert_path_matches(path: &Path) -> Repository {
    let repo = Repository::init(path).expect("failed to init repo");
    assert_same_path(repo.workdir().unwrap(), path);
    repo
}

/// Create a temporary Git repository and return the TempDir and Repository handle
pub fn init_repo_in_tempdir() -> (tempfile::TempDir, Repository) {
    let tmp = tempfile::tempdir().expect("failed to create tempdir");
    let repo = init_repo_assert_path_matches(tmp.path());
    (tmp, repo)
}

pub fn add_commit_to_repo<'repo>(repo: &'repo Repository) -> Result<(), GitBrustError> {
    // Create an empty tree
    let tree_id = {
        let mut index = repo.index()?;
        index.write_tree()?
    };
    let tree = repo.find_tree(tree_id)?;

    // Create a signature
    let sig = git2::Signature::now("Test User", "test@example.com")?;

    // Create the empty commit
    let commit_id = repo.commit(
        Some("HEAD"),   // update HEAD
        &sig,           // author
        &sig,           // committer
        "Empty commit", // commit message
        &tree,          // tree
        &[],            // parents
    )?;
    info!("Empty commit created: {}", commit_id);
    Ok(())
}

pub fn get_local_repo_branches<'repo>(
    repo: &'repo Repository,
) -> Result<Vec<Branch<'repo>>, GitBrustError> {
    let branches: Vec<Branch> = repo
        .branches(None)?
        .map(|res| res.map(|(branch, _)| branch))
        .collect::<Result<Vec<_>, _>>()?;
    info!(
        "Local Branches: {}",
        git::names_from_branches(&branches)?.join(", ")
    );
    Ok(branches)
}
