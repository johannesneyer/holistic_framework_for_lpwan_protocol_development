# Server Application

build firmware:

``` shell
pushd .; cd ../firmware; cargo build --release --features log-serial; popd
```

then run:

``` shell
cargo run --release 0.0.0.0:8883 /path/to/target/thumbv7em-none-eabi/release/lightning_firmware_for_stm32wl55
```

run on server with `tmux`:

``` shell
tmux new -s mt_cloud
CLICOLOR_FORCE=1 cargo run --release 0.0.0.0:8883 /path/to/target/thumbv7em-none-eabi/release/lightning_firmware_for_stm32wl55 | tee /path/to/cloud_stdout
# to detach: C-b d
# to attach: tmux a -t mt_cloud
```

run locally and forward data from server to localhost (debug add-on still connects to server):

``` shell
ssh neye@srv-lab-t-430.zhaw.ch -R 0.0.0.0:8883:localhost:8883
cargo run --release 0.0.0.0:8883 /path/to/target/thumbv7em-none-eabi/release/lightning_firmware_for_stm32wl55
```
