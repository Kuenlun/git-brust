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

use git2::{Branch, BranchType, Commit, Repository};

use crate::models::GitBrustError;

use log::{debug, info, trace};

/// Returns the local branch with the highest number of merge commits in its history.
///
/// This function iterates through all local branches and counts merge commits
/// (commits with more than one parent) to determine which branch has been
/// the target of the most merges.
pub fn get_branch_with_more_merges<'repo>(
    repo: &'repo Repository,
) -> Result<Option<Branch<'repo>>, GitBrustError> {
    debug!("Starting analysis to find branch with most merge commits");
    let branches = repo.branches(Some(BranchType::Local))?;
    let mut selected_branch: Option<Branch> = None;
    let mut max_merges = 0;
    let mut branches_analyzed = 0;

    // Iterate through all local branches to find the one with most merge commits
    for branch_result in branches {
        let (branch, _branch_type) = branch_result?;
        let branch_name = branch.name()?.unwrap_or("<unnamed>");
        let merge_count = count_merge_commits_in_branch(repo, &branch)?;
        branches_analyzed += 1;

        // Format logging with fixed width for better alignment
        trace!("{:>4} <-> '{}'", merge_count, branch_name);

        // Update selected branch if this one has more merges or if it's the first valid branch
        if selected_branch.is_none() || merge_count > max_merges {
            selected_branch = Some(branch);
            max_merges = merge_count;
        }
    }

    if let Some(_) = selected_branch {
        info!(
            "Analysis complete: {} branches analyzed, winner has {} merges",
            branches_analyzed, max_merges
        );
    } else {
        info!("No branches found in repository");
    }

    Ok(selected_branch)
}

/// Counts the number of merge commits in a branch's history.
///
/// A merge commit is defined as a commit with more than one parent.
/// This function uses revwalk for efficient traversal of the commit history.
fn count_merge_commits_in_branch(
    repo: &Repository,
    branch: &Branch,
) -> Result<usize, GitBrustError> {
    let commit = branch.get().peel_to_commit()?;

    // Use revwalk for more efficient traversal
    let mut revwalk = repo.revwalk()?;
    revwalk.push(commit.id())?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;

    let mut merge_count = 0;

    // Traverse commits using revwalk (more efficient than manual parent following)
    for commit_id in revwalk {
        let commit_id = commit_id?;
        let commit = repo.find_commit(commit_id)?;

        // Check if current commit is a merge (has multiple parents)
        if commit.parent_count() > 1 {
            merge_count += 1;
        }
    }

    Ok(merge_count)
}

/// Attempts to find a branch by name in the given repository
/// Checks for a local branch first and only looks for a remote branch if the local one is not found
/// Returns the branch if found or an error if it does not exist in either location
fn branch_from_name<'repo>(
    repo: &'repo Repository,
    branch_name: &str,
) -> Result<Branch<'repo>, GitBrustError> {
    [BranchType::Local, BranchType::Remote]
        .iter()
        .find_map(|&branch_type| repo.find_branch(branch_name, branch_type).ok())
        .ok_or_else(|| GitBrustError::BranchNotFound(branch_name.into()))
}

/// Converts a list of branch names into their corresponding Branch objects
/// Checks each branch locally first and remotely only if it is not found locally
/// Returns an error if any branch in the list does not exist
pub fn branches_from_names<'repo, S: AsRef<str>>(
    repo: &'repo Repository,
    branch_names: &[S],
) -> Result<Vec<Branch<'repo>>, GitBrustError> {
    branch_names
        .iter()
        .map(|name| branch_from_name(repo, name.as_ref()))
        .collect()
}

/// Returns the name of a branch as a &str
/// Returns an error if the branch has no valid name
pub fn name_from_branch<'repo>(branch: &'repo Branch) -> Result<&'repo str, GitBrustError> {
    branch.name()?.ok_or(GitBrustError::BranchNameInvalid)
}

/// Converts a list of branches into their names
/// Returns an error if any branch has no valid name
pub fn names_from_branches<'repo>(
    branches: &'repo [Branch],
) -> Result<Vec<&'repo str>, GitBrustError> {
    branches.iter().map(|b| name_from_branch(b)).collect()
}

/// Get the short commit id as `String`.
/// Falls back to `"INVALID"` if UTF-8 conversion fails.
pub fn commit_short_id(commit: &Commit) -> Result<String, GitBrustError> {
    Ok(commit
        .as_object()
        .short_id()?
        .as_str()
        .ok_or(GitBrustError::BranchNameInvalid)?
        .to_string())
}

/// Resolves a branch to its current commit
pub fn resolve_branch_to_commit<'repo>(
    branch: &'repo Branch,
) -> Result<Commit<'repo>, git2::Error> {
    branch.get().peel_to_commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models;
    use crate::test_utils;
    use git2::{Signature, Time};

    #[test]
    fn test_get_branch_with_more_merges_empty_repo() -> Result<(), GitBrustError> {
        models::init_logger();
        let (repo, _tmp) = test_utils::init_empty_repo_in_tempdir();

        let result = get_branch_with_more_merges(&repo)?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_selects_branch_with_most_merges() -> Result<(), GitBrustError> {
        models::init_logger();

        let (repo, _tmp) = test_utils::init_empty_repo_in_tempdir();

        // Setup: Create initial commit and tree
        let signature = Signature::new("Test User", "test@example.com", &Time::new(0, 0))?;
        let tree_id = {
            let mut index = repo.index()?;
            index.write_tree()?
        };
        let tree = repo.find_tree(tree_id)?;

        let initial_commit = repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Initial commit",
            &tree,
            &[],
        )?;

        // Create feature branch from initial commit
        let initial_commit_obj = repo.find_commit(initial_commit)?;
        let _feature_branch = repo.branch("feature", &initial_commit_obj, false)?;

        // Switch to feature and create commits
        repo.set_head("refs/heads/feature")?;
        let mut last_commit = initial_commit_obj;
        for i in 1..=3 {
            let commit_id = repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &format!("Feature commit {}", i),
                &tree,
                &[&last_commit],
            )?;
            last_commit = repo.find_commit(commit_id)?;
        }

        // Switch back to master and create merge commits
        repo.set_head("refs/heads/master")?;
        let master_commit = repo.find_commit(initial_commit)?;

        // First merge commit
        let merge_commit_id = repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Merge feature branch",
            &tree,
            &[&master_commit, &last_commit], // Two parents = merge commit
        )?;

        // Second merge commit
        let merge_commit = repo.find_commit(merge_commit_id)?;
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Another merge",
            &tree,
            &[&merge_commit, &last_commit], // Another merge
        )?;

        // Test: master should be selected (has 2 merges vs feature's 0)
        let result = get_branch_with_more_merges(&repo)?;
        assert!(result.is_some());

        let winner_branch = result.unwrap();
        let branch_name = winner_branch.name()?.unwrap();
        assert_eq!(branch_name, "master");

        Ok(())
    }
}
