use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use clap::Parser;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs,
    io::{self, stdin, stdout, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
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
    #[serde(rename = "type")]
    run_type: Option<RunType>,
}

impl WatchProcess {
    async fn run(&self, sender: Sender<String>) -> Result<(), WatchError> {
        let ty = self.run_type.as_ref().unwrap_or(&RunType::Cmd);
        if *ty == RunType::Cmd {
            let (cmd, args) =
                self.cmd
                    .split(' ')
                    .fold(("", Vec::<&str>::new()), |(mut cmd, mut args), item| {
                        if item.is_empty() {
                            cmd = item;
                        } else {
                            args.push(item)
                        }

                        (cmd, args)
                    });

            let mut c = Command::new(cmd)
                .args(args.iter())
                .spawn()
                .map_err(WatchError::IoChildProcess)?;
            let stdout = c.stdout.take().unwrap();
        } else {
            let mut c = Command::new("bash")
                .arg("-c")
                .arg(&self.cmd)
                .spawn()
                .map_err(WatchError::IoChildProcess)?;
            let stdout = c.stdout.take().unwrap();
        };

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum RunType {
    #[serde(rename = "shell")]
    Shell,
    #[serde(rename = "cmd")]
    Cmd,
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

    #[error("io failed to spawn child process")]
    IoChildProcess(#[from] io::Error),
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

async fn run(cli: WatchMux, config: Config) -> Result<(), WatchError> {
    let (tx, rx) = mpsc::channel::<String>(num_cpus::get_physical() * 2);

    for process in config.processes.into_iter() {
        let sender = tx.clone();
        tokio::spawn(async move {
            // TODO
            let a = process.run(sender).await;
        });
    }

    let mut w = StdoutWriter(rx);
    let writer = w.write();
    tokio::pin!(writer);

    tokio::select! {
        _ = &mut writer => {
            // TODO
        }
    }
    // TODO
    // let cpus = num_cpus::get_physical();
    // for _ in 0..cpus {
    //     tokio::spawn(async move {

    //         // TODO
    //     });
    // }
    Ok(())
}

struct StdoutWriter(Receiver<String>);

impl StdoutWriter {
    async fn write(&mut self) {
        let mut out = io::stdout();

        while let Some(message) = self.0.recv().await {
            // TOOD write to the stdout
            let a = out.write_all(message.as_bytes()).await;
        }
    }
}
