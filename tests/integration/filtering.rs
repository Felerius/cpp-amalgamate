use crate::util;

use anyhow::Result;
use indoc::indoc;

#[test]
fn blacklist_filters() -> Result<()> {
    util::builder()
        .source_file(indoc! {"
            #include <a.hpp>
            #include <b.hpp>
        "})?
        .search_dir("-d", [("a.hpp", "// a.hpp\n"), ("b.hpp", "// b.hpp\n")])?
        .command()
        .args(["--filter", "**/b.hpp"])
        .assert()
        .success()
        .stdout(indoc! {"
            // a.hpp
            #include <b.hpp>
        "});
    Ok(())
}

#[test]
fn whilelist_filters() -> Result<()> {
    util::builder()
        .source_file(indoc! {"
            #include <a.hpp>
            #include <b.hpp>
        "})?
        .search_dir("-d", [("a.hpp", "// a.hpp\n"), ("b.hpp", "// b.hpp\n")])?
        .command()
        .args(["-f", "**", "-f", "!**/b.hpp"])
        .assert()
        .success()
        .stdout(indoc! {"
            #include <a.hpp>
            // b.hpp
        "});
    Ok(())
}

#[test]
fn quote_and_system_only_filters() -> Result<()> {
    util::builder()
        .source_file(indoc! {r#"
            #include <a.hpp>
            #include <b.hpp>
            #include "a.hpp"
            #include "b.hpp"
        "#})?
        .search_dir("-d", [("a.hpp", "// a.hpp\n"), ("b.hpp", "// b.hpp\n")])?
        .command()
        .args([
            "-f",
            "**",
            "--filter-quote",
            "!**/b.hpp",
            "--filter-system",
            "!**/a.hpp",
        ])
        .assert()
        .success()
        .stdout(indoc! {r#"
            // a.hpp
            #include <b.hpp>
            #include "a.hpp"
            // b.hpp
        "#});
    Ok(())
}

#[test]
fn filter_precedence() -> Result<()> {
    util::builder()
        .source_file("#include <a/b/c.hpp>")?
        .search_dir("-d", [("a/b/c.hpp", "arst")])?
        .command()
        .args(["-f", "**/a/**", "-f", "!**/a/b/**", "-f", "**/a/b/c.hpp"])
        .assert()
        .success()
        .stdout("#include <a/b/c.hpp>");
    Ok(())
}
