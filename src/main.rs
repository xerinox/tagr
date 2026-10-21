use clap::Parser;

use tagr::{
    TagName,
    commands::{
        search::{SearchCommand, run as run_search},
        tag::{TagCommand, run as run_tag},
    },
    store::{Store, StoreError},
};

#[derive(Debug, Parser)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug, Clone)]
enum Commands {
    Tag { tag: String, file: String },
    Search { tag: String },
}

#[tokio::main]
async fn main() -> Result<(), StoreError> {
    let args = Args::parse();
    let store = Store::new("tagr").connect().await.unwrap();
    store.migrate().await?;

    match args.command {
        Commands::Tag { tag, file } => {
            let cmd = TagCommand::new(file, TagName(tag));
            run_tag(cmd, &store).await
        }
        Commands::Search { tag } => {
            let cmd = SearchCommand::new(TagName(tag));

            match run_search(cmd, &store).await? {
                None => println!("No files found with tag"),
                Some(files) => {
                    for file in files {
                        println!("Found file: {file}");
                    }
                }
            }
            Ok(())
        }
    }
}
