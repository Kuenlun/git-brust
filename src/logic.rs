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

use git2::{Commit, Oid, Repository};
use indexmap::IndexMap;
use itertools::Itertools;
use log::{debug, trace};
use std::collections::HashSet;

use crate::models::BranchFPChain;
use crate::models::FPChain;
use crate::models::GitBrustError;
use crate::models::Relation;
use crate::models::RelationType;
use crate::models::RepositoryExt;

pub fn analyze_branch_relations<'repo>(
    repo: &'repo Repository,
    branches: Vec<String>,
) -> Result<Vec<Relation<'repo>>, GitBrustError> {
    // Gets the first-parent commit chains for each branch, skipping commits already included in previous branches
    let branch_fp_commit_chains = get_unique_fp_commits_chain(&repo, branches)?;
    print_branch_commits(&branch_fp_commit_chains)?;

    // Obtains all Relations between commits from the first-parent chains
    let relations = calculate_fp_relations(&repo, &branch_fp_commit_chains)?;
    Ok(relations)
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
        if let Ok(new_merge_base) = repo.merge_base(merge_base_oid, *current_oid) {
            if new_merge_base != merge_base_oid {
                let relation = Relation {
                    src: merge_base_oid,
                    dst: previous_oid,
                    rel_type: RelationType::Merge,
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
        rel_type: RelationType::Birth,
        repo,
    };
    debug!(
        "Found feature birth {} -> {} : {}",
        base_branch, iter_branch, relation
    );
    Some((relation, slice.len()))
}
