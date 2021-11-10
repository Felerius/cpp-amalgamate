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
#![cfg_attr(test, allow(clippy::type_complexity))]

mod cli;
mod filter;
mod logging;
mod process;
mod resolve;

use std::{
    fs::File,
    io::{self, BufWriter, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};

use crate::{
    cli::Opts, filter::InliningFilter, logging::ErrorHandling, process::Processor,
    resolve::IncludeResolver,
};

fn run_with_writer(opts: &Opts, writer: impl Write) -> Result<()> {
    let resolver = IncludeResolver::new(
        opts.quote_search_dirs().map(PathBuf::from).collect(),
        opts.system_search_dirs().map(PathBuf::from).collect(),
    )?;
    let filter = InliningFilter::new(opts.quote_globs().cloned(), opts.system_globs().cloned())?;
    let mut processor = Processor::new(
        writer,
        resolver,
        filter,
        opts.cyclic_include,
        opts.unresolvable_quote_include_handling(),
        opts.unresolvable_system_include_handling(),
    );
    opts.files
        .iter()
        .try_for_each(|source_file| processor.process(source_file))
}

fn try_main() -> Result<()> {
    let opts = Opts::parse();
    logging::setup(opts.log, opts.color);
    if let Some(out_file) = &opts.output {
        log::info!("Writing to {:?}", out_file);
        let writer = BufWriter::new(File::create(out_file).context("Failed to open output file")?);
        run_with_writer(&opts, writer)
    } else {
        log::info!("Writing to terminal");
        let stdout = io::stdout();
        run_with_writer(&opts, stdout.lock())
    }
}

fn main() {
    if let Err(error) = try_main() {
        log::error!("{:#}", error);
    }
}
