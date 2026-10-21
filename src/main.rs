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

#[tokio::main]
async fn main() {
    //let args = Args::parse();
    let store = Store::new("tagr").connect().await.unwrap();

    dbg!(store);
    /*match args.command {
        Commands::Tag{tag, file} => {
            let tag = TagName(tag);
            let cmd = TagCommand::new(tag, file);
            run_tag(cmd, &store);
            todo!()
        },
        Commands::Search{tag} => {
            let tag = TagName(tag);
            let cmd = SearchCommand::new(tag);
            run_search(cmd, &store);
            todo!();
        }

    }
    */
}
