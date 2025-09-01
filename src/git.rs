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
