// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! IDs on standard input: the input of `download`, `cancel` and `wait`.
//!
//! A command that takes IDs reads them from standard input when it is given
//! no ID and standard input is not a terminal, or when one of its arguments
//! is `-`. This way the output of `giant-squid list --ids` (or `--json`) is
//! the input of the next command:
//!
//! ```text
//! giant-squid list --job-states completed --ids | giant-squid download
//! ```
//!
//! The commands do not filter the IDs that they read.

use std::io::{IsTerminal, Read};

use crate::helpers::{
    parse_job_id_or_obs_id, parse_many_job_ids_or_obs_ids, ObsIdOrJobId, ParseError,
};
use crate::mwa_asvo::AsvoJobId;
use crate::obs_id::ObsId;

#[cfg(test)]
mod tests;

/// The argument that means "read standard input".
const STDIN_ARG: &str = "-";

/// What a piece of text on standard input holds. See
/// [`parse_ids_from_text`].
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct TextIds {
    /// The Job IDs, in ascending order for a JSON list and in the order of
    /// the text otherwise.
    pub job_ids: Vec<AsvoJobId>,
    /// The Obs IDs, in the order of the text.
    pub obs_ids: Vec<ObsId>,
    /// The text is the JSON of a job list that has no job (`{}`). It is not
    /// a mistake, unlike an empty text.
    pub empty_job_list: bool,
}

/// The key of the status of the one line of JSON that a failed command
/// prints, and its value.
const JSON_STATUS_KEY: &str = "status";
const JSON_STATUS_FAILED: &str = "failed";
/// The key of the message of that line.
const JSON_MESSAGE_KEY: &str = "message";

/// Read the IDs that a command gets on standard input (or from a file that
/// has the same form).
///
/// The text is one of two things:
///
/// - The JSON of `giant-squid list --json` (or `wait --json`, or
///   `--legacy-json`): an object with one entry for each job, and the Job ID
///   as its key. The Job IDs are the keys. The Obs IDs are not used: an Obs ID
///   can have more than one job.
/// - IDs that whitespace separates (one on each line, as `list --ids` prints
///   them). As in a file, a number that is a valid Obs ID is an Obs ID, and
///   any other number is a Job ID.
///
/// Text with no ID in it (`list --ids` for a list with no job prints no text)
/// gives no ID, and [`TextIds::empty_job_list`] is false.
///
/// # Errors
///
/// [`ParseError`] if a word is not a number, if the text is JSON that is not
/// a job list, or if it is the error line of a command that failed (the
/// message of the error is in the [`ParseError`]).
pub(super) fn parse_ids_from_text(text: &str) -> Result<TextIds, ParseError> {
    if text.trim_start().starts_with('{') {
        return parse_job_list_json(text);
    }
    let mut ids = TextIds::default();
    for word in text.split_whitespace() {
        match parse_job_id_or_obs_id(word) {
            Some(ObsIdOrJobId::O(obs_id)) => ids.obs_ids.push(obs_id),
            Some(ObsIdOrJobId::J(job_id)) => ids.job_ids.push(job_id),
            Some(ObsIdOrJobId::Zero) => return Err(ParseError::ZeroJobId),
            None => {
                return Err(ParseError::InsideStdin {
                    text: word.to_string(),
                })
            }
        }
    }
    Ok(ids)
}

/// The Job IDs of the JSON of a job list.
fn parse_job_list_json(text: &str) -> Result<TextIds, ParseError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| ParseError::StdinJson {
            problem: e.to_string(),
        })?;
    let serde_json::Value::Object(entries) = value else {
        return Err(ParseError::StdinJson {
            problem: "it is not an object".to_string(),
        });
    };

    // A command that failed prints one line of JSON with the error.
    if entries
        .get(JSON_STATUS_KEY)
        .and_then(serde_json::Value::as_str)
        == Some(JSON_STATUS_FAILED)
    {
        let message = entries
            .get(JSON_MESSAGE_KEY)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("no message");
        return Err(ParseError::StdinError {
            message: message.to_string(),
        });
    }

    let mut job_ids = Vec::with_capacity(entries.len());
    for (key, job) in &entries {
        let job_id = key.parse::<u64>().ok().and_then(AsvoJobId::new);
        match job_id {
            Some(job_id) if job.is_object() => job_ids.push(job_id),
            _ => {
                return Err(ParseError::StdinJson {
                    problem: format!("the entry '{key}' is not a job with a Job ID as its key"),
                })
            }
        }
    }
    // The keys of a JSON object are text, so they are in the order of the
    // text, not of the numbers.
    job_ids.sort_unstable();
    job_ids.dedup();
    Ok(TextIds {
        empty_job_list: job_ids.is_empty(),
        job_ids,
        obs_ids: Vec::new(),
    })
}

/// The IDs of a command, from its arguments and from standard input.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ResolvedIds {
    /// The Job IDs.
    pub job_ids: Vec<AsvoJobId>,
    /// The Obs IDs.
    pub obs_ids: Vec<ObsId>,
    /// Standard input was read.
    pub from_stdin: bool,
    /// Standard input is the JSON of a job list with no job (`{}`). It is
    /// not a mistake, unlike no IDs at all.
    pub empty_job_list: bool,
}

impl ResolvedIds {
    /// There is no ID.
    pub(super) fn is_empty(&self) -> bool {
        self.job_ids.is_empty() && self.obs_ids.is_empty()
    }

    /// There is no ID, because the list on standard input has no job. The
    /// command has nothing to do, and that is not an error.
    pub(super) fn is_empty_job_list(&self) -> bool {
        self.is_empty() && self.empty_job_list
    }

    /// The Job IDs, for a command that takes no Obs ID.
    ///
    /// # Errors
    ///
    /// [`ParseError::ObsIdsGiven`] if there is an Obs ID.
    pub(super) fn into_job_ids(self) -> Result<Vec<AsvoJobId>, ParseError> {
        if self.obs_ids.is_empty() {
            Ok(self.job_ids)
        } else {
            Err(ParseError::ObsIdsGiven {
                obs_ids: self.obs_ids,
            })
        }
    }
}

/// The IDs of a command: those of its arguments (IDs and files of IDs), and
/// those on standard input when the command is given no argument and
/// standard input is not a terminal, or when an argument is `-`.
///
/// # Errors
///
/// [`ParseError`] if an argument or the text on standard input is not valid.
pub(super) fn resolve_ids(args: &[String]) -> Result<ResolvedIds, ParseError> {
    let stdin = std::io::stdin();
    let is_terminal = stdin.is_terminal();
    resolve_ids_from(args, &mut stdin.lock(), is_terminal)
}

/// As [`resolve_ids`], with the standard input and what it is given.
fn resolve_ids_from(
    args: &[String],
    stdin: &mut impl Read,
    stdin_is_terminal: bool,
) -> Result<ResolvedIds, ParseError> {
    let marked = args.iter().any(|a| a == STDIN_ARG);
    // With no argument, a terminal is not read: it would wait for the
    // person to type, and a command with no ID is a mistake.
    let read_stdin = marked || (args.is_empty() && !stdin_is_terminal);

    let others: Vec<String> = args.iter().filter(|a| *a != STDIN_ARG).cloned().collect();
    let (mut job_ids, mut obs_ids) = parse_many_job_ids_or_obs_ids(&others)?;
    if !read_stdin {
        return Ok(ResolvedIds {
            job_ids,
            obs_ids,
            ..ResolvedIds::default()
        });
    }

    let mut text = String::new();
    stdin
        .read_to_string(&mut text)
        .map_err(|source| ParseError::StdinIO { source })?;
    let from_text = parse_ids_from_text(&text)?;
    job_ids.extend(from_text.job_ids);
    obs_ids.extend(from_text.obs_ids);
    Ok(ResolvedIds {
        job_ids,
        obs_ids,
        from_stdin: true,
        empty_job_list: from_text.empty_job_list,
    })
}
