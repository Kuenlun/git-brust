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

use git2::{Branch, Oid, Repository, Signature};
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

/// Create a temporary Git repository and return the Repository and TempDir
pub fn init_empty_repo_in_tempdir() -> (Repository, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("failed to create tempdir");
    let repo = init_repo_assert_path_matches(tmp.path());
    (repo, tmp)
}

pub fn init_repo_in_tempdir() -> (Repository, tempfile::TempDir) {
    let (repo, tmp) = init_empty_repo_in_tempdir();

    let tree_id = {
        let mut index = repo.index().unwrap();
        index.write_tree().unwrap()
    };
    {
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();
    }

    (repo, tmp)
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

/// Creates a new branch with the specified name
pub fn create_branch<'repo>(
    repo: &'repo Repository,
    branch_name: &str,
) -> Result<(), GitBrustError> {
    // Get the current commit from HEAD
    let head_commit = repo.head()?.peel_to_commit()?;

    // Create the new branch pointing to the current commit
    repo.branch(branch_name, &head_commit, false)?;

    info!("Branch '{}' created successfully", branch_name);
    Ok(())
}

/// Creates an empty commit on the specified branch
pub fn add_commit_to_branch<'repo>(
    repo: &'repo Repository,
    branch_name: &str,
) -> Result<Oid, GitBrustError> {
    // Switch to the specified branch
    let branch_ref_name = format!("refs/heads/{}", branch_name);

    // Verify that the branch exists
    let branch_ref = repo.find_reference(&branch_ref_name)?;
    let branch_commit = branch_ref.peel_to_commit()?;

    // Create an empty tree
    let tree_id = {
        let mut index = repo.index()?;
        // If we want the commit to have the same content as the parent commit
        // we can load the parent commit's tree into the index:
        // index.read_tree(&branch_commit.tree()?)?;
        index.write_tree()?
    };
    let tree = repo.find_tree(tree_id)?;

    // Create a signature
    let sig = Signature::now("Test User", "test@example.com")?;

    // Create the commit on the specified branch
    let commit_id = repo.commit(
        Some(&branch_ref_name), // Update the branch reference
        &sig,                   // author
        &sig,                   // committer
        "Empty commit",         // commit message
        &tree,                  // tree
        &[&branch_commit],      // parents - the previous commit on the branch
    )?;

    info!(
        "Empty commit created in branch '{}': {}",
        branch_name, commit_id
    );
    Ok(commit_id)
}

/// Convenience function that combines creating a branch and making a commit in it
pub fn create_branch_and_commit<'repo>(
    repo: &'repo Repository,
    branch_name: &str,
) -> Result<Oid, GitBrustError> {
    // Create the branch
    create_branch(repo, branch_name)?;

    // Make a commit on the new branch
    add_commit_to_branch(repo, branch_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_branch() {
        let (repo, _temp) = init_repo_in_tempdir();

        let result = create_branch(&repo, "test-branch");
        assert!(result.is_ok());

        // Verify that the branch was created
        let branch = repo.find_branch("test-branch", git2::BranchType::Local);
        assert!(branch.is_ok());
    }

    #[test]
    fn test_add_commit_to_branch() {
        let (repo, _temp) = init_repo_in_tempdir();

        // First create a branch
        create_branch(&repo, "test-branch").unwrap();

        // Then make a commit on that branch
        let result = add_commit_to_branch(&repo, "test-branch");
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_branch_and_commit() {
        let (repo, _temp) = init_repo_in_tempdir();

        let result = create_branch_and_commit(&repo, "new-branch");
        assert!(result.is_ok());

        // Verify that both the branch and commit were created
        let branch = repo.find_branch("new-branch", git2::BranchType::Local);
        assert!(branch.is_ok());
    }
}
