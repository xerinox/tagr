use clap::Parser;

use tagr::{
    Command, QueryCriteria, TagName, commands::tag::run as run_tag, store::{Store, StoreError},
    query
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
    let store = Store::new("test").connect().await.unwrap();
    store.migrate().await?;

    match args.command {
        Commands::Tag { tag, file } => {
            let cmd = Command::TagCommand{file, tag: TagName(tag)};
            run_tag(cmd, &store).await
        }
        Commands::Search { tag } => {
            let tags = Some(vec![TagName(tag)]);
            let cmd = QueryCriteria{tags};

            match query::execute(&cmd, &store).await? {
                None => println!("No files found with tag"),
                Some(files) => {
                    for file in files {
                        println!("{}", file.display());
                    }
                }
            }
            Ok(())
        }
    }
}
