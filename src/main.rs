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
use git2::Repository;

use crate::cli::Args;
use crate::models::GitBrustError;

mod cli;
mod git;
mod logic;
mod models;
mod ui;

#[cfg(test)]
mod test_utils;

fn main() -> Result<(), GitBrustError> {
    // Initialize logging first
    models::init_logger();

    // Parse arguments
    let args = Args::parse();

    // Discover repository starting from specified path or current directory
    let repo = Repository::discover(args.get_repo_path())?;

    // Resolve branch names to Branch objects
    let branches = args.get_branches_to_use(&repo)?;

    // Calculate the first-parent chain per branch and their relations
    let relations = logic::run_logic(&repo, &branches)?;

    // Render UI
    ui::render(relations);

    Ok(())
}
