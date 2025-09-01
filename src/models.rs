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
use git2::{Branch, Commit, Oid, Repository};
use log::LevelFilter;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::Once;
use thiserror::Error;

use crate::git;

#[derive(Debug, Error)]
pub enum GitBrustError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    #[error("merge-base not found in first-parent commit chain of current branches")]
    MergeBaseError,

    #[error("branch not found: {0}")]
    BranchNotFound(String),

    #[error("branch does not have a valid name")]
    BranchNameInvalid,
}

static INIT: Once = Once::new();
pub fn init_logger() {
    INIT.call_once(|| {
        Builder::new().filter_level(LevelFilter::Trace).init();
    });
}

pub enum RelationType {
    Birth,
    Merge,
}

/// Relation between commits from two first-parent chains
pub struct Relation<'repo> {
    pub src: Oid,
    pub dst: Oid,
    pub rel_type: RelationType,
    pub repo: &'repo Repository, // Used only for printing short commit IDs
}

/// First-parent commit chain from a branch
pub struct FPChain<'repo> {
    pub branch: &'repo Branch<'repo>,
    pub chain: Vec<Commit<'repo>>,
}

impl<'repo> fmt::Display for FPChain<'repo> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let chain_str: Result<Vec<_>, _> = self
            .chain
            .iter()
            .map(|commit| git::commit_short_id(commit))
            .collect();
        let branch_str = git::name_from_branch(self.branch).map_err(|_| fmt::Error)?;

        match chain_str {
            Ok(ids) => write!(f, "{}: {}", branch_str, ids.join(", ")),
            Err(_) => Err(fmt::Error),
        }
    }
}

// Allow treating FPChain as a Vec<Oid> (immutable)
impl<'repo> Deref for FPChain<'repo> {
    type Target = Vec<Commit<'repo>>;

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

// Displays a Relation using the short IDs of the source and destination commits
impl<'repo> fmt::Display for Relation<'repo> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let short_id_base = self.repo.short_id_str(self.src);
        let short_id_merge = self.repo.short_id_str(self.dst);

        match self.rel_type {
            RelationType::Merge => write!(f, "Merge: ")?,
            RelationType::Birth => write!(f, "Birth: ")?,
        }
        write!(f, "{} -> {}", short_id_base, short_id_merge)
    }
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
