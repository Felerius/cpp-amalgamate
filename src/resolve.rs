//! Resolves paths in include statements to the included files.
use std::{
    fmt::{self, Display, Formatter},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

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
            log::trace!("Trying to resolve {} to {:?}", printer, potential_path);

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
        log::debug!("Resolved {}{}{} to {:?}", left, path, right, resolved);
    } else {
        log::debug!("Failed to resolve {}{}{}", left, path, right);
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

        log::debug!("Quote search dirs: {:#?}", quote_search_dirs);
        log::debug!("System search dirs: {:#?}", system_search_dirs);
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn create_files<const N: usize>(dir: &Path, names: [&str; N]) -> Result<[PathBuf; N]> {
        let mut paths = [(); N].map(|_| PathBuf::new());
        for (name, path) in names.into_iter().zip(&mut paths) {
            let rel_path = dir.join(name);
            if let Some(parent) = rel_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&rel_path, "")?;
            *path = rel_path.canonicalize()?;
        }

        Ok(paths)
    }

    fn setup<const N: usize, const M: usize>(
        quote_names: [&str; N],
        system_names: [&str; M],
    ) -> Result<(
        IncludeResolver,
        TempDir,
        PathBuf,
        PathBuf,
        [PathBuf; N],
        [PathBuf; M],
    )> {
        let temp_dir = TempDir::new()?;
        let quote_dir = temp_dir.path().join("quote");
        let system_dir = temp_dir.path().join("system");
        fs::create_dir(&quote_dir)?;
        fs::create_dir(&system_dir)?;
        let quote_paths = create_files(&quote_dir, quote_names)?;
        let system_paths = create_files(&system_dir, system_names)?;
        let resolver = IncludeResolver::new(vec![quote_dir.clone()], vec![system_dir.clone()])?;
        Ok((
            resolver,
            temp_dir,
            quote_dir,
            system_dir,
            quote_paths,
            system_paths,
        ))
    }

    #[test]
    fn resolves_basic_files() -> Result<()> {
        let (resolver, temp_dir, _, system_dir, [a_path], [b_path]) = setup(["a"], ["b"])?;

        assert_eq!(resolver.resolve_quote("a", temp_dir.path())?, Some(a_path));
        assert!(resolver.resolve_quote("b", temp_dir.path())?.is_none());
        assert_eq!(
            resolver.resolve_quote("b", &system_dir)?,
            Some(b_path.clone())
        );
        assert!(resolver.resolve_quote("c", &system_dir)?.is_none());

        assert_eq!(resolver.resolve_system("b")?, Some(b_path));
        assert!(resolver.resolve_system("a")?.is_none());

        Ok(())
    }

    #[test]
    fn resolves_nested_paths() -> Result<()> {
        let (resolver, _temp_dir, _, _, _, [path, _]) = setup([], ["a/b/c", "a/d/c"])?;

        assert_eq!(resolver.resolve_system("a/b/c")?, Some(path.clone()));
        assert_eq!(resolver.resolve_system("a/d/../b/c")?, Some(path));

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn resolves_symlinks() -> Result<()> {
        use std::os::unix;

        let (resolver, _temp_dir, _, system_dir, _, [a_path]) = setup([], ["a"])?;
        unix::fs::symlink(&a_path, system_dir.join("b"))?;

        assert_eq!(resolver.resolve_system("b")?, Some(a_path));

        Ok(())
    }
}
