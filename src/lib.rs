#![allow(clippy::uninlined_format_args)]

use regex::Regex;
use thiserror::Error;

const PATH_CURRENT: &str = ".";
const PATH_PARENT: &str = "..";
const WILDCARD_ANY: char = '*';
const WILDCARD_SINGLE: char = '?';
const UNIX_SEP: &str = "/";

#[derive(Debug, Error)]
pub enum PatternError {
    #[error("Pattern must not contain parent traversals")]
    NoParents,

    #[error("Pattern compiled to an invalid regex: {0}")]
    CompiledRegex(#[from] regex::Error),
}

#[derive(Clone, Debug)]
struct ProcessedPattern<T> {
    pattern: T,
    prefix_pattern: T,
    max_depth: usize,
}

fn pattern_to_regex_string(
    pattern: &str,
    separator: &str,
) -> Result<ProcessedPattern<String>, PatternError> {
    use itertools::Itertools as _;

    assert_ne!(separator.len(), 0, "Separator cannot be empty string");
    let platform_separator_literal = regex::escape(separator);
    let path_current_literal = regex::escape(PATH_CURRENT);
    let components = pattern.split(UNIX_SEP);
    let mut path_depth = 0;
    let mut regex_str = String::from("^");
    let mut prefix_regex_strs = Vec::new();
    let mut is_trailing_dir = false;
    for (idx, component) in components.enumerate() {
        is_trailing_dir = false;
        if component.is_empty() {
            if idx == 0 {
                path_depth += 1;
            } else {
                is_trailing_dir = true;
            }
            continue;
        } else if component == PATH_CURRENT {
            continue;
        } else if component == PATH_PARENT {
            return Err(PatternError::NoParents);
        }

        if path_depth > 0 {
            regex_str += &platform_separator_literal;
            // For prefixes, we make the trailing separator optional
            let prefix_str = format!("{}?$", regex_str);
            prefix_regex_strs.push(prefix_str);
        }
        path_depth += 1;

        for character in component.chars() {
            if character == WILDCARD_ANY {
                regex_str += &format!("[^{}]*", platform_separator_literal);
            } else if character == WILDCARD_SINGLE {
                regex_str += &format!("[^{}]", platform_separator_literal);
            } else {
                regex_str += &regex::escape(&String::from(character));
            }
        }
    }
    if path_depth == 0 {
        regex_str += &path_current_literal;
    }
    regex_str += &platform_separator_literal;
    if !is_trailing_dir {
        regex_str += "?";
    }
    regex_str += "$";
    let prefix_str = std::iter::once(&regex_str)
        .chain(prefix_regex_strs.iter())
        .join("|");
    Ok(ProcessedPattern {
        pattern: regex_str,
        prefix_pattern: prefix_str,
        max_depth: path_depth,
    })
}

fn pattern_to_regex(
    pattern: &str,
    separator: &str,
) -> Result<ProcessedPattern<Regex>, PatternError> {
    let processed = pattern_to_regex_string(pattern, separator)?;
    let pattern_regex = Regex::new(processed.pattern.as_str())?;
    let prefix_pattern_regex = Regex::new(processed.prefix_pattern.as_str())?;
    Ok(ProcessedPattern {
        pattern: pattern_regex,
        prefix_pattern: prefix_pattern_regex,
        max_depth: processed.max_depth,
    })
}

#[derive(Clone, Debug)]
pub struct PathMatch {
    inner: ProcessedPattern<Regex>,
}

impl PathMatch {
    pub fn from_pattern(pattern: &str, separator: &str) -> Result<PathMatch, PatternError> {
        let inner = pattern_to_regex(pattern, separator)?;
        let result = PathMatch { inner };
        Ok(result)
    }

    pub fn max_depth(&self) -> usize {
        self.inner.max_depth
    }
}

impl PathMatch {
    pub fn matches<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.inner.pattern.is_match(path)
    }

    pub fn matches_prefix<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.inner.prefix_pattern.is_match(path)
    }
}

pub struct PathMatchBuilder {
    processed: Vec<ProcessedPattern<String>>,
    separator: String,
}

impl PathMatchBuilder {
    pub fn new(separator: &str) -> PathMatchBuilder {
        PathMatchBuilder {
            processed: Vec::new(),
            separator: separator.into(),
        }
    }

    pub fn add_pattern(&mut self, pattern: &str) -> Result<(), PatternError> {
        let processed = pattern_to_regex_string(pattern, self.separator.as_str())?;
        self.processed.push(processed);
        Ok(())
    }

    pub fn build(self) -> Result<PathMatch, PatternError> {
        use itertools::Itertools as _;

        let combined_pattern = self.processed.iter().map(|p| &p.pattern).join("|");
        let combined_prefix_pattern = self.processed.iter().map(|p| &p.prefix_pattern).join("|");
        let max_depth = self
            .processed
            .iter()
            .map(|p| p.max_depth)
            .max()
            .unwrap_or(0);

        let combined_pattern = Regex::new(&combined_pattern)?;
        let combined_prefix_pattern = Regex::new(&combined_prefix_pattern)?;
        let result = ProcessedPattern {
            pattern: combined_pattern,
            prefix_pattern: combined_prefix_pattern,
            max_depth,
        };
        let result = PathMatch { inner: result };
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
            let pattern = PathMatch::from_pattern(".////foo/*/*/hel?o/", separator)?;
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
}
