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
use log::trace;

use crate::git;
use crate::models::GitBrustError;

/// Visualize git branch flows
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Branches to compare
    branches: Vec<String>,
}

/// Retrieves branches from command-line arguments, or uses the branch with most merges if none provided
pub fn get_branches_from_args<'repo>(
    repo: &'repo Repository,
) -> Result<Vec<Branch<'repo>>, GitBrustError> {
    let args = Args::parse();

    if args.branches.is_empty() {
        trace!("No branches specified, using default branch with most merges");
        match git::get_branch_with_more_merges(repo)? {
            Some(branch) => Ok(vec![branch]),
            None => {
                trace!("No default branch found");
                Ok(vec![])
            }
        }
    } else {
        trace!("Branches from args: {:?}", args.branches);
        git::branches_from_names(repo, &args.branches)
    }
}
