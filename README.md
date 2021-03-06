<div align="center">

# cpp-amalgamate

[![Build status](https://github.com/Felerius/cpp-inline-includes/actions/workflows/ci.yml/badge.svg)](https://github.com/Felerius/cpp-inline-includes/actions)
[![License](https://img.shields.io/crates/l/cpp-amalgamate)](https://github.com/Felerius/cpp-amalgamate/blob/main/LICENSE)
[![Crates.io](https://img.shields.io/crates/v/cpp-amalgamate)](https://crates.io/crates/cpp-amalgamate)
  
</div>

cpp-amalgamate recursively combines C++ source files and the headers they include into a single
output file. It tracks which headers have been included and skips any further references to them.
Which includes are inlined and which are left as is can be precisely controlled.

It originated as an automated way to inline pre-written snippets when submitting to competitive
programming sites such as [Codeforces](https://codeforces.com/) or [AtCoder](https://atcoder.jp).
Since then, it has been generalized and might be useful in other contexts as well.

## Features & limitations

When provided with one or more source files and accompanying search directories for includes,
cpp-amalgamate will concatenate the source files and recursively inline all include statements it
can resolve using the given search directories (or from the current directory, for
`#include "..."`). While, by default, all resolvable includes are inlined, this can be controlled
using the `--filter*` family of options. It can also insert corresponding `#line num "file"`
directives which allows compilers or debuggers to resolve lines in the combined file back to their
origin.

However, cpp-amalgamate does not interpret preprocessor instructions beyond `#include`. This notably
means that it cannot understand traditional header guards using `#if` instructions. Instead,
cpp-amalgamate assumes that every header should be included at most once, as if it was guarded by a
header guard or `#pragma once`. It does detect `#pragma once` instructions and removes them, as
these cause warnings or errors when compiling the combined file with some compilers.

This simplified behavior also might cause problems if `#include` statements themselves are inside
`#if` blocks. If the same header is referenced inside two separate `#if` blocks, it will only be
expanded in the former while the latter `#include` will be removed.

## Usage

The basic invocation for cpp-amalgamate is

```shell
cpp-amalgamate [options] source-files...
```

To specify search directories, use `-d`/`--dir`. You can also use `--dir-quote` or `--dir-system`
for search directories that should only be used for quote (i.e., `#include "..."`) or system
includes (i.e., `#include <...>`). Note that cpp-amalgamate does not use any search directories by
default!

### Filtering

Using `-f`/`--filter`, you can specify globs for includes that should not be inlined. As with search
directories, `--filter-quote` and `--filter-system` are versions only applicable to one type of
include. Globs can be inverted with a leading `!`, causing matching headers to be inlined even if a
previous glob excluded them. Globs are evaluated in order, with the last matching glob determining
whether a header is included or not. By default (i.e., if no glob matches), all headers are inlined.

Note that these globs are applied to the absolute path of the header with all symbolic links
resolved. This means that often a `**` fragment will be necessary, which matches any number of path
entries. That is,

* `**` matches any file,
* `**/*.hpp` all files with the extension `.hpp`,
* and `/usr/local/include/**` all files in `/usr/local/include`.

For the full details on the supported syntax, check the
[globset documentation](https://docs.rs/globset/0.4.8/globset/#syntax).

### Miscellaneous

Other flags supported by cpp-amalgamate are:

* `-o`/`--output`: Write the combined source file to a file rather than the standard output.
* `--line-directives`: Add `#line num "file"` directives to the output, allowing compilers and
  debuggers to resolve lines to their original files.
* `-v`/`--verbose` and `-q`/`--quiet`: Increase or decrease the level of log messages shown. By
  default, only warnings and errors are shown.
* `--unresolvable-include`: Specifies what is done when an include cannot be resolved.  Possible
  values are `error`, `warn`, and `ignore`, with the latter being the default. This can be useful
  to assert that all includes end up inlined Also available as `--unresolvable-quote-include` and
  `--unresolvable-system-include`.
* `--cyclic-include`: Specifies how a cyclic include is handled. Supports the same values as
  `--unresolvable-include` except with `error` as the default.

## Installation

Each [GitHub release](https://github.com/Felerius/cpp-amalgamate/releases) contains precompiled
binaries for most common operating systems/architectures. Alternatively, cpp-amalgamate can be
installed using cargo which is bundled with Rust:

```shell
cargo install cpp-amalgamate
```
