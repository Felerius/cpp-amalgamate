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
    in_stack: bool,
}

#[derive(Debug)]
pub struct Processor<W> {
    writer: W,
    resolver: IncludeResolver,
    inlining_filter: InliningFilter,
    files: Vec<FileState>,
    known_files: HashMap<PathBuf, usize>,
    tail_idx: usize,
    cyclic_include_handling: ErrorHandling,
    unresolvable_quote_include_handling: ErrorHandling,
    unresolvable_system_include_handling: ErrorHandling,
    include_regex: Regex,
    include_regex_locs: CaptureLocations,
    pragma_once_regex: Regex,
}

impl<W: Write> Processor<W> {
    pub fn new(
        writer: W,
        resolver: IncludeResolver,
        inlining_filter: InliningFilter,
        cyclic_include_handling: ErrorHandling,
        unresolvable_quote_include_handling: ErrorHandling,
        unresolvable_system_include_handling: ErrorHandling,
    ) -> Self {
        let include_regex = static_regex(r#"^\s*#\s*include\s*(["<][^>"]+[">])\s*$"#);
        let include_regex_locs = include_regex.capture_locations();
        Self {
            writer,
            resolver,
            inlining_filter,
            files: Vec::new(),
            known_files: HashMap::new(),
            tail_idx: EMPTY_STACK_IDX,
            cyclic_include_handling,
            unresolvable_quote_include_handling,
            unresolvable_system_include_handling,
            include_regex,
            include_regex_locs,
            pragma_once_regex: static_regex(r"^\s*#\s*pragma\s+once\s*$"),
        }
    }

    fn assign_index(&mut self, canonical_path: PathBuf) -> Result<Option<usize>> {
        match self.known_files.entry(canonical_path) {
            Entry::Vacant(entry) => {
                let idx = self.files.len();
                self.files.push(FileState {
                    canonical_path: entry.key().clone(),
                    included_by: self.tail_idx,
                    in_stack: true,
                });
                info!("Processing {:?}", debug_file_name(entry.key()));
                entry.insert(idx);
                Ok(Some(idx))
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
                        self.cyclic_include_handling,
                        "{}",
                        CyclicIncludeError { cycle }
                    )?;
                } else {
                    debug!(
                        "Skipping {:?}, already included",
                        debug_file_name(entry.key())
                    );
                }

                Ok(None)
            }
        }
    }

    fn process_include(&mut self, include_ref: &str, current_dir: &Path) -> Result<()> {
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
            return Ok(());
        };
        let is_system = include_ref.starts_with('<');

        if let Some(resolved_path) = maybe_resolved_path {
            if self
                .inlining_filter
                .should_inline(&resolved_path, is_system)
            {
                self.process_recursively(resolved_path)?;
            }
        } else {
            let handling = if is_system {
                self.unresolvable_system_include_handling
            } else {
                self.unresolvable_quote_include_handling
            };
            error_handling_handle!(handling, "Could not resolve {}", include_ref)?;
        }

        Ok(())
    }

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

        if self.pragma_once_regex.is_match(line) {
            trace!("Skipping pragma once");
            return Ok(true);
        }

        let maybe_match = self
            .include_regex
            .captures_read(&mut self.include_regex_locs, line);
        if maybe_match.is_none() {
            write!(self.writer, "{}", line).context("Failed writing to output")?;
            return Ok(true);
        }

        let (ref_start, ref_end) = self
            .include_regex_locs
            .get(1)
            .expect("invalid hardcoded regex: missing capture group");
        self.process_include(&line[ref_start..ref_end], current_dir)?;
        Ok(true)
    }

    fn process_recursively(&mut self, canonical_path: PathBuf) -> Result<()> {
        if let Some(idx) = self.assign_index(canonical_path)? {
            self.tail_idx = idx;
        } else {
            return Ok(());
        };
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
        self.process_recursively(canonical_path)?;
        assert_eq!(self.tail_idx, EMPTY_STACK_IDX);

        Ok(())
    }
}
