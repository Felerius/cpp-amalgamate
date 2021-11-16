mod common;

use std::path::PathBuf;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use predicates::prelude::*;

#[test]
fn cyclic_includes() -> Result<()> {
    for handling in [None, Some("error"), Some("warn"), Some("ignore")] {
        let builder = common::builder()
            .source_file("#include <a.hpp>")?
            .search_dir(
                "-d",
                [("a.hpp", "#include <b.hpp>"), ("b.hpp", "#include <a.hpp>")],
            )?;
        let mut command = builder.command();
        if let Some(handling) = handling {
            command.args(["--cyclic-include", handling]);
        }

        let mut assert = command.assert();
        let handling = handling.unwrap_or("error");
        if handling == "error" {
            assert = assert.failure();
        } else {
            assert = assert.success().stdout("#include <a.hpp>");
        }
        if handling != "ignore" {
            assert.stderr(predicate::str::is_empty().not());
        }
    }

    Ok(())
}

#[test]
fn cyclic_include_back_to_source_file() -> Result<()> {
    let mut a_path = PathBuf::new();
    common::builder()
        .search_dir_setup("-d", |dir| {
            dir.child("a.hpp").write_str("#include <a.hpp>")?;
            dir.child("b.hpp").write_str("#include <b.hpp>")?;
            a_path = dir.child("a.hpp").to_path_buf();
            Ok(())
        })?
        .command()
        .arg(a_path)
        .assert()
        .failure();
    Ok(())
}

#[test]
fn already_included_source_file() -> Result<()> {
    let mut include_path = PathBuf::new();
    common::builder()
        .source_file("#include <a.hpp>")?
        .search_dir_setup("-d", |dir| {
            dir.child("a.hpp").write_str("arst")?;
            include_path = dir.child("a.hpp").to_path_buf();
            Ok(())
        })?
        .command()
        .arg(include_path)
        .assert()
        .success()
        .stdout("arst");
    Ok(())
}

#[test]
fn include_headers_at_most_once() -> Result<()> {
    common::builder()
        .source_file(indoc! {"
            #include <a.hpp>
            #include <b.hpp>
        "})?
        .search_dir("-d", [("a.hpp", "#include <b.hpp>"), ("b.hpp", "arst\n")])?
        .command()
        .assert()
        .success()
        .stdout("arst\n");
    Ok(())
}

#[test]
fn file_identity_considers_symlinks() -> Result<()> {
    common::builder()
        .source_file(indoc! {"
            #include <a.hpp>
            #include <b.hpp>
        "})?
        .search_dir_setup("-d", |dir| {
            dir.child("a.hpp").write_str("arst\n")?;
            dir.child("b.hpp").symlink_to_file(dir.child("a.hpp"))?;
            Ok(())
        })?
        .command()
        .assert()
        .success()
        .stdout("arst\n");
    Ok(())
}

#[test]
fn weird_include_statements() -> Result<()> {
    common::builder()
        .source_file("# \t include \t <a.hpp> \t ")?
        .search_dir("-d", [("a.hpp", "arst")])?
        .command()
        .assert()
        .success()
        .stdout("arst");
    Ok(())
}

#[test]
fn line_directives() -> Result<()> {
    let builder = common::builder()
        .source_file(indoc! {"
            arst
            #include <a.hpp>

            arst
            #include <b.hpp>
            arst
        "})?
        .search_dir("-d", [("a.hpp", "#include <b.hpp>\n"), ("b.hpp", "qwfp\n")])?;

    let src_file = builder.source_files[0].to_path_buf().canonicalize()?;
    let dir = &builder.search_dirs[0].1;
    let b_hpp = dir.child("b.hpp").to_path_buf().canonicalize()?;

    builder
        .command()
        .arg("--line-directives")
        .assert()
        .success()
        .stdout(formatdoc! {r#"
            #line 1 "{src_file}"
            arst
            #line 1 "{b_hpp}"
            qwfp
            #line 3 "{src_file}"

            arst
            #line 6 "{src_file}"
            arst
            "#,
            src_file=src_file.display(),
            b_hpp=b_hpp.display()
        });
    Ok(())
}

#[test]
fn pragma_once_removal() -> Result<()> {
    common::builder()
        .source_file(indoc! {"
            #include <a.hpp>
            #include <b.hpp>
        "})?
        .search_dir(
            "-d",
            [
                ("a.hpp", "#pragma once\n"),
                ("b.hpp", "# \tpragma\t  once  \t\n"),
            ],
        )?
        .command()
        .assert()
        .success()
        .stdout("");
    Ok(())
}
