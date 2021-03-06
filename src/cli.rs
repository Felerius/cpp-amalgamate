//! Definition and parsing of cli arguments
use std::path::{Path, PathBuf};

use clap::{ArgMatches, FromArgMatches, IntoApp, Parser};
use itertools::Itertools;
use log::LevelFilter;

use crate::{filter::InvertibleGlob, logging::ErrorHandling};

const ABOUT: &str = "cpp-amalgamate recursively combines C++ source files and the headers they
include into a single output file. It tracks which headers have been included and skips any further
references to them. Which includes are inlined and which are left as is can be precisely
controlled.";
const HELP_TEMPLATE: &str = "\
{before-help}{bin} {version}\n\
{author-with-newline}\
{about-section}\n\
{usage-heading}\n    {usage}\n\
\n\
{all-args}{after-help}\
";

#[derive(Debug, Parser)]
#[clap(
    author,
    version,
    about=ABOUT,
    help_template=HELP_TEMPLATE,
    hide_possible_values=true,
    // To make this work, we cannot use default_value for arguments.
    arg_required_else_help=true,
)]
pub struct Opts {
    /// ArgMatches used to create this instance
    #[clap(skip)]
    matches: ArgMatches,

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
    dir: Vec<PathBuf>,

    /// Add a search directory for quote includes
    #[clap(
        long,
        parse(from_os_str),
        value_name = "dir",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    dir_quote: Vec<PathBuf>,

    /// Add a search directory for system includes
    #[clap(
        long,
        parse(from_os_str),
        value_name = "dir",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    dir_system: Vec<PathBuf>,

    /// Filter which includes are inlined.
    ///
    /// By default, cpp-amalgamate inlines every header it can resolve using the given search
    /// directories. With this option, headers can be excluded from being inlined. By prefixing the
    /// glob with '!', previously excluded files can be selectively added again. The globs given to
    /// this and --filter-quote/--filter-system are evaluated in order, with the latest matching
    /// glob taking precedence.
    ///
    /// Globs are matched on the full path of the included file, with all symlinks resolved. A '**'
    /// can be used to match any number of directories, for example '**/a.hpp',
    /// '/usr/local/include/**', or even '/usr/**/*.hpp'.
    #[clap(
        short,
        long,
        value_name = "glob",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    filter: Vec<InvertibleGlob>,

    /// Filter which quote includes are inlined.
    ///
    /// This option works just like --filter, except it only applies to quote includes.
    #[clap(
        long,
        value_name = "glob",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    filter_quote: Vec<InvertibleGlob>,

    /// Filter which system includes are inlined.
    ///
    /// This option works just like --filter, except it only applies to system includes.
    #[clap(
        long,
        value_name = "glob",
        multiple_occurrences = true,
        number_of_values = 1
    )]
    filter_system: Vec<InvertibleGlob>,

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
    unresolvable_include: Option<ErrorHandling>,

    /// How to handle an unresolvable quote include.
    ///
    /// Works like --unresolvable-include, except only for quote includes.
    #[clap(
        long,
        value_name = "handling",
        possible_values = &ErrorHandling::NAMES,
    )]
    unresolvable_quote_include: Option<ErrorHandling>,

    /// How to handle an unresolvable system include.
    ///
    /// Works like --unresolvable-include, except only for system includes.
    #[clap(
        long,
        value_name = "handling",
        possible_values = &ErrorHandling::NAMES,
    )]
    unresolvable_system_include: Option<ErrorHandling>,

    /// How to handle a cyclic include.
    ///
    /// Uses the same values as --unresolvable-include (error, warn, ignore), except that it
    /// defaults to error.
    #[clap(
        long,
        value_name = "handling",
        possible_values = &ErrorHandling::NAMES,
    )]
    cyclic_include: Option<ErrorHandling>,

    /// Increase the verbosity of the output (can be passed multiple times).
    ///
    /// By default, only warnings and errors are reported. Passing '-v' includes info, '-vv' debug,
    /// and '-vvv` trace log messages.
    #[clap(short, long, parse(from_occurrences))]
    verbose: i8,

    /// Report only errors (-q) or nothing (-qq)
    #[clap(short, long, parse(from_occurrences), conflicts_with = "verbose")]
    quiet: i8,

    /// Add #line directives.
    ///
    /// These allow compilers and debuggers to resolve lines in the amalgamated file to their
    /// original files.
    #[clap(long)]
    pub line_directives: bool,
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
        let cmd = Self::command();
        let matches = cmd.get_matches();
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

    /// Returns a list of all filter globs for quote includes in the order given on the cli.
    ///
    /// This is a merged list of the --filter and --filter-quote options.
    pub fn quote_filter_globs(&self) -> impl Iterator<Item = &InvertibleGlob> {
        self.merge_by_cli_order(&self.filter, "filter", &self.filter_quote, "filter-quote")
    }

    /// Returns a list of all filter globs for system includes in the order given on the cli.
    ///
    /// This is a merged list of the --filter and --filter-system options.
    pub fn system_filter_globs(&self) -> impl Iterator<Item = &InvertibleGlob> {
        self.merge_by_cli_order(&self.filter, "filter", &self.filter_system, "filter-system")
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

    pub fn cyclic_include_handling(&self) -> ErrorHandling {
        self.cyclic_include.unwrap_or(ErrorHandling::Error)
    }

    pub fn log_level(&self) -> LevelFilter {
        match self.verbose - self.quiet {
            i8::MIN..=-2 => LevelFilter::Off,
            -1 => LevelFilter::Error,
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            3..=i8::MAX => LevelFilter::Trace,
        }
    }
}
