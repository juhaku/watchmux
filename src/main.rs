use std::path::PathBuf;

use clap::Parser;
use config::{Config, ConfigError, WatchError};
use futures::future;
use thiserror::Error;
use tokio::{io::AsyncWriteExt, sync::mpsc};

mod config;
#[derive(Parser, Debug)]
struct WatchMux {
    /// Path to the config file of watchmux.
    #[clap(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
}

#[derive(Error, Debug)]
enum WatchmuxError {
    #[error("failed to resolve config: {0:?}")]
    Config(#[from] ConfigError),
    #[error("failed to run watch process: {0:?}")]
    WatchError(#[from] WatchError),
}

#[tokio::main]
async fn main() -> Result<(), WatchmuxError> {
    let cli = WatchMux::parse();

    let config = config::read_config(cli.config).await?;

    run(config).await.map_err(WatchmuxError::WatchError)
}

async fn run(config: Config) -> Result<(), WatchError> {
    let (tx, mut rx) = mpsc::channel::<String>(1024);

    let processes = future::join_all(
        config
            .processes
            .into_iter()
            .map(|process| {
                let sender = tx.clone();
                tokio::spawn(async move { process.run(sender).await })
            })
            .collect::<Vec<_>>(),
    );
    tokio::pin!(processes);

    let mut stdout = tokio::io::stdout();
    loop {
        tokio::select! {
            _ = &mut processes => {
                rx.close();
                break;
            },
            Some(line) = rx.recv() => {
                stdout.write_all(line.as_bytes()).await?
            }
        };
    }

    Ok(())
}
