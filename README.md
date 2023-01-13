# Trivial path matching

This is a utility library for another project. It is factored out into a separate
repository since this makes it easier to run tests.

Implements the ability to match patterns against paths:
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
- The paths being matched must be normalized - they must use only the specified
  separator and no `.` instances may appear in the path if it could be
  expressed without them.

Why would someone want a library with so many restrictions? 

This library is used by a project which compiles to WASM (not using WASI)
running on node.js, meaning that the semantics of `std::path` are unclear -
there is neither filesystem access and the properties of the host filesystem
are only known at runtime. This also rules out most existing glob
implementations which assume you want to evaluate your glob and find files on
the local filesystem.
