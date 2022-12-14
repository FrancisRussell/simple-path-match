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

fn pattern_to_regex_string(
    pattern: &str,
    separator: &str,
) -> Result<(String, usize), PatternError> {
    let platform_separator_literal = regex::escape(separator);
    let path_current_literal = regex::escape(PATH_CURRENT);
    let components = pattern.split(UNIX_SEP);
    let mut path_depth = 0;
    let mut regex_str = String::from("^");
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
        }
        path_depth += 1;

        for character in component.chars() {
            if character == WILDCARD_ANY {
                regex_str += &format!("[^{}]*", UNIX_SEP);
            } else if character == WILDCARD_SINGLE {
                regex_str += &format!("[^{}]", UNIX_SEP);
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
    println!("REGEX: {}", regex_str);
    Ok((regex_str, path_depth))
}

fn pattern_to_regex(pattern: &str, separator: &str) -> Result<(Regex, usize), PatternError> {
    let (regex_string, max_depth) = pattern_to_regex_string(pattern, separator)?;
    let regex = Regex::new(regex_string.as_str())?;
    Ok((regex, max_depth))
}

#[derive(Debug)]
pub struct PathMatch {
    regex: Regex,
    max_depth: usize,
}

impl PathMatch {
    pub fn from_pattern(pattern: &str, separator: &str) -> Result<PathMatch, PatternError> {
        let (regex, max_depth) = pattern_to_regex(pattern, separator)?;
        let result = PathMatch { regex, max_depth };
        Ok(result)
    }

    pub fn max_depth(&self) -> usize {
        self.max_depth
    }
}

impl PathMatch {
    pub fn matches<P: AsRef<str>>(&self, path: P) -> bool {
        let path = path.as_ref();
        self.regex.is_match(&path)
    }
}

#[derive(Default)]
pub struct PathMatchBuilder {
    regex_strings: Vec<String>,
    separator: String,
    max_depth: usize,
}

impl PathMatchBuilder {
    pub fn new(separator: &str) -> PathMatchBuilder {
        PathMatchBuilder {
            regex_strings: Vec::new(),
            separator: separator.into(),
            max_depth: 0,
        }
    }

    pub fn add_pattern(&mut self, pattern: &str) -> Result<(), PatternError> {
        let (regex_string, max_depth) = pattern_to_regex_string(pattern, self.separator.as_str())?;
        self.regex_strings.push(regex_string);
        self.max_depth = std::cmp::max(self.max_depth, max_depth);
        Ok(())
    }

    pub fn build(self) -> Result<PathMatch, PatternError> {
        use itertools::Itertools;
        let combined = self.regex_strings.iter().join("|");
        let regex = Regex::new(combined.as_str())?;
        let result = PathMatch {
            regex,
            max_depth: self.max_depth,
        };
        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_syntax() -> Result<(), PatternError> {
        let path = r"foo\bar\hmm\hello\";
        let pattern = PathMatch::from_pattern(".////foo/*/*/hel?o/", r"\")?;
        assert!(pattern.matches(path));
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
