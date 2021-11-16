use crate::util;

use anyhow::Result;
use assert_fs::{prelude::*, NamedTempFile};

#[test]
fn invoking_help() {
    // Running with -h
    let short_help_output = util::command()
        .arg("-h")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // Running without arguments
    util::command().assert().failure().stderr(short_help_output);

    // Running with --help
    util::command().arg("--help").assert().success();
}

#[test]
fn missing_source_files() {
    util::command().arg("-d").arg("/").assert().failure();
}

#[test]
fn redirecting_output() -> Result<()> {
    let out_file = NamedTempFile::new("out.cpp")?;
    util::builder()
        .source_file("arst")?
        .command()
        .arg("-o")
        .arg(out_file.path())
        .assert()
        .success()
        .stdout("");
    out_file.assert("arst");
    Ok(())
}

#[test]
fn multiple_source_files() -> Result<()> {
    util::builder()
        .source_file("a")?
        .source_file("b")?
        .source_file("c")?
        .command()
        .assert()
        .success()
        .stdout("abc");
    Ok(())
}
