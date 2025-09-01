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

use git2::{Branch, Commit, Oid, Repository};
use itertools::Itertools;
use log::{debug, trace};
use std::collections::HashSet;

use crate::git;
use crate::models::{FPChain, GitBrustError, Relation, RelationType, RepositoryExt};

#[cfg(test)]
use crate::test_utils;

pub fn analyze_branch_relations<'repo>(
    repo: &'repo Repository,
    branches: &'repo [Branch],
) -> Result<Vec<Relation<'repo>>, GitBrustError> {
    // Gets the first-parent commit chains for each branch, skipping commits already included in previous branches
    let branch_fp_commit_chains = get_unique_fp_commits_chain(branches)?;
    print_fp_chains(&branch_fp_commit_chains)?;

    // Obtains all Relations between commits from the first-parent chains
    let relations = calculate_fp_relations(&repo, &branch_fp_commit_chains)?;
    Ok(relations)
}

/// Gets the first-parent commit chains for each branch, skipping commits already included in previous branches
fn get_unique_fp_commits_chain<'repo>(
    branches: &'repo [Branch],
) -> Result<Vec<FPChain<'repo>>, git2::Error> {
    // Map each branch name to its first-parent commit chain
    let mut first_parent_chains: Vec<FPChain<'repo>> = Vec::new();
    // Track commit IDs to ensure each commit is processed only once across branches
    let mut seen_ids: HashSet<Oid> = HashSet::new();

    // Process each branch by resolving its commit chain following first parents
    for branch in branches {
        let mut chain: FPChain = FPChain {
            branch,
            chain: vec![],
        };
        let mut current = git::resolve_branch_to_commit(&branch)?;

        // Traverse the first-parent chain until reaching an already seen commit or there is no parent commit
        while seen_ids.insert(current.id()) {
            let parent = current.parent(0);
            // Add the current commit to the result chain
            chain.push(current);
            // Update current to its first parent, if available
            current = match parent {
                Ok(parent) => parent,
                Err(_) => break, // Stop if no first parent is available
            };
        }
        // Add the chain to the map.
        first_parent_chains.push(chain);
    }
    Ok(first_parent_chains)
}

/// Print branch names and their commits (short IDs)
fn print_fp_chains<'repo>(fp_chains: &Vec<FPChain<'repo>>) -> Result<(), GitBrustError> {
    for fp_chain in fp_chains {
        debug!("{}", fp_chain);
    }
    Ok(())
}

/// Calculate relations (merges and feature births) between first-parent commit chains.
fn calculate_fp_relations<'repo>(
    repo: &'repo Repository,
    fp_chains: &Vec<FPChain<'repo>>,
) -> Result<Vec<Relation<'repo>>, GitBrustError> {
    let mut out_relations = Vec::new();

    // Iterate over unique pairs of branches
    for (fp1, fp2) in fp_chains
        .iter()
        .combinations(2)
        .map(|pair| (pair[0], pair[1]))
    {
        trace!(
            "Pair: {} <-> {}",
            git::name_from_branch(fp1.branch)?,
            git::name_from_branch(fp2.branch)?
        );

        let mut slice1 = &fp1[..];
        let mut slice2 = &fp2[..];

        while let (Some(commit_fp1), Some(commit_fp2)) = (slice1.first(), slice2.first()) {
            let merge_base_oid = repo.merge_base(commit_fp1.id(), commit_fp2.id())?;
            trace!("  merge-base id: {}", repo.short_id_str(merge_base_oid));

            // Find merge-base position in each branch slice
            let idx_b1 = slice1
                .iter()
                .position(|commit| commit.id() == merge_base_oid);
            let idx_b2 = slice2
                .iter()
                .position(|commit| commit.id() == merge_base_oid);

            let (slice, next_slice, iter_branch, base_branch) = match (idx_b1, idx_b2) {
                (Some(idx), None) => {
                    trace!(
                        "merge-base found in branch {} at idx {} ({})",
                        git::name_from_branch(fp1.branch)?,
                        idx,
                        repo.short_id_str(merge_base_oid)
                    );
                    (slice2, &slice1[idx..], fp2, fp1)
                }
                (None, Some(idx)) => {
                    trace!(
                        "merge-base found in branch {} at idx {} ({})",
                        git::name_from_branch(fp2.branch)?,
                        idx,
                        repo.short_id_str(merge_base_oid)
                    );
                    (slice1, &slice2[idx..], fp1, fp2)
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
                if git::name_from_branch(iter_branch.branch)? == git::name_from_branch(fp1.branch)?
                {
                    slice1 = &slice1[advanced..];
                } else {
                    slice2 = &slice2[advanced..];
                }
                // Keep the other slice as is
                if git::name_from_branch(iter_branch.branch)? == git::name_from_branch(fp1.branch)?
                {
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
    slice: &[Commit],
    base_branch: &FPChain,
    iter_branch: &FPChain,
) -> Option<(Relation<'repo>, usize)> {
    let (first, rest) = slice.split_first()?;
    let mut previous_commit = first;

    for (i, current_commit) in rest.iter().enumerate() {
        if let Ok(new_merge_base) = repo.merge_base(merge_base_oid, current_commit.id()) {
            if new_merge_base != merge_base_oid {
                let relation = Relation {
                    src: merge_base_oid,
                    dst: previous_commit.id(),
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

        previous_commit = &current_commit;
    }

    // If no merge-base change detected, relation is a feature birth
    let relation = Relation {
        src: merge_base_oid,
        dst: previous_commit.id(),
        rel_type: RelationType::Birth,
        repo,
    };
    debug!(
        "Found feature birth {} -> {} : {}",
        base_branch, iter_branch, relation
    );
    Some((relation, slice.len()))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::models;

    #[test]
    fn test_empty_repo() -> Result<(), GitBrustError> {
        models::init_logger();
        // Create a empty git repository
        let (repo, _tmp) = test_utils::init_empty_repo_in_tempdir();
        // Get local branches from the repo
        let branches = test_utils::get_local_repo_branches(&repo)?;
        assert!(branches.is_empty());

        // Check it works even if the repo is empty
        let relations = analyze_branch_relations(&repo, &branches)?;
        assert!(relations.is_empty());

        Ok(())
    }

    #[test]
    fn test_repo_only_one_commit() -> Result<(), GitBrustError> {
        models::init_logger();
        // Create a basic git repository
        let (repo, _tmp) = test_utils::init_repo_in_tempdir();

        // Get local branches from the repo
        let branches = test_utils::get_local_repo_branches(&repo)?;
        assert!(!branches.is_empty());

        let relations = analyze_branch_relations(&repo, &branches)?;

        // Check it works even if the repo is empty
        assert!(relations.is_empty());

        Ok(())
    }
}
