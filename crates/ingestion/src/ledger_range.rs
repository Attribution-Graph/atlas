//! Configurable ledger range for ingest windows.
//!
//! Provides a validated [`LedgerRange`] type that represents a closed interval
//! `[start, end]` of Stellar ledger sequence numbers. Handles construction,
//! validation, splitting into sub-ranges, and iteration.

use std::fmt;

use crate::models::LedgerSequence;

/// The minimum valid ledger sequence on the Stellar network.
pub const GENESIS_LEDGER: LedgerSequence = 1;

/// Maximum recommended window size to avoid excessive memory use in a single fetch.
pub const MAX_WINDOW_SIZE: u32 = 10_000;

/// A validated, closed ledger range `[start, end]`.
///
/// Invariant: `start <= end` and both are `>= GENESIS_LEDGER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LedgerRange {
    start: LedgerSequence,
    end: LedgerSequence,
}

/// Errors that can occur when constructing or using a [`LedgerRange`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LedgerRangeError {
    #[error("start ledger {start} must be <= end ledger {end}")]
    InvalidRange {
        start: LedgerSequence,
        end: LedgerSequence,
    },

    #[error("ledger sequence {0} is below genesis ledger ({GENESIS_LEDGER})")]
    BelowGenesis(LedgerSequence),

    #[error(
        "window size {size} exceeds maximum recommended size {MAX_WINDOW_SIZE}; use split() to chunk"
    )]
    WindowTooLarge { size: u32 },
}

impl LedgerRange {
    /// Create a new ledger range, validating the bounds.
    ///
    /// # Errors
    /// Returns an error if `start > end` or if either bound is 0.
    pub fn new(start: LedgerSequence, end: LedgerSequence) -> Result<Self, LedgerRangeError> {
        if start < GENESIS_LEDGER {
            return Err(LedgerRangeError::BelowGenesis(start));
        }
        if end < GENESIS_LEDGER {
            return Err(LedgerRangeError::BelowGenesis(end));
        }
        if start > end {
            return Err(LedgerRangeError::InvalidRange { start, end });
        }
        Ok(Self { start, end })
    }

    /// Create a ledger range for a single ledger.
    pub fn single(ledger: LedgerSequence) -> Result<Self, LedgerRangeError> {
        Self::new(ledger, ledger)
    }

    /// Return the start of the range (inclusive).
    pub fn start(&self) -> LedgerSequence {
        self.start
    }

    /// Return the end of the range (inclusive).
    pub fn end(&self) -> LedgerSequence {
        self.end
    }

    /// Return the number of ledgers in this range.
    pub fn size(&self) -> u32 {
        self.end - self.start + 1
    }

    /// Check whether a given ledger is within this range.
    pub fn contains(&self, ledger: LedgerSequence) -> bool {
        ledger >= self.start && ledger <= self.end
    }

    /// Check whether this range overlaps with another.
    pub fn overlaps(&self, other: &LedgerRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    /// Split this range into sub-ranges of at most `chunk_size` ledgers each.
    ///
    /// Useful for processing very large ranges without holding all transactions
    /// in memory at once.
    ///
    /// # Panics
    /// Panics if `chunk_size` is 0.
    pub fn split(&self, chunk_size: u32) -> Vec<LedgerRange> {
        assert!(chunk_size > 0, "chunk_size must be > 0");
        let mut chunks = Vec::new();
        let mut current = self.start;
        while current <= self.end {
            let chunk_end = (current + chunk_size - 1).min(self.end);
            // Safety: current <= end and chunk_end <= end, so this is valid
            chunks.push(LedgerRange {
                start: current,
                end: chunk_end,
            });
            current = chunk_end + 1;
        }
        chunks
    }

    /// Warn if the window size exceeds the recommended maximum.
    pub fn check_size(&self) -> Result<(), LedgerRangeError> {
        if self.size() > MAX_WINDOW_SIZE {
            return Err(LedgerRangeError::WindowTooLarge { size: self.size() });
        }
        Ok(())
    }
}

impl fmt::Display for LedgerRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}..{}]", self.start, self.end)
    }
}

/// An iterator over individual ledger sequences within a [`LedgerRange`].
pub struct LedgerRangeIter {
    current: LedgerSequence,
    end: LedgerSequence,
    done: bool,
}

impl IntoIterator for LedgerRange {
    type Item = LedgerSequence;
    type IntoIter = LedgerRangeIter;

    fn into_iter(self) -> Self::IntoIter {
        LedgerRangeIter {
            current: self.start,
            end: self.end,
            done: false,
        }
    }
}

impl Iterator for LedgerRangeIter {
    type Item = LedgerSequence;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.current > self.end {
            return None;
        }
        let val = self.current;
        if self.current == self.end {
            self.done = true;
        } else {
            self.current += 1;
        }
        Some(val)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.done || self.current > self.end {
            (0, Some(0))
        } else {
            let remaining = (self.end - self.current + 1) as usize;
            (remaining, Some(remaining))
        }
    }
}

impl ExactSizeIterator for LedgerRangeIter {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_range() {
        let r = LedgerRange::new(100, 200).unwrap();
        assert_eq!(r.start(), 100);
        assert_eq!(r.end(), 200);
        assert_eq!(r.size(), 101);
    }

    #[test]
    fn test_single_ledger() {
        let r = LedgerRange::single(500).unwrap();
        assert_eq!(r.size(), 1);
        assert!(r.contains(500));
    }

    #[test]
    fn test_invalid_range_start_after_end() {
        let err = LedgerRange::new(200, 100).unwrap_err();
        assert_eq!(
            err,
            LedgerRangeError::InvalidRange {
                start: 200,
                end: 100
            }
        );
    }

    #[test]
    fn test_below_genesis() {
        let err = LedgerRange::new(0, 100).unwrap_err();
        assert_eq!(err, LedgerRangeError::BelowGenesis(0));
    }

    #[test]
    fn test_contains() {
        let r = LedgerRange::new(100, 200).unwrap();
        assert!(r.contains(100));
        assert!(r.contains(150));
        assert!(r.contains(200));
        assert!(!r.contains(99));
        assert!(!r.contains(201));
    }

    #[test]
    fn test_overlaps() {
        let r1 = LedgerRange::new(100, 200).unwrap();
        let r2 = LedgerRange::new(150, 250).unwrap();
        let r3 = LedgerRange::new(201, 300).unwrap();
        assert!(r1.overlaps(&r2));
        assert!(!r1.overlaps(&r3));
    }

    #[test]
    fn test_split_even() {
        let r = LedgerRange::new(1, 10).unwrap();
        let chunks = r.split(5);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], LedgerRange::new(1, 5).unwrap());
        assert_eq!(chunks[1], LedgerRange::new(6, 10).unwrap());
    }

    #[test]
    fn test_split_uneven() {
        let r = LedgerRange::new(1, 11).unwrap();
        let chunks = r.split(5);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[2].size(), 1); // ledger 11 alone
    }

    #[test]
    fn test_iterate() {
        let r = LedgerRange::new(1, 5).unwrap();
        let ledgers: Vec<u32> = r.into_iter().collect();
        assert_eq!(ledgers, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_exact_size_iterator() {
        let r = LedgerRange::new(10, 20).unwrap();
        assert_eq!(r.into_iter().len(), 11);
    }

    #[test]
    fn test_display() {
        let r = LedgerRange::new(1000, 2000).unwrap();
        assert_eq!(r.to_string(), "[1000..2000]");
    }

    #[test]
    fn test_check_size_ok() {
        let r = LedgerRange::new(1, 100).unwrap();
        assert!(r.check_size().is_ok());
    }

    #[test]
    fn test_check_size_too_large() {
        let r = LedgerRange::new(1, MAX_WINDOW_SIZE + 1).unwrap();
        assert!(matches!(
            r.check_size(),
            Err(LedgerRangeError::WindowTooLarge { .. })
        ));
    }
}
