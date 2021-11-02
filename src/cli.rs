//! Definition and parsing of cli arguments
use std::path::{Path, PathBuf};

use itertools::Itertools;
use log::LevelFilter;
use structopt::{
    clap::{AppSettings, ArgMatches},
    StructOpt,
};
use termcolor::ColorChoice;

use crate::logger::ErrorLevel;

const LOG_LEVEL_NAMES: [&str; 6] = ["off", "error", "warn", "info", "debug", "trace"];
const COLOR_OPTIONS: [&str; 3] = ["always", "auto", "never"];
const CLAP_SETTINGS: &[AppSettings] = &[AppSettings::UnifiedHelpMessage];

fn parse_color_option(value: &str) -> ColorChoice {
    match value {
        "always" => ColorChoice::Always,
        "auto" => ColorChoice::Auto,
        "never" => ColorChoice::Never,
        _ => unreachable!("clap did not filter out invalid --color value"),
    }
}

/// Combine C++ sources and headers into a single file.
#[derive(Debug, StructOpt)]
#[structopt(author, settings = &CLAP_SETTINGS)]
pub struct Opts {
    /// ArgMatches instance used to create this instance.
    #[structopt(skip)]
    pub matches: ArgMatches<'static>,

    /// Source files to process
    #[structopt(required = true)]
    pub files: Vec<PathBuf>,

    /// Redirect to output to a file
    #[structopt(short, long, value_name = "file")]
    pub output: Option<PathBuf>,

    /// Add a search directory for both system and quote includes
    #[structopt(
        short = "d",
        long = "search-dir",
        value_name = "dir",
        number_of_values = 1
    )]
    pub search_dirs: Vec<PathBuf>,

    /// Add a search directory exclusively for quote includes
    #[structopt(long = "search-dir-quote", value_name = "dir", number_of_values = 1)]
    pub quote_only_search_dirs: Vec<PathBuf>,

    /// Add a search directory exclusively for system includes
    #[structopt(long = "search-dir-system", value_name = "dir", number_of_values = 1)]
    pub system_only_search_dirs: Vec<PathBuf>,

    /// How to handle a missing include
    #[structopt(
        long,
        value_name = "level",
        default_value = "error",
        possible_values = &ErrorLevel::NAMES
    )]
    pub missing_include: ErrorLevel,

    /// How to handle a cyclic include
    #[structopt(
        long,
        value_name = "level",
        default_value = "error",
        possible_values = &ErrorLevel::NAMES
    )]
    pub cyclic_include: ErrorLevel,

    /// Control when to print colored output
    #[structopt(
        long,
        parse(from_str = parse_color_option),
        default_value = "auto",
        possible_values = &COLOR_OPTIONS,
        value_name = "when"
    )]
    pub color: ColorChoice,

    /// Set minimum level for printed log messages
    #[structopt(
        long,
        default_value = "warn",
        possible_values = &LOG_LEVEL_NAMES,
        value_name = "level"
    )]
    pub log: LevelFilter,
}

impl Opts {
    pub fn parse() -> Self {
        let clap = Self::clap();
        let matches = clap.get_matches();
        let mut opts = Self::from_clap(&matches);
        opts.matches = matches;
        opts
    }

    fn merge_by_cli_order<'a>(
        &'a self,
        list1: &'a [PathBuf],
        name1: &str,
        list2: &'a [PathBuf],
        name2: &str,
    ) -> impl Iterator<Item = &'a Path> + 'a {
        let with_indices1 = self
            .matches
            .indices_of(name1)
            .into_iter()
            .flatten()
            .zip(list1);
        let with_indices2 = self
            .matches
            .indices_of(name2)
            .into_iter()
            .flatten()
            .zip(list2);
        with_indices1.merge(with_indices2).map(|(_, p)| p.as_path())
    }

    /// Returns a list of all quote search dirs in the order given on the cli.
    ///
    /// This is a merged list of the shared search dirs and the quote only search dirs.
    pub fn quote_search_dirs(&self) -> impl Iterator<Item = &Path> {
        self.merge_by_cli_order(
            &self.search_dirs,
            "search-dirs",
            &self.quote_only_search_dirs,
            "quote-only-search-dirs",
        )
    }

    /// Returns a list of all system search dirs in the order given on the cli.
    ///
    /// This is a merged list of the shared search dirs and the system only search dirs.
    pub fn system_search_dirs(&self) -> impl Iterator<Item = &Path> {
        self.merge_by_cli_order(
            &self.search_dirs,
            "search-dirs",
            &self.system_only_search_dirs,
            "system-only-search-dirs",
        )
    }
}
