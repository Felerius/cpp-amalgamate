//! Resolves paths in include statements to the included files.
use std::{
    fmt::{self, Display, Formatter},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use log::{debug, trace};

#[derive(Debug)]
struct IncludePrinter<'a>(&'a str, bool);

impl Display for IncludePrinter<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.1 {
            write!(f, "\"{}\"", self.0)
        } else {
            write!(f, "<{}>", self.0)
        }
    }
}

#[derive(Debug)]
pub struct IncludeResolver {
    quote_search_paths: Vec<PathBuf>,
    system_search_paths: Vec<PathBuf>,
}

fn resolve(
    path: &str,
    search_path: &[PathBuf],
    current_dir: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let printer = IncludePrinter(path, current_dir.is_some());
    let current_dir_canonicalized = current_dir
        .map(Path::canonicalize)
        .transpose()
        .context("failed to canonicalize current directory")?;

    let maybe_resolved = current_dir_canonicalized
        .as_deref()
        .into_iter()
        .chain(search_path.iter().map(PathBuf::as_path))
        .find_map(|include_dir| {
            let potential_path = include_dir.join(path);
            trace!("Trying to resolve {} to {:?}", printer, potential_path);

            (potential_path.exists() && !potential_path.is_dir()).then(|| {
                potential_path.canonicalize().with_context(|| {
                    format!(
                        "Failed to canonicalize path to include: \"{}\"",
                        potential_path.display()
                    )
                })
            })
        })
        .transpose()?;

    let (left, right) = current_dir.map_or(('<', '>'), |_| ('"', '"'));
    if let Some(resolved) = &maybe_resolved {
        debug!("Resolved {}{}{} to {:?}", left, path, right, resolved);
    } else {
        debug!("Failed to resolve {}{}{}", left, path, right);
    }

    Ok(maybe_resolved)
}

impl IncludeResolver {
    pub fn new(
        mut quote_search_dirs: Vec<PathBuf>,
        mut system_search_dirs: Vec<PathBuf>,
    ) -> Result<Self> {
        for path_vec in [&mut quote_search_dirs, &mut system_search_dirs] {
            for path in path_vec {
                *path = path.canonicalize().with_context(|| {
                    format!("Failed to canonicalize search path: \"{}\"", path.display())
                })?;
            }
        }

        debug!("Quote search dirs: {:#?}", quote_search_dirs);
        debug!("System search dirs: {:#?}", system_search_dirs);
        Ok(Self {
            quote_search_paths: quote_search_dirs,
            system_search_paths: system_search_dirs,
        })
    }

    /// Tries to find the file referenced in a quote include statement.
    ///
    /// If found, returns the canonicalized path to the file.
    pub fn resolve_quote(&self, path: &str, current_dir: &Path) -> Result<Option<PathBuf>> {
        resolve(path, &self.quote_search_paths, Some(current_dir))
    }

    /// Tries to find the file referenced in a system include statement.
    ///
    /// If found, returns the canonicalized path to the file.
    pub fn resolve_system(&self, path: &str) -> Result<Option<PathBuf>> {
        resolve(path, &self.system_search_paths, None)
    }
}
