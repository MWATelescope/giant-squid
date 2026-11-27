// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! IDs on standard input: the input of `download`, `cancel` and `wait`.
//!
//! A command that takes IDs reads them from standard input when one of its
//! arguments is `-`, and only then. This way the output of
//! `giant-squid list --ids` (or `--json`) is the input of the next command:
//!
//! ```text
//! giant-squid list --job-states completed --ids | giant-squid download -
//! ```
//!
//! Standard input is not guessed at. A command that reads it without being
//! asked could wait for ever, or act on IDs that it was not meant to have,
//! when a program that runs it leaves a pipe open on its input. A command
//! that is given no ID and no `-` has been used wrongly, and it fails.
//!
//! Input that has no ID in it is nothing to do, and not an error:
//! `list --ids` prints nothing for a list with no job.
//!
//! The commands do not filter the IDs that they read, but an ID that is given
//! more than once (in the arguments, in a file, or on standard input) is used
//! once, at its first place.

use std::io::Read;

// The parsing of the text is the library's.
use crate::helpers::{
    parse_ids_from_text, parse_many_job_ids_or_obs_ids, without_repeats, ParseError,
};
use crate::mwa_asvo::MwaAsvoJobId;
use crate::obs_id::ObsId;

#[cfg(test)]
mod tests;

/// The argument that means "read standard input".
const STDIN_ARG: &str = "-";

/// The IDs of a command, from its arguments and from standard input.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ResolvedIds {
    /// The Job IDs.
    pub job_ids: Vec<MwaAsvoJobId>,
    /// The Obs IDs.
    pub obs_ids: Vec<ObsId>,
    /// Standard input was read.
    pub from_stdin: bool,
}

impl ResolvedIds {
    /// There is no ID.
    pub(super) fn is_empty(&self) -> bool {
        self.job_ids.is_empty() && self.obs_ids.is_empty()
    }

    /// There is no ID, and standard input was read (a `-` asked for it): it had
    /// no ID in it (`list --ids` printed nothing, or `list --json` printed
    /// `{}`). The command has nothing to do, and that is not an error. With
    /// no ID and no `-`, the command has been used wrongly.
    pub(super) fn is_empty_input(&self) -> bool {
        self.from_stdin && self.is_empty()
    }

    /// The Job IDs, for a command that takes no Obs ID.
    ///
    /// # Errors
    ///
    /// [`ParseError::ObsIdsGiven`] if there is an Obs ID.
    pub(super) fn into_job_ids(self) -> Result<Vec<MwaAsvoJobId>, ParseError> {
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
/// those on standard input when an argument is `-`.
///
/// # Errors
///
/// [`ParseError`] if an argument or the text on standard input is not valid.
pub(super) fn resolve_ids(args: &[String]) -> Result<ResolvedIds, ParseError> {
    let stdin = std::io::stdin();
    resolve_ids_from(args, &mut stdin.lock())
}

/// As [`resolve_ids`], with the standard input.
fn resolve_ids_from(args: &[String], stdin: &mut impl Read) -> Result<ResolvedIds, ParseError> {
    let read_stdin = args.iter().any(|a| a == STDIN_ARG);

    let others: Vec<String> = args.iter().filter(|a| *a != STDIN_ARG).cloned().collect();
    let (mut job_ids, mut obs_ids) = parse_many_job_ids_or_obs_ids(&others)?;
    if !read_stdin {
        return Ok(ResolvedIds {
            job_ids: without_repeats(job_ids),
            obs_ids: without_repeats(obs_ids),
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
        job_ids: without_repeats(job_ids),
        obs_ids: without_repeats(obs_ids),
        from_stdin: true,
    })
}
