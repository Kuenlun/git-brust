/*!
git-brust - Rust CLI tool to visualize git branch flows
Copyright (C) 2025  Juan Luis Leal Contreras (Kuenlun)

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 2 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use env_logger::Builder;
use git2::{Commit, Oid, Repository};
use indexmap::IndexMap;
use log::{error, info, trace};
use std::collections::HashSet;
use thiserror::Error;

const DEBUG_BRANCHES: bool = true;

#[derive(Debug, Error)]
enum GitBrustError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    #[error("no branches provided. Usage: git-brust <base> <compare>")]
    MissingInputBranches,
}

fn main() -> Result<(), GitBrustError> {
    // Initialize the logger
    Builder::new().filter_level(log::LevelFilter::Trace).init();

    // Open git repository
    let repo = Repository::open(".")?;

    // Parse branch names from the program arguments
    let branches = get_branches_from_args()?;
    info!("Branches to use: {}", branches.join(", "));

    let branch_fp_commit_chains = get_unique_fp_commits_chain(&repo, branches)?;
    print_branch_commits(&branch_fp_commit_chains)?;

    Ok(())
}

fn get_branches_from_args() -> Result<Vec<String>, GitBrustError> {
    let args: Vec<String> = std::env::args().skip(1).collect(); // Skip the executable name

    if args.is_empty() {
        if DEBUG_BRANCHES {
            let branches = vec!["master".to_string(), "develop".to_string()];
            trace!("No input branches, using default: {}", branches.join(", "));
            Ok(branches)
        } else {
            Err(GitBrustError::MissingInputBranches)
        }
    } else {
        trace!("Branches from args: {:?}", args);
        Ok(args)
    }
}

fn resolve_branch_to_commit<'repo>(
    repo: &'repo Repository,
    branch_name: &str,
) -> Result<Commit<'repo>, git2::Error> {
    let current_commit = repo.revparse_single(branch_name)?.peel_to_commit()?;
    Ok(current_commit)
}

type BranchFPChain<'repo> = IndexMap<String, Vec<Commit<'repo>>>;

fn get_unique_fp_commits_chain<'repo>(
    repo: &'repo Repository,
    branches_names: Vec<String>,
) -> Result<BranchFPChain<'repo>, git2::Error> {
    // Map each branch name to its first-parent commit chain.
    let mut first_parent_chains: IndexMap<String, Vec<Commit>> = IndexMap::new();
    // Track commit IDs to ensure each commit is processed only once across branches.
    let mut seen_ids: HashSet<Oid> = HashSet::new();

    // Process each branch by resolving its commit chain following first parents.
    for branch in branches_names {
        let mut chain: Vec<Commit> = Vec::new();
        let mut current = resolve_branch_to_commit(repo, &branch)?;

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
