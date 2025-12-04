use std::ops::{Range, RangeBounds};
use std::slice;

pub trait ToSubStr: Sized {
    /// Turns string into substring without re-allocation.
    ///
    /// Returns number of removed bytes
    ///
    /// # Panics
    ///
    /// This function will panic if either range exceeds the end of the slice,
    /// or if the end of `src` is before the start
    /// as well as if range forms an invalid utf-8 sequence
    fn to_substr<R: RangeBounds<usize>>(&mut self, range: R);
}

impl ToSubStr for String {
    fn to_substr<R: RangeBounds<usize>>(&mut self, range: R) {
        let Range { start, end } = slice::range(range, ..self.len());
        if !self.is_char_boundary(start) || !self.is_char_boundary(end) {
            panic!("invalid str range");
        }

        unsafe {
            let bytes = self.as_mut_vec();

            // SAFETY: range boundaries are checked above
            bytes.copy_within(start..end, 0);
            bytes.truncate(end - start);
        };
    }
}
