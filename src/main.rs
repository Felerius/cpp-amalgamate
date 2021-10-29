use anyhow::{Context, Result};
use regex::{Captures, Regex};
use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fmt::{self, Display, Formatter},
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    iter, mem,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

/// Recursively inline all non-system includes in a C++ source file.
///
/// Works under the assumption that all includes should be included at most once (as if guarded by
/// include guards or #pragma once). Existing #pragma once lines are removed, as some compilers
/// consider them a warning or even an error if encountered in a .cpp file. To detect multiple
/// includes of the same file, the absolute path of the file with all symlinks resolved is used.
#[derive(Debug, StructOpt)]
struct Opts {
    /// Source file to process
    file: PathBuf,

    /// Include directories to consider (in the order they are given)
    directories: Vec<PathBuf>,

    /// Output file to write to (default: stdout)
    #[structopt(short, long, value_name = "file")]
    output: Option<PathBuf>,

    /// Output `#line num "file"` directives for compilers/debuggers
    #[structopt(long)]
    line_directives: bool,

    /// Trim leading and trailing blank lines from each included file
    #[structopt(long)]
    trim_blank: bool,

    /// Comment added at the beginning of each included file. Each occurrence of {relative} and
    /// {absolute} will be replaced by the relative and absolute path to the file respectively.
    #[structopt(long, value_name = "template")]
    file_begin_comment: Option<String>,

    /// Comment added at the end of each included file. Uses the same template syntax as
    /// --file-begin-comment.
    #[structopt(long, value_name = "template")]
    file_end_comment: Option<String>,
}

#[derive(Debug)]
struct CyclicIncludeError {
    cycle: Vec<PathBuf>,
}

impl Error for CyclicIncludeError {}

impl Display for CyclicIncludeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "cyclic include detected")
    }
}

fn static_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("invalid hardcoded regex")
}

fn try_canonicalize(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("failed to canonicalize path \"{}\"", path.display()))
}

fn find_included_file(
    include_path: &str,
    current_dir: &Path,
    include_directories: &[PathBuf],
) -> Result<(PathBuf, PathBuf)> {
    iter::once(current_dir)
        .chain(include_directories.iter().map(PathBuf::as_path))
        .map(|include_dir| {
            let potential_file = include_dir.join(include_path);
            if !potential_file.is_file() {
                return Ok(None);
            }

            let canonical_path = try_canonicalize(&potential_file)?;
            let display_path = if include_dir == current_dir {
                // While the include path is relative, the file is most likely still contained in one of
                // the include directories (the same that contained the file that included this one).
                // Try to find its path relative to that include directory as a better display path.
                include_directories
                    .iter()
                    .flat_map(|dir| canonical_path.strip_prefix(dir).ok())
                    .next()
                    .map(Path::to_path_buf)
                    .unwrap_or(potential_file)
            } else {
                // We already know the relevant include dir and the relative path inside it.
                PathBuf::from(include_path)
            };

            Ok(Some((canonical_path, display_path)))
        })
        .find_map(Result::transpose)
        .with_context(|| format!("included file \"{}\" not found", include_path))?
}

#[derive(Copy, Clone, Debug)]
enum ProcessingState {
    InStack(usize),
    Done,
}

#[derive(Debug, Clone)]
struct FileState {
    /// Canonical path to the file, used as its identity.
    canonical_path: PathBuf,

    /// Relative path to include directory (if possible), used for display the file name.
    display_path: PathBuf,

    /// Has anything from this file been written to the output?
    has_written: bool,
}

#[derive(Debug)]
struct Processor<W> {
    writer: W,
    opts: Opts,
    include_regex: Regex,
    pragma_once_regex: Regex,
    template_placeholder_regex: Regex,
    known_files: HashMap<PathBuf, ProcessingState>,
    stack: Vec<FileState>,
    current_file: FileState,
    next_line: (usize, usize), // Pair of (index in stack, 1-based line index)
}

impl<W: Write> Processor<W> {
    fn new(writer: W, mut opts: Opts) -> Result<Self> {
        for include_dir in &mut opts.directories {
            *include_dir = try_canonicalize(include_dir)?;
        }
        let current_file = FileState {
            canonical_path: try_canonicalize(&opts.file)?,
            display_path: opts.file.clone(),
            has_written: false,
        };
        Ok(Self {
            writer,
            opts,
            include_regex: static_regex(r#"^\s*#\s*include\s*"([^"]+)"\s*$"#),
            pragma_once_regex: static_regex(r#"^\s*#\s*pragma\s+once\s*$"#),
            template_placeholder_regex: static_regex(r#"\{(relative|absolute)\}"#),
            known_files: HashMap::from_iter([(
                current_file.canonical_path.clone(),
                ProcessingState::InStack(0),
            )]),
            stack: Vec::new(),
            current_file,
            next_line: (0, 1),
        })
    }

    fn expand_comment_template<'a>(&self, template: &'a str) -> Cow<'a, str> {
        self.template_placeholder_regex
            .replace_all(template, |captures: &Captures| {
                let path = if &captures[0] == "{relative}" {
                    &self.current_file.display_path
                } else {
                    &self.current_file.canonical_path
                };
                path.display().to_string()
            })
    }

    fn emit_line(&mut self, line: &str, line_num: usize) -> Result<()> {
        if !self.current_file.has_written && !self.stack.is_empty() {
            if let Some(template) = &self.opts.file_begin_comment {
                let comment = self.expand_comment_template(template);
                writeln!(self.writer, "{}", comment)?;
            }
        }

        let file_idx = self.stack.len();
        if self.next_line != (file_idx, line_num) && self.opts.line_directives {
            writeln!(
                self.writer,
                "#line {} \"{}\"",
                line_num,
                self.current_file.canonical_path.display()
            )?;
        }
        self.next_line = (file_idx, line_num + 1);

        write!(self.writer, "{}", line)?;
        self.current_file.has_written = true;

        Ok(())
    }

    fn emit_blank_lines(&mut self, lines: &mut Vec<(String, usize)>) -> Result<()> {
        for (line, line_num) in lines.drain(..) {
            self.emit_line(&line, line_num)?;
        }

        Ok(())
    }

    fn process(&mut self) -> Result<()> {
        let current_dir = self
            .current_file
            .canonical_path
            .parent()
            .context("processed file has no parent directory")?
            .to_path_buf();

        let mut reader = BufReader::new(
            File::open(&self.current_file.canonical_path).with_context(|| {
                format!(
                    "failed to open included file \"{}\"",
                    self.current_file.display_path.display()
                )
            })?,
        );
        let mut try_read_line = move |this: &mut Self, line: &mut String| {
            line.clear();
            reader.read_line(line).with_context(|| {
                format!(
                    "failed to read from included file \"{}\"",
                    this.current_file.display_path.display()
                )
            })
        };

        let mut line = String::new();
        let mut line_num = 0;
        let mut deferred_blank_lines = Vec::new();
        let mut match_locs = self.include_regex.capture_locations();

        while try_read_line(self, &mut line)? != 0 {
            line_num += 1;
            let trimmed_line = line.trim_end();
            if trimmed_line.is_empty() && self.opts.trim_blank {
                deferred_blank_lines.push((mem::take(&mut line), line_num));
                continue;
            }

            if self.pragma_once_regex.is_match(trimmed_line) {
                continue;
            }

            let maybe_match = self
                .include_regex
                .captures_read(&mut match_locs, trimmed_line);
            if maybe_match.is_some() {
                let (path_start, path_end) = match_locs.get(1).expect("invalid hardcoded regex");
                let path = &trimmed_line[path_start..path_end];
                let (canonical_path, display_path) =
                    find_included_file(path, &current_dir, &self.opts.directories)?;

                match self.known_files.entry(canonical_path.clone()) {
                    Entry::Occupied(entry) => {
                        if let ProcessingState::InStack(idx) = *entry.get() {
                            let cycle = self.stack[idx..]
                                .iter()
                                .chain(iter::once(&self.current_file))
                                .map(|file| file.display_path.clone())
                                .collect();
                            return Err(CyclicIncludeError { cycle }.into());
                        }
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(ProcessingState::InStack(self.stack.len()));
                        if !self.opts.trim_blank || self.current_file.has_written {
                            self.emit_blank_lines(&mut deferred_blank_lines)?;
                        }

                        let new_state = FileState {
                            canonical_path,
                            display_path,
                            has_written: false,
                        };
                        self.stack
                            .push(mem::replace(&mut self.current_file, new_state));
                        self.process()?;

                        let sub_state = mem::replace(
                            &mut self.current_file,
                            self.stack.pop().expect("empty processed file stack"),
                        );
                        if sub_state.has_written {
                            deferred_blank_lines.clear();
                        }
                        self.known_files
                            .insert(sub_state.canonical_path, ProcessingState::Done);
                    }
                }
            } else {
                if self.opts.trim_blank && !self.current_file.has_written {
                    deferred_blank_lines.clear();
                } else {
                    self.emit_blank_lines(&mut deferred_blank_lines)?;
                }

                self.emit_line(&line, line_num)?;
            }
        }

        if !self.opts.trim_blank {
            self.emit_blank_lines(&mut deferred_blank_lines)?;
        }

        if self.current_file.has_written && !self.stack.is_empty() {
            if let Some(template) = &self.opts.file_end_comment {
                let comment = self.expand_comment_template(template);
                writeln!(self.writer, "{}", comment)?;
            }
        }

        Ok(())
    }
}

fn process_file(writer: impl Write, opts: Opts) -> Result<()> {
    let mut processor = Processor::new(writer, opts)?;
    processor.process()?;
    Ok(())
}

fn fallible_main() -> Result<()> {
    let opts = Opts::from_args();
    if let Some(ref output_path) = opts.output {
        let writer = BufWriter::new(File::create(output_path)?);
        process_file(writer, opts)
    } else {
        let stdout = io::stdout();
        let stdout_locked = stdout.lock();
        process_file(stdout_locked, opts)
    }
}

fn main() {
    if let Err(error) = fallible_main() {
        match error.downcast::<CyclicIncludeError>() {
            Ok(cyc_error) => {
                println!("Error: {}", cyc_error);
                for file in cyc_error.cycle {
                    println!("    {}", file.display());
                }
            }
            Err(error) => println!("Error: {:#}", error),
        }
    }
}
