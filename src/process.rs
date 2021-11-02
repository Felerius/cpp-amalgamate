/// Main recursive processing of source files/includes.
use std::{
    collections::{hash_map::Entry, HashMap},
    error,
    ffi::OsStr,
    fmt::{self, Debug, Display, Formatter},
    fs::File,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use regex::{CaptureLocations, Regex};

use crate::{error_level_enabled, error_level_log, resolve::IncludeResolver, ErrorLevel};

fn static_regex(re: &'static str) -> Regex {
    Regex::new(re).expect("invalid hardcoded regex")
}

fn debug_filename(path: &Path) -> &OsStr {
    path.file_name()
        .unwrap_or_else(|| "<no file name?>".as_ref())
}

const EMPTY_STACK_IDX: usize = usize::MAX;

#[derive(Debug)]
pub struct CyclicIncludeError {
    pub cycle: Vec<PathBuf>,
}

impl Display for CyclicIncludeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "cyclic include detected:")?;
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
    files: Vec<FileState>,
    known_files: HashMap<PathBuf, usize>,
    tail_idx: usize,
    on_cyclic_include: ErrorLevel,
    on_missing_include: ErrorLevel,
    include_regex: Regex,
    include_regex_locs: CaptureLocations,
    pragma_once_regex: Regex,
}

impl<W: Write> Processor<W> {
    pub fn new(
        writer: W,
        resolver: IncludeResolver,
        on_missing_include: ErrorLevel,
        on_cyclic_include: ErrorLevel,
    ) -> Self {
        let include_regex = static_regex(r#"^\s*#\s*include\s*(["<][^>"]+[">])\s*$"#);
        let include_regex_locs = include_regex.capture_locations();
        Self {
            writer,
            resolver,
            files: Vec::new(),
            known_files: HashMap::new(),
            tail_idx: EMPTY_STACK_IDX,
            on_cyclic_include,
            on_missing_include,
            include_regex,
            include_regex_locs,
            pragma_once_regex: static_regex(r"^\s*#\s*pragma\s+once\s*$"),
        }
    }

    fn check_should_process(&mut self, canonical_path: PathBuf) -> Result<bool> {
        match self.known_files.entry(canonical_path) {
            Entry::Vacant(entry) => {
                let idx = self.files.len();
                self.files.push(FileState {
                    canonical_path: entry.key().clone(),
                    included_by: self.tail_idx,
                    in_stack: true,
                });
                log::info!("Processing new file {:?}", debug_filename(entry.key()));
                entry.insert(idx);
                Ok(true)
            }
            Entry::Occupied(entry) => {
                let idx = *entry.get();
                if self.files[idx].in_stack {
                    assert_ne!(
                        self.tail_idx, EMPTY_STACK_IDX,
                        "cannot get include cycles with only one file on the stack"
                    );

                    if error_level_enabled!(self.on_cyclic_include) {
                        let mut cycle = vec![self.files[self.tail_idx].canonical_path.clone()];
                        let mut cycle_tail_idx = self.tail_idx;
                        while cycle_tail_idx != idx {
                            cycle_tail_idx = self.files[cycle_tail_idx].included_by;
                            cycle.push(self.files[cycle_tail_idx].canonical_path.clone());
                        }

                        error_level_log!(
                            self.on_cyclic_include,
                            "{}",
                            CyclicIncludeError { cycle }
                        )?;
                    }
                } else {
                    log::debug!(
                        "Skipping {:?}, already included",
                        debug_filename(entry.key())
                    );
                }

                Ok(false)
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
            log::debug!("Found weird include-like statement: {}", include_ref);
            return Ok(());
        };

        if let Some(resolved_path) = maybe_resolved_path {
            self.process_recursively(resolved_path)?;
        } else {
            error_level_log!(self.on_missing_include, "could not resolve {}", include_ref)?;
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
            log::trace!("Skipping pragma once");
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
        if !self.check_should_process(canonical_path)? {
            return Ok(());
        }

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
        log::info!("Processing source file {:?}", debug_filename(source_file));
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
