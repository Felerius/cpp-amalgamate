/// Main recursive processing of source files/includes.
use std::{
    collections::{hash_map::Entry, HashMap},
    error,
    fmt::{self, Debug, Display, Formatter},
    fs::File,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use log::{debug, info, trace};
use regex::{CaptureLocations, Regex};

use crate::{
    error_handling_handle, filter::InliningFilter, logging::debug_file_name,
    resolve::IncludeResolver, ErrorHandling,
};

fn static_regex(re: &'static str) -> Regex {
    Regex::new(re).expect("invalid hardcoded regex")
}

const EMPTY_STACK_IDX: usize = usize::MAX;

#[derive(Debug)]
pub struct CyclicIncludeError {
    pub cycle: Vec<PathBuf>,
}

impl Display for CyclicIncludeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Cyclic include detected:")?;
        for file in &self.cycle {
            writeln!(f, "\t{}", file.display())?;
        }
        Ok(())
    }
}

impl error::Error for CyclicIncludeError {}

#[derive(Debug)]
struct FileState {
    canonical_path: PathBuf,
    included_by: usize,
    line_num: usize,
    in_stack: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineRef {
    file_idx: usize,
    num: usize,
}

#[derive(Debug)]
pub struct ErrorHandlingOpts {
    pub cyclic_include: ErrorHandling,
    pub unresolvable_quote_include: ErrorHandling,
    pub unresolvable_system_include: ErrorHandling,
}

#[derive(Debug)]
struct Regexes {
    include: Regex,
    include_locs: CaptureLocations,
    pragma_once: Regex,
}

impl Regexes {
    fn new() -> Self {
        let include = static_regex(r#"^\s*#\s*include\s*(["<][^>"]+[">])\s*$"#);
        let include_locs = include.capture_locations();
        Self {
            include,
            include_locs,
            pragma_once: static_regex(r"^\s*#\s*pragma\s+once\s*$"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum IncludeHandling {
    Inline,
    Remove,
    Leave,
}

#[derive(Debug)]
pub struct Processor<W> {
    writer: W,
    resolver: IncludeResolver,
    inlining_filter: InliningFilter,
    files: Vec<FileState>,
    known_files: HashMap<PathBuf, usize>,
    tail_idx: usize,
    expected_line: Option<LineRef>,
    error_handling_opts: ErrorHandlingOpts,
    regexes: Regexes,
}

impl<W: Write> Processor<W> {
    pub fn new(
        writer: W,
        resolver: IncludeResolver,
        line_directives: bool,
        inlining_filter: InliningFilter,
        error_handling_opts: ErrorHandlingOpts,
    ) -> Self {
        let expected_line = line_directives.then(|| LineRef {
            file_idx: EMPTY_STACK_IDX,
            num: 0,
        });
        Self {
            writer,
            resolver,
            inlining_filter,
            files: Vec::new(),
            known_files: HashMap::new(),
            tail_idx: EMPTY_STACK_IDX,
            expected_line,
            error_handling_opts,
            regexes: Regexes::new(),
        }
    }

    fn push_to_stack(&mut self, canonical_path: PathBuf) -> Result<IncludeHandling> {
        match self.known_files.entry(canonical_path) {
            Entry::Vacant(entry) => {
                let idx = self.files.len();
                self.files.push(FileState {
                    canonical_path: entry.key().clone(),
                    included_by: self.tail_idx,
                    line_num: 0,
                    in_stack: true,
                });
                info!("Processing {:?}", debug_file_name(entry.key()));
                entry.insert(idx);
                self.tail_idx = idx;
                Ok(IncludeHandling::Inline)
            }
            Entry::Occupied(entry) => {
                let idx = *entry.get();
                if self.files[idx].in_stack {
                    assert_ne!(
                        self.tail_idx, EMPTY_STACK_IDX,
                        "cannot get include cycles with only one file on the stack"
                    );

                    let mut cycle = vec![self.files[self.tail_idx].canonical_path.clone()];
                    let mut cycle_tail_idx = self.tail_idx;
                    while cycle_tail_idx != idx {
                        cycle_tail_idx = self.files[cycle_tail_idx].included_by;
                        cycle.push(self.files[cycle_tail_idx].canonical_path.clone());
                    }

                    error_handling_handle!(
                        self.error_handling_opts.cyclic_include,
                        "{}",
                        CyclicIncludeError { cycle }
                    )?;
                    Ok(IncludeHandling::Leave)
                } else {
                    debug!(
                        "Skipping {:?}, already included",
                        debug_file_name(entry.key())
                    );
                    Ok(IncludeHandling::Remove)
                }
            }
        }
    }

    fn output_copied_line(&mut self, line: &str) -> Result<()> {
        if let Some(expected_line) = &mut self.expected_line {
            let cur_file = &self.files[self.tail_idx];
            let cur_line = LineRef {
                file_idx: self.tail_idx,
                num: cur_file.line_num,
            };
            if cur_line != *expected_line {
                writeln!(
                    self.writer,
                    "#line {} \"{}\"",
                    cur_line.num,
                    cur_file.canonical_path.display()
                )?;
                *expected_line = cur_line;
            }
            expected_line.num += 1;
        }

        write!(self.writer, "{}", line)?;
        Ok(())
    }

    /// Returns `true` if the include statement should be kept, `false` if it shouldn't.
    fn process_include(&mut self, include_ref: &str, current_dir: &Path) -> Result<bool> {
        assert!(
            include_ref.len() >= 3,
            "error in hardcoded include regex: include ref too short"
        );

        let maybe_resolved_path = if include_ref.starts_with('"') && include_ref.ends_with('"') {
            self.resolver
                .resolve_quote(&include_ref[1..(include_ref.len() - 1)], current_dir)?
        } else if include_ref.starts_with('<') && include_ref.ends_with('>') {
            self.resolver
                .resolve_system(&include_ref[1..(include_ref.len() - 1)])?
        } else {
            debug!("Found weird include-like statement: {}", include_ref);
            return Ok(true);
        };
        let is_system = include_ref.starts_with('<');

        if let Some(resolved_path) = maybe_resolved_path {
            if self
                .inlining_filter
                .should_inline(&resolved_path, is_system)
            {
                return Ok(match self.push_to_stack(resolved_path)? {
                    IncludeHandling::Inline => {
                        self.process_recursively()?;
                        false
                    }
                    IncludeHandling::Remove => false,
                    IncludeHandling::Leave => true,
                });
            }
        } else {
            let handling = if is_system {
                self.error_handling_opts.unresolvable_system_include
            } else {
                self.error_handling_opts.unresolvable_quote_include
            };
            error_handling_handle!(handling, "Could not resolve {}", include_ref)?;
        }

        Ok(true)
    }

    /// Returns `true` when a line was processed, `false` if at eof.
    fn process_line(
        &mut self,
        mut reader: impl BufRead,
        line: &mut String,
        current_dir: &Path,
    ) -> Result<bool> {
        line.clear();
        let bytes_read = reader.read_line(line).with_context(|| {
            format!(
                "Failed to read from \"{}\"",
                self.files[self.tail_idx].canonical_path.display()
            )
        })?;

        if bytes_read == 0 {
            return Ok(false);
        }

        self.files[self.tail_idx].line_num += 1;
        if self.regexes.pragma_once.is_match(line) {
            trace!("Skipping pragma once");
            return Ok(true);
        }

        let maybe_match = self
            .regexes
            .include
            .captures_read(&mut self.regexes.include_locs, line);
        if maybe_match.is_some() {
            let (ref_start, ref_end) = self
                .regexes
                .include_locs
                .get(1)
                .expect("invalid hardcoded regex: missing capture group");
            if !self.process_include(&line[ref_start..ref_end], current_dir)? {
                return Ok(true);
            }
        }

        self.output_copied_line(line)
            .context("Failed writing to output")?;
        Ok(true)
    }

    fn process_recursively(&mut self) -> Result<()> {
        let path = &self.files[self.tail_idx].canonical_path;
        let current_dir = path
            .parent()
            .context("Processed file has no parent directory")?
            .to_path_buf();

        let mut reader = File::open(path)
            .with_context(|| format!("Failed to open file \"{}\"", path.display()))
            .map(BufReader::new)?;
        let mut line = String::new();

        while self.process_line(&mut reader, &mut line, &current_dir)? {}

        self.files[self.tail_idx].in_stack = false;
        self.tail_idx = self.files[self.tail_idx].included_by;

        Ok(())
    }

    pub fn process(&mut self, source_file: &Path) -> Result<()> {
        info!("Processing source file {:?}", debug_file_name(source_file));
        let canonical_path = source_file.canonicalize().with_context(|| {
            format!(
                "Failed to canonicalize source file path \"{}\"",
                source_file.display()
            )
        })?;

        assert_eq!(self.tail_idx, EMPTY_STACK_IDX);
        if self.push_to_stack(canonical_path)? == IncludeHandling::Inline {
            self.process_recursively()?;
        }
        assert_eq!(self.tail_idx, EMPTY_STACK_IDX);

        Ok(())
    }
}
