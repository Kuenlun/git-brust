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

/// Calculate relations (merges and feature births) between first-parent commit chains.
fn calculate_fp_relations<'repo>(
    repo: &'repo Repository,
    fp_chains: &BranchFPChain<'repo>,
) -> Result<Vec<Relation<'repo>>, GitBrustError> {
    let mut out_relations = Vec::new();

    // Iterate over unique pairs of branches
    for (branch_1, fp1, branch_2, fp2) in fp_chains
        .iter()
        .combinations(2)
        .map(|pair| (pair[0].0, &pair[0].1.chain, pair[1].0, &pair[1].1.chain))
    {
        trace!("Pair: {} <-> {}", branch_1, branch_2);

        let mut slice1 = &fp1[..];
        let mut slice2 = &fp2[..];

        while let (Some(&oid1), Some(&oid2)) = (slice1.first(), slice2.first()) {
            let merge_base_oid = repo.merge_base(oid1, oid2)?;
            trace!("  merge-base id: {}", repo.short_id_str(merge_base_oid));

            // Find merge-base position in each branch slice
            let idx_b1 = slice1.iter().position(|oid| *oid == merge_base_oid);
            let idx_b2 = slice2.iter().position(|oid| *oid == merge_base_oid);

            let (slice, next_slice, iter_branch, base_branch) = match (idx_b1, idx_b2) {
                (Some(idx), None) => {
                    trace!(
                        "merge-base found in branch {} at idx {} ({})",
                        branch_1,
                        idx,
                        repo.short_id_str(merge_base_oid)
                    );
                    (slice2, &slice1[idx..], branch_2, branch_1)
                }
                (None, Some(idx)) => {
                    trace!(
                        "merge-base found in branch {} at idx {} ({})",
                        branch_2,
                        idx,
                        repo.short_id_str(merge_base_oid)
                    );
                    (slice1, &slice2[idx..], branch_1, branch_2)
                }
                (None, None) => {
                    trace!("Merge-base not present in either branch of this pair");
                    break;
                }
                _ => return Err(GitBrustError::MergeBaseError),
            };

            if let Some((relation, advanced)) =
                detect_relation(repo, merge_base_oid, slice, base_branch, iter_branch)
            {
                out_relations.push(relation);
                // Advance the slice of the branch we iterated
                if iter_branch == branch_1 {
                    slice1 = &slice1[advanced..];
                } else {
                    slice2 = &slice2[advanced..];
                }
                // Keep the other slice as is
                if iter_branch == branch_1 {
                    slice2 = next_slice;
                } else {
                    slice1 = next_slice;
                }
            } else {
                break;
            }
        }
    }

    Ok(out_relations)
}

/// Detects a relation by scanning a commit slice until a merge-base change occurs
/// Returns a relation and the number of commits consumed
fn detect_relation<'repo>(
    repo: &'repo Repository,
    merge_base_oid: Oid,
    slice: &[Oid],
    base_branch: &str,
    iter_branch: &str,
) -> Option<(Relation<'repo>, usize)> {
    let (first, rest) = slice.split_first()?;
    let mut previous_oid = *first;

    for (i, current_oid) in rest.iter().enumerate() {
        trace!(
            "Oid from non merge-base branch: {}",
            repo.short_id_str(*current_oid)
        );

        if let Ok(new_merge_base) = repo.merge_base(merge_base_oid, *current_oid) {
            if new_merge_base != merge_base_oid {
                let relation = Relation {
                    src: merge_base_oid,
                    dst: previous_oid,
                    repo,
                };
                debug!(
                    "Found intermerge: {} -> {} : {}",
                    base_branch, iter_branch, relation
                );
                return Some((relation, i + 1));
            }
        }

        previous_oid = *current_oid;
    }

    // If no merge-base change detected, relation is a feature birth
    let relation = Relation {
        src: merge_base_oid,
        dst: previous_oid,
        repo,
    };
    debug!(
        "Found feature birth {} -> {} : {}",
        base_branch, iter_branch, relation
    );
    Some((relation, slice.len()))
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
