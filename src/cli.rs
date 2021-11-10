//! Definition and parsing of cli arguments
use std::path::{Path, PathBuf};

use clap::{AppSettings, ArgMatches, FromArgMatches, IntoApp, Parser};
use itertools::Itertools;
use log::LevelFilter;
use termcolor::ColorChoice;

use crate::{filter::InvertibleGlob, logging::ErrorHandling};

const LOG_LEVEL_NAMES: [&str; 6] = ["off", "error", "warn", "info", "debug", "trace"];
const COLOR_OPTIONS: [&str; 3] = ["always", "auto", "never"];
const CLAP_SETTINGS: AppSettings = AppSettings::HidePossibleValuesInHelp;

fn parse_color_option(value: &str) -> ColorChoice {
    match value {
        "always" => ColorChoice::Always,
        "auto" => ColorChoice::Auto,
        "never" => ColorChoice::Never,
        _ => unreachable!("clap did not filter out invalid --color value"),
    }
}

/// cpp-amalgamate combines one or more C++ source files and recursively inlines included headers.
/// It tracks which headers have been included and skips any further includes of them. Which
/// includes are inlined and which are left intact can be controlled with various options.
///
/// Use -h for short descriptions of the available options or --help for more details.
#[derive(Debug, Parser)]
#[clap(author, version, setting = CLAP_SETTINGS)]
pub struct Opts {
    /// ArgMatches used to create this instance
    #[clap(skip)]
    pub matches: ArgMatches,

    /// Source files to process
    #[clap(required = true, parse(from_os_str))]
    pub files: Vec<PathBuf>,

    /// Redirect output to a file
    #[clap(short, long, parse(from_os_str), value_name = "file")]
    pub output: Option<PathBuf>,

    /// Add a search directory for both system and quote includes
    #[clap(
        short,
        long,
        value_name = "dir",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    pub dir: Vec<PathBuf>,

    /// Add a search directory for quote includes
    #[clap(
        long,
        parse(from_os_str),
        value_name = "dir",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    pub dir_quote: Vec<PathBuf>,

    /// Add a search directory for system includes
    #[clap(
        long,
        parse(from_os_str),
        value_name = "dir",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    pub dir_system: Vec<PathBuf>,

    /// Filter which includes are inlined
    ///
    /// By default, cpp-amalgamate inlines every header it can resolve using the given search
    /// directories. With this option, headers can be excluded from being inlined. By prefixing the
    /// glob with '!', previously excluded files can be selectively added again. The globs given to
    /// this and --ignore-quote/--ignore-system are evaluated in order, with the latest matching
    /// glob taking precedence.
    #[clap(
        short,
        long,
        value_name = "glob",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    pub ignore: Vec<InvertibleGlob>,

    /// Filter which quote includes are inlined
    ///
    /// This option works just like --ignore, except it only applies to quote includes.
    #[clap(
        long,
        value_name = "glob",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    pub ignore_quote: Vec<InvertibleGlob>,

    /// Filter which system includes are inlined
    ///
    /// This option works just like --ignore, except it only applies to system includes.
    #[clap(
        long,
        value_name = "glob",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    pub ignore_system: Vec<InvertibleGlob>,

    /// How to handle an unresolvable include.
    ///
    /// By default, cpp-amalgamate ignores includes which cannot be resolved to allow specifying
    /// only the necessary search directories. This flag can be used to assert that all includes are
    /// being inlined.
    ///
    /// The possible values for this flag are error (aborts processing), warn (continues
    /// processing), and ignore (the default).
    #[clap(
        long,
        value_name = "handling",
        possible_values = &ErrorHandling::NAMES,
        conflicts_with_all = &["unresolvable-quote-include", "unresolvable-system-include"]
    )]
    pub unresolvable_include: Option<ErrorHandling>,

    /// How to handle an unresolvable quote include.
    ///
    /// Works like --unresolvable-include, except only for quote includes.
    #[clap(
        long,
        value_name = "handling",
        possible_values = &ErrorHandling::NAMES,
    )]
    pub unresolvable_quote_include: Option<ErrorHandling>,

    /// How to handle an unresolvable system include.
    ///
    /// Works like --unresolvable-include, except only for system includes.
    #[clap(
        long,
        value_name = "handling",
        possible_values = &ErrorHandling::NAMES,
    )]
    pub unresolvable_system_include: Option<ErrorHandling>,

    /// How to handle a cyclic include.
    ///
    /// Uses the same values as --unresolvable-include (error, warn, ignore), except that it
    /// defaults to error.
    #[clap(
        long,
        value_name = "level",
        default_value = "error",
        possible_values = &ErrorHandling::NAMES,
        hide_default_value = true,
    )]
    pub cyclic_include: ErrorHandling,

    /// Control whether to print colored output
    ///
    /// The available values are always, auto (the default), and never.
    #[clap(
        long,
        parse(from_str = parse_color_option),
        value_name = "when",
        default_value = "auto",
        possible_values = &COLOR_OPTIONS,
        hide_default_value = true,
    )]
    pub color: ColorChoice,

    /// Print log messages.
    ///
    /// The possible values are (in order from least to most messages): off, error, warn, info,
    /// debug, and trace. The default is off.
    #[clap(
        long,
        value_name = "level",
        default_value = "off",
        possible_values = &LOG_LEVEL_NAMES,
        hide_default_value = true,
    )]
    pub log: LevelFilter,
}

fn with_indices<'a, T>(
    matches: &'a ArgMatches,
    name: &str,
    values: &'a [T],
) -> impl Iterator<Item = (usize, &'a T)> + 'a {
    matches.indices_of(name).into_iter().flatten().zip(values)
}

impl Opts {
    pub fn parse() -> Self {
        let app = Self::into_app();
        let matches = app.get_matches();
        let mut opts = Self::from_arg_matches(&matches)
            .expect("from_arg_matches should never return None when derived?!");
        opts.matches = matches;
        opts
    }

    fn merge_by_cli_order<'a, T>(
        &'a self,
        list1: &'a [T],
        name1: &str,
        list2: &'a [T],
        name2: &str,
    ) -> impl Iterator<Item = &'a T> + 'a {
        with_indices(&self.matches, name1, list1)
            .merge_by(with_indices(&self.matches, name2, list2), |x, y| x.0 < y.0)
            .map(|(_, val)| val)
    }

    /// Returns a list of all quote search dirs in the order given on the cli.
    ///
    /// This is a merged list of the shared search dirs and the quote only search dirs.
    pub fn quote_search_dirs(&self) -> impl Iterator<Item = &Path> {
        self.merge_by_cli_order(&self.dir, "dir", &self.dir_quote, "dir-quote")
            .map(PathBuf::as_path)
    }

    /// Returns a list of all system search dirs in the order given on the cli.
    ///
    /// This is a merged list of the shared search dirs and the system only search dirs.
    pub fn system_search_dirs(&self) -> impl Iterator<Item = &Path> {
        self.merge_by_cli_order(&self.dir, "dir", &self.dir_system, "dir-system")
            .map(PathBuf::as_path)
    }

    /// Returns a list of all ignore globs for quote includes in the order given on the cli.
    ///
    /// This is a merged list of the --ignore and --ignore-quote options.
    pub fn quote_globs(&self) -> impl Iterator<Item = &InvertibleGlob> {
        self.merge_by_cli_order(&self.ignore, "ignore", &self.ignore_quote, "ignore-quote")
    }

    /// Returns a list of all ignore globs for system includes in the order given on the cli.
    ///
    /// This is a merged list of the --ignore and --ignore-system options.
    pub fn system_globs(&self) -> impl Iterator<Item = &InvertibleGlob> {
        self.merge_by_cli_order(&self.ignore, "ignore", &self.ignore_system, "ignore-system")
    }

    pub fn unresolvable_quote_include_handling(&self) -> ErrorHandling {
        self.unresolvable_include
            .or(self.unresolvable_quote_include)
            .unwrap_or(ErrorHandling::Ignore)
    }

    pub fn unresolvable_system_include_handling(&self) -> ErrorHandling {
        self.unresolvable_include
            .or(self.unresolvable_system_include)
            .unwrap_or(ErrorHandling::Ignore)
    }
}
