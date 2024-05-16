use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    // TODO: commands
    // pub search
    // pub update
    // pub get
    /// Path to your config file
    #[arg(short, long, verbatim_doc_comment)]
    pub config: Option<PathBuf>,

    /// wow dir root path
    #[arg(short, long, verbatim_doc_comment)]
    pub path: Option<PathBuf>,

    /// tmp addon files path
    #[arg(long, verbatim_doc_comment)]
    pub tmp: Option<PathBuf>,

    /// your CurseForge API token
    #[arg(long, verbatim_doc_comment)]
    pub token_curse: Option<String>,
}
