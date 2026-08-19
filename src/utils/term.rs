//! Minimal ANSI color helpers for terminal output.

/// Clears any styles set by the codes below.
pub const RESET: &str = "\x1b[0m";
/// Headers and totals.
pub const BOLD: &str = "\x1b[1m";
/// Skipped/up-to-date targets.
pub const DIM: &str = "\x1b[2m";
/// Successful builds ("x ... completed").
pub const GREEN: &str = "\x1b[32m";
/// Failed or signaled targets ("x ... failed").
pub const RED: &str = "\x1b[31m";
/// Reserved for warnings.
pub const YELLOW: &str = "\x1b[33m";
/// A currently building target ("x Building ...").
pub const CYAN: &str = "\x1b[36m";
