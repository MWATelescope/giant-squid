// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The job table that the `list` and `wait` commands print.
//!
//! The library prints nothing, so the table lives in the CLI.

use prettytable::{row, Cell, Row, Table};

use crate::asvo::{AsvoJob, AsvoJobVec, JobState, JobType};

/// The format for the "Completed" column.
const COMPLETED_FORMAT: &str = "%Y-%m-%d %H:%M";

/// Print `jobs` to stdout as a table, or a short message if there are none.
/// If `no_colour` is true then don't colour the output.
pub fn print_jobs_table(jobs: AsvoJobVec, no_colour: bool) {
    if jobs.0.is_empty() {
        println!("You have no jobs.");
    } else {
        let mut table = Table::new();
        table.set_format(*prettytable::format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

        table.set_titles(row![
            b => "Job ID",
            "Obsid",
            "Job Type",
            "Job State",
            "File Size",
            "Delivery",
            "Completed"
        ]);

        for j in jobs.0 {
            table.add_row(Row::new(vec![
                Cell::new(j.job_id.to_string().as_str()),
                Cell::new(j.obs_id.to_string().as_str()),
                Cell::new(j.job_type.map(|t| t.name()).unwrap_or_default())
                    .style_spec(&job_type_table_style(j.job_type, no_colour)),
                Cell::new(job_state_text(&j).as_str())
                    .style_spec(&job_state_table_style(j.job_state, no_colour)),
                Cell::new(
                    match &j.product {
                        None => "".to_string(),
                        Some(p) => {
                            let size: u64 = p.files.iter().map(|f| f.size).sum();
                            bytesize::ByteSize(size).display().iec().to_string()
                        }
                    }
                    .as_str(),
                ),
                Cell::new(
                    // The first file's delivery type. An empty file list
                    // (possible for a job built by a program) shows nothing.
                    j.product
                        .as_ref()
                        .and_then(|p| p.files.first())
                        .map(|f| f.r#type.to_string())
                        .unwrap_or_default()
                        .as_str(),
                ),
                Cell::new(
                    j.completed
                        .map(|dt| dt.strftime(COMPLETED_FORMAT).to_string())
                        .unwrap_or_default()
                        .as_str(),
                ),
            ]));
        }

        table.printstd();
    }
}

/// The prettytable style spec for a job type cell. A job with no type has
/// no style.
pub fn job_type_table_style(job_type: Option<JobType>, no_colour: bool) -> String {
    match job_type {
        Some(job_type) if !no_colour => match job_type.name() {
            "metadata" => "Fy",
            "voltage" => "Fm",
            "cancel" => "Fr",
            _ => "Fb",
        }
        .to_string(),
        _ => "".to_string(),
    }
}

/// The text of a job state cell: the schema's value (for example
/// `completed`), and for a job with an error, its message.
fn job_state_text(job: &AsvoJob) -> String {
    match (&job.job_state, &job.error_text) {
        (JobState::Error, Some(text)) => format!("{}: {}", job.job_state, text),
        (state, _) => state.to_string(),
    }
}

/// The prettytable style spec for a job state cell.
pub fn job_state_table_style(job_state: JobState, no_colour: bool) -> String {
    if no_colour {
        "".to_string()
    } else {
        match job_state {
            JobState::Queued => "FW",
            JobState::Waitcal => "Fm",
            JobState::Staging => "Fm",
            JobState::Staged => "Fm",
            JobState::Preparing => "Fm",
            JobState::Downloading => "Fm",
            JobState::Preprocessing => "Fm",
            JobState::Imaging => "Fm",
            JobState::Delivering => "Fm",
            JobState::Completed => "Fg",
            JobState::Error => "Fr",
            JobState::Cancelled => "Fr",
        }
        .to_string()
    }
}
