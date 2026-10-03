use clap::Parser;
use tagr::{
    TagName,
    commands::{
        search::{SearchCommand, run as run_search},
        tag::{TagCommand, run as run_tag},
    },
    store::Store,
};

#[derive(Debug, Parser)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug, Clone)]
enum Commands {
    Tag { tag: String, file: Option<String> },
    Search { tag: String },
}

fn main() {
    let args = Args::parse();
    dbg!(args);
    println!("Hello, world!");
}
