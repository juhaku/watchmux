# watchmux ~ Mux all your watch processes in Rust

Watchmux is a simple async cli tool to mux all your watch processes to single stdout.

Watchmux can run any number of commands or custom shell scripts which will be
executed with bash when type is set to `shell`. Shell scripts and commands can
be named with title and they can be provided with additional set of environment
variables. Commands and shell scripts are executed in parallel and each output
will be multiplexed to single stdout. Currently hard limit for concurrent
processes is 1024. Program will exit when all processes complete or by pressing
`<C-c>` to terminate program.

<video src="https://user-images.githubusercontent.com/26358664/188331152-4576fa69-2967-4a07-849a-fcd1e54f5952.mp4" controls></video>

## Install

Install it directly from git with following command.
```bash
cargo install --git https://github.com/juhaku/watchmux
```

## Usage

```bash
USAGE:
    watchmux [OPTIONS]

OPTIONS:
    -c, --config <FILE>
            Path to the config file of watchmux

    -h, --help
            Print help information
```

## Configuration file syntax (.watchmuxrc.yaml)

```yaml
processes:
  - title: command title
    cmd: echo hello world $NAME
    type: shell
    env:
      NAME: Nate
  - title: cargo
    cmd: cargo run
```

* **title**: text shown left most of the output to distinct where the output is originated.
* **cmd**: the actual command or shell script to exeucte e.g `cargo run` or with type `shell`
      this can multiline shell script e.g.
  ```bash
  cmd: |
    while [[ true == true ]]; do
      echo "this is true"
      sleep 1
    done
  ```
* **type**: `shell` or `cmd` which is default if not provided. `shell` for shell script which 
    are exeucted with bash -c `cmd`. `cmd` is executed as is and is expected to be found from `$PATH`
* **env**: map of environment variables to provided to `cmd`.

## Examples

Run wathcmux with `.watchmuxrc.yaml` in current directory:
```bash
watchmux
```

Run watchmux with custom config file:
```bash
watchmux -c path/to/config
```

Run watchmux with config from stdin:
```bash
cat <<EOF | watchmux -c -
processes:
  - title: foobar
    cmd: echo foobar
    type: shell
EOF
```

# License

Licensed under either of [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) license at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this crate
by you, shall be dual licensed, without any additional terms or conditions.
