use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};

use ansi_term::{Color, Style};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs,
    io::{self, AsyncBufRead, AsyncBufReadExt, BufReader, Lines},
    process::{Child, Command},
    sync::mpsc::{error::SendError, Sender},
    task::JoinError,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub processes: Vec<WatchProcess>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum RunType {
    #[serde(rename = "shell")]
    Shell,
    #[serde(rename = "cmd")]
    Cmd,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WatchProcess {
    title: String,
    cmd: String,
    #[serde(default = "default_true")]
    log: bool,
    #[serde(rename = "type")]
    run_type: Option<RunType>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    wait_for: String,
}

fn default_true() -> bool {
    true
}

impl WatchProcess {
    pub async fn run(&self, tx: Sender<String>) -> Result<(), WatchError> {
        if !self.wait_for.is_empty() {
            let child = Command::new("bash")
                .arg("-c")
                .arg(&self.wait_for)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .envs(&self.env)
                .spawn()
                .map_err(WatchError::IoChildProcess)?;

            self.execute_and_await(child, tx.clone(), &self.title)
                .await
                .and_then(|status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(WatchError::AwaitFor(status))
                    }
                })?;
        };

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
                .stderr(Stdio::piped())
                .args(args.iter())
                .envs(&self.env)
                .spawn()
                .map_err(WatchError::IoChildProcess)?;

            self.execute_and_await(child, tx, &self.title).await?
        } else {
            let child = Command::new("bash")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .arg("-c")
                .arg(&self.cmd)
                .envs(&self.env)
                .spawn()
                .map_err(WatchError::IoChildProcess)?;

            self.execute_and_await(child, tx, &self.title).await?
        };

        Ok(())
    }

    async fn listen_out<T>(
        mut out: Lines<T>,
        title: String,
        color: u8,
        sender: Sender<String>,
    ) -> Result<(), WatchError>
    where
        T: Unpin + Send + AsyncBufRead + 'static,
    {
        while let Ok(Some(line)) = out.next_line().await {
            let title = Style::new()
                .on(Color::Fixed(color))
                .paint(format!("[ {title} ] "));

            sender
                .send(format!("{title} {line}\n"))
                .await
                .map_err(WatchError::SendError)?
        }

        Ok(())
    }

    async fn execute_and_await(
        &self,
        mut child: Child,
        sender: Sender<String>,
        title: &str,
    ) -> Result<ExitStatus, WatchError> {
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdout_lines = BufReader::new(stdout).lines();
        let stderr_lines = BufReader::new(stderr).lines();

        let (out, err) = tokio::join!(
            WatchProcess::listen_out(stdout_lines, title.to_string(), 173, sender.clone()),
            WatchProcess::listen_out(stderr_lines, title.to_string(), 167, sender),
        );
        let child_process = tokio::spawn(async move { child.wait().await });

        if [out, err]
            .into_iter()
            .collect::<Result<(), WatchError>>()
            .is_err()
        {
            child_process.abort()
        };

        child_process.await?.map_err(WatchError::IoChildProcess)
    }
}

#[derive(Error, Debug)]
pub enum WatchError {
    #[error("child process io error: {0:?}")]
    IoChildProcess(#[from] io::Error),

    #[error("{0:?}")]
    ChildProcessExecute(#[from] JoinError),

    #[error("send failed to parent")]
    SendError(#[from] SendError<String>),

    #[error("await for failed with status: {0}, cannot proceed to run command!")]
    AwaitFor(ExitStatus),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("serde yaml")]
    Parse(#[from] serde_yaml::Error),

    #[error("config file not provided stdin")]
    Missing,

    #[error("no .watchmuxrc.yaml file in current directory")]
    NoRcFile,

    #[error("io failed to read file from path")]
    Io(#[from] io::Error),
}

pub async fn load(path: Option<PathBuf>) -> Result<Config, ConfigError> {
    match path {
        Some(path) => {
            if path.as_path().as_os_str() == "-" {
                read_config_file_stdin().await
            } else {
                read_config_file_path(path.as_path()).await
            }
        }
        None => read_config_from_rc_file().await,
    }
}

async fn read_config_file_stdin() -> Result<Config, ConfigError> {
    let stdin = tokio::io::stdin();
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

async fn read_config_from_rc_file() -> Result<Config, ConfigError> {
    let mut current_dir = env::current_dir()?;
    current_dir.push(".watchmuxrc.yaml");

    match current_dir.try_exists() {
        Ok(_) => read_config_file_path(current_dir.as_path()).await,
        Err(_) => Err(ConfigError::NoRcFile),
    }
}
