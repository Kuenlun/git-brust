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

use env_logger::Builder;
use git2::{Commit, Oid, Repository};
use indexmap::IndexMap;
use itertools::Itertools;
use log::{debug, error, info, trace};
use std::ops::{Deref, DerefMut};
use std::{collections::HashSet, fmt, vec};
use thiserror::Error;

#[derive(Debug, Error)]
enum GitBrustError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    #[error("no branches provided. Usage: git-brust <base> <compare>")]
    MissingInputBranches,

    #[error("merge-base not found in first-parent commit chain of current branches")]
    MergeBaseError,
}

/// If true, uses example branches when no input branches are provided
const DEBUG_BRANCHES: bool = true;

/// First-parent commit chain from a branch
struct FPChain<'repo> {
    chain: Vec<Oid>,
    repo: &'repo Repository, // Used only for printing short commit IDs
}
type BranchFPChain<'repo> = IndexMap<String, FPChain<'repo>>;

impl<'repo> fmt::Display for FPChain<'repo> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let chain_str = self
            .chain
            .iter()
            .map(|oid| self.repo.short_id_str(*oid))
            .join(", ");
        write!(f, "{}", chain_str)
    }
}

// Allow treating FPChain as a Vec<Oid> (immutable)
impl<'repo> Deref for FPChain<'repo> {
    type Target = Vec<Oid>;

    fn deref(&self) -> &Self::Target {
        &self.chain
    }
}

// Allow treating FPChain as a Vec<Oid> (mutable)
impl<'repo> DerefMut for FPChain<'repo> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.chain
    }
}

/// Relation between commits from two first-parent chains
struct Relation<'repo> {
    src: Oid,
    dst: Oid,
    repo: &'repo Repository, // Used only for printing short commit IDs
}

// Displays a Relation using the short IDs of the source and destination commits
impl<'repo> fmt::Display for Relation<'repo> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let short_id_base = self.repo.short_id_str(self.src);
        let short_id_merge = self.repo.short_id_str(self.dst);
        write!(f, "{} -> {}", short_id_base, short_id_merge)
    }
}

fn main() -> Result<(), GitBrustError> {
    // Initialize the logger
    Builder::new().filter_level(log::LevelFilter::Trace).init();

    // Open git repository
    let repo = Repository::open(".")?;

    // Parse branch names from the program arguments
    let branches = get_branches_from_args()?;
    info!("Branches to use: {}", branches.join(", "));

    // Gets the first-parent commit chains for each branch, skipping commits already included in previous branches
    let branch_fp_commit_chains = get_unique_fp_commits_chain(&repo, branches)?;
    print_branch_commits(&branch_fp_commit_chains)?;

    // Obtains all Relations between commits from the first-parent chains
    let relations = calculate_fp_relations(&repo, &branch_fp_commit_chains)?;
    info!("Relations:");
    for relation in relations {
        info!("{}", relation);
    }

    Ok(())
}

/// Extension trait for Repository
pub trait RepositoryExt {
    fn short_id_str(&self, oid: Oid) -> String;
}

// Repository helper to get an object's short ID
impl RepositoryExt for Repository {
    fn short_id_str(&self, oid: Oid) -> String {
        let obj = match self.find_object(oid, None) {
            Ok(o) => o,
            Err(_) => return "?".to_string(),
        };
        let short_id = match obj.short_id() {
            Ok(s) => s,
            Err(_) => return "?".to_string(),
        };
        short_id.as_str().unwrap_or("?").to_string()
    }
}

fn calculate_fp_relations<'repo>(
    repo: &'repo Repository,
    fp_chains: &BranchFPChain,
) -> Result<Vec<Relation<'repo>>, GitBrustError> {
    let mut out_relations: Vec<Relation> = vec![];

    for pair in fp_chains.iter().combinations(2) {
        let (branch_1, commits_branch_1) = pair[0];
        let (branch_2, commits_branch_2) = pair[1];

        trace!("Pair: {} <-> {}", branch_1, branch_2);

        // Create two indexes to iterate the two fp chains
        let mut i = 0;
        let mut j = 0;

        while i < commits_branch_1.len() && j < commits_branch_2.len() {
            // Get the ids from that indexes
            let oid1 = commits_branch_1[i];
            let oid2 = commits_branch_2[j];

            // Find merge base
            let merge_base_oid = repo.merge_base(oid1, oid2)?;
            trace!("  merge-base id: {}", repo.short_id_str(merge_base_oid));

            // See wich branch does not contains the merge base
            let idx_b1_ = commits_branch_1[i..]
                .iter()
                .position(|oid| oid.eq(&merge_base_oid));
            let idx_b2_ = commits_branch_2[j..]
                .iter()
                .position(|oid| oid.eq(&merge_base_oid));
            let (oids_slice, idx_to_iterate, iter_branch, base_branch) = match (idx_b1_, idx_b2_) {
                (Some(idx_b1), None) => {
                    i += idx_b1; // Move the index to the merge-base
                    trace!(
                        "merge-base is in branch 1: {}, oid: {}, idx: {}",
                        branch_1,
                        repo.short_id_str(merge_base_oid),
                        idx_b1
                    );
                    // Save the slice to be iterated and the used index (from the branch that does not contain the merge-base)
                    (&commits_branch_2[j..], &mut j, branch_2, branch_1)
                }
                (None, Some(idx_b2)) => {
                    j += idx_b2; // Move the index to the merge-base
                    trace!(
                        "merge-base is in branch 2: {}, oid: {}, idx: {}",
                        branch_2,
                        repo.short_id_str(merge_base_oid),
                        idx_b2
                    );
                    // Save the slice to be iterated and the used index (from the branch that does not contain the merge-base)
                    (&commits_branch_1[i..], &mut i, branch_1, branch_2)
                }
                (None, None) => {
                    trace!("Merge-base is outside this pair of branches");
                    continue;
                }
                _ => return Err(GitBrustError::MergeBaseError),
            };

            let mut merge_base_merge_commit_oid: Option<&Oid> = None;

            let mut flag_broke_from_loop = false;
            // Iterate over the commits from the branch that do not include the merge base
            for oit_comm in oids_slice {
                trace!(
                    "Oid from non merge-base branch: {}",
                    repo.short_id_str(*oit_comm)
                );
                // Check if the merge base is still the same
                let new_merge_base_oid = repo.merge_base(merge_base_oid, *oit_comm)?;
                if new_merge_base_oid != merge_base_oid {
                    trace!(
                        "New merge base found: {} -> {}",
                        repo.short_id_str(merge_base_oid),
                        repo.short_id_str(new_merge_base_oid)
                    );

                    let relation = Relation {
                        src: merge_base_oid,
                        dst: *merge_base_merge_commit_oid.unwrap(),
                        repo,
                    };

                    debug!(
                        "Found intermerge: {} -> {} : {}",
                        base_branch, iter_branch, relation
                    );

                    out_relations.push(relation);

                    flag_broke_from_loop = true;
                    break;
                } else {
                    // Store the previous oid
                    merge_base_merge_commit_oid = Some(oit_comm);
                }
                *idx_to_iterate += 1;
            }
            if !flag_broke_from_loop {
                let relation = Relation {
                    src: merge_base_oid,
                    dst: *merge_base_merge_commit_oid.unwrap(),
                    repo,
                };
                debug!(
                    "Found feature birth {} -> {} : {}",
                    base_branch, iter_branch, relation
                );
                out_relations.push(relation);
            }
        }
    }
    Ok(out_relations)
}

/// Retrieves branch names from command-line arguments, or uses default branches if in debug mode
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

/// Resolves a branch name to its latest commit in the repository
fn resolve_branch_to_commit<'repo>(
    repo: &'repo Repository,
    branch_name: &str,
) -> Result<Commit<'repo>, git2::Error> {
    let current_commit = repo.revparse_single(branch_name)?.peel_to_commit()?;
    Ok(current_commit)
}

/// Gets the first-parent commit chains for each branch, skipping commits already included in previous branches
fn get_unique_fp_commits_chain<'repo>(
    repo: &'repo Repository,
    branches_names: Vec<String>,
) -> Result<BranchFPChain<'repo>, git2::Error> {
    // Map each branch name to its first-parent commit chain
    let mut first_parent_chains: BranchFPChain = IndexMap::new();
    // Track commit IDs to ensure each commit is processed only once across branches
    let mut seen_ids: HashSet<Oid> = HashSet::new();

    // Process each branch by resolving its commit chain following first parents
    for branch in branches_names {
        let mut chain: FPChain = FPChain {
            chain: vec![],
            repo,
        };
        let mut current = resolve_branch_to_commit(repo, &branch)?;

        // Traverse the first-parent chain until reaching an already seen commit or there is no parent commit
        while seen_ids.insert(current.id()) {
            let parent = current.parent(0);
            // Add the current commit to the result chain
            chain.push(current.id());
            // Update current to its first parent, if available
            current = match parent {
                Ok(parent) => parent,
                Err(_) => break, // Stop if no first parent is available
            };
        }
        // Add the chain to the map.
        first_parent_chains.insert(branch, chain);
    }
    Ok(first_parent_chains)
}

/// Print branch names and their commits (short IDs)
fn print_branch_commits(branches: &BranchFPChain) -> Result<(), GitBrustError> {
    for (branch, ids) in branches {
        debug!("Branch {} -> {}", branch, ids);
    }
    Ok(())
}
