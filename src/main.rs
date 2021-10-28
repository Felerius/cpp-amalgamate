use anyhow::{Context, Result};
use regex::Regex;
use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fmt::{self, Display, Formatter},
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    iter,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

/// Recursively inline all non-system includes in a C++ source file.
///
/// Works under the assumption that all includes should be included at most once (as if guarded by
/// include guards or #pragma once). Existing #pragma once lines are removed, as some compilers
/// consider them a warning or even an error if encountered in a .cpp file. To detect multiple
/// includes of the same file, the absolute path of the file with all symlinks resolved is used.
///
/// To make the output more readable, all leading and trailing blank lines are removed from each
/// included file, after removing includes leading to already included files. Each included file is
/// also surrounded by a pair of comments to identify it.
#[derive(Debug, StructOpt)]
struct Opts {
    /// Source file to process
    file: PathBuf,

    /// Include directories to consider (in the order they are given)
    directories: Vec<PathBuf>,

    /// Output file to write to (default: stdout)
    #[structopt(short, long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct IncludedFile {
    /// Canonical path to the file, used as its identity.
    canonical_path: PathBuf,

    /// Relative path to include directory (if possible), used for display the file name.
    rel_display_path: PathBuf,
}

#[derive(Debug)]
struct CyclicIncludeError {
    cycle: Vec<PathBuf>,
}

impl Display for CyclicIncludeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "cyclic include detected")
    }
}

impl Error for CyclicIncludeError {}

#[derive(Copy, Clone, Debug)]
enum ProcessingState {
    InStack(usize),
    Done,
}

#[derive(Debug)]
struct State {
    processed_files: HashMap<PathBuf, ProcessingState>,
    stack: Vec<IncludedFile>,
    include_directories: Vec<PathBuf>,
    include_regex: Regex,
    pragma_once_regex: Regex,
}

fn find_included_file(
    include_path: &str,
    current_dir: &Path,
    include_directories: &[PathBuf],
) -> Result<IncludedFile> {
    iter::once(current_dir)
        .chain(include_directories.iter().map(PathBuf::as_path))
        .map(|include_dir| {
            let potential_file = include_dir.join(include_path);
            if !potential_file.is_file() {
                return Ok(None);
            }

            let canonical_path = try_canonicalize(&potential_file)?;
            let rel_display_path = if include_dir == current_dir {
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

            Ok(Some(IncludedFile {
                canonical_path,
                rel_display_path,
            }))
        })
        .find_map(Result::transpose)
        .with_context(|| format!("included file \"{}\" not found", include_path))?
}

fn process_file(file_path: &Path, writer: &mut impl Write, state: &mut State) -> Result<()> {
    let directory = file_path.parent().context("invalid file name")?;
    let mut reader = BufReader::new(File::open(file_path)?);
    let mut locs = state.include_regex.capture_locations();
    let mut line = String::new();
    let mut before_first_line = true;
    let mut previous_blank_lines = 0;

    while reader.read_line(&mut line)? != 0 {
        let trimmed_line = line.trim_end();
        if trimmed_line.is_empty() {
            previous_blank_lines += 1;
            line.clear();
            continue;
        }
        if state.pragma_once_regex.is_match(trimmed_line) {
            line.clear();
            continue;
        }

        if !before_first_line {
            for _ in 0..previous_blank_lines {
                writeln!(writer)?;
            }
        }
        previous_blank_lines = 0;

        let maybe_match = state.include_regex.captures_read(&mut locs, trimmed_line);
        match maybe_match {
            None => {
                write!(writer, "{}", line)?;
                before_first_line = false;
            }
            Some(_) => {
                let (idx_l, idx_r) = locs.get(1).context("invalid hardcoded regex")?;
                let path = &trimmed_line[idx_l..idx_r];
                let included_file =
                    find_included_file(path, directory, &state.include_directories)?;

                match state
                    .processed_files
                    .entry(included_file.canonical_path.clone())
                {
                    Entry::Occupied(occupied_entry) => {
                        if let ProcessingState::InStack(idx) = *occupied_entry.get() {
                            let cycle = state.stack[idx..]
                                .iter()
                                .map(|file| file.rel_display_path.clone())
                                .collect();
                            return Err(CyclicIncludeError { cycle }.into());
                        }
                    }
                    Entry::Vacant(vacant_entry) => {
                        vacant_entry.insert(ProcessingState::InStack(state.stack.len()));
                        state.stack.push(included_file.clone());

                        let path_display = included_file.rel_display_path.display();
                        writeln!(writer, "// begin \"{}\"", path_display)?;
                        process_file(&included_file.canonical_path, writer, state)?;
                        writeln!(writer, "// end \"{}\"", path_display)?;
                        before_first_line = false;

                        state.stack.pop();
                        state
                            .processed_files
                            .insert(included_file.canonical_path, ProcessingState::Done);
                    }
                }
            }
        }

        line.clear();
    }

    Ok(())
}

fn try_canonicalize(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("failed to canonicalize path: {}", path.display()))
}

fn static_regex(pattern: &str) -> Result<Regex> {
    Regex::new(pattern).context("invalid hardcoded regex")
}

fn fallible_main() -> Result<()> {
    let mut opts = Opts::from_args();
    for include_dir in opts.directories.iter_mut() {
        *include_dir = try_canonicalize(include_dir)?;
    }

    let include_regex = static_regex(r#"^\s*#\s*include\s*"([^"]+)"\s*$"#)?;
    let pragma_once_regex = static_regex(r#"^\s*#\s*pragma\s+once\s*$"#)?;

    // include main file in include stack to be able to detect cyclic include leading back to it
    let main_file = IncludedFile {
        canonical_path: try_canonicalize(&opts.file)?,
        rel_display_path: opts.file.clone(),
    };
    let processed_files = [(
        main_file.canonical_path.clone(),
        ProcessingState::InStack(0),
    )]
    .into_iter()
    .collect();
    let mut state = State {
        processed_files,
        stack: vec![main_file],
        include_directories: opts.directories,
        include_regex,
        pragma_once_regex,
    };

    if let Some(output_path) = opts.output {
        let mut writer = BufWriter::new(File::create(output_path)?);
        process_file(&opts.file, &mut writer, &mut state)?;
    } else {
        let stdout = io::stdout();
        let mut stdout_locked = stdout.lock();
        process_file(&opts.file, &mut stdout_locked, &mut state)?;
    }

    Ok(())
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
            Err(error) => println!("Error: {}", error),
        }
    }
}
