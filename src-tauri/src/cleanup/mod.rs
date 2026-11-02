//! Cleanup planning and execution.
//!
//! `planner` turns user-selected recommendations into an exact, previewable
//! plan. `executor` carries out a confirmed plan. Every path passes
//! `safety::authorize_destructive` in both steps (again at execution time, in
//! case anything changed in between). This module is the only place in
//! DiskPilot that removes files.

pub mod executor;
pub mod planner;
