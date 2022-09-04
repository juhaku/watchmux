use std::path::PathBuf;

use clap::Parser;
use config::{Config, ConfigError, WatchError};
use futures::future;
use thiserror::Error;
use tokio::{io::AsyncWriteExt, sync::mpsc};

mod config;

/// Multiplex your watch commands.
///
/// Watchmux can run any number of commands or custom shell scripts which will be
/// executed with bash when type is set to `shell`. Shell scripts and commands can
/// be named with title and they can be provided with additional set of environment
/// variables. Commands and shell scripts are executed in parallel and each output
/// will be multiplexed to single stdout. Currently hard limit for concurrent
/// processes is 1024. Program will exit when all processes complete or by pressing
/// <C-c> to terminate program.
///
/// Configuration file format is yaml listing processes to be executed:
/// processes:
///   - title: command title
///     cmd: echo hello world $NAME
///     type: shell
///     env:
///       NAME: Nate
///
/// * title: text shown left most of the output to distinct where the output is originated.
/// * cmd: the actual command or shell script to exeucte e.g `cargo run` or with type `shell`
///        this can multiline shell script e.g.
///        cmd: |
///          while [[ true == true ]]; do
///             echo "this is true"
///             sleep 1
///          done
/// * type: `shell` for shell script which are exeucted with `bash -c `cmd`.
/// * env: map of environment variables to provided to `cmd`.
///
/// EXAMPLES:
///
/// Run wathcmux with `.watchmuxrc.yaml` in current directory:
/// watchmux
///
/// Run watchmux with custom config file:
/// watchmux -c path/to/config
///
/// Run watchmux with config from stdin:
/// cat <<EOF | watchmux -c -
/// processes:                                        
///   - title: foobar
///     cmd: echo foobar
///     type: shell
/// EOF
///
/// Run watchmux with
#[derive(Parser, Debug)]
#[clap(verbatim_doc_comment)]
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

    let config = config::load(cli.config).await?;

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
