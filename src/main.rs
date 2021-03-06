#![warn(
    // Lint groups
    future_incompatible,
    nonstandard_style,
    rust_2018_compatibility,
    rust_2018_idioms,
    rust_2021_compatibility,
    // Allow by default
    elided_lifetimes_in_paths,
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    // Clippy
    clippy::all,
    clippy::pedantic,
    clippy::cargo,
    clippy::clone_on_ref_ptr,
    clippy::decimal_literal_representation,
    clippy::filetype_is_file,
    clippy::float_cmp_const,
    clippy::get_unwrap,
    clippy::if_then_some_else_none,
    clippy::rc_mutex,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::shadow_unrelated,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used,
    clippy::verbose_file_reads,
)]
#![allow(clippy::module_name_repetitions, clippy::non_ascii_literal)]

mod cli;
mod filter;
mod logging;
mod process;
mod resolve;

use std::{
    env,
    fs::File,
    io::{self, BufWriter, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use log::{error, info};

use crate::{
    cli::Opts,
    filter::InliningFilter,
    logging::ErrorHandling,
    process::{ErrorHandlingOpts, Processor},
    resolve::IncludeResolver,
};

fn run_with_writer(opts: &Opts, writer: impl Write) -> Result<()> {
    let resolver = IncludeResolver::new(
        opts.quote_search_dirs().map(PathBuf::from).collect(),
        opts.system_search_dirs().map(PathBuf::from).collect(),
    )?;
    let filter = InliningFilter::new(
        opts.quote_filter_globs().cloned(),
        opts.system_filter_globs().cloned(),
    )?;
    let error_handling_opts = ErrorHandlingOpts {
        cyclic_include: opts.cyclic_include_handling(),
        unresolvable_quote_include: opts.unresolvable_quote_include_handling(),
        unresolvable_system_include: opts.unresolvable_system_include_handling(),
    };
    let mut processor = Processor::new(
        writer,
        resolver,
        opts.line_directives,
        filter,
        error_handling_opts,
    );
    opts.files
        .iter()
        .try_for_each(|source_file| processor.process(source_file))
}

fn try_main() -> Result<()> {
    let opts = Opts::parse();

    let mut builder = env_logger::builder();
    if env::var_os("RUST_LOG_VERBOSE").is_some() {
        builder.format_timestamp_millis();
    } else {
        builder
            .format_level(true)
            .format_module_path(false)
            .format_target(false)
            .format_timestamp(None);
    }
    builder.filter_level(opts.log_level()).init();

    if let Some(out_file) = &opts.output {
        info!("Writing to {:?}", out_file);
        let writer = BufWriter::new(File::create(out_file).context("Failed to open output file")?);
        run_with_writer(&opts, writer)
    } else {
        info!("Writing to terminal");
        let stdout = io::stdout();
        run_with_writer(&opts, stdout.lock())
    }
}

fn main() {
    if let Err(error) = try_main() {
        error!("{:#}", error);
        std::process::exit(1);
    }
}
