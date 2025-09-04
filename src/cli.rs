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

use clap::Parser;
use git2::{Branch, Repository};
use log::{info, trace};
use std::path::Path;

use crate::git;
use crate::models::GitBrustError;

#[derive(Parser)]
#[command(name = "git-brust")]
#[command(about = "Visualize git branch flows")]
#[command(version)]
#[command(
    long_about = "A Rust CLI tool that analyzes relationships between Git branches, showing merge-base."
)]
pub struct Args {
    /// Branches to compare
    #[arg(
        help = "Branch names to analyze. If not provided, automatically selects the branch with most merge commits."
    )]
    pub branches: Vec<String>,

    /// Path to git repository (defaults to current directory)
    #[arg(
        short = 'C',
        long = "git-dir",
        value_name = "PATH",
        help = "Path to the Git repository directory"
    )]
    pub repo_path: Option<String>,

    /// Verbose output
    #[arg(short, long, help = "Enable verbose output")]
    pub verbose: bool,
}

impl Args {
    pub fn get_repo_path(&self) -> &Path {
        match &self.repo_path {
            Some(repo_path) => Path::new(repo_path),
            None => Path::new("."),
        }
    }

    /// Retrieves branches from command-line arguments, or uses the branch with most merges if none provided
    pub fn get_branches_to_use<'repo>(
        &self,
        repo: &'repo Repository,
    ) -> Result<Vec<Branch<'repo>>, GitBrustError> {
        let branches = if self.branches.is_empty() {
            trace!("No branches specified, using default branch with most merges");
            let branch_most_commits = git::get_branch_with_more_merges(repo)?;
            git::get_recent_branches_excluding(repo, branch_most_commits, 2)?
        } else {
            trace!("Branches from args: {:?}", self.branches);
            git::branches_from_names(repo, &self.branches)?
        };
        info!(
            "Branches to use: {}",
            git::names_from_branches(&branches)?.join(", ")
        );
        Ok(branches)
    }
}
