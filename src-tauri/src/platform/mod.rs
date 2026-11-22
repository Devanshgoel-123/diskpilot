//! The few operating-system specifics the shared code needs, behind one
//! small interface: file identity, allocated size, volume capacity, and
//! local time.
//!
//! On Windows, junctions, symbolic links and volume mount points are all
//! reparse points that `symlink_metadata` reports as links, and the shared
//! code never follows links. So a walk that starts on one volume stays there
//! without comparing device numbers, and [`device`] is the same for every
//! file.

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

/// What a file is, independent of the name it is reached by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub volume: u64,
    pub index: u64,
    /// Number of names (hard links) the file has.
    pub links: u64,
}
