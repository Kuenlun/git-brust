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

use git2::{Branch, Repository};
use log::{info, trace};
use std::vec;

mod git;
mod logic;
mod models;
mod ui;

#[cfg(test)]
mod test_utils;

use crate::models::GitBrustError;

/// If true, uses example branches when no input branches are provided
const DEBUG_BRANCHES: bool = true;

fn main() -> Result<(), GitBrustError> {
    models::init_logger();

    // Open git repository
    let repo = Repository::open(".")?;

    // Parse branch names from the program arguments
    let branches = get_branches_from_args(&repo)?;
    info!(
        "Branches to use: {}",
        git::names_from_branches(&branches)?.join(", ")
    );

    // Calculate the first-parent chain per branch and their relations
    let relations = logic::analyze_branch_relations(&repo, &branches)?;

    // Render UI
    ui::render(relations);

    Ok(())
}

/// Retrieves branch names from command-line arguments, or uses default branches if in debug mode
fn get_branches_from_args<'repo>(
    repo: &'repo Repository,
) -> Result<Vec<Branch<'repo>>, GitBrustError> {
    let branches: Vec<String> = std::env::args().skip(1).collect(); // Skip the executable name

    if branches.is_empty() {
        if DEBUG_BRANCHES {
            let branches = vec!["master", "develop"];
            trace!("No input branches, using default: {}", branches.join(", "));
            Ok(git::branches_from_names(&repo, &branches)?)
        } else {
            Err(GitBrustError::MissingInputBranches)
        }
    } else {
        trace!("Branches from args: {:?}", branches);
        Ok(git::branches_from_names(&repo, &branches)?)
    }
}
