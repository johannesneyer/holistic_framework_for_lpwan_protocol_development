//  _____       ______   ____
// |_   _|     |  ____|/ ____|  Institute of Embedded Systems
//   | |  _ __ | |__  | (___    Zurich University of Applied Sciences
//   | | | '_ \|  __|  \___ \   8401 Winterthur, Switzerland
//  _| |_| | | | |____ ____) |
// |_____|_| |_|______|_____/
//
// Copyright 2025 Institute of Embedded Systems at Zurich University of Applied Sciences.
// All rights reserved.
// SPDX-License-Identifier: MIT

#![no_std]
#![no_main]

#[cfg(feature = "log-rtt")]
use defmt_rtt as _;

#[cfg(feature = "log-serial")]
use defmt_serial as _;

use embassy_executor::Spawner;
use panic_probe as _;

mod iv;

#[allow(unused_imports)]
use defmt::{dbg, debug, error, info, panic, warn};
use embassy_stm32::rng::{self, Rng};
use embassy_stm32::{bind_interrupts, gpio, peripherals, spi::Spi, time};
use embassy_time::{Delay, Duration, Instant, Timer};
use heapless::Vec;
use lora_modulation::BaseBandModulationParams;
use lora_phy::{
    mod_params::{Bandwidth, CodingRate, ModulationParams, SpreadingFactor, *},
    mod_traits::RadioKind,
    sx126x::{self, Stm32wl, Sx126x},
    LoRa, RxMode,
};
use postcard::{from_bytes, to_vec};

// for log-serial
#[allow(unused_imports)]
use embassy_stm32::{
    dma::NoDma,
    usart::{self, Uart},
};
#[cfg(feature = "log-serial")]
use static_cell::StaticCell;

use lightning::{self, Lightning, Message, OwnAndChildData};
use protocol_api::*;

// TODO: add checksum to messages to detect transmission errors

/// The first 32bits of the UID64 is a unique (among stm32wl5x devices) device number
const DEVNUM_PTR: *const u32 = 0x1FFF_7580 as *const u32;

const MAX_MESSAGE_SIZE: usize = 32;

const LORA_SPREADING_FACTOR: SpreadingFactor = SpreadingFactor::_8;
const LORA_BANDWIDTH: Bandwidth = Bandwidth::_125KHz;
/// Coding rate of 4/5 provides best trade off according to stm reference manual
const LORA_CODING_RATE: CodingRate = CodingRate::_4_6;
const LORA_PREAMBLE_LEN: u16 = 12;
const LORA_IMPLICIT_HEADER: bool = false;
const LORA_CRC_ON: bool = true;
const LORA_IQ_INVERTED: bool = false;
/// Output power in dBm [-17, 22]
const LORA_OUTPUT_POWER: i32 = 10;
const LORA_RX_BOOST: bool = false;
const LORA_USE_HIGH_POWER_PA: bool = false;

/// Packets with lower RSSI than this value get ignored.
const MIN_RSSI: i16 = -70;

// https://www.ofcomnet.ch/api/rir/1008/44 fits 10 125khz channels with channel distance of
// 125khz * 1.5: (865e6-863e6-125e3/2)/(125e3*1.5) ~= 10.33
fn get_channel_frequency(n: u8) -> u32 {
    assert!(n <= 10);
    863_000_000 + LORA_BANDWIDTH.hz() * (2 + 3 * n as u32) / 2
}

/// Required for calculating time on air
#[allow(dead_code)]
const LORA_PARAMS: BaseBandModulationParams =
    BaseBandModulationParams::new(LORA_SPREADING_FACTOR, LORA_BANDWIDTH, LORA_CODING_RATE);

bind_interrupts!(struct Irqs{
    SUBGHZ_RADIO => iv::InterruptHandler;
    // for log-serial
    USART1 => usart::InterruptHandler<peripherals::USART1>;
    RNG => rng::InterruptHandler<peripherals::RNG>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: time::Hertz(32_000_000),
            mode: HseMode::Bypass,
            prescaler: HsePrescaler::DIV1,
        });
        config.rcc.mux = ClockSrc::PLL1_R;
        config.rcc.pll = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV2,
            mul: PllMul::MUL6,
            divp: None,
            divq: Some(PllQDiv::DIV2), // PLL1_Q clock (32 / 2 * 6 / 2), used for RNG
            divr: Some(PllRDiv::DIV2), // sysclk 48Mhz clock (32 / 2 * 6 / 2)
        });
    }
    let p = embassy_stm32::init(config);

    // init pin early so it's stable when read
    let is_sink_pin = gpio::Input::new(p.PB3, gpio::Pull::Up);

    #[cfg(feature = "log-serial")]
    {
        let mut uart_config = usart::Config::default();
        uart_config.baudrate = 115200;
        static UART: StaticCell<Uart<'static, peripherals::USART1, peripherals::DMA2_CH1>> =
            StaticCell::new();
        let uart = UART
            .init(Uart::new(p.USART1, p.PB7, p.PB6, Irqs, p.DMA2_CH1, NoDma, uart_config).unwrap());
        defmt_serial::defmt_serial(uart);
    }

    // let _led1 = gpio::Output::new(p.PB9, gpio::Level::Low, gpio::Speed::Low);
    // let _led2 = gpio::Output::new(p.PB15, gpio::Level::High, gpio::Speed::Low);

    // Nucleo rf switch
    let _rf_ctrl1 = gpio::Output::new(p.PC4, gpio::Level::High, gpio::Speed::High);
    let rf_ctrl2 = gpio::Output::new(p.PC5, gpio::Level::High, gpio::Speed::High);
    let _rf_ctrl3 = gpio::Output::new(p.PC3, gpio::Level::High, gpio::Speed::High);

    let lora_spi = Spi::new_subghz(p.SUBGHZSPI, p.DMA1_CH1, p.DMA1_CH2);
    let lora_spi = iv::SubghzSpiDevice(lora_spi);

    let node_id = unsafe { DEVNUM_PTR.read() };

    let mut node = Lightning::new(node_id);

    node.is_sink = is_sink_pin.is_low();

    let mut rng = Rng::new(p.RNG, Irqs);

    let config = sx126x::Config {
        chip: Stm32wl {
            use_high_power_pa: LORA_USE_HIGH_POWER_PA,
        },
        tcxo_ctrl: Some(sx126x::TcxoCtrlVoltage::Ctrl1V7),
        use_dcdc: true,
        rx_boost: LORA_RX_BOOST,
    };

    let iv = iv::Stm32wlInterfaceVariant::new(Irqs, None, Some(rf_ctrl2)).unwrap();

    let mut lora = LoRa::new(Sx126x::new(lora_spi, iv, config), false, Delay)
        .await
        .unwrap();

    let modulation_params = get_modulation_params(&mut lora, 0);

    let mut tx_pkt_params = {
        match lora.create_tx_packet_params(
            LORA_PREAMBLE_LEN,
            LORA_IMPLICIT_HEADER,
            LORA_CRC_ON,
            LORA_IQ_INVERTED,
            &modulation_params,
        ) {
            Ok(pp) => pp,
            Err(err) => {
                info!("radio error = {}", err);
                return;
            }
        }
    };

    let mut receive_buffer = [0u8; MAX_MESSAGE_SIZE];

    let rx_pkt_params = {
        match lora.create_rx_packet_params(
            LORA_PREAMBLE_LEN,
            LORA_IMPLICIT_HEADER,
            receive_buffer.len() as u8,
            LORA_CRC_ON,
            LORA_IQ_INVERTED,
            &modulation_params,
        ) {
            Ok(pp) => pp,
            Err(err) => {
                info!("radio error = {}", err);
                return;
            }
        }
    };

    let mut rx_msg = None;
    let mut n: u16 = 0;
    loop {
        if !node.has_payload() {
            node.set_payload(n);
            n += 1;
        }

        let mut now = Instant::now().as_millis();
        let (action, uplink_data) = node.progress(now, rx_msg.take(), &mut rng);
        if let Some(uplink_data) = uplink_data {
            info!(
                "New uplink data: {}",
                OwnAndChildData::from_iter(uplink_data)
            );
        }
        match action {
            Action::Wait { end } => {
                Timer::at(Instant::from_millis(end)).await;
            }
            Action::Receive { end, channel } => {
                let modulation_params = get_modulation_params(&mut lora, channel);
                while end > now {
                    match lora_receive(
                        &mut lora,
                        &rx_pkt_params,
                        &modulation_params,
                        &mut receive_buffer,
                        Duration::from_millis(end - now),
                    )
                    .await
                    {
                        Ok(()) => match from_bytes(&receive_buffer) {
                            Ok(msg) => {
                                rx_msg = Some(msg);
                                break;
                            }
                            Err(err) => warn!("could not de-serialize message: {:?}", err),
                        },
                        Err(ReceiveError::RadioError) | Err(ReceiveError::Timeout) => {
                            break;
                        }
                        Err(ReceiveError::InsufficientSignalStrength) => {
                            info!("ignoring message with low RSSI");
                            // try decoding
                            if let Ok(msg) = from_bytes::<Message>(receive_buffer.as_ref()) {
                                info!("message: {}", msg);
                            }
                        }
                    }

                    now = Instant::now().as_millis();
                }

                if let Err(err) = lora.enter_standby().await {
                    error!("radio could not enter standby: {}", err);
                };
            }
            Action::Transmit {
                channel,
                message,
                delay,
            } => {
                if let Some(delay_ms) = delay {
                    Timer::after_millis(delay_ms).await;
                }
                let modulation_params = get_modulation_params(&mut lora, channel);
                let transmit_buffer: Vec<u8, MAX_MESSAGE_SIZE> = to_vec(&message).unwrap();
                info!("transmitting {}", message);
                lora_transmit(
                    &mut lora,
                    &mut tx_pkt_params,
                    &modulation_params,
                    transmit_buffer.as_slice(),
                )
                .await;
            }
            Action::None => {}
        }
    }
    // match lora.sleep(false).await {
    //     Ok(()) => info!("Sleep successful"),
    //     Err(err) => info!("Sleep unsuccessful = {}", err),
    // }
}

fn get_modulation_params<RK, DLY>(lora: &mut LoRa<RK, DLY>, channel: u8) -> ModulationParams
where
    RK: RadioKind,
    DLY: lora_phy::DelayNs,
{
    lora.create_modulation_params(
        LORA_SPREADING_FACTOR,
        LORA_BANDWIDTH,
        LORA_CODING_RATE,
        get_channel_frequency(channel),
    )
    .unwrap()
}

async fn lora_transmit<RK, DLY>(
    lora: &mut LoRa<RK, DLY>,
    tx_pkt_params: &mut PacketParams,
    modulation_params: &ModulationParams,
    buffer: &[u8],
) where
    RK: RadioKind,
    DLY: lora_phy::DelayNs,
{
    // info!(
    //     "time on air: {} us",
    //     LORA_PARAMS.time_on_air_us(
    //         Some(LORA_PREAMBLE_LEN as u8),
    //         !LORA_IMPLICIT_HEADER,
    //         buffer.len() as u8,
    //     )
    // );

    if let Err(err) = lora
        .prepare_for_tx(modulation_params, tx_pkt_params, LORA_OUTPUT_POWER, buffer)
        .await
    {
        info!("radio error = {}", err);
        return;
    };

    // TODO: return error
    if let Err(err) = lora.tx().await {
        info!("radio error = {}", err);
    };
}

async fn lora_receive<RK, DLY>(
    lora: &mut LoRa<RK, DLY>,
    rx_pkt_params: &PacketParams,
    modulation_params: &ModulationParams,
    buffer: &mut [u8; MAX_MESSAGE_SIZE],
    timeout: Duration,
) -> Result<(), ReceiveError>
where
    RK: RadioKind,
    DLY: lora_phy::DelayNs,
{
    match lora
        .prepare_for_rx(RxMode::Continuous, modulation_params, rx_pkt_params)
        .await
    {
        Ok(_) => {}
        Err(err) => {
            error!("radio error: {}", err);
            Err(ReceiveError::RadioError)?;
        }
    };

    *buffer = [00u8; MAX_MESSAGE_SIZE];

    match embassy_time::with_timeout(timeout, lora.rx(rx_pkt_params, buffer)).await {
        Ok(rx) => match rx {
            Ok((_received_len, rx_pkt_status)) => {
                info!(
                    "received message (rssi: {} dBm, snr: {} dB)",
                    rx_pkt_status.rssi, rx_pkt_status.snr,
                );
                if rx_pkt_status.rssi < MIN_RSSI {
                    Err(ReceiveError::InsufficientSignalStrength)?
                }
            }
            Err(err) => {
                info!("rx unsuccessful: {}", err);
                Err(ReceiveError::RadioError)?
            }
        },
        Err(_) => {
            Err(ReceiveError::Timeout)?;
        }
    }

    Ok(())
}

enum ReceiveError {
    InsufficientSignalStrength,
    RadioError,
    Timeout,
}

// prevent panic messages from being printed twice when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}
