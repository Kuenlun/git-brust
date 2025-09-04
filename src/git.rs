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

use git2::{Branch, BranchType, Commit, Oid, Repository};

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

/// Returns up to `count` recent local branches ordered for consumption:
/// - If `exclude_branch` is provided, it is placed first in the output.
/// - The remainder is filled with the most recent local branches by tip commit time,
///   excluding the same branch by name or by tip Oid to avoid duplicates.
/// - If there are fewer branches available, all available ones are returned.
/// - If `count` is zero, an empty vector is returned.
///
/// Selection rules:
/// - Unnamed branches are skipped from candidates since they cannot be referenced reliably.
/// - Unborn branches are skipped from candidates because they have no tip commit time.
/// - Sorting is by descending tip commit time (newest first).
pub fn get_recent_branches_excluding<'repo>(
    repo: &'repo Repository,
    mut exclude_branch: Option<Branch<'repo>>,
    count: usize,
) -> Result<Vec<Branch<'repo>>, GitBrustError> {
    debug!(
        "Selecting up to {} recent branches with optional exclusion first",
        count
    );

    if count == 0 {
        trace!("Requested count is zero, returning empty selection");
        return Ok(Vec::new());
    }

    // Resolve exclusion identity before moving it
    let (excluded_name, excluded_tip): (Option<String>, Option<Oid>) = match exclude_branch.as_ref()
    {
        Some(b) => {
            // Best-effort name resolution for equality checks
            let name = b.name()?.map(|s| s.to_string());
            // Tip Oid if present, to guard against renamed branches pointing to same tip
            let tip = b.get().target();
            (name, tip)
        }
        None => (None, None),
    };

    // Gather candidate branches with their tip times and names
    // Only include valid, named, non-unborn branches
    let mut candidates: Vec<(i64, Branch<'repo>, String, Oid)> = Vec::new();
    let branches = repo.branches(Some(BranchType::Local))?;

    for branch_result in branches {
        let (branch, _ty) = branch_result?;

        // Skip unnamed branches
        let Some(name) = branch.name()?.map(|s| s.to_string()) else {
            trace!("Skipping unnamed branch");
            continue;
        };

        // Skip unborn branches
        let Some(tip_oid) = branch.get().target() else {
            trace!("Skipping unborn branch '{}'", name);
            continue;
        };

        // Get tip commit time
        let Some(tip_time) = tip_time_seconds(&branch)? else {
            // Defensive: if there is a target but we failed to read time, skip
            trace!("Skipping branch '{}' due to missing tip time", name);
            continue;
        };

        // Exclude the branch that matches the provided exclusion by name or tip Oid
        let exclude_by_name = excluded_name.as_deref() == Some(name.as_str());
        let exclude_by_tip = excluded_tip.is_some() && excluded_tip == Some(tip_oid);
        if exclude_by_name || exclude_by_tip {
            trace!(
                "Excluding branch '{}' from candidates (name match: {}, tip match: {})",
                name, exclude_by_name, exclude_by_tip
            );
            continue;
        }

        trace!("{:>12} secs -> '{}' ({})", tip_time, name, tip_oid);
        candidates.push((tip_time, branch, name, tip_oid));
    }

    // Sort by time descending, newest first
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    // Build output: first the excluded branch if present, then fill with newest
    let mut out: Vec<Branch<'repo>> = Vec::with_capacity(count);

    if let Some(b) = exclude_branch.take() {
        trace!("Placing excluded branch first in the result vector");
        out.push(b);
        if out.len() == count {
            info!("Selection complete with only the excluded branch");
            return Ok(out);
        }
    }

    for (_time, branch, name, _oid) in candidates.into_iter() {
        trace!("Adding branch '{}' to selection", name);
        out.push(branch);
        if out.len() == count {
            break;
        }
    }

    info!("Selected {} branch(es)", out.len());
    Ok(out)
}

/// Helper: obtain the tip commit time (seconds since epoch) of a branch.
/// Returns Ok(None) for unborn branches; propagates other git errors.
fn tip_time_seconds(branch: &Branch) -> Result<Option<i64>, GitBrustError> {
    let reference = branch.get();
    match reference.peel_to_commit() {
        Ok(commit) => Ok(Some(commit.time().seconds())),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(e) => Err(e.into()),
    }
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

    #[test]
    fn test_get_branch_with_more_merges_empty_repo() -> Result<(), GitBrustError> {
        models::init_logger();

        let b = test_utils::RepoBuilder::new_temp()?;

        let fix = b.build();
        let repo = &fix.repo;

        let result = get_branch_with_more_merges(&repo)?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_selects_branch_with_most_merges() -> Result<(), GitBrustError> {
        models::init_logger();

        let mut b = test_utils::RepoBuilder::new_temp()?;
        // master: C1, C2, C3
        b.commit_on("master", "C1")?;
        b.commit_on("master", "C2")?;
        b.commit_on("master", "C3")?;

        // feature branched at C2: F1, F2
        b.branch_from("feature", "master")?;
        // Move feature to point at C2 explicitly if needed:
        // b.ensure_branch_at("feature", b.head_of("master_at_C2"));
        b.commit_on("feature", "F1")?;
        b.commit_on("feature", "F2")?;

        // merge feature into master producing M1
        b.merge("feature", "master", "M1")?;

        let fix = b.build();
        let repo = &fix.repo;

        // Test: master should be selected (has 2 merges vs feature's 0)
        let result = get_branch_with_more_merges(&repo)?;
        assert!(result.is_some());

        let winner_branch = result.unwrap();
        let branch_name = winner_branch.name()?.unwrap();
        assert_eq!(branch_name, "master");

        Ok(())
    }
}
