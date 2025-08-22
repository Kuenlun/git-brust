// git-brust - Rust CLI tool to visualize git branch flows
// Copyright (C) 2025  Juan Luis Leal Contreras (Kuenlun)
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use git2::{Commit, Oid, Repository};
use indexmap::IndexMap;
use std::collections::HashSet;

fn main() -> Result<(), git2::Error> {
    // Git repository from path.
    let repo = Repository::open(".")?;
    // List of branches. Later will be passed as program parameter.
    let branch_names = vec!["master", "develop"];

    let branch_fp_commit_chains = get_unique_fp_commits_chain(&repo, &branch_names)?;
    print_branch_commits(&branch_fp_commit_chains)?;

    Ok(())
}

/// Resolves a branch name to its tip commit.
///
/// Attempts to resolve the provided branch name (e.g., "main", "origin/feature")
/// into a commit object, peeling tags or annotated references as needed.
///
/// \param repo        Git repository to operate on.
/// \param branch_name Name of the branch (or any valid revspec).
/// \return            Resolved commit object.
/// \throws git2::Error If the reference or commit cannot be resolved.
fn resolve_branch_to_commit<'repo>(
    repo: &'repo Repository,
    branch_name: &str,
) -> Result<Commit<'repo>, git2::Error> {
    let current_commit = repo.revparse_single(branch_name)?.peel_to_commit()?;
    Ok(current_commit)
}

type BranchFPChain<'repo> = IndexMap<String, Vec<Commit<'repo>>>;

/// Builds a mapping from branch names to their unique first-parent commit chains.
///
/// For each provided branch, this function traverses its history following only the
/// first parent of each commit, collecting the chain of commits that are unique
/// (i.e., not shared with the first-parent chains of previously processed branches).
///
/// \param repo         Git repository to operate on.
/// \param branch_names List of branch names to process.
/// \return             Map from branch name to a vector of unique first-parent commits.
/// \throws git2::Error If resolving any branch or commit fails.
fn get_unique_fp_commits_chain<'repo>(
    repo: &'repo Repository,
    branch_names: &[&str],
) -> Result<BranchFPChain<'repo>, git2::Error> {
    // Map each branch name to its first-parent commit chain.
    let mut first_parent_chains: IndexMap<String, Vec<Commit>> = IndexMap::new();
    // Track commit IDs to ensure each commit is processed only once across branches.
    let mut seen_ids: HashSet<Oid> = HashSet::new();

    // Process each branch by resolving its commit chain following first parents.
    for &branch in branch_names {
        let mut chain: Vec<Commit> = Vec::new();
        let mut current = resolve_branch_to_commit(repo, branch)?;

        // Traverse the first-parent chain until reaching an already seen commit or there is no parent commit.
        while seen_ids.insert(current.id()) {
            let parent = current.parent(0);
            // Add the current commit to the result chain.
            chain.push(current);
            // Update current to its first parent, if available.
            current = match parent {
                Ok(parent) => parent,
                Err(_) => break, // Stop if no first parent is available.
            };
        }
        // Add the chain to the map.
        first_parent_chains.insert(branch.to_string(), chain);
    }
    Ok(first_parent_chains)
}

/// Print branch names and their commits (short IDs).
fn print_branch_commits<'repo>(
    branches: &IndexMap<String, Vec<Commit<'repo>>>,
) -> Result<(), git2::Error> {
    for (branch, commits) in branches {
        println!("Branch: {}", branch);
        for commit in commits {
            println!(
                "  {}",
                commit.as_object().short_id()?.as_str().unwrap_or("?")
            );
        }
    }
    Ok(())
}
