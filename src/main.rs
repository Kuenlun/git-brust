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

use git2::Repository;
use log::info;

mod git;
mod input;
mod logic;
mod models;
mod ui;

#[cfg(test)]
mod test_utils;

use crate::models::GitBrustError;

fn main() -> Result<(), GitBrustError> {
    models::init_logger();

    // Open git repository
    let repo = Repository::open(".")?;

    // Parse branch names from the program arguments
    let branches = input::get_branches_from_args(&repo)?;
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
