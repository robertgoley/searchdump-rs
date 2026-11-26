//! Child `searchdump` processes: probing the binary, running it, reading its counts.

mod probe;
mod runner;
mod stats;

pub(crate) use probe::*;
pub(crate) use runner::*;
pub(crate) use stats::*;
