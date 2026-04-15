//! Parallel step execution (SPEC §29).
//!
//! This module coordinates concurrent execution of `async: true` steps, the
//! synchronization at `action: join` barriers, and the merging of branch
//! results back into the main session's turn log.
//!
//! # Threading model
//!
//! Uses [`std::thread::scope`] — scoped threads guarantee that borrowed
//! references (like `&dyn Runner`) cannot outlive the scope, so we don't need
//! `Arc` wrapping. Each async branch runs on its own thread with a forked
//! [`Session`] produced by [`Session::fork_for_branch`].
//!
//! # Runner Sync requirement
//!
//! For `std::thread::scope`, spawned closures must be `Send`. A `&dyn Runner`
//! is `Send` only if `Runner: Sync`. All current runner implementations are
//! `Sync` by composition, so we declare the parallel entry points as taking
//! `&(dyn Runner + Sync)`. Top-level `execute_core` inherits the same bound.
//!
//! # Join merge behaviour (SPEC §29.4, §29.5)
//!
//! - **String join** (default): concatenate branch responses in declaration
//!   order with `[step_id]:\n<response>` headers.
//! - **Structured join**: when all branches declared `output_schema`, parse
//!   each response as JSON and merge into a single object namespaced by step id.
//!
//! # Error handling (SPEC §29.7)
//!
//! - `JoinErrorMode::FailFast` (default): first branch failure cancels
//!   in-flight siblings via a shared [`CancelToken`] and propagates the error.
//! - `JoinErrorMode::WaitForAll`: all branches run to completion; failed
//!   branches contribute `{ "error": …, "error_type": … }` envelopes to the
//!   merged output (structured-only).

#![allow(clippy::result_large_err)]

use std::collections::HashMap;
use std::sync::{Condvar, Mutex};
use std::time::SystemTime;

use uuid::Uuid;

use crate::config::domain::{ActionKind, JoinErrorMode, Step, StepBody};
use crate::error::AilError;
use crate::runner::CancelToken;
use crate::session::turn_log::TurnEntry;

/// Simple counting semaphore used to enforce `defaults.max_concurrency`
/// (SPEC §29.10). Implemented with `Mutex<usize> + Condvar` — no external
/// dependency.
///
/// `acquire()` blocks until a slot is free or the cancel token fires.
/// `release()` decrements the counter and wakes one waiter.
pub struct ConcurrencySemaphore {
    max: usize,
    state: Mutex<usize>,
    cv: Condvar,
}

impl ConcurrencySemaphore {
    pub fn new(max: usize) -> Self {
        ConcurrencySemaphore {
            max,
            state: Mutex::new(0),
            cv: Condvar::new(),
        }
    }

    /// Acquire a slot. Blocks until `current < max` or the cancel token fires.
    /// Returns `true` if a slot was acquired, `false` if cancelled before acquire.
    pub fn acquire(&self, cancel: &CancelToken) -> bool {
        let mut guard = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        loop {
            if cancel.is_cancelled() {
                return false;
            }
            if *guard < self.max {
                *guard += 1;
                return true;
            }
            guard = match self
                .cv
                .wait_timeout(guard, std::time::Duration::from_millis(50))
            {
                Ok((g, _)) => g,
                Err(p) => p.into_inner().0,
            };
        }
    }

    pub fn release(&self) {
        let mut guard = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if *guard > 0 {
            *guard -= 1;
        }
        self.cv.notify_one();
    }
}

/// Result of executing one async branch on a spawned thread.
pub struct BranchResult {
    pub step_id: String,
    /// The branch's outcome: the completed `TurnEntry` or an error.
    pub outcome: Result<TurnEntry, AilError>,
    pub launched_at: String,
    pub completed_at: String,
    /// Entries the branch produced (in addition to the outcome entry) —
    /// useful if the branch has a `then:` chain (currently unused, kept for
    /// future observability).
    pub extra_entries: Vec<TurnEntry>,
}

/// Generate a short concurrent-group ID (SPEC §29.8).
pub fn new_concurrent_group_id() -> String {
    format!("cg-{}", &Uuid::new_v4().simple().to_string()[..8])
}

/// Render a `SystemTime` as an ISO 8601 UTC timestamp to millisecond
/// precision. Used for `launched_at` / `completed_at` on turn log entries.
pub fn iso8601(t: SystemTime) -> String {
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();
    // Compute Y-M-D H:M:S in UTC without any external dep.
    let (y, m, d, hh, mm, ss) = civil_from_secs(secs);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z")
}

/// Minimal civil calendar conversion for ISO 8601 timestamps. Avoids a
/// chrono dependency for a single formatting use case. Handles dates in
/// the range [1970-01-01, 9999-12-31].
fn civil_from_secs(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let ss = (secs % 60) as u32;
    let mm = ((secs / 60) % 60) as u32;
    let hh = ((secs / 3600) % 24) as u32;
    let mut days = (secs / 86400) as i64;

    // Shift from 1970-01-01 to 0000-03-01 (civil-from-days algorithm, Howard Hinnant).
    days += 719468;
    let era = days.div_euclid(146097);
    let doe = days.rem_euclid(146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m, d, hh, mm, ss)
}

/// True when the step is an `action: join` step.
pub fn is_join_step(step: &Step) -> bool {
    matches!(step.body, StepBody::Action(ActionKind::Join { .. }))
}

/// Extract the join's error mode. Returns `None` when the step is not a join.
pub fn join_error_mode(step: &Step) -> Option<&JoinErrorMode> {
    match &step.body {
        StepBody::Action(ActionKind::Join { on_error_mode }) => Some(on_error_mode),
        _ => None,
    }
}

/// Merge collected branch results into a single `TurnEntry` representing
/// the join step's output. Implements both string join (§29.4) and
/// structured join (§29.5).
///
/// `deps_in_order` is the join's `depends_on` list in declaration order —
/// used to order the merged output.
pub fn merge_join_results(
    join_step: &Step,
    deps_in_order: &[&str],
    branches: &[BranchResult],
    on_error_mode: &JoinErrorMode,
) -> Result<TurnEntry, AilError> {
    // Build a lookup map step_id → BranchResult.
    let branch_map: HashMap<&str, &BranchResult> =
        branches.iter().map(|b| (b.step_id.as_str(), b)).collect();

    // Check if every declared dependency has output_schema — if so, use
    // structured join. Otherwise, string join.
    // (Parse-time validation already enforced all-or-nothing for structured
    // joins declaring output_schema on the join itself.)
    let all_structured = deps_in_order.iter().all(|dep_id| {
        branch_map
            .get(dep_id)
            .and_then(|b| b.outcome.as_ref().ok())
            .map(|_| {
                // Look up the dep step's output_schema from the pipeline's
                // step list. We can't do that here without the pipeline —
                // instead, treat "structured" as "join declares output_schema".
                join_step.output_schema.is_some()
            })
            .unwrap_or(false)
    });

    let use_structured = join_step.output_schema.is_some() && all_structured;

    let response = if use_structured {
        merge_structured(deps_in_order, &branch_map, on_error_mode)?
    } else {
        merge_string(deps_in_order, &branch_map, on_error_mode)?
    };

    let earliest_launch = branches
        .iter()
        .map(|b| b.launched_at.clone())
        .min()
        .unwrap_or_default();
    let latest_complete = branches
        .iter()
        .map(|b| b.completed_at.clone())
        .max()
        .unwrap_or_default();

    Ok(TurnEntry {
        step_id: join_step.id.as_str().to_string(),
        prompt: format!("join:[{}]", deps_in_order.join(",")),
        response: Some(response),
        launched_at: Some(earliest_launch),
        completed_at: Some(latest_complete),
        ..Default::default()
    })
}

/// String join (SPEC §29.4): concatenate branch responses with `[step_id]:`
/// headers in declaration order.
fn merge_string(
    deps_in_order: &[&str],
    branches: &HashMap<&str, &BranchResult>,
    on_error_mode: &JoinErrorMode,
) -> Result<String, AilError> {
    let mut out = String::new();
    for (i, dep_id) in deps_in_order.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let br = branches.get(dep_id).copied();
        match br.map(|b| &b.outcome) {
            Some(Ok(entry)) => {
                let resp = entry.response.as_deref().unwrap_or("");
                out.push_str(&format!("[{dep_id}]:\n{resp}\n"));
            }
            Some(Err(e)) => {
                if matches!(on_error_mode, JoinErrorMode::FailFast) {
                    return Err(AilError::pipeline_aborted(format!(
                        "Join dependency '{dep_id}' failed (fail_fast): {}",
                        e.detail()
                    )));
                }
                out.push_str(&format!("[{dep_id}]:\n<error: {}>\n", e.detail()));
            }
            None => {
                out.push_str(&format!("[{dep_id}]:\n<missing>\n"));
            }
        }
    }
    Ok(out)
}

/// Structured join (SPEC §29.5): merge each branch's structured response into
/// a single JSON object keyed by step id.
fn merge_structured(
    deps_in_order: &[&str],
    branches: &HashMap<&str, &BranchResult>,
    on_error_mode: &JoinErrorMode,
) -> Result<String, AilError> {
    use serde_json::{Map, Value};
    let mut merged = Map::new();

    for dep_id in deps_in_order {
        let br = branches.get(dep_id).copied();
        match br.map(|b| &b.outcome) {
            Some(Ok(entry)) => {
                let resp = entry.response.as_deref().unwrap_or("");
                let parsed: Value = serde_json::from_str(resp).map_err(|e| {
                    AilError::config_validation(format!(
                        "Join dependency '{dep_id}' did not return valid JSON despite declaring \
                         output_schema: {e}"
                    ))
                })?;
                merged.insert(dep_id.to_string(), parsed);
            }
            Some(Err(e)) => {
                if matches!(on_error_mode, JoinErrorMode::FailFast) {
                    return Err(AilError::pipeline_aborted(format!(
                        "Join dependency '{dep_id}' failed (fail_fast): {}",
                        e.detail()
                    )));
                }
                // wait_for_all: emit an error envelope for this dependency.
                let mut env = Map::new();
                env.insert("error".to_string(), Value::String(e.detail().to_string()));
                env.insert(
                    "error_type".to_string(),
                    Value::String(e.error_type().to_string()),
                );
                merged.insert(dep_id.to_string(), Value::Object(env));
            }
            None => {
                if matches!(on_error_mode, JoinErrorMode::FailFast) {
                    return Err(AilError::pipeline_aborted(format!(
                        "Join dependency '{dep_id}' produced no result"
                    )));
                }
                merged.insert(dep_id.to_string(), Value::Null);
            }
        }
    }

    serde_json::to_string(&Value::Object(merged)).map_err(|e| {
        AilError::config_validation(format!("Failed to serialize structured join: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_formats_epoch() {
        let s = iso8601(SystemTime::UNIX_EPOCH);
        assert_eq!(s, "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn iso8601_monotonic() {
        let t1 = iso8601(SystemTime::UNIX_EPOCH);
        let t2 = iso8601(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(86_400));
        assert!(t2 > t1);
        assert_eq!(t2, "1970-01-02T00:00:00.000Z");
    }

    #[test]
    fn semaphore_limits_concurrency() {
        let sem = ConcurrencySemaphore::new(2);
        let ct = CancelToken::new();
        assert!(sem.acquire(&ct));
        assert!(sem.acquire(&ct));
        // Third acquire would block, so don't test it here without threads.
        sem.release();
        assert!(sem.acquire(&ct));
        sem.release();
        sem.release();
    }
}
