# Trivial path matching

This is a utility library for another project. It is factored out into a separate
repository since this makes it easier to run tests.

Implements the ability to match patterns against paths by compiling the pattern
to a regular expression:
- Patterns are limited to glob expression syntax but with only `*` and `?`
  being supported. 
- `*` cannot match path separators.
- Paths can only be UTF-8 strings - neither slices of bytes nor `OsStr` is
  supported. 
- There is no direct support for matching against `std::path`.
- There is no ability to use a pattern to iterate the filesystem - it's an
  matcher against glob patterns, not a glob evaluator.
- The separator of the paths to be matched against is specified at run-time.
- No `..` instances may appear in the pattern - the library is only intended
  for evaluating relative paths below a root path.
- No `..` instances should appear in the paths being matched either, but this
  is not checked for.
- The paths being matched must be normalized - no `.` instances may
  appear in the path if it could be expressed without them.

Why would someone want a library with such an absurd set of restrictions? 

This library is used by a project which compiles to WASM (not using WASI)
running on node.js, meaning that the semantics of `std::path` are unclear -
there is neither filesystem access and the properties of the host filesystem
are only known at runtime. This also rules out most existing glob
implementations which assume you want to evaluate your glob and find files on
the local filesystem.
