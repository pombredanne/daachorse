//! # 🐎 Daac Horse: Double-Array Aho-Corasick
//!
//! A fast implementation of the Aho-Corasick algorithm
//! using the compact double-array data structure.
//!
//! ## Overview
//!
//! `daachorse` is a crate for fast multiple pattern matching using
//! the [Aho-Corasick algorithm](https://dl.acm.org/doi/10.1145/360825.360855),
//! running in linear time over the length of the input text.
//! For time- and memory-efficiency, the pattern match automaton is implemented using
//! the [compact double-array data structure](https://doi.org/10.1016/j.ipm.2006.04.004).
//! The data structure not only supports constant-time state-to-state traversal,
//! but also represents each state in a compact space of only 12 bytes.
//!
//! ## Example: Finding overlapped occurrences
//!
//! To search for all occurrences of registered patterns that allow for positional overlap in the
//! input text, use [`DoubleArrayAhoCorasick::find_overlapping_iter()`].
//!
//! When you use [`DoubleArrayAhoCorasick::new()`] for constraction,
//! unique identifiers are assigned to each pattern in the input order.
//! The match result has the byte positions of the occurrence and its identifier.
//!
//! ```
//! use daachorse::DoubleArrayAhoCorasick;
//!
//! let patterns = vec!["bcd", "ab", "a"];
//! let pma = DoubleArrayAhoCorasick::new(patterns).unwrap();
//!
//! let mut it = pma.find_overlapping_iter("abcd");
//!
//! let m = it.next().unwrap();
//! assert_eq!((0, 1, 2), (m.start(), m.end(), m.value()));
//!
//! let m = it.next().unwrap();
//! assert_eq!((0, 2, 1), (m.start(), m.end(), m.value()));
//!
//! let m = it.next().unwrap();
//! assert_eq!((1, 4, 0), (m.start(), m.end(), m.value()));
//!
//! assert_eq!(None, it.next());
//! ```
//!
//! ## Example: Finding non-overlapped occurrences
//!
//! If you do not want to allow positional overlap,
//! use [`DoubleArrayAhoCorasick::find_iter()`] instead.
//!
//! ```
//! use daachorse::DoubleArrayAhoCorasick;
//!
//! let patterns = vec!["bcd", "ab", "a"];
//! let pma = DoubleArrayAhoCorasick::new(patterns).unwrap();
//!
//! let mut it = pma.find_iter("abcd");
//!
//! let m = it.next().unwrap();
//! assert_eq!((0, 1, 2), (m.start(), m.end(), m.value()));
//!
//! let m = it.next().unwrap();
//! assert_eq!((1, 4, 0), (m.start(), m.end(), m.value()));
//!
//! assert_eq!(None, it.next());
//! ```
//!
//! ## Example: Associating arbitrary values with patterns
//!
//! To build the automaton from pairs of a pattern and integer value instead of assigning
//! identifiers automatically, use [`DoubleArrayAhoCorasick::with_values()`].
//!
//! ```
//! use daachorse::DoubleArrayAhoCorasick;
//!
//! let patvals = vec![("bcd", 0), ("ab", 10), ("a", 20)];
//! let pma = DoubleArrayAhoCorasick::with_values(patvals).unwrap();
//!
//! let mut it = pma.find_overlapping_iter("abcd");
//!
//! let m = it.next().unwrap();
//! assert_eq!((0, 1, 20), (m.start(), m.end(), m.value()));
//!
//! let m = it.next().unwrap();
//! assert_eq!((0, 2, 10), (m.start(), m.end(), m.value()));
//!
//! let m = it.next().unwrap();
//! assert_eq!((1, 4, 0), (m.start(), m.end(), m.value()));
//!
//! assert_eq!(None, it.next());
//! ```
mod builder;
pub mod errors;
#[cfg(test)]
mod tests_fixed;
#[cfg(test)]
mod tests_random;

pub use builder::DoubleArrayAhoCorasickBuilder;
use errors::DaachorseError;

// The maximum BASE value used as an invalid value.
pub(crate) const BASE_INVALID: u32 = std::u32::MAX;
// The maximum output position value used as an invalid value.
pub(crate) const OUTPUT_POS_INVALID: u32 = std::u32::MAX;
// The maximum FAIL value.
pub(crate) const FAIL_MAX: u32 = 0xFF_FFFF;
// The mask value of FAIL for `State::fach`.
const FAIL_MASK: u32 = FAIL_MAX << 8;
// The mask value of CEHCK for `State::fach`.
const CHECK_MASK: u32 = 0xFF;
// The root index position.
pub(crate) const ROOT_STATE_IDX: u32 = 0;
// The dead index position.
pub(crate) const DEAD_STATE_IDX: u32 = 1;

#[derive(Clone, Copy)]
struct State {
    base: u32,
    fach: u32,
    output_pos: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            base: BASE_INVALID,
            fach: 0,
            output_pos: OUTPUT_POS_INVALID,
        }
    }
}

impl State {
    #[inline(always)]
    pub fn base(&self) -> Option<u32> {
        Some(self.base).filter(|&x| x != BASE_INVALID)
    }

    #[inline(always)]
    pub const fn check(&self) -> u8 {
        #![allow(clippy::cast_possible_truncation)]
        (self.fach & 0xFF) as u8
    }

    #[inline(always)]
    pub const fn fail(&self) -> u32 {
        self.fach >> 8
    }

    #[inline(always)]
    pub fn output_pos(&self) -> Option<u32> {
        Some(self.output_pos).filter(|&x| x != OUTPUT_POS_INVALID)
    }

    #[inline(always)]
    pub fn set_base(&mut self, x: u32) {
        self.base = x;
    }

    #[inline(always)]
    pub fn set_check(&mut self, x: u8) {
        self.fach &= !CHECK_MASK;
        self.fach |= u32::from(x);
    }

    #[inline(always)]
    pub fn set_fail(&mut self, x: u32) {
        self.fach &= !FAIL_MASK;
        self.fach |= x << 8;
    }

    #[inline(always)]
    pub fn set_output_pos(&mut self, x: u32) {
        self.output_pos = x;
    }
}

#[derive(Copy, Clone)]
struct Output {
    value: u32,
    length: u32, // 1 bit is borrowed by a beginning flag
}

impl Output {
    #[inline(always)]
    pub fn new(value: u32, length: u32, is_begin: bool) -> Self {
        Self {
            value,
            length: (length << 1) | u32::from(is_begin),
        }
    }

    #[inline(always)]
    pub const fn value(self) -> u32 {
        self.value
    }

    #[inline(always)]
    pub const fn length(self) -> u32 {
        self.length >> 1
    }

    #[inline(always)]
    pub const fn is_begin(self) -> bool {
        self.length & 1 == 1
    }
}

/// Match result.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Match {
    length: usize,
    end: usize,
    value: usize,
}

impl Match {
    /// Starting position of the match.
    #[inline(always)]
    pub const fn start(&self) -> usize {
        self.end - self.length
    }

    /// Ending position of the match.
    #[inline(always)]
    pub const fn end(&self) -> usize {
        self.end
    }

    /// Value associated with the pattern.
    #[inline(always)]
    pub const fn value(&self) -> usize {
        self.value
    }
}

/// Iterator created by [`DoubleArrayAhoCorasick::find_iter()`].
pub struct FindIterator<'a, P>
where
    P: AsRef<[u8]>,
{
    pma: &'a DoubleArrayAhoCorasick,
    haystack: P,
    pos: usize,
}

impl<'a, P> Iterator for FindIterator<'a, P>
where
    P: AsRef<[u8]>,
{
    type Item = Match;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let mut state_id = ROOT_STATE_IDX;
        let haystack = self.haystack.as_ref();
        for (pos, &c) in haystack.iter().enumerate().skip(self.pos) {
            // state_id is always smaller than self.pma.states.len() because
            // self.pma.get_next_state_id_unchecked() ensures to return such a value.
            state_id = unsafe { self.pma.get_next_state_id_unchecked(state_id, c) };
            if let Some(output_pos) = unsafe {
                self.pma
                    .states
                    .get_unchecked(state_id as usize)
                    .output_pos()
            } {
                // output_pos is always smaller than self.pma.outputs.len() because
                // State::output_pos() ensures to return such a value when it is Some.
                let out = unsafe { self.pma.outputs.get_unchecked(output_pos as usize) };
                self.pos = pos + 1;
                return Some(Match {
                    length: out.length() as usize,
                    end: self.pos,
                    value: out.value() as usize,
                });
            }
        }
        self.pos = haystack.len();
        None
    }
}

/// Iterator created by [`DoubleArrayAhoCorasick::find_overlapping_iter()`].
pub struct FindOverlappingIterator<'a, P>
where
    P: AsRef<[u8]>,
{
    pma: &'a DoubleArrayAhoCorasick,
    haystack: P,
    state_id: u32,
    pos: usize,
    output_pos: usize,
}

impl<'a, P> Iterator for FindOverlappingIterator<'a, P>
where
    P: AsRef<[u8]>,
{
    type Item = Match;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // self.output_pos is always smaller than self.pma.outputs.len() because
        // State::output_pos() ensures to return such a value when it is Some.
        let out = unsafe { self.pma.outputs.get_unchecked(self.output_pos) };
        if !out.is_begin() {
            self.output_pos += 1;
            return Some(Match {
                length: out.length() as usize,
                end: self.pos,
                value: out.value() as usize,
            });
        }
        let haystack = self.haystack.as_ref();
        for (pos, &c) in haystack.iter().enumerate().skip(self.pos) {
            // self.state_id is always smaller than self.pma.states.len() because
            // self.pma.get_next_state_id_unchecked() ensures to return such a value.
            self.state_id = unsafe { self.pma.get_next_state_id_unchecked(self.state_id, c) };
            if let Some(output_pos) = unsafe {
                self.pma
                    .states
                    .get_unchecked(self.state_id as usize)
                    .output_pos()
            } {
                self.pos = pos + 1;
                self.output_pos = output_pos as usize + 1;
                let out = unsafe { self.pma.outputs.get_unchecked(output_pos as usize) };
                return Some(Match {
                    length: out.length() as usize,
                    end: self.pos,
                    value: out.value() as usize,
                });
            }
        }
        self.pos = haystack.len();
        None
    }
}

/// Iterator created by [`DoubleArrayAhoCorasick::find_overlapping_no_suffix_iter()`].
pub struct FindOverlappingNoSuffixIterator<'a, P>
where
    P: AsRef<[u8]>,
{
    pma: &'a DoubleArrayAhoCorasick,
    haystack: P,
    state_id: u32,
    pos: usize,
}

impl<'a, P> Iterator for FindOverlappingNoSuffixIterator<'a, P>
where
    P: AsRef<[u8]>,
{
    type Item = Match;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let haystack = self.haystack.as_ref();
        for (pos, &c) in haystack.iter().enumerate().skip(self.pos) {
            // self.state_id is always smaller than self.pma.states.len() because
            // self.pma.get_next_state_id_unchecked() ensures to return such a value.
            self.state_id = unsafe { self.pma.get_next_state_id_unchecked(self.state_id, c) };
            if let Some(output_pos) = unsafe {
                self.pma
                    .states
                    .get_unchecked(self.state_id as usize)
                    .output_pos()
            } {
                // output_pos is always smaller than self.pma.outputs.len() because
                // State::output_pos() ensures to return such a value when it is Some.
                let out = unsafe { self.pma.outputs.get_unchecked(output_pos as usize) };
                self.pos = pos + 1;
                return Some(Match {
                    length: out.length() as usize,
                    end: self.pos,
                    value: out.value() as usize,
                });
            }
        }
        self.pos = haystack.len();
        None
    }
}

/// Iterator created by [`DoubleArrayAhoCorasick::leftmost_find_iter()`].
pub struct LestmostFindIterator<'a, P>
where
    P: AsRef<[u8]>,
{
    pma: &'a DoubleArrayAhoCorasick,
    haystack: P,
    pos: usize,
}

impl<'a, P> Iterator for LestmostFindIterator<'a, P>
where
    P: AsRef<[u8]>,
{
    type Item = Match;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let mut state_id = ROOT_STATE_IDX;
        let mut last_output_pos = OUTPUT_POS_INVALID;

        let haystack = self.haystack.as_ref();
        for (pos, &c) in haystack.iter().enumerate().skip(self.pos) {
            state_id = unsafe { self.pma.get_next_state_id_leftmost_unchecked(state_id, c) };
            if state_id == DEAD_STATE_IDX {
                debug_assert_ne!(last_output_pos, OUTPUT_POS_INVALID);
                break;
            }

            // state_id is always smaller than self.pma.states.len() because
            // self.pma.get_next_state_id_leftmost_unchecked() ensures to return such a value.
            if let Some(output_pos) = unsafe {
                self.pma
                    .states
                    .get_unchecked(state_id as usize)
                    .output_pos()
            } {
                last_output_pos = output_pos;
                self.pos = pos + 1;
            }
        }

        if last_output_pos == OUTPUT_POS_INVALID {
            None
        } else {
            // last_output_pos is always smaller than self.pma.outputs.len() because
            // State::output_pos() ensures to return such a value when it is Some.
            let out = unsafe { self.pma.outputs.get_unchecked(last_output_pos as usize) };
            Some(Match {
                length: out.length() as usize,
                end: self.pos,
                value: out.value() as usize,
            })
        }
    }
}

/// Fast multiple pattern match automaton implemented
/// with the Aho-Corasick algorithm and compact double-array data structure.
///
/// [`DoubleArrayAhoCorasick`] implements a pattern match automaton based on
/// the [Aho-Corasick algorithm](https://dl.acm.org/doi/10.1145/360825.360855),
/// supporting linear-time pattern matching.
/// The internal data structure employs
/// the [compact double-array structure](https://doi.org/10.1016/j.ipm.2006.04.004)
/// that is the fastest trie representation technique.
/// It supports constant-time state-to-state traversal,
/// allowing for very fast pattern matching.
/// Moreover, each state is represented in a compact space of only 12 bytes.
///
/// # Build instructions
///
/// [`DoubleArrayAhoCorasick`] supports the following two types of input data:
///
/// - [`DoubleArrayAhoCorasick::new`] builds an automaton from a set of byte strings
///    while assigning unique identifiers in the input order.
///
/// - [`DoubleArrayAhoCorasick::with_values`] builds an automaton
///    from a set of pairs of a byte string and a `u32` value.
///
/// # Limitations
///
/// For memory- and cache-efficiency, a FAIL pointer is represented in 24 bits.
/// Thus, if a very large pattern set is given, [`DaachorseError`] will be reported.
pub struct DoubleArrayAhoCorasick {
    states: Vec<State>,
    outputs: Vec<Output>,
    match_kind: MatchKind,
    num_states: usize,
}

impl DoubleArrayAhoCorasick {
    /// Creates a new [`DoubleArrayAhoCorasick`] from input patterns.
    /// The value `i` is automatically associated with `patterns[i]`.
    ///
    /// # Arguments
    ///
    /// * `patterns` - List of patterns.
    ///
    /// # Errors
    ///
    /// [`DaachorseError`] is returned when
    ///   - the `patterns` contains duplicate entries,
    ///   - the scale of `patterns` exceeds the expected one, or
    ///   - the scale of the resulting automaton exceeds the expected one.
    ///
    /// # Examples
    ///
    /// ```
    /// use daachorse::DoubleArrayAhoCorasick;
    ///
    /// let patterns = vec!["bcd", "ab", "a"];
    /// let pma = DoubleArrayAhoCorasick::new(patterns).unwrap();
    ///
    /// let mut it = pma.find_iter("abcd");
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((0, 1, 2), (m.start(), m.end(), m.value()));
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((1, 4, 0), (m.start(), m.end(), m.value()));
    ///
    /// assert_eq!(None, it.next());
    /// ```
    pub fn new<I, P>(patterns: I) -> Result<Self, DaachorseError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<[u8]>,
    {
        DoubleArrayAhoCorasickBuilder::new().build(patterns)
    }

    /// Creates a new [`DoubleArrayAhoCorasick`] from input pattern-value pairs.
    ///
    /// # Arguments
    ///
    /// * `patvals` - List of pattern-value pairs, in which the value is of type `u32` and less than `u32::MAX`.
    ///
    /// # Errors
    ///
    /// [`DaachorseError`] is returned when
    ///   - the `patvals` contains duplicate patterns,
    ///   - the scale of `patvals` exceeds the expected one, or
    ///   - the scale of the resulting automaton exceeds the expected one.
    ///
    /// # Examples
    ///
    /// ```
    /// use daachorse::DoubleArrayAhoCorasick;
    ///
    /// let patvals = vec![("bcd", 0), ("ab", 1), ("a", 2), ("e", 1)];
    /// let pma = DoubleArrayAhoCorasick::with_values(patvals).unwrap();
    ///
    /// let mut it = pma.find_iter("abcde");
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((0, 1, 2), (m.start(), m.end(), m.value()));
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((1, 4, 0), (m.start(), m.end(), m.value()));
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((4, 5, 1), (m.start(), m.end(), m.value()));
    ///
    /// assert_eq!(None, it.next());
    /// ```
    pub fn with_values<I, P>(patvals: I) -> Result<Self, DaachorseError>
    where
        I: IntoIterator<Item = (P, u32)>,
        P: AsRef<[u8]>,
    {
        DoubleArrayAhoCorasickBuilder::new().build_with_values(patvals)
    }

    /// Returns an iterator of non-overlapping matches in the given haystack.
    ///
    /// # Arguments
    ///
    /// * `haystack` - String to search for.
    ///
    /// # Panics
    ///
    /// When you specify `MatchKind::{LeftmostFirst,LeftmostLongest}` in the construction,
    /// the iterator is not supported and the function will call panic!.
    ///
    /// # Examples
    ///
    /// ```
    /// use daachorse::DoubleArrayAhoCorasick;
    ///
    /// let patterns = vec!["bcd", "ab", "a"];
    /// let pma = DoubleArrayAhoCorasick::new(patterns).unwrap();
    ///
    /// let mut it = pma.find_iter("abcd");
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((0, 1, 2), (m.start(), m.end(), m.value()));
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((1, 4, 0), (m.start(), m.end(), m.value()));
    ///
    /// assert_eq!(None, it.next());
    /// ```
    pub fn find_iter<P>(&self, haystack: P) -> FindIterator<P>
    where
        P: AsRef<[u8]>,
    {
        assert!(
            self.match_kind.is_standard(),
            "Error: match_kind must be standard."
        );
        FindIterator {
            pma: self,
            haystack,
            pos: 0,
        }
    }

    /// Returns an iterator of overlapping matches in the given haystack.
    ///
    /// # Arguments
    ///
    /// * `haystack` - String to search for.
    ///
    /// # Panics
    ///
    /// When you specify `MatchKind::{LeftmostFirst,LeftmostLongest}` in the construction,
    /// the iterator is not supported and the function will call panic!.
    ///
    /// # Examples
    ///
    /// ```
    /// use daachorse::DoubleArrayAhoCorasick;
    ///
    /// let patterns = vec!["bcd", "ab", "a"];
    /// let pma = DoubleArrayAhoCorasick::new(patterns).unwrap();
    ///
    /// let mut it = pma.find_overlapping_iter("abcd");
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((0, 1, 2), (m.start(), m.end(), m.value()));
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((0, 2, 1), (m.start(), m.end(), m.value()));
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((1, 4, 0), (m.start(), m.end(), m.value()));
    ///
    /// assert_eq!(None, it.next());
    /// ```
    pub fn find_overlapping_iter<P>(&self, haystack: P) -> FindOverlappingIterator<P>
    where
        P: AsRef<[u8]>,
    {
        assert!(
            self.match_kind.is_standard(),
            "Error: match_kind must be standard."
        );
        FindOverlappingIterator {
            pma: self,
            haystack,
            state_id: ROOT_STATE_IDX,
            pos: 0,
            output_pos: 0,
        }
    }

    /// Returns an iterator of overlapping matches without suffixes in the given haystack.
    ///
    /// The Aho-Corasick algorithm reads through the haystack from left to right and reports
    /// matches when it reaches the end of each pattern. In the overlapping match, more than one
    /// pattern can be returned per report.
    ///
    /// This iterator returns the first match on each report.
    ///
    /// # Arguments
    ///
    /// * `haystack` - String to search for.
    ///
    /// # Panics
    ///
    /// When you specify `MatchKind::{LeftmostFirst,LeftmostLongest}` in the construction,
    /// the iterator is not supported and the function will call panic!.
    ///
    /// # Examples
    ///
    /// ```
    /// use daachorse::DoubleArrayAhoCorasick;
    ///
    /// let patterns = vec!["bcd", "cd", "abc"];
    /// let pma = DoubleArrayAhoCorasick::new(patterns).unwrap();
    ///
    /// let mut it = pma.find_overlapping_no_suffix_iter("abcd");
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((0, 3, 2), (m.start(), m.end(), m.value()));
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((1, 4, 0), (m.start(), m.end(), m.value()));
    ///
    /// assert_eq!(None, it.next());
    /// ```
    pub fn find_overlapping_no_suffix_iter<P>(
        &self,
        haystack: P,
    ) -> FindOverlappingNoSuffixIterator<P>
    where
        P: AsRef<[u8]>,
    {
        assert!(
            self.match_kind.is_standard(),
            "Error: match_kind must be standard."
        );
        FindOverlappingNoSuffixIterator {
            pma: self,
            haystack,
            state_id: ROOT_STATE_IDX,
            pos: 0,
        }
    }

    /// Returns an iterator of leftmost matches in the given haystack.
    ///
    /// The leftmost match greedily searches the longest possible match at each iteration, and
    /// the match results do not overlap positionally such as [`DoubleArrayAhoCorasick::find_iter()`].
    ///
    /// According to the [`MatchKind`] option you specified in the construction,
    /// the behavior is changed for multiple possible matches, as follows.
    ///
    ///  - If you set [`MatchKind::LeftmostLongest`], it reports the match
    ///    corresponding to the longest pattern.
    ///
    ///  - If you set [`MatchKind::LeftmostFirst`], it reports the match
    ///    corresponding to the pattern earlier registered to the automaton.
    ///
    /// # Arguments
    ///
    /// * `haystack` - String to search for.
    ///
    /// # Panics
    ///
    /// When you do not specify `MatchKind::{LeftmostFirst,LeftmostLongest}` in the construction,
    /// the iterator is not supported and the function will call panic!.
    ///
    /// # Examples
    ///
    /// ## LeftmostLongest
    ///
    /// ```
    /// use daachorse::{DoubleArrayAhoCorasickBuilder, MatchKind};
    ///
    /// let patterns = vec!["ab", "abcd"];
    /// let pma = DoubleArrayAhoCorasickBuilder::new()
    ///           .match_kind(MatchKind::LeftmostLongest)
    ///           .build(&patterns)
    ///           .unwrap();
    ///
    /// let mut it = pma.leftmost_find_iter("abcd");
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((0, 4, 1), (m.start(), m.end(), m.value()));
    ///
    /// assert_eq!(None, it.next());
    /// ```
    ///
    /// ## LeftmostFirst
    ///
    /// ```
    /// use daachorse::{DoubleArrayAhoCorasickBuilder, MatchKind};
    ///
    /// let patterns = vec!["ab", "abcd"];
    /// let pma = DoubleArrayAhoCorasickBuilder::new()
    ///           .match_kind(MatchKind::LeftmostFirst)
    ///           .build(&patterns)
    ///           .unwrap();
    ///
    /// let mut it = pma.leftmost_find_iter("abcd");
    ///
    /// let m = it.next().unwrap();
    /// assert_eq!((0, 2, 0), (m.start(), m.end(), m.value()));
    ///
    /// assert_eq!(None, it.next());
    /// ```
    pub fn leftmost_find_iter<P>(&self, haystack: P) -> LestmostFindIterator<P>
    where
        P: AsRef<[u8]>,
    {
        assert!(
            self.match_kind.is_leftmost(),
            "Error: match_kind must be leftmost."
        );
        LestmostFindIterator {
            pma: self,
            haystack,
            pos: 0,
        }
    }

    /// Returns the total amount of heap used by this automaton in bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use daachorse::DoubleArrayAhoCorasick;
    ///
    /// let patterns = vec!["bcd", "ab", "a"];
    /// let pma = DoubleArrayAhoCorasick::new(patterns).unwrap();
    ///
    /// assert_eq!(pma.heap_bytes(), 3104);
    /// ```
    pub fn heap_bytes(&self) -> usize {
        self.states.len() * std::mem::size_of::<State>()
            + self.outputs.len() * std::mem::size_of::<Output>()
    }

    /// Returns the total number of states this automaton has.
    ///
    /// # Examples
    ///
    /// ```
    /// use daachorse::DoubleArrayAhoCorasick;
    ///
    /// let patterns = vec!["bcd", "ab", "a"];
    /// let pma = DoubleArrayAhoCorasick::new(patterns).unwrap();
    ///
    /// assert_eq!(pma.num_states(), 6);
    /// ```
    pub const fn num_states(&self) -> usize {
        self.num_states
    }

    /// # Safety
    ///
    /// `state_id` must be smaller than the length of states.
    #[inline(always)]
    unsafe fn get_child_index_unchecked(&self, state_id: u32, c: u8) -> Option<u32> {
        // child_idx is always smaller than states.len() because
        //  - states.len() is 256 * k for some integer k, and
        //  - base() returns smaller than states.len() when it is Some.
        self.states
            .get_unchecked(state_id as usize)
            .base()
            .and_then(|base| {
                let child_idx = base ^ u32::from(c);
                Some(child_idx).filter(|&x| self.states.get_unchecked(x as usize).check() == c)
            })
    }

    /// # Safety
    ///
    /// `state_id` must be smaller than the length of states.
    #[inline(always)]
    unsafe fn get_next_state_id_unchecked(&self, mut state_id: u32, c: u8) -> u32 {
        // In the loop, state_id is always set to values smaller than states.len(),
        // because get_child_index_unchecked() and fail() return such values.
        loop {
            if let Some(state_id) = self.get_child_index_unchecked(state_id, c) {
                return state_id;
            }
            if state_id == ROOT_STATE_IDX {
                return ROOT_STATE_IDX;
            }
            state_id = self.states.get_unchecked(state_id as usize).fail();
        }
    }

    /// # Safety
    ///
    /// `state_id` must be smaller than the length of states.
    #[inline(always)]
    unsafe fn get_next_state_id_leftmost_unchecked(&self, mut state_id: u32, c: u8) -> u32 {
        // In the loop, state_id is always set to values smaller than states.len(),
        // because get_child_index_unchecked() and fail() return such values.
        loop {
            if let Some(state_id) = self.get_child_index_unchecked(state_id, c) {
                return state_id;
            }
            if state_id == ROOT_STATE_IDX {
                return ROOT_STATE_IDX;
            }
            let fail_id = self.states.get_unchecked(state_id as usize).fail();
            if fail_id == DEAD_STATE_IDX {
                return DEAD_STATE_IDX;
            }
            state_id = fail_id;
        }
    }

    #[cfg(test)]
    #[inline(always)]
    fn get_child_index(&self, state_id: u32, c: u8) -> Option<u32> {
        self.states[state_id as usize].base().and_then(|base| {
            let child_idx = base ^ u32::from(c);
            Some(child_idx).filter(|&x| self.states[x as usize].check() == c)
        })
    }
}

/// An search option of the Aho-Corasick automaton
/// specified in [`DoubleArrayAhoCorasickBuilder::match_kind`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchKind {
    /// The standard match semantics, which enables
    /// [`find_iter()`](DoubleArrayAhoCorasick::find_iter()),\
    /// [`find_overlapping_iter()`](DoubleArrayAhoCorasick::find_overlapping_iter()), and
    /// [`find_overlapping_no_suffix_iter()`](DoubleArrayAhoCorasick::find_overlapping_no_suffix_iter()).
    /// Patterns are reported in the order that follows the normal behaviour of the Aho-Corasick
    /// algorithm.
    Standard,

    /// The leftmost-longest match semantics, which enables
    /// [`leftmost_find_iter()`](DoubleArrayAhoCorasick::leftmost_find_iter()).
    /// When multiple patterns are started from the same positions, the longest pattern will be
    /// reported. For example, when matching patterns `ab|a|abcd` over `abcd`, `abcd` will be
    /// reported.
    LeftmostLongest,

    /// The leftmost-first match semantics, which enables
    /// [`leftmost_find_iter()`](DoubleArrayAhoCorasick::leftmost_find_iter()).
    /// When multiple patterns are started from the same positions, the pattern that is registered
    /// earlier will be reported. For example, when matching patterns `ab|a|abcd` over `abcd`,
    /// `ab` will be reported.
    LeftmostFirst,
}

impl Default for MatchKind {
    fn default() -> Self {
        Self::Standard
    }
}

impl MatchKind {
    fn is_standard(self) -> bool {
        self == Self::Standard
    }

    fn is_leftmost(self) -> bool {
        self == Self::LeftmostFirst || self == Self::LeftmostLongest
    }

    pub(crate) fn is_leftmost_first(self) -> bool {
        self == Self::LeftmostFirst
    }
}
