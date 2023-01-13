#![warn(clippy::pedantic)]
#![allow(clippy::uninlined_format_args, clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::collections::VecDeque;
use thiserror::Error;

const PATH_CURRENT: &str = ".";
const PATH_PARENT: &str = "..";
const UNIX_SEP: &str = "/";
const WILDCARD_ANY: &str = "*";

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum PathComponent {
    Current,
    DirectoryMarker,
    Name(String),
    RootName(String),
    Parent,
}

impl PathComponent {
    fn traversal_depth(&self) -> usize {
        match self {
            PathComponent::Current | PathComponent::DirectoryMarker => 0,
            PathComponent::Name(_) | PathComponent::RootName(_) | PathComponent::Parent => 1,
        }
    }
}

impl std::fmt::Display for PathComponent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            PathComponent::Current => formatter.write_str(PATH_CURRENT),
            PathComponent::DirectoryMarker => Ok(()),
            PathComponent::Name(s) | PathComponent::RootName(s) => formatter.write_str(s),
            PathComponent::Parent => formatter.write_str(PATH_PARENT),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct StartsEndsWith(String, String);

impl StartsEndsWith {
    pub fn matches(&self, name: &str) -> bool {
        name.starts_with(&self.0) && name.ends_with(&self.1)
    }
}

impl std::fmt::Display for StartsEndsWith {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        formatter.write_str(&self.0)?;
        formatter.write_str(WILDCARD_ANY)?;
        formatter.write_str(&self.1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PatternComponent {
    Literal(PathComponent),
    StartsEndsWith(StartsEndsWith),
}

impl std::fmt::Display for PatternComponent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            PatternComponent::Literal(c) => c.fmt(formatter),
            PatternComponent::StartsEndsWith(m) => m.fmt(formatter),
        }
    }
}

#[derive(Debug, Error)]
pub enum PatternError {
    #[error("Pattern must not contain parent traversals")]
    NoParents,

    #[error("Only one wilcard allowed in component: `{0}`")]
    WildcardPosition(String),
}

struct StringComponentIter<'a> {
    path_string: std::iter::Enumerate<std::str::Split<'a, &'a str>>,
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
    type Item = PathComponent;

    fn next(&mut self) -> Option<PathComponent> {
        for (idx, component) in self.path_string.by_ref() {
            self.is_dir = false;
            match component {
                "" => {
                    if idx == 0 {
                        return Some(PathComponent::RootName(component.to_string()));
                    }
                    self.is_dir = true;
                }
                PATH_CURRENT => return Some(PathComponent::Current),
                PATH_PARENT => return Some(PathComponent::Parent),
                _ => return Some(PathComponent::Name(component.to_string())),
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

fn normalized<I: IntoIterator<Item = PathComponent>>(components: I) -> Vec<PathComponent> {
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
                Some(c) => panic!(
                    "Component found in unexpected place during normalization: {:?}",
                    c
                ),
            },
            PathComponent::Current => {}
        }
    }
    if result.is_empty() {
        result.push(PathComponent::Current);
    }
    result
}

fn path_to_pattern<I: IntoIterator<Item = PathComponent>>(
    components: I,
) -> Result<Vec<PatternComponent>, PatternError> {
    let components = components.into_iter();
    let mut result = Vec::with_capacity(components.size_hint().0);
    for component in components {
        match component {
            PathComponent::Name(component) => {
                let matcher = if let Some(idx) = component.find(WILDCARD_ANY) {
                    let (start, end) = component.split_at(idx);
                    let (_, end) = end.split_at(WILDCARD_ANY.len());
                    if start.contains(WILDCARD_ANY) || end.contains(WILDCARD_ANY) {
                        return Err(PatternError::WildcardPosition(component));
                    }
                    PatternComponent::StartsEndsWith(StartsEndsWith(
                        start.to_string(),
                        end.to_string(),
                    ))
                } else {
                    PatternComponent::Literal(PathComponent::Name(component))
                };
                result.push(matcher);
            }
            PathComponent::Parent => return Err(PatternError::NoParents),
            PathComponent::Current => {}
            PathComponent::DirectoryMarker => {
                if result.is_empty() {
                    result.push(PatternComponent::Literal(PathComponent::Current));
                }
                result.push(PatternComponent::Literal(component));
            }
            PathComponent::RootName(_) => result.push(PatternComponent::Literal(component)),
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
    literals: HashMap<PathComponent, PathMatchNode>,
    starts_ends_with: HashMap<StartsEndsWith, PathMatchNode>,
    min_traversals: usize,
    max_traversals: usize,
}

impl Default for PathMatchNode {
    fn default() -> PathMatchNode {
        PathMatchNode {
            can_end: false,
            literals: HashMap::new(),
            starts_ends_with: HashMap::new(),
            min_traversals: 0,
            max_traversals: usize::MAX,
        }
    }
}

impl std::fmt::Display for PathMatchNode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        use std::fmt::Write as _;

        let literals_iter = self.literals.iter().map(|(k, v)| (k.to_string(), v));
        let matchers_iter = self
            .starts_ends_with
            .iter()
            .map(|(k, v)| (k.to_string(), v));
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
            PatternComponent::StartsEndsWith(pattern) => {
                self.starts_ends_with.entry(pattern).or_default()
            }
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
        for (component, node) in &mut self.literals {
            let (node_min, node_max) = node.recompute_depth_bounds();
            let component_depth = component.traversal_depth();
            *min = std::cmp::min(*min, node_min + component_depth);
            *max = std::cmp::max(*max, node_max + component_depth);
        }
        for node in self.starts_ends_with.values_mut() {
            let (node_min, node_max) = node.recompute_depth_bounds();
            let component_depth = 1;
            *min = std::cmp::min(*min, node_min + component_depth);
            *max = std::cmp::max(*max, node_max + component_depth);
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
            let path_is_dir_marker =
                path.len() == 1 && path.last() == Some(&PathComponent::DirectoryMarker);
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

#[derive(Clone, Debug)]
pub struct PathMatch {
    separator: String,
    match_tree: PathMatchNode,
}

impl std::fmt::Display for PathMatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.match_tree.fmt(formatter)
    }
}

impl PathMatch {
    pub fn from_pattern(pattern: &str, separator: &str) -> Result<PathMatch, PatternError> {
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

    pub fn matches<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.matches_common(path, false)
    }

    pub fn matches_prefix<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.matches_common(path, true)
    }

    fn matches_common(&self, path: &str, match_prefix: bool) -> bool {
        let components = normalized(StringComponentIter::new(path, &self.separator));
        PathMatchNode::matches(&self.match_tree, &components, match_prefix)
    }

    #[must_use]
    pub fn max_depth(&self) -> usize {
        self.match_tree.max_traversals
    }
}

pub struct PathMatchBuilder {
    processed: Vec<Vec<PatternComponent>>,
    separator: String,
}

impl PathMatchBuilder {
    #[must_use]
    pub fn new(separator: &str) -> PathMatchBuilder {
        PathMatchBuilder {
            processed: Vec::new(),
            separator: separator.into(),
        }
    }

    pub fn add_pattern(&mut self, pattern: &str) -> Result<(), PatternError> {
        let components = StringComponentIter::new(pattern, UNIX_SEP);
        let processed = path_to_pattern(components)?;
        self.processed.push(processed);
        Ok(())
    }

    pub fn build(self) -> Result<PathMatch, PatternError> {
        let mut match_tree = PathMatchNode::default();
        for pattern in &self.processed {
            match_tree.insert(pattern.clone());
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
    fn basic_syntax() -> Result<(), PatternError> {
        let path = r"foo|bar|hmm|hello|";
        for separator in ["/", "\\"] {
            let path = path.replace("|", separator);
            let pattern = PathMatch::from_pattern(".////foo/*/*/hel*o/", separator)?;
            assert!(pattern.matches(path));
        }
        Ok(())
    }

    #[test]
    fn star() -> Result<(), PatternError> {
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
    fn root() -> Result<(), PatternError> {
        for pattern in ["/", "/./", "/.", "/////", "///./."] {
            let pattern = PathMatch::from_pattern(pattern, "/")?;
            assert!(pattern.matches("/"));
            assert!(!pattern.matches("./"));
        }
        Ok(())
    }

    #[test]
    fn cwd() -> Result<(), PatternError> {
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
    fn file_path() -> Result<(), PatternError> {
        for pattern in ["hello", "./hello", "././hello"] {
            let pattern = PathMatch::from_pattern(pattern, "/")?;
            assert!(pattern.matches("hello"));
        }
        Ok(())
    }

    #[test]
    fn prefix_matching() -> Result<(), PatternError> {
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
    fn max_depth() -> Result<(), PatternError> {
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
    fn no_patterns_match_nothing() -> Result<(), PatternError> {
        let builder = PathMatchBuilder::new("/");
        let pattern = builder.build()?;
        assert!(!pattern.matches("non_empty"));
        assert!(!pattern.matches(""));
        assert!(!pattern.matches("/"));
        Ok(())
    }

    #[test]
    fn multiple_wildcard() -> Result<(), PatternError> {
        let pattern = PathMatch::from_pattern("*/*", r"\")?;
        assert!(!pattern.matches(r"."));
        assert!(!pattern.matches(r"hello"));
        assert!(pattern.matches(r"hello\there"));
        assert!(!pattern.matches(r"hello\there\friend"));
        Ok(())
    }

    #[test]
    fn single_wildcard() -> Result<(), PatternError> {
        let pattern = PathMatch::from_pattern("*", r"\")?;
        assert!(!pattern.matches(r"."));
        assert!(pattern.matches(r".hello"));
        assert!(pattern.matches(r"hello"));
        Ok(())
    }

    #[test]
    fn wildcard_matches_dot_in_middle() -> Result<(), PatternError> {
        let pattern = PathMatch::from_pattern("hello*there", r"\")?;
        assert!(pattern.matches(r"hello.there"));
        Ok(())
    }
}
