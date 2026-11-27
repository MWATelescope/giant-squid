// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The job table that the `list` and `wait` commands print.
//!
//! The library prints nothing, so the table lives in the CLI.

use comfy_table::presets::ASCII_FULL_CONDENSED;
use comfy_table::{Attribute, Cell, Color, LineStyle, Row, Table, TableStyle};
use jiff::Timestamp;

use super::print_line;
use crate::mwa_asvo::{job_field_value, JobFile, JobState, JobType, MwaAsvoJob, MwaAsvoJobVec};

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
pub(super) fn job_files(job: &MwaAsvoJob) -> &[JobFile] {
    job.product
        .as_ref()
        .map(|p| p.files.as_slice())
        .unwrap_or_default()
}

/// The total size of the files of `job` ([`MwaAsvoJob::total_size`]), for a
/// person to read (for example `117.7 MiB`). A job with no product is `0 B`.
pub(super) fn job_size_text(job: &MwaAsvoJob) -> String {
    let bytes = job.total_size().unwrap_or_default();
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
/// `NO_COLOR` (set and not empty). It has one more column for each of
/// `fields` (names that [`crate::parse_job_field`] accepted), after the
/// others; a cell is empty if the job does not have the field.
///
/// # Errors
///
/// An error of the write to stdout. A closed pipe is not an error: the
/// reader (for example `head`) has ended.
pub fn print_jobs_table(
    jobs: MwaAsvoJobVec,
    no_colour: bool,
    fields: &[String],
) -> std::io::Result<()> {
    if jobs.0.is_empty() {
        return print_line("You have no jobs.");
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
            .copied()
            .chain(fields.iter().map(String::as_str))
            .map(|title| Cell::new(title).add_attribute(Attribute::Bold)),
    );

    // One time for the whole table, so that every row is judged at the same
    // moment.
    let now = Timestamp::now();
    for j in jobs.0 {
        // No size for a job with no product.
        let size = j.product.as_ref().map(|_| job_size_text(&j));
        // The first file's delivery type. An empty file list (possible for a
        // job built by a program) shows nothing.
        let delivery = job_files(&j).first().map(|f| f.type_.to_string());
        let completed = j
            .completed
            .map(|dt| dt.strftime(COMPLETED_FORMAT).to_string());
        let mut cells = vec![
            Cell::new(j.job_id()),
            Cell::new(j.obs_id()),
            Cell::new(j.job_type).fg(job_type_colour(j.job_type)),
            Cell::new(job_state_text(&j, now)).fg(job_state_colour(&j, now)),
            Cell::new(size.unwrap_or_default()),
            Cell::new(delivery.unwrap_or_default()),
            Cell::new(completed.unwrap_or_default()),
        ];
        cells.extend(
            fields
                .iter()
                .map(|field| Cell::new(job_field_value(&j, field).unwrap_or_default())),
        );
        table.add_row(Row::from(cells));
    }

    print_line(table)
}

/// The colour of a job type cell.
fn job_type_colour(job_type: JobType) -> Color {
    match job_type {
        JobType::Metadata => Color::DarkYellow,
        JobType::Voltage => Color::DarkMagenta,
        JobType::Cancel => Color::DarkRed,
        _ => Color::DarkBlue,
    }
}

/// The text of a job state cell: the schema's value (for example
/// `queued`), for a completed job `completed (ready)` or `completed
/// (expired)` ([`MwaAsvoJob::display_state_at`]), and for a job with an error,
/// its message.
fn job_state_text(job: &MwaAsvoJob, now: Timestamp) -> String {
    match (&job.job_state, &job.error_text) {
        (state @ JobState::Error, Some(text)) => format!("{state}: {text}"),
        _ => job.display_state_at(now),
    }
}

/// The colour of a job state cell. A completed job whose download links have
/// expired is not green: it cannot be downloaded.
fn job_state_colour(job: &MwaAsvoJob, now: Timestamp) -> Color {
    match job.job_state {
        JobState::Queued => Color::White,
        JobState::Waitcal
        | JobState::Staging
        | JobState::Staged
        | JobState::Preparing
        | JobState::Downloading
        | JobState::Preprocessing
        | JobState::Imaging
        | JobState::Delivering => Color::DarkMagenta,
        JobState::Completed if job.is_expired_at(now) => Color::DarkYellow,
        JobState::Completed => Color::DarkGreen,
        JobState::Error | JobState::Cancelled => Color::DarkRed,
    }
}
