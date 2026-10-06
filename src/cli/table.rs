// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The job table that the `list` and `wait` commands print.
//!
//! The library prints nothing, so the table lives in the CLI.

use comfy_table::presets::ASCII_FULL_CONDENSED;
use comfy_table::{Attribute, Cell, Color, LineStyle, Row, Table, TableStyle};

use crate::mwa_asvo::{AsvoJob, AsvoJobVec, JobFile, JobState, JobType};

/// The format for the "Completed" column.
const COMPLETED_FORMAT: &str = "%Y-%m-%d %H:%M";

/// The look of the job table: ASCII borders, `=` under the header, and no
/// lines between the rows.
const JOB_TABLE_STYLE: TableStyle =
    ASCII_FULL_CONDENSED.header_separator(LineStyle::new('+', '=', '+', '+'));

/// The column titles of the job table.
const JOB_TABLE_HEADER: [&str; 7] = [
    "Job ID",
    "Obs ID",
    "Job Type",
    "Job State",
    "File Size",
    "Delivery",
    "Completed",
];

/// The files of `job`: none if it has no product.
pub(super) fn job_files(job: &AsvoJob) -> &[JobFile] {
    job.product
        .as_ref()
        .map(|p| p.files.as_slice())
        .unwrap_or_default()
}

/// The total size of the files of `job`, for a person to read (for example
/// `117.7 MiB`).
pub(super) fn job_size_text(job: &AsvoJob) -> String {
    let bytes: u64 = job_files(job).iter().map(JobFile::size_bytes).sum();
    bytesize::ByteSize(bytes).display().iec().to_string()
}

/// The environment variable that turns colours off when it is set and not
/// empty (<https://no-color.org>). The help honours it too (see
/// [`HELP_STYLES`](super::HELP_STYLES)).
const ENV_NO_COLOR: &str = "NO_COLOR";

/// Whether the job table is monochrome: with `--no-colour`, or when
/// `no_color` (the value of [`ENV_NO_COLOR`]) is set and not empty.
pub(super) fn monochrome(no_colour: bool, no_color: Option<std::ffi::OsString>) -> bool {
    no_colour || no_color.is_some_and(|value| !value.is_empty())
}

/// Print `jobs` to stdout as a table, or a short message if there are none.
/// The table is coloured only on a terminal, and not with `no_colour` or
/// `NO_COLOR` (set and not empty).
pub fn print_jobs_table(jobs: AsvoJobVec, no_colour: bool) {
    if jobs.0.is_empty() {
        println!("You have no jobs.");
        return;
    }
    let mut table = Table::new();
    table.load_style(JOB_TABLE_STYLE);
    if monochrome(no_colour, std::env::var_os(ENV_NO_COLOR)) {
        // Without a terminal, the table has no colours and no bold.
        table.force_no_tty();
    }
    table.set_header(
        JOB_TABLE_HEADER
            .iter()
            .map(|title| Cell::new(title).add_attribute(Attribute::Bold)),
    );

    for j in jobs.0 {
        // No size for a job with no product.
        let size = j.product.as_ref().map(|_| job_size_text(&j));
        // The first file's delivery type. An empty file list (possible for a
        // job built by a program) shows nothing.
        let delivery = job_files(&j).first().map(|f| f.type_.to_string());
        let completed = j
            .completed
            .map(|dt| dt.strftime(COMPLETED_FORMAT).to_string());
        table.add_row(Row::from(vec![
            Cell::new(j.job_id()),
            Cell::new(j.obs_id()),
            coloured(
                Cell::new(j.job_type.map(|t| t.name()).unwrap_or_default()),
                job_type_colour(j.job_type),
            ),
            coloured(
                Cell::new(job_state_text(&j)),
                Some(job_state_colour(j.job_state)),
            ),
            Cell::new(size.unwrap_or_default()),
            Cell::new(delivery.unwrap_or_default()),
            Cell::new(completed.unwrap_or_default()),
        ]));
    }

    println!("{table}");
}

/// `cell` in the colour `colour`, if it has one.
fn coloured(cell: Cell, colour: Option<Color>) -> Cell {
    match colour {
        Some(colour) => cell.fg(colour),
        None => cell,
    }
}

/// The colour of a job type cell. A job with no type has no colour.
fn job_type_colour(job_type: Option<JobType>) -> Option<Color> {
    job_type.map(|job_type| match job_type.name() {
        "metadata" => Color::DarkYellow,
        "voltage" => Color::DarkMagenta,
        "cancel" => Color::DarkRed,
        _ => Color::DarkBlue,
    })
}

/// The text of a job state cell: the schema's value (for example
/// `completed`), and for a job with an error, its message.
fn job_state_text(job: &AsvoJob) -> String {
    match (&job.job_state, &job.error_text) {
        (JobState::Error, Some(text)) => format!("{}: {}", job.job_state, text),
        (state, _) => state.to_string(),
    }
}

/// The colour of a job state cell.
fn job_state_colour(job_state: JobState) -> Color {
    match job_state {
        JobState::Queued => Color::White,
        JobState::Waitcal
        | JobState::Staging
        | JobState::Staged
        | JobState::Preparing
        | JobState::Downloading
        | JobState::Preprocessing
        | JobState::Imaging
        | JobState::Delivering => Color::DarkMagenta,
        JobState::Completed => Color::DarkGreen,
        JobState::Error | JobState::Cancelled => Color::DarkRed,
    }
}
