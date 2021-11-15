mod common;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use predicates::prelude::*;

#[test]
fn basic_file_resolving() -> Result<()> {
    common::builder()
        .source_file(indoc! {r#"
            #include "a.hpp"
            // hello?
            #include <b/c.hpp>
        "#})?
        .search_dir("-d", [("a.hpp", "// a.hpp\n")])?
        .search_dir("--dir", [("b/c.hpp", "// b/c.hpp\n")])?
        .command()
        .assert()
        .success()
        .stdout(indoc! {"
            // a.hpp
            // hello?
            // b/c.hpp
        "});

    Ok(())
}

#[test]
fn quote_and_system_only_search_dirs() -> Result<()> {
    common::builder()
        .source_file(indoc! {r#"
            #include "a.hpp"
            #include "b.hpp"
            #include "c.hpp"
            #include <a.hpp>
            #include <b.hpp>
            #include <c.hpp>
        "#})?
        .search_dir("--dir-quote", [("a.hpp", "// a.hpp quote\n")])?
        .search_dir("--dir-system", [("b.hpp", "// b.hpp system\n")])?
        .search_dir(
            "-d",
            [
                ("a.hpp", "// a.hpp shared\n"),
                ("b.hpp", "// b.hpp shared\n"),
                ("c.hpp", "// c.hpp shared\n"),
            ],
        )?
        .command()
        .assert()
        .stdout(indoc! {"
            // a.hpp quote
            // b.hpp shared
            // c.hpp shared
            // a.hpp shared
            // b.hpp system
        "});
    Ok(())
}

#[test]
fn precedence_of_search_dirs() -> Result<()> {
    common::builder()
        .source_file("#include <a.hpp>")?
        .search_dir("-d", [("a.hpp", "// 1")])?
        .search_dir("-d", [("a.hpp", "// 2")])?
        .command()
        .assert()
        .success()
        .stdout("// 1");

    Ok(())
}

#[test]
fn resolving_to_file_symlinks() -> Result<()> {
    common::builder()
        .source_file("#include <b.hpp>")?
        .search_dir_setup("-d", |dir| {
            let a_path = dir.child("a.hpp");
            a_path.write_str("// a.hpp")?;
            dir.child("b.hpp").symlink_to_file(a_path)?;
            Ok(())
        })?
        .command()
        .assert()
        .success()
        .stdout("// a.hpp");

    Ok(())
}

#[test]
fn directories_are_not_valid_resolves() -> Result<()> {
    common::builder()
        .source_file("#include <a>")?
        .search_dir_setup("-d", |dir| {
            dir.child("a").create_dir_all()?;
            Ok(())
        })?
        .command()
        .assert()
        .success()
        .stdout("#include <a>");
    Ok(())
}

#[test]
fn unresolvable_include_error_options() -> Result<()> {
    let handling_options = ["error", "warn", "ignore"];
    for handling in handling_options {
        for (left, right) in [('<', '>'), ('"', '"')] {
            let input = format!("#include {}a{}", left, right);
            let mut assert = common::builder()
                .source_file(&input)?
                .command()
                .args(["--unresolvable-include", handling])
                .assert();

            if handling == "error" {
                assert = assert.failure();
            } else {
                assert = assert.success().stdout(input);
            }
            if handling != "ignore" {
                assert.stderr(predicate::str::is_empty().not());
            }
        }
    }

    for quote_handling in handling_options {
        for system_handling in handling_options {
            for (left, right, relevant_handling) in
                [('<', '>', system_handling), ('"', '"', quote_handling)]
            {
                let input = format!("#include {}a{}", left, right);
                let mut assert = common::builder()
                    .source_file(&input)?
                    .command()
                    .args([
                        "--unresolvable-quote-include",
                        quote_handling,
                        "--unresolvable-system-include",
                        system_handling,
                    ])
                    .assert();

                if relevant_handling == "error" {
                    assert = assert.failure();
                } else {
                    assert = assert.success().stdout(input);
                }
                if relevant_handling != "ignore" {
                    assert.stderr(predicate::str::is_empty().not());
                }
            }
        }
    }
    Ok(())
}
