# Trivial path matching

This is a utility library for another project. It is factored out into a separate
repository since this makes it easier to run tests.

Implements the ability to match patterns against paths:
* The filesystem separator is specified at run-time.
- Patterns are limited to glob expression syntax but with only `*` being
  supported.
- `*` cannot match path separators.
- Multiple `*`s cannot appear in a single component.
- Paths can only be UTF-8 strings - neither slices of bytes nor `OsStr`s are
  supported. 
- Paths can be tested to see if they are a prefix of a potentially matching
  path - this enables one to prune traversal of a directory structure when
  searching for matches.
- There is no direct support for matching against `std::path`.
- There is no ability to use a pattern to iterate the filesystem - it's a
  matcher against glob patterns, not a glob evaluator.
- The separator of the paths to be matched against is specified at run-time.
- No `..` instances may appear in the pattern - the library is only intended
  for evaluating relative paths below a root path.

## Why would someone want a library with so many restrictions? 

This library is used by a project which compiles to WASM (not using WASI)
running on node.js, meaning that the semantics of `std::path` are unclear -
there is neither filesystem access and the properties of the host filesystem
are only known at runtime. 

This existing glob libraries I found were statically tied to the expected
separator for the host file system and/or the use of `std::path`.

In addition, this library doesn't make of the `regex` crate and is `no_std`
compatible.

## Usage example

Matcher against multiple paths:
```rust
// We use raw string literals here because we use backslash as a separator
let mut builder = PathMatchBuilder::new(r"\");
builder.add_pattern("./pdfs/*.pdf")?;
builder.add_pattern("./oggs/*.ogg")?;
builder.add_pattern("./folder_a")?;
builder.add_pattern("./folder_b/")?;
builder.add_pattern("./*/*/prefix.*")?;
let matcher = builder.build()?;

assert!(matcher.matches(r".\pdfs\test.pdf"));
assert!(matcher.matches(r"oggs\test.ogg"));

// Will match with or without a trailing slash
assert!(matcher.matches(r"folder_a"));
assert!(matcher.matches(r"folder_a\"));

// This does not match since trailing slashes are required if specified
assert!(!matcher.matches(r"folder_b"));
// But this one will
assert!(matcher.matches(r"folder_b\"));

// Wildcards are fine anywhere in a component, but we can only have one e.g. no *.*
assert!(matcher.matches(r"a\b\prefix.txt"));
Ok(())
```
