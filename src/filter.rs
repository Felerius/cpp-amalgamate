//! Filtering of which includes to inline
use std::{path::Path, str::FromStr};

use anyhow::{Error, Result};
use globset::{Candidate, Glob, GlobSet, GlobSetBuilder};
use log::{debug, log_enabled, Level};

use crate::logging::debug_file_name;

#[derive(Debug, Clone)]
pub struct InvertibleGlob {
    glob: Glob,
    inverted: bool,
}

impl FromStr for InvertibleGlob {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let (inverted, remainder) = s.strip_prefix('!').map_or((false, s), |tail| (true, tail));
        Ok(Self {
            glob: Glob::new(remainder)?,
            inverted,
        })
    }
}

#[derive(Debug)]
struct GlobInfo {
    str: String,
    inverted: bool,
}

#[derive(Debug)]
pub struct InliningFilter {
    quote_set: GlobSet,
    quote_infos: Vec<GlobInfo>,
    system_set: GlobSet,
    system_infos: Vec<GlobInfo>,
    indices: Vec<usize>,
}

fn build_set_and_infos(
    type_name: &str,
    globs: impl IntoIterator<Item = InvertibleGlob>,
) -> Result<(GlobSet, Vec<GlobInfo>)> {
    let mut set_builder = GlobSetBuilder::new();
    let infos: Vec<_> = globs
        .into_iter()
        .map(|invertible_glob| {
            let str = invertible_glob.glob.glob().to_owned();
            set_builder.add(invertible_glob.glob);
            GlobInfo {
                str,
                inverted: invertible_glob.inverted,
            }
        })
        .collect();

    if log_enabled!(Level::Debug) {
        let glob_strs: Vec<_> = infos.iter().map(|info| info.str.clone()).collect();
        debug!("{} ignore globs: {:?}", type_name, glob_strs);
    }

    Ok((set_builder.build()?, infos))
}

fn check_should_inline(
    path: &Path,
    set: &GlobSet,
    infos: &[GlobInfo],
    indices: &mut Vec<usize>,
) -> bool {
    let candidate = Candidate::new(path);
    let log_name = debug_file_name(path);
    set.matches_candidate_into(&candidate, indices);
    if let Some(&idx) = indices.last() {
        let glob_str = &infos[idx].str;
        if infos[idx].inverted {
            debug!("Inlining {:?} (cause: '{}')", log_name, glob_str);
            true
        } else {
            debug!("Not inlining {:?} (cause: '{}')", log_name, glob_str);
            false
        }
    } else {
        debug!("Inlining {:?} by default", log_name);
        true
    }
}

impl InliningFilter {
    pub fn new(
        quote_globs: impl IntoIterator<Item = InvertibleGlob>,
        system_globs: impl IntoIterator<Item = InvertibleGlob>,
    ) -> Result<Self> {
        let (quote_set, quote_infos) = build_set_and_infos("Quote", quote_globs)?;
        let (system_set, system_infos) = build_set_and_infos("System", system_globs)?;
        Ok(Self {
            quote_set,
            quote_infos,
            system_set,
            system_infos,
            indices: Vec::new(),
        })
    }

    /// Check whether a path should be included.
    pub fn should_inline(&mut self, path: &Path, is_system: bool) -> bool {
        let (set, infos) = if is_system {
            (&self.system_set, &self.system_infos)
        } else {
            (&self.quote_set, &self.quote_infos)
        };
        check_should_inline(path, set, infos, &mut self.indices)
    }
}
