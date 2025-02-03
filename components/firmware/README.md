# Firmware

Rust/Embassy based firmware for the NUCLEO-WL55JC development kit.

## Setup

Install Rust toolchain

``` shell
doas pacman -S rustup
rustup target add thumbv7em-none-eabi
```

Test firmware during development

``` shell
cargo install probe-rs-tools --locked
DEFMT_LOG=debug cargo run --release --features log-rtt
```

Build firmware for running on test network

``` shell
cargo build --release --features log-serial
```

To connect to the chip with a debug probe, the readout protection (RDP) must be
disabled first. This can be done by mass erasing the chip using
[STM32CubeProgrammer](https://www.st.com/en/development-tools/stm32cubeprog.html).

Alternatively the [stm32wl-unlock](https://github.com/newAM/stm32wl-unlock) tool
might work:

``` shell
cargo install --git https://github.com/newAM/stm32wl-unlock.git
stm32wl-unlock
```

## Notes

- Connect pins CN10.32 and CN10.31 with a jumper to configures the node as a sink
