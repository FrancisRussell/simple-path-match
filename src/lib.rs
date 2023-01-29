#![warn(clippy::pedantic)]
#![allow(clippy::uninlined_format_args, clippy::missing_errors_doc)]
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::{String, ToString as _};
use alloc::vec::Vec;
use beef::Cow;
use snafu::Snafu;

const PATH_CURRENT: &str = ".";
const PATH_PARENT: &str = "..";
const UNIX_SEP: &str = "/";
const WILDCARD_ANY: &str = "*";

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
enum PathComponent<'a> {
    Current,
    DirectoryMarker,
    Name(Cow<'a, str>),
    Parent,
    RootName(Cow<'a, str>),
}

impl alloc::fmt::Display for PathComponent<'_> {
    fn fmt(&self, formatter: &mut alloc::fmt::Formatter<'_>) -> Result<(), alloc::fmt::Error> {
        match self {
            PathComponent::Current => formatter.write_str(PATH_CURRENT),
            PathComponent::DirectoryMarker => Ok(()),
            PathComponent::Name(s) | PathComponent::RootName(s) => formatter.write_str(s),
            PathComponent::Parent => formatter.write_str(PATH_PARENT),
        }
    }
}

impl PathComponent<'_> {
    fn traversal_depth(&self) -> usize {
        match self {
            PathComponent::Current | PathComponent::DirectoryMarker => 0,
            PathComponent::Name(_) | PathComponent::RootName(_) | PathComponent::Parent => 1,
        }
    }

    fn into_owned(self) -> PathComponent<'static> {
        match self {
            PathComponent::Current => PathComponent::Current,
            PathComponent::Parent => PathComponent::Parent,
            PathComponent::DirectoryMarker => PathComponent::DirectoryMarker,
            PathComponent::Name(n) => PathComponent::Name(n.into_owned().into()),
            PathComponent::RootName(n) => PathComponent::RootName(n.into_owned().into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct StartsEndsWith(String, String);

impl alloc::fmt::Display for StartsEndsWith {
    fn fmt(&self, formatter: &mut alloc::fmt::Formatter<'_>) -> Result<(), alloc::fmt::Error> {
        formatter.write_str(&self.0)?;
        formatter.write_str(WILDCARD_ANY)?;
        formatter.write_str(&self.1)
    }
}

impl StartsEndsWith {
    pub fn matches(&self, name: &str) -> bool {
        name.starts_with(&self.0) && name.ends_with(&self.1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PatternComponent {
    Literal(PathComponent<'static>),
    StartsEndsWith(StartsEndsWith),
}

impl alloc::fmt::Display for PatternComponent {
    fn fmt(&self, formatter: &mut alloc::fmt::Formatter<'_>) -> Result<(), alloc::fmt::Error> {
        match self {
            PatternComponent::Literal(c) => c.fmt(formatter),
            PatternComponent::StartsEndsWith(m) => m.fmt(formatter),
        }
    }
}

/// Errors that can occur during pattern compilation
#[derive(Debug, Snafu)]
pub enum Error {
    /// The supplied pattern contained a parent traversal (`..`)
    #[snafu(display("Pattern must not contain parent traversals"))]
    NoParents,

    /// A wilcard was used in a component in an invalid way
    #[snafu(display("Only one wilcard allowed in component: `{}`", component))]
    WildcardPosition { component: String },
}

struct StringComponentIter<'a> {
    path_string: core::iter::Enumerate<core::str::Split<'a, &'a str>>,
    is_dir: bool,
}

impl<'a> StringComponentIter<'a> {
    pub fn new(path: &'a str, separator: &'a str) -> StringComponentIter<'a> {
        StringComponentIter {
            path_string: path.split(separator).enumerate(),
            is_dir: false,
        }
    }
}

impl<'a> Iterator for StringComponentIter<'a> {
    type Item = PathComponent<'a>;

    fn next(&mut self) -> Option<PathComponent<'a>> {
        for (idx, component) in self.path_string.by_ref() {
            self.is_dir = false;
            match component {
                "" => {
                    if idx == 0 {
                        return Some(PathComponent::RootName(component.into()));
                    }
                    self.is_dir = true;
                }
                PATH_CURRENT => return Some(PathComponent::Current),
                PATH_PARENT => return Some(PathComponent::Parent),
                _ => return Some(PathComponent::Name(component.into())),
            }
        }
        if self.is_dir {
            self.is_dir = false;
            Some(PathComponent::DirectoryMarker)
        } else {
            None
        }
    }
}

fn normalized<'a, I: IntoIterator<Item = PathComponent<'a>>>(components: I) -> Vec<PathComponent<'a>> {
    let components = components.into_iter();
    let mut result = Vec::with_capacity(components.size_hint().0);
    for component in components {
        match component {
            PathComponent::Name(_) | PathComponent::RootName(_) => result.push(component),
            PathComponent::DirectoryMarker => {
                if result.is_empty() {
                    result.push(PathComponent::Current);
                }
                result.push(PathComponent::DirectoryMarker);
            }
            PathComponent::Parent => match result.last() {
                None | Some(PathComponent::Parent) => result.push(PathComponent::Parent),
                Some(PathComponent::Name(_)) => drop(result.pop()),
                Some(PathComponent::RootName(_)) => {}
                Some(c) => panic!("Component found in unexpected place during normalization: {:?}", c),
            },
            PathComponent::Current => {}
        }
    }
    if result.is_empty() {
        result.push(PathComponent::Current);
    }
    result
}

fn path_to_pattern<'a, I: IntoIterator<Item = PathComponent<'a>>>(
    components: I,
) -> Result<Vec<PatternComponent>, Error> {
    let components = components.into_iter();
    let mut result = Vec::with_capacity(components.size_hint().0);
    for component in components {
        match component {
            PathComponent::Name(ref name) => {
                let matcher = if let Some(idx) = name.find(WILDCARD_ANY) {
                    let (start, end) = name.split_at(idx);
                    let (_, end) = end.split_at(WILDCARD_ANY.len());
                    if start.contains(WILDCARD_ANY) || end.contains(WILDCARD_ANY) {
                        return Err(Error::WildcardPosition {
                            component: name.to_string(),
                        });
                    }
                    PatternComponent::StartsEndsWith(StartsEndsWith(start.to_string(), end.to_string()))
                } else {
                    PatternComponent::Literal(component.into_owned())
                };
                result.push(matcher);
            }
            PathComponent::Parent => return Err(Error::NoParents),
            PathComponent::Current => {}
            PathComponent::DirectoryMarker => {
                if result.is_empty() {
                    result.push(PatternComponent::Literal(PathComponent::Current));
                }
                result.push(PatternComponent::Literal(component.into_owned()));
            }
            PathComponent::RootName(_) => {
                result.push(PatternComponent::Literal(component.into_owned()));
            }
        }
    }
    if result.is_empty() {
        result.push(PatternComponent::Literal(PathComponent::Current));
    }
    Ok(result)
}

#[derive(Clone, Debug)]
struct PathMatchNode {
    can_end: bool,
    literals: BTreeMap<PathComponent<'static>, PathMatchNode>,
    starts_ends_with: BTreeMap<StartsEndsWith, PathMatchNode>,
    min_traversals: usize,
    max_traversals: usize,
}

impl Default for PathMatchNode {
    fn default() -> PathMatchNode {
        PathMatchNode {
            can_end: false,
            literals: BTreeMap::new(),
            starts_ends_with: BTreeMap::new(),
            min_traversals: 0,
            max_traversals: usize::MAX,
        }
    }
}

impl alloc::fmt::Display for PathMatchNode {
    fn fmt(&self, formatter: &mut alloc::fmt::Formatter<'_>) -> Result<(), alloc::fmt::Error> {
        use alloc::fmt::Write as _;

        let literals_iter = self.literals.iter().map(|(k, v)| (k.to_string(), v));
        let matchers_iter = self.starts_ends_with.iter().map(|(k, v)| (k.to_string(), v));
        let subnodes_iter = literals_iter.chain(matchers_iter);
        let mut output = String::new();
        let mut has_multiple_options = false;
        for (idx, (k, v)) in subnodes_iter.enumerate() {
            if idx > 0 {
                output += "|";
                has_multiple_options = true;
            }
            output += &k;
            if v.can_end {
                output += "$";
            }
            if !v.is_empty() {
                output += UNIX_SEP;
                write!(&mut output, "{}", v)?;
            }
        }
        if has_multiple_options {
            formatter.write_str("(")?;
        }
        formatter.write_str(&output)?;
        if has_multiple_options {
            formatter.write_str(")")?;
        }
        Ok(())
    }
}

impl PathMatchNode {
    fn insert_component(&mut self, component: PatternComponent) -> &mut PathMatchNode {
        self.min_traversals = 0;
        self.max_traversals = usize::MAX;
        match component {
            PatternComponent::Literal(literal) => self.literals.entry(literal).or_default(),
            PatternComponent::StartsEndsWith(pattern) => self.starts_ends_with.entry(pattern).or_default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.starts_ends_with.is_empty() && self.literals.is_empty()
    }

    fn recompute_depth_bounds(&mut self) -> (usize, usize) {
        let min = &mut self.min_traversals;
        let max = &mut self.max_traversals;
        *min = if self.can_end { 0 } else { usize::MAX };
        *max = 0;
        let node_iter = self
            .literals
            .iter_mut()
            .map(|(k, v)| (k.traversal_depth(), v))
            .chain(self.starts_ends_with.values_mut().map(|v| (1, v)));
        for (component_depth, node) in node_iter {
            let (node_min, node_max) = node.recompute_depth_bounds();
            *min = core::cmp::min(*min, node_min + component_depth);
            *max = core::cmp::max(*max, node_max + component_depth);
        }
        (*min, *max)
    }

    pub fn insert(&mut self, mut pattern: Vec<PatternComponent>) {
        let mut node = self;
        for head in pattern.drain(..) {
            node = node.insert_component(head);
        }
        node.can_end = true;
    }

    pub fn matches(node: &PathMatchNode, path: &[PathComponent], match_prefix: bool) -> bool {
        let mut candidates = VecDeque::new();
        candidates.push_front((node, path));
        while let Some((node, path)) = candidates.pop_back() {
            let path = if match_prefix && path.first() == Some(&PathComponent::Current) {
                // It is invalid to do this in the non-prefix case, since we might need
                // to match ".". We need to do this for the prefix case since "." is a prefix
                // of any relative path, but won't match other paths.
                &path[1..]
            } else {
                path
            };
            let can_match = node.can_end || match_prefix;
            let path_is_dir_marker = path.len() == 1 && path.last() == Some(&PathComponent::DirectoryMarker);
            if path_is_dir_marker && can_match {
                return true;
            }
            if let Some(component) = path.first() {
                if let Some(matching_node) = node.literals.get(component) {
                    candidates.push_front((matching_node, &path[1..]));
                }
                for (name_matcher, matching_node) in &node.starts_ends_with {
                    if let PathComponent::Name(name) = component {
                        if name_matcher.matches(name) {
                            candidates.push_front((matching_node, &path[1..]));
                        }
                    }
                }
            } else if can_match {
                return true;
            }
        }
        false
    }
}

/// Matches against a path
#[derive(Clone, Debug)]
pub struct PathMatch {
    separator: String,
    match_tree: PathMatchNode,
}

impl alloc::fmt::Display for PathMatch {
    fn fmt(&self, formatter: &mut alloc::fmt::Formatter<'_>) -> Result<(), alloc::fmt::Error> {
        self.match_tree.fmt(formatter)
    }
}

impl PathMatch {
    /// Constructs a `PathMatch` for a single pattern.
    ///
    /// The pattern must use the forward slash as a separator. The following
    /// restrictions apply:
    /// * Each component must either be a literal name or can contain a single
    ///   asterisk (representing a wildcard) with an optional literal prefix and
    ///   suffix.
    /// * `?` is not supported.
    /// * The pattern must not contain parent traversals (`..`) but `.` is
    ///   supported.
    /// * No escaping of special characters is supported.
    ///
    /// Construction will return an error if parent traverals are present or
    /// a component contains multiple wildcard characters.
    ///
    /// The supplied separator is used when parsing the supplied paths. The idea
    /// is that the patterns you use are specified in an OS-independent
    /// manner so they can be compile-time constant, but the separator is
    /// supplied at run-time to allow adaptation to OS.
    pub fn from_pattern(pattern: &str, separator: &str) -> Result<PathMatch, Error> {
        let components = StringComponentIter::new(pattern, UNIX_SEP);
        let pattern = path_to_pattern(components)?;
        let mut match_tree = PathMatchNode::default();
        match_tree.insert(pattern);
        match_tree.recompute_depth_bounds();
        let result = PathMatch {
            separator: separator.to_string(),
            match_tree,
        };
        Ok(result)
    }

    /// Returns `true` if the specified string matches the pattern, `false`
    /// otherwise. Unlike patterns, paths may contain `..`, but if the parent
    /// traversal cannot be normalized out, no matches can occur.
    pub fn matches<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.matches_common(path, false)
    }

    /// Returns `true` if the specified string forms a prefix path of one of the
    /// patterns matches.
    ///
    /// The prefix must consist of full components. e.g. `first/second` is a
    /// prefix of `first/second/third`, but `first/sec` is not.
    pub fn matches_prefix<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.matches_common(path, true)
    }

    fn matches_common(&self, path: &str, match_prefix: bool) -> bool {
        let components = normalized(StringComponentIter::new(path, &self.separator));
        PathMatchNode::matches(&self.match_tree, &components, match_prefix)
    }

    /// Returns the maximum number of components a matching path could have.
    /// This assumes a normalized path - a matching path could always have
    /// an arbitrary number of `.` components.
    #[must_use]
    pub fn max_depth(&self) -> usize {
        self.match_tree.max_traversals
    }
}

/// Builds a `PathMatch` which can match against multiple expressions.
pub struct PathMatchBuilder {
    processed: Vec<Vec<PatternComponent>>,
    separator: String,
}

impl PathMatchBuilder {
    /// Constructs a `PathMatchBuilder` where paths to be matched will use the
    /// supplied separator.
    #[must_use]
    pub fn new(separator: &str) -> PathMatchBuilder {
        PathMatchBuilder {
            processed: Vec::new(),
            separator: separator.into(),
        }
    }

    /// Adds the specified pattern to the matcher.
    ///
    /// This will return an error if the pattern contains parent traversals or a
    /// component containing multiple wildcards. See also
    /// `PathMatch::from_pattern`.
    pub fn add_pattern(&mut self, pattern: &str) -> Result<(), Error> {
        let components = StringComponentIter::new(pattern, UNIX_SEP);
        let processed = path_to_pattern(components)?;
        self.processed.push(processed);
        Ok(())
    }

    /// Constructs the `PathMatch` which can be used to match against paths.
    pub fn build(self) -> Result<PathMatch, Error> {
        let mut match_tree = PathMatchNode::default();
        for pattern in self.processed {
            match_tree.insert(pattern);
        }
        match_tree.recompute_depth_bounds();
        let result = PathMatch {
            separator: self.separator,
            match_tree,
        };
        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_syntax() -> Result<(), Error> {
        let path = r"foo|bar|hmm|hello|";
        for separator in ["/", "\\"] {
            let path = path.replace("|", separator);
            let pattern = PathMatch::from_pattern(".////foo/*/*/hel*o/", separator)?;
            assert!(pattern.matches(path));
        }
        Ok(())
    }

    #[test]
    fn star() -> Result<(), Error> {
        let path = r"foo|bar|hmm|hello|";
        for separator in ["/", "\\"] {
            let path = &path.replace("|", separator);

            let pattern = PathMatch::from_pattern("./*", separator)?;
            assert!(!pattern.matches(path));

            let pattern = PathMatch::from_pattern("./*/*", separator)?;
            assert!(!pattern.matches(path));

            let pattern = PathMatch::from_pattern("./*/*/*", separator)?;
            assert!(!pattern.matches(path));

            let pattern = PathMatch::from_pattern("./*/*/*/*", separator)?;
            assert!(pattern.matches(path));
        }

        Ok(())
    }

    #[test]
    fn root() -> Result<(), Error> {
        for pattern in ["/", "/./", "/.", "/////", "///./."] {
            let pattern = PathMatch::from_pattern(pattern, "/")?;
            assert!(pattern.matches("/"));
            assert!(!pattern.matches("./"));
        }
        Ok(())
    }

    #[test]
    fn cwd() -> Result<(), Error> {
        // Dot is not necessarily a folder
        for pattern in [".", "././././.", ".////."] {
            let pattern = PathMatch::from_pattern(pattern, "/")?;
            assert!(pattern.matches("."));
            assert!(pattern.matches("./"));
            assert!(!pattern.matches("/"));
            assert!(!pattern.matches("/."));
            assert!(!pattern.matches("/./"));
        }

        // Dot is a folder
        for pattern in ["./", "./././"] {
            let pattern = PathMatch::from_pattern(pattern, "/")?;
            assert!(pattern.matches("./"));
            assert!(!pattern.matches("."));
            assert!(!pattern.matches("/"));
            assert!(!pattern.matches("/."));
            assert!(!pattern.matches("/./"));
        }
        Ok(())
    }

    #[test]
    fn file_path() -> Result<(), Error> {
        for pattern in ["hello", "./hello", "././hello"] {
            let pattern = PathMatch::from_pattern(pattern, "/")?;
            assert!(pattern.matches("hello"));
        }
        Ok(())
    }

    #[test]
    fn prefix_matching() -> Result<(), Error> {
        let pattern = "hello/there/friend";
        for separator in ["/", "\\"] {
            let pattern = PathMatch::from_pattern(pattern, separator)?;
            for path in [
                ".",
                ".|",
                "hello",
                "hello|",
                "hello|there",
                "hello|there|",
                "hello|there|friend",
                "hello|there|friend|",
            ] {
                let path = path.replace("|", separator);
                assert!(pattern.matches_prefix(path));
            }
        }
        Ok(())
    }

    #[test]
    fn max_depth() -> Result<(), Error> {
        for (pattern, depth) in [
            (".", 0),
            ("./", 0),
            ("./*", 1),
            ("./*/", 1),
            ("./././", 0),
            ("*/*/*/", 3),
            ("./hello/", 1),
            ("./*/*", 2),
        ] {
            let pattern_1 = PathMatch::from_pattern(pattern, "/")?;
            let pattern_2 = {
                let mut builder = PathMatchBuilder::new("/");
                builder.add_pattern(pattern)?;
                builder.build()?
            };
            assert_eq!(pattern_1.max_depth(), depth);
            assert_eq!(pattern_2.max_depth(), depth);
        }
        Ok(())
    }

    #[test]
    fn multiple_builder_patterns() -> Result<(), Error> {
        let mut builder = PathMatchBuilder::new("/");
        for pattern in [
            "./a",
            "./b/",
            "a/b/c/d/e",
            "./b/foo*",
            "./b/bar",
            "./b/test*pattern",
            "./b/test*pattern/final",
            "./c",
            "./c/",
        ] {
            builder.add_pattern(pattern)?;
        }
        let pattern = builder.build()?;

        // These should match
        for path in [
            "a",
            "a/",
            "b/",
            "a/b/c/d/e",
            "b/foobar",
            "b/foocar",
            "b/bar",
            "b/test_wildcard_pattern",
            "b/test_wildcard_pattern/final",
            "c",
            "c/",
        ] {
            assert!(pattern.matches(path));
        }

        // These should not
        for path in ["b", "a/b/c/d", "b/folbar", "b/barfoo", "b/tes_attern"] {
            assert!(!pattern.matches(path));
        }

        // These should prefix-match
        for path in [
            "b",
            "a/b/c",
            "a/b/c/",
            "b/test_wildcard_pattern",
            "b/test_wildcard_pattern/",
        ] {
            assert!(pattern.matches_prefix(path));
        }

        Ok(())
    }

    #[test]
    fn no_patterns_match_nothing() -> Result<(), Error> {
        let builder = PathMatchBuilder::new("/");
        let pattern = builder.build()?;
        assert!(!pattern.matches("non_empty"));
        assert!(!pattern.matches(""));
        assert!(!pattern.matches("/"));
        Ok(())
    }

    #[test]
    fn multiple_wildcard() -> Result<(), Error> {
        let pattern = PathMatch::from_pattern("*/*", r"\")?;
        assert!(!pattern.matches(r"."));
        assert!(!pattern.matches(r"hello"));
        assert!(pattern.matches(r"hello\there"));
        assert!(!pattern.matches(r"hello\there\friend"));
        Ok(())
    }

    #[test]
    fn single_wildcard() -> Result<(), Error> {
        let pattern = PathMatch::from_pattern("*", r"\")?;
        assert!(!pattern.matches(r"."));
        assert!(pattern.matches(r".hello"));
        assert!(pattern.matches(r"hello"));
        Ok(())
    }

    #[test]
    fn wildcard_matches_dot_in_middle() -> Result<(), Error> {
        let pattern = PathMatch::from_pattern("hello*there", r"\")?;
        assert!(pattern.matches(r"hello.there"));
        Ok(())
    }
}
