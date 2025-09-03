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

pub fn run_logic<'repo>(
    repo: &'repo Repository,
    branches: &'repo [Branch<'repo>],
) -> Result<Vec<Relation>, GitBrustError> {
    // Gets the first-parent commit chains for each branch, skipping commits already included in previous branches
    let branch_fp_commit_chains = unique_fp_chains(branches)?;

    // Obtains all Relations between commits from the first-parent chains
    let relations = build_relations(repo, &branch_fp_commit_chains)?;
    Ok(relations)
}

/// Returns, for each branch, its first-parent commit chain, excluding any commit
/// that already appeared in an earlier branch of the input slice. The order of
/// `branches` defines precedence.
pub fn unique_fp_chains<'repo>(
    branches: &'repo [Branch<'repo>],
) -> Result<Vec<FPChain<'repo>>, git2::Error> {
    let mut fp_chains: Vec<FPChain<'repo>> = Vec::with_capacity(branches.len());
    let mut seen: HashSet<Oid> = HashSet::new();

    for branch in branches {
        let mut commits: Vec<Commit<'repo>> = Vec::new();
        let mut current = git::resolve_branch_to_commit(branch)?;

        // Walk first-parent until root or a previously seen commit
        while seen.insert(current.id()) {
            // Grab parent before moving `current` into `commits`
            let parent = current.parent(0);
            commits.push(current);

            current = match parent {
                Ok(p) => p,
                Err(e) if e.code() == git2::ErrorCode::NotFound => break, // no first parent
                Err(e) => return Err(e), // propagate real libgit2 errors
            };
        }

        // Build an FPChain object
        let fp_chain = FPChain {
            branch,
            chain: commits,
        };
        debug!("{}", fp_chain);

        // Add the FPChain to the list of chains
        fp_chains.push(fp_chain);
    }

    Ok(fp_chains)
}

fn build_relations<'repo>(
    repo: &'repo Repository,
    fp_chains: &'repo [FPChain<'repo>],
) -> Result<Vec<Relation>, GitBrustError> {
    let mut relations: Vec<Relation> = Vec::new();
    for (fp_chain1, fp_chain2) in fp_chains.iter().tuple_combinations() {
        // TODO: Study if better not to extend and save the results of each pair
        relations.extend(build_relations_from_pair(repo, fp_chain1, fp_chain2)?);
    }
    Ok(relations)
}

fn build_relations_from_pair<'repo>(
    repo: &'repo Repository,
    fp_chain_a: &'repo FPChain<'repo>,
    fp_chain_b: &'repo FPChain<'repo>,
) -> Result<Vec<Relation>, GitBrustError> {
    let branch_a_name = git::name_from_branch(fp_chain_a.branch)?;
    let branch_b_name = git::name_from_branch(fp_chain_b.branch)?;
    trace!(
        "Analyzing chains for branches: {} <-> {}",
        branch_a_name, branch_b_name
    );
    let mut chain_a = fp_chain_a.iter().peekable();
    let mut chain_b = fp_chain_b.iter().peekable();

    let mut out_relations = Vec::new();

    loop {
        // Take only Oids so the mutable borrow from peek does not escape this statement
        let (commit_a_id, commit_b_id) = match (chain_a.peek(), chain_b.peek()) {
            (Some(a), Some(b)) => (a.id(), b.id()),
            _ => break,
        };
        trace!(
            "  commit_a: {}, commit_b: {}",
            repo.short_id_str(commit_a_id),
            repo.short_id_str(commit_b_id)
        );

        // Find merge-base between the two commits
        let merge_base_oid = repo.merge_base(commit_a_id, commit_b_id)?;
        trace!("  merge-base: {}", repo.short_id_str(merge_base_oid));

        // Find in which chain the merge-base is located
        let in_chain_a = fp_chain_a.iter().any(|c| c.id() == merge_base_oid);
        let in_chain_b = fp_chain_b.iter().any(|c| c.id() == merge_base_oid);

        // Decide roles and build an owned Commit for the "no-merge-base" side
        let relation = match (in_chain_a, in_chain_b) {
            (true, false) => {
                // merge-base is in A, so current on B
                detect_relation(
                    repo,
                    &mut chain_a,
                    &mut chain_b,
                    merge_base_oid,
                    commit_b_id,
                )?
            }
            (false, true) => {
                // merge-base is in B, so current on A
                detect_relation(
                    repo,
                    &mut chain_b,
                    &mut chain_a,
                    merge_base_oid,
                    commit_a_id,
                )?
            }
            (false, false) => {
                trace!("Merge-base not found in either chain");
                break;
            }
            (true, true) => {
                return Err(GitBrustError::RelationPair(
                    "Merge-base is in both chains".into(),
                ));
            }
        };

        // Add relation to output
        out_relations.push(relation);
    }

    Ok(out_relations)
}

fn detect_relation<'repo>(
    repo: &'repo Repository,
    chain_merge_base: &mut std::iter::Peekable<std::slice::Iter<'repo, Commit<'repo>>>,
    chain_no_merge_base: &mut std::iter::Peekable<std::slice::Iter<'repo, Commit<'repo>>>,
    merge_base_oid: Oid,
    current_commit_no_merge_base: Oid,
) -> Result<Relation, GitBrustError> {
    // Advance the iterator of the merge-base chain up to the merge-base
    while let Some(commit) = chain_merge_base.peek() {
        if commit.id() == merge_base_oid {
            debug!("  Advanced to merge-base in its chain");
            break;
        }
        trace!(
            "  Advancing merge-base chain: {}",
            repo.short_id_str(commit.id())
        );
        chain_merge_base.next();
    }
    // Fix the iterator of the merge-base chain and iterate the other chain
    // calculating the merge-base until it changes
    let mut last_commit_before_change = current_commit_no_merge_base;
    while let Some(commit) = chain_no_merge_base.peek() {
        trace!(
            "  Checking commit in no-merge-base chain: {}",
            repo.short_id_str(commit.id())
        );
        let new_merge_base_oid = repo.merge_base(merge_base_oid, commit.id())?;
        trace!(
            "  Merge-base between {} and {} is {}",
            repo.short_id_str(merge_base_oid),
            repo.short_id_str(commit.id()),
            repo.short_id_str(new_merge_base_oid)
        );
        if new_merge_base_oid != merge_base_oid {
            // Merge-base changed, we found a relation
            trace!(
                "Merge-base changed: {} -> {}",
                repo.short_id_str(merge_base_oid),
                repo.short_id_str(new_merge_base_oid),
            );
            trace!(
                "  last_commit_before_change: {}",
                repo.short_id_str(last_commit_before_change)
            );
            return Ok(Relation {
                src: merge_base_oid,
                dst: last_commit_before_change,
                rel_type: RelationType::Merge,
            });
        }
        // Update last seen commit before merge-base change
        last_commit_before_change = commit.id();

        trace!("  Advancing commit in no-merge-base chain");
        chain_no_merge_base.next();
    }
    // If we exit the loop without detecting a merge-base change, it's a branch birth
    Ok(Relation {
        src: merge_base_oid,
        dst: last_commit_before_change,
        rel_type: RelationType::Birth,
    })
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
        let relations = run_logic(&repo, &branches)?;
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

        let relations = run_logic(&repo, &branches)?;

        // Check it works even if the repo is empty
        assert!(relations.is_empty());

        Ok(())
    }
}
