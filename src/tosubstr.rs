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

#[cfg(test)]
mod tests {
    use crate::tosubstr::ToSubStr;

    #[test]
    fn empty() {
        let mut src = "".to_string();
        src.to_substr(..);
        assert_eq!(src, "");
    }

    #[test]
    #[should_panic]
    fn utf8_split() {
        let mut src = "key: 💣".to_string();
        src.to_substr(..6);
    }

    #[test]
    fn end() {
        let mut src = "key: value".to_string();

        src.to_substr(5..);

        let expected = "value";
        assert_eq!(src, expected);
    }

    #[test]
    fn middle() {
        let mut src = r#""key": "value""#.to_string();

        src.to_substr(8..13);

        let expected = "value";
        assert_eq!(src, expected);
    }

    #[test]
    fn start() {
        let mut src = r#"key: "value""#.to_string();

        src.to_substr(0..3);

        let expected = "key";
        assert_eq!(src, expected);
    }
}
