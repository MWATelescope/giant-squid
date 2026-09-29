// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The job table that the `list` and `wait` commands print.
//!
//! The library prints nothing, so the table lives in the CLI.

use prettytable::{row, Cell, Row, Table};

use crate::asvo::{AsvoJobState, AsvoJobType, AsvoJobVec};

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

        let mut has_unknown_job_type: bool = false;

        for j in jobs.0 {
            table.add_row(Row::new(vec![
                Cell::new(j.jobid.to_string().as_str()),
                Cell::new(j.obsid.to_string().as_str()),
                Cell::new(j.jtype.to_string().as_str())
                    .style_spec(&job_type_table_style(j.jtype, no_colour)),
                Cell::new(j.state.to_string().as_str())
                    .style_spec(&job_state_table_style(j.state, no_colour)),
                Cell::new(
                    match &j.files {
                        None => "".to_string(),
                        Some(v) => {
                            let mut size = 0;
                            for f in v {
                                size += f.size;
                            }
                            bytesize::ByteSize(size).display().iec().to_string()
                        }
                    }
                    .as_str(),
                ),
                Cell::new(
                    match j.files {
                        None => "".to_string(),
                        Some(v) => v.first().unwrap().r#type.to_string(),
                    }
                    .as_str(),
                ),
                Cell::new(
                    j.completed
                        .map(|dt| dt.format(COMPLETED_FORMAT).to_string())
                        .unwrap_or_default()
                        .as_str(),
                ),
            ]));

            // If has_unknown_job_type is already True, stay true. If False, but this job is unknown set to True.
            has_unknown_job_type |= j.jtype == AsvoJobType::Unknown;
        }

        table.printstd();

        // if we had an unknown job type emit a warning
        if has_unknown_job_type {
            log::warn!("giant-squid needs to be updated: one of more of your jobs contains a job_type that is unknown to this version of giant-squid. Please update to the latest version.");
        }
    }
}

/// The prettytable style spec for a job type cell.
pub fn job_type_table_style(job_type: AsvoJobType, no_colour: bool) -> String {
    if no_colour {
        "".to_string()
    } else {
        match job_type {
            AsvoJobType::Conversion => "Fb",
            AsvoJobType::DownloadVisibilities => "Fb",
            AsvoJobType::DownloadBeamformer => "Fb",
            AsvoJobType::DownloadMetadata => "Fy",
            AsvoJobType::DownloadVoltage => "Fm",
            AsvoJobType::CancelJob => "Fr",
            AsvoJobType::Imaging => "Fb",
            AsvoJobType::Unknown => "Fr",
        }
        .to_string()
    }
}

/// The prettytable style spec for a job state cell.
pub fn job_state_table_style(job_state: AsvoJobState, no_colour: bool) -> String {
    if no_colour {
        "".to_string()
    } else {
        match job_state {
            AsvoJobState::Queued => "FW",
            AsvoJobState::WaitCal => "Fm",
            AsvoJobState::Staging => "Fm",
            AsvoJobState::Staged => "Fm",
            AsvoJobState::Preparing => "Fm",
            AsvoJobState::Downloading => "Fm",
            AsvoJobState::Preprocessing => "Fm",
            AsvoJobState::Imaging => "Fm",
            AsvoJobState::Delivering => "Fm",
            AsvoJobState::Ready => "Fg",
            AsvoJobState::Error(_) => "Fr",
            AsvoJobState::Expired => "Fw",
            AsvoJobState::Cancelled => "Fr",
        }
        .to_string()
    }
}
