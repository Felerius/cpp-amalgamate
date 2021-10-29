# cpp-inline-includes

[![Build status](https://github.com/Felerius/cpp-inline-includes/actions/workflows/ci.yml/badge.svg)](https://github.com/Felerius/cpp-inline-includes/actions)

Small utility to recursively inline all non-system includes (i.e. `#include "..."` but not
`#include <...>`) in a C++ file. It was created to combine submissions for online competitive
programming judges such as [Codeforces](https://codeforces.com) or [AtCoder](atcoder.jp) into a
single file, but might be useful in other contexts as well.

To avoid reimplementing a version of the C++ preprocessor, some assumptions are made that should be
reasonable for most use cases. Firstly, all files are included at most once, as if guarded by an
include guard or `#pragma once`. To check whether two includes point to the same file, the absolute
path with all symlinks resolved is used. To avoid warnings/errors, all `#pragma once` lines in
included files are removed.

## Usage

To use cpp-inline-includes, run

```shell
cpp-inline-includes main.cpp include-dir1 include-dir2
```

This inlines all includes in `main.cpp`, using `include-dir1` and `include-dir2` as the include
directories to resolve included files. The result is printed to the standard output, but can be
redirected to a file with the `-o`/`--output` flag.

To customize the output, the following options are available:

* `--line-directives`: This adds `#line num "file"` directives to the output. This allows compilers
  and debuggers to print the original file name and line numbers.
* `--trim-blank`: Trims all blank (only whitespace) lines from the beginning and end of each file.
* `--file-begin-comment <template>`/`--file-end-comment <template>`: Comments to place before the
  first and after the last line of each included file. The templates can contain `{absolute}` and/or
  `{relative}`, which will be replaced with the absolute or relative (with respect to its include
  directory) path of the included file.
