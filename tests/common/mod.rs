// Usages by other integration tests don't seem to be picked up consistently?
#![allow(dead_code)]
use std::path::PathBuf;

use anyhow::Result;
use assert_cmd::Command;
use assert_fs::{prelude::*, NamedTempFile, TempDir};
use once_cell::sync::Lazy;

static BINARY: Lazy<PathBuf> =
    Lazy::new(|| assert_cmd::cargo::cargo_bin(assert_cmd::crate_name!()));

pub fn command() -> Command {
    Command::new(&*BINARY)
}

pub fn builder() -> TestSetupBuilder {
    TestSetupBuilder {
        search_dirs: Vec::new(),
        source_files: Vec::new(),
    }
}

pub struct TestSetupBuilder {
    pub search_dirs: Vec<(&'static str, TempDir)>,
    pub source_files: Vec<NamedTempFile>,
}

impl TestSetupBuilder {
    pub fn source_file(mut self, content: &str) -> Result<Self> {
        let file = NamedTempFile::new("src.cpp")?;
        file.write_str(content)?;
        self.source_files.push(file);
        Ok(self)
    }

    pub fn search_dir_setup(
        mut self,
        option: &'static str,
        setup: impl FnOnce(&mut TempDir) -> Result<()>,
    ) -> Result<Self> {
        let mut temp_dir = TempDir::new()?;
        setup(&mut temp_dir)?;
        self.search_dirs.push((option, temp_dir));
        Ok(self)
    }

    pub fn search_dir<'a>(
        self,
        option: &'static str,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self> {
        self.search_dir_setup(option, |dir| {
            for (path, content) in files {
                dir.child(path).write_str(content)?;
            }
            Ok(())
        })
    }

    pub fn command(&self) -> Command {
        let mut cmd = command();
        cmd.args(self.source_files.iter().map(NamedTempFile::path));
        for (option, dir) in &self.search_dirs {
            cmd.arg(option).arg(dir.path());
        }
        cmd
    }
}
