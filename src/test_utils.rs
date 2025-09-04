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

use git2::{Branch, Commit, Oid, Repository, Signature, Time, Tree};
use log::info;
use std::collections::HashMap;
use tempfile::TempDir;

use crate::git;
use crate::models::GitBrustError;

/// Temporary non-bare Git repository. Lives until dropped.
pub struct RepoFixture {
    pub repo: Repository,
    _tmpdir: TempDir,
}

/// Deterministic signature for tests
fn deterministic_sig() -> Signature<'static> {
    let secs = 1_600_000_000; // 2020-09-13 12:26:40 UTC
    let time = Time::new(secs, 0);
    Signature::new("Test", "test@example.com", &time).expect("signature")
}

/// Create an empty tree and return its OID
fn empty_tree_oid(repo: &Repository) -> Result<Oid, git2::Error> {
    let mut idx = repo.index()?;
    idx.clear()?;
    idx.write_tree()
}

/// Builder to construct deterministic repo topologies without touching the worktree
pub struct RepoBuilder {
    fix: RepoFixture,
    empty_tree: Oid,
    sig: Signature<'static>,
    heads: HashMap<String, Oid>, // branch -> tip commit
}

impl RepoBuilder {
    /// Create a new temporary non-bare repository with deterministic components
    pub fn new_temp() -> Result<Self, GitBrustError> {
        let tmp = TempDir::new()?;
        let repo = Repository::init(tmp.path())?;
        let empty_tree = empty_tree_oid(&repo)?;
        Ok(Self {
            fix: RepoFixture { repo, _tmpdir: tmp },
            empty_tree,
            sig: deterministic_sig(),
            heads: HashMap::new(),
        })
    }

    /// Get a `Tree` for the cached empty tree OID
    fn empty_tree(&self) -> Result<Tree<'_>, git2::Error> {
        self.fix.repo.find_tree(self.empty_tree)
    }

    /// Ensure a branch exists and optionally move it to `oid`
    fn ensure_branch_at(&mut self, branch: &str, oid: Option<Oid>) -> Result<(), git2::Error> {
        let reference = format!("refs/heads/{}", branch);
        if let Some(to) = oid {
            match self.fix.repo.find_reference(&reference) {
                Ok(mut r) => {
                    r.set_target(to, "move branch head")?;
                }
                Err(_) => {
                    self.fix
                        .repo
                        .reference(&reference, to, true, "create branch")?;
                }
            }
        }
        Ok(())
    }

    /// Create the initial commit on a branch, or append a new commit on top of its head
    pub fn commit_on(&mut self, branch: &str, message: &str) -> Result<Oid, git2::Error> {
        let oid = {
            let parents: Vec<Commit> = match self.heads.get(branch) {
                Some(&tip) => vec![self.fix.repo.find_commit(tip)?],
                None => vec![],
            };
            let parents_refs: Vec<&Commit> = parents.iter().collect();

            let tree = self.empty_tree()?;
            let reference = format!("refs/heads/{}", branch);
            self.fix.repo.commit(
                Some(&reference),
                &self.sig,
                &self.sig,
                message,
                &tree,
                &parents_refs,
            )?
        };

        self.heads.insert(branch.to_string(), oid);
        // Not strictly needed because commit already moved the ref
        Ok(oid)
    }

    /// Create a new branch from an existing branch tip
    pub fn branch_from(&mut self, new_branch: &str, from: &str) -> Result<(), git2::Error> {
        let base = *self
            .heads
            .get(from)
            .ok_or_else(|| git2::Error::from_str("base branch has no commits"))?;
        self.ensure_branch_at(new_branch, Some(base))?;
        self.heads.insert(new_branch.to_string(), base);
        Ok(())
    }

    /// Create a merge commit on `target_branch` that merges `source_branch` into it
    pub fn merge(
        &mut self,
        source_branch: &str,
        target_branch: &str,
        message: &str,
    ) -> Result<Oid, git2::Error> {
        let oid = {
            let src = *self
                .heads
                .get(source_branch)
                .ok_or_else(|| git2::Error::from_str("source branch missing"))?;
            let dst = *self
                .heads
                .get(target_branch)
                .ok_or_else(|| git2::Error::from_str("target branch missing"))?;

            let p_src = self.fix.repo.find_commit(src)?;
            let p_dst = self.fix.repo.find_commit(dst)?;
            let parents = vec![&p_dst, &p_src]; // parent[0] = target, parent[1] = source

            let tree = self.empty_tree()?;
            let reference = format!("refs/heads/{}", target_branch);
            self.fix.repo.commit(
                Some(&reference),
                &self.sig,
                &self.sig,
                message,
                &tree,
                &parents,
            )?
        };
        self.heads.insert(target_branch.to_string(), oid);
        Ok(oid)
    }

    /// Finalize and return the fixture
    pub fn build(self) -> RepoFixture {
        self.fix
    }
}

pub fn get_local_repo_branches<'repo>(
    repo: &'repo Repository,
) -> Result<Vec<Branch<'repo>>, GitBrustError> {
    let branches: Vec<Branch> = repo
        .branches(None)?
        .map(|res| res.map(|(branch, _)| branch))
        .collect::<Result<Vec<_>, _>>()?;
    info!(
        "Local Branches: {}",
        git::names_from_branches(&branches)?.join(", ")
    );
    Ok(branches)
}
