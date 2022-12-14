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

fn pattern_to_regex_string(pattern: &str, separator: &str) -> Result<String, PatternError> {
    let platform_separator_literal = regex::escape(separator);
    let path_current_literal = regex::escape(PATH_CURRENT);
    let components = pattern.split(UNIX_SEP);
    let mut regex_str = String::from("^");
    let mut is_trailing_dir = false;
    let mut first_component = true;
    for (idx, component) in components.enumerate() {
        is_trailing_dir = false;
        if component.is_empty() {
            if idx == 0 {
                first_component = false;
            } else {
                is_trailing_dir = true;
            }
            continue;
        } else if component == PATH_CURRENT {
            continue;
        } else if component == PATH_PARENT {
            return Err(PatternError::NoParents);
        }

        if first_component {
            first_component = false;
        } else {
            regex_str += &platform_separator_literal;
        }

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
    if first_component {
        regex_str += &path_current_literal;
    }
    regex_str += &platform_separator_literal;
    if !is_trailing_dir {
        regex_str += "?";
    }
    regex_str += "$";
    println!("REGEX: {}", regex_str);
    Ok(regex_str)
}

fn pattern_to_regex(pattern: &str, separator: &str) -> Result<Regex, PatternError> {
    let regex_string = pattern_to_regex_string(pattern, separator)?;
    let regex = Regex::new(regex_string.as_str())?;
    Ok(regex)
}

#[derive(Debug)]
pub struct PathMatch {
    regex: Regex,
}

impl PathMatch {
    pub fn from_pattern(pattern: &str, separator: &str) -> Result<PathMatch, PatternError> {
        let regex = pattern_to_regex(pattern, separator)?;
        let result = PathMatch { regex };
        Ok(result)
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
}

impl PathMatchBuilder {
    pub fn new(separator: &str) -> PathMatchBuilder {
        PathMatchBuilder {
            regex_strings: Vec::new(),
            separator: separator.into(),
        }
    }

    pub fn add_pattern(&mut self, pattern: &str) -> Result<(), PatternError> {
        let regex_string = pattern_to_regex_string(pattern, self.separator.as_str())?;
        self.regex_strings.push(regex_string);
        Ok(())
    }

    pub fn build(self) -> Result<PathMatch, PatternError> {
        use itertools::Itertools;
        let combined = self.regex_strings.iter().join("|");
        let regex = Regex::new(combined.as_str())?;
        let result = PathMatch { regex };
        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_pattern() -> Result<(), PatternError> {
        let path = r"foo\bar\hmm\hello\";
        println!("{}", path);
        let pattern = PathMatch::from_pattern(".////foo/*/*/hel?o", r"\")?;
        assert!(pattern.matches(path));
        Ok(())
    }
}
