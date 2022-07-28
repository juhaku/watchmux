use std::path::{Path, PathBuf};

use clap::Parser;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs,
    io::{self, stdin, AsyncBufReadExt, BufReader},
};

#[derive(Parser, Debug)]
struct WatchMux {
    /// Path to the config file of watchmux.
    #[clap(short, long, value_name = "FILE")]
    config: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    filter: Option<String>,
    processes: Vec<WatchProcess>,
}

#[derive(Serialize, Deserialize, Debug)]
struct WatchProcess {
    title: String,
    cmd: String,
    #[serde(default = "default_true")]
    log: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Error, Debug)]
enum ConfigError {
    #[error("serde yaml")]
    Parse(#[from] serde_yaml::Error),

    #[error("config file not provided stdin")]
    Missing,

    #[error("io failed to read file from path")]
    Io(#[from] io::Error),
}

#[derive(Error, Debug)]
enum WatchError {
    #[error("config")]
    Config(#[from] ConfigError),
}

#[tokio::main]
async fn main() -> Result<(), WatchError> {
    let cli = WatchMux::parse();
    println!("Hello, world!");

    dbg!(&cli);

    if cli.config.as_path().as_os_str() == "-" {
        let config = read_config_file_stdin().await?;
        dbg!(&config);
    } else {
        let config = read_config_file_path(cli.config.as_path()).await?;
        dbg!(&config);
    }

    // process actions simultaneously
    // multiplex output to the single stdout

    Ok(())
}

async fn read_config_file_stdin() -> Result<Config, ConfigError> {
    let stdin = stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let mut config = String::new();

    while let Ok(Some(line)) = lines.next_line().await {
        config.push_str(line.as_str());
        config.push('\n');
    }

    if config.is_empty() {
        Err(ConfigError::Missing)
    } else {
        serde_yaml::from_str(config.as_str()).map_err(ConfigError::Parse)
    }
}

async fn read_config_file_path<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
    let config = fs::read_to_string(path.as_ref()).await?;

    serde_yaml::from_str(config.as_str()).map_err(ConfigError::Parse)
}
