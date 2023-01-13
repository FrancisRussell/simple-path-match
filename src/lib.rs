#![warn(clippy::pedantic)]
#![allow(clippy::uninlined_format_args, clippy::missing_errors_doc)]

use std::collections::VecDeque;
use thiserror::Error;

const PATH_CURRENT: &str = ".";
const PATH_PARENT: &str = "..";
const UNIX_DELIMITER: &str = ":";
const UNIX_SEP: &str = "/";
const WILDCARD_ANY: &str = "*";

#[derive(Clone, Debug, PartialEq, Eq)]
enum PathComponent {
    Current,
    DirectoryMarker,
    Name(String),
    RootName(String),
    Parent,
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum PatternComponent {
    Literal(PathComponent),
    StartsEndsWith(String, String),
}

impl PatternComponent {
    fn matches(&self, component: &PathComponent) -> bool {
        match (self, component) {
            (PatternComponent::Literal(l), c) => l == c,
            (PatternComponent::StartsEndsWith(prefix, suffix), PathComponent::Name(n)) => {
                n.starts_with(prefix) && n.ends_with(suffix)
            }
            (_, _) => false,
        }
    }
}

impl std::fmt::Display for PatternComponent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            PatternComponent::Literal(c) => c.fmt(formatter),
            PatternComponent::StartsEndsWith(s, e) => {
                formatter.write_str(s)?;
                formatter.write_str(WILDCARD_ANY)?;
                formatter.write_str(e)
            }
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

fn normalized<I: IntoIterator<Item = PathComponent>>(components: I) -> VecDeque<PathComponent> {
    let components = components.into_iter();
    let mut result = VecDeque::with_capacity(components.size_hint().0);
    for component in components {
        match component {
            PathComponent::Name(_) | PathComponent::RootName(_) => result.push_back(component),
            PathComponent::DirectoryMarker => {
                if result.is_empty() {
                    result.push_back(PathComponent::Current);
                }
                result.push_back(PathComponent::DirectoryMarker);
            }
            PathComponent::Parent => match result.back() {
                None | Some(PathComponent::Parent) => result.push_back(PathComponent::Parent),
                Some(PathComponent::Name(_)) => drop(result.pop_back()),
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
        result.push_back(PathComponent::Current);
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
                    PatternComponent::StartsEndsWith(start.to_string(), end.to_string())
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
pub struct PathMatch {
    patterns: Vec<Vec<PatternComponent>>,
    separator: String,
}

impl std::fmt::Display for PathMatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut first_pattern = true;
        for pattern in &self.patterns {
            if first_pattern {
                first_pattern = false;
            } else {
                UNIX_DELIMITER.fmt(formatter)?;
            }
            let mut first_component = true;
            for component in pattern {
                if first_component {
                    first_component = false;
                } else {
                    UNIX_SEP.fmt(formatter)?;
                }
                component.fmt(formatter)?;
            }
        }
        Ok(())
    }
}

impl PathMatch {
    pub fn from_pattern(pattern: &str, separator: &str) -> Result<PathMatch, PatternError> {
        let components = StringComponentIter::new(pattern, UNIX_SEP);
        let pattern = path_to_pattern(components)?;
        let result = PathMatch {
            patterns: vec![pattern],
            separator: separator.to_string(),
        };
        Ok(result)
    }

    pub fn matches<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        for pattern in &self.patterns {
            if self.matches_single_pattern(path, pattern, false) {
                return true;
            }
        }
        false
    }

    pub fn matches_prefix<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        for pattern in &self.patterns {
            if self.matches_single_pattern(path, pattern, true) {
                return true;
            }
        }
        false
    }

    fn matches_single_pattern(
        &self,
        path: &str,
        match_components: &[PatternComponent],
        match_prefix: bool,
    ) -> bool {
        let mut components = normalized(StringComponentIter::new(path, &self.separator));
        if match_prefix && components.front() == Some(&PathComponent::Current) {
            // It is invalid to do this in the non-prefix case, since we might need
            // to match ".".
            components.pop_front();
        }
        // If our path is too long, strip the trailing directory suffix
        if components.len() > match_components.len()
            && components.back() == Some(&PathComponent::DirectoryMarker)
        {
            components.pop_back();
        }
        // Reduce our list of matchers to the path length if we're matching a prefix
        let match_components = if match_prefix {
            &match_components[..std::cmp::min(components.len(), match_components.len())]
        } else {
            match_components
        };
        // If our path lengths don't match, this can't be a match
        if components.len() != match_components.len() {
            return false;
        }
        for (matcher, component) in match_components.iter().zip(components.into_iter()) {
            let matches = matcher.matches(&component)
                || match_prefix && component == PathComponent::DirectoryMarker;
            if !matches {
                return false;
            }
        }
        true
    }

    #[must_use]
    pub fn max_depth(&self) -> usize {
        self.patterns
            .iter()
            .map(|pattern| {
                pattern
                    .iter()
                    .filter(|c| {
                        if let PatternComponent::Literal(literal) = c {
                            !matches!(
                                literal,
                                PathComponent::Current | PathComponent::DirectoryMarker
                            )
                        } else {
                            true
                        }
                    })
                    .count()
            })
            .max()
            .unwrap_or(0)
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
        let result = PathMatch {
            patterns: self.processed,
            separator: self.separator,
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
            let pattern = PathMatch::from_pattern(pattern, "/")?;
            assert_eq!(pattern.max_depth(), depth);
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
