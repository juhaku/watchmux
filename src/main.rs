use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use ansi_term::{Color, Style};
use clap::Parser;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs,
    io::{self, stdin, stdout, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc::{self, error::SendError, Receiver, Sender},
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
                        if cmd.is_empty() {
                            cmd = item;
                        } else {
                            args.push(item)
                        }

                        (cmd, args)
                    });

            let child = Command::new(cmd)
                .stdout(Stdio::piped())
                .args(args.iter())
                .spawn()
                .map_err(WatchError::IoChildProcess)?;

            self.execute_and_await(child, sender, &*self.title).await?
        } else {
            let child = Command::new("bash")
                .stdout(Stdio::piped())
                .arg("-c")
                .arg(&self.cmd)
                .spawn()
                .map_err(WatchError::IoChildProcess)?;

            self.execute_and_await(child, sender, &*self.title).await?
        };

        Ok(())
    }

    async fn execute_and_await(
        &self,
        mut child: Child,
        sender: Sender<String>,
        title: &str,
    ) -> Result<(), WatchError> {
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();

        let process_title = String::from(title);
        tokio::spawn(async move {
            if let Err(error) = child.wait().await {
                eprintln!("child '{process_title}' process encountered error, {error}")
            }
        });

        while let Some(line) = &lines.next_line().await? {
            let title = Style::new()
                .on(Color::Fixed(173))
                .paint(format!("[ {title} ]>"));

            sender.send(format!("{title} {line}\n")).await?;
        }
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

    #[error("send failed to parent")]
    SendError(#[from] SendError<String>),
}

#[tokio::main]
async fn main() -> Result<(), WatchError> {
    let cli = WatchMux::parse();

    let config = if cli.config.as_path().as_os_str() == "-" {
        read_config_file_stdin().await?
    } else {
        read_config_file_path(cli.config.as_path()).await?
    };

    run(config).await
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

async fn run(config: Config) -> Result<(), WatchError> {
    // let (tx, rx) = mpsc::channel::<String>(num_cpus::get_physical() * 2);
    let (tx, mut rx) = mpsc::channel::<String>(1024);

    for process in config.processes.into_iter() {
        let sender = tx.clone();
        tokio::spawn(async move {
            process
                .run(sender)
                .await
                .unwrap_or_else(|e| panic!("process {process:?} errored, error: {e:?}"));
        });
    }

    // let mut w = StdoutWriter(rx);
    // let writer = w.write();
    // tokio::pin!(writer);

    // tokio::select! {
    //     _ = &mut writer => {
    //         // TODO
    //     }
    // }
    // TODO
    // let cpus = num_cpus::get_physical();
    // for _ in 0..cpus {
    //     tokio::spawn(async move {

    //         // TODO
    //     });
    // }

    let mut out = stdout();
    while let Some(line) = rx.recv().await {
        out.write_all(line.as_bytes()).await?
    }
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
