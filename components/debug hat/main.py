#  _____       ______   ____
# |_   _|     |  ____|/ ____|  Institute of Embedded Systems
#   | |  _ __ | |__  | (___    Zurich University of Applied Sciences
#   | | | '_ \|  __|  \___ \   8401 Winterthur, Switzerland
#  _| |_| | | | |____ ____) |
# |_____|_| |_|______|_____/
#
# Copyright 2025 Institute of Embedded Systems at Zurich University of Applied Sciences.
# All rights reserved.
# SPDX-License-Identifier: MIT

import gc
import select
import time

import micropython
from machine import UART, Pin, Timer
from micropython import const

import cbor2
from cbor2._decoder import CBORDecodeError
from cloud import Cloud, Network
from cobs import Cobs
from node import Node
from stm32bootloader import FLASH_BASE_ADDRESS, Stm32Bootloader
from watchdog import Watchdog

# TODO: use a PIO SPI to receive logs for better performance/reliability
# with a baudrate of 115200 a log message takes ~1 ms to be transmitted

HOSTNAME_PREFIX = const("lightning-debug-hat-")

network_selection_pin = Pin(2, Pin.IN, Pin.PULL_UP)

if network_selection_pin.value() == 0:  # jumper on
    WLAN_SSID = "REDACTED"
    WLAN_PASS = "REDACTED"
    CLOUD_HOST = "REDACTED"
    CLOUD_PORT = 8883
else:
    WLAN_SSID = "REDACTED"
    WLAN_PASS = "REDACTED"
    CLOUD_HOST = "REDACTED"
    CLOUD_PORT = 50000

BOOTLOADER_PINS = {
    "nss_pin": 17,
    "boot0_pin": 21,
    "nrst_pin": 20,
}

WLAN_CONNECT_TIMEOUT_MS = const(60 * 1000)

# 8_388 ms is max on rp2040
WATCHDOG_TIMEOUT_MS = const(8_388)

SOCKET_TIMEOUT_S = const(4)

# periodically run garbage collector instead of only when we run out of memory
gc.threshold(10_000)

# required for producing an error report in an ISR
micropython.alloc_emergency_exception_buf(100)

NODE_ID_FILE = const("/node_id")

UART_RX_BUFFER_SIZE_BYTES = const(16384)

led = Pin("LED", Pin.OUT)
led_timer = Timer()


def led_off(_timer):
    led.off()


def flash_led():
    led.on()
    led_timer.init(period=50, mode=Timer.ONE_SHOT, callback=led_off)


def init_uart(poller) -> UART:
    uart = UART(0)  # UART0: GP0-1
    uart.init(115200, rxbuf=UART_RX_BUFFER_SIZE_BYTES)
    poller.register(uart, select.POLLIN)
    return uart


def reset(bl: Stm32Bootloader, cloud: Cloud):
    bl.reset()
    cloud.send_halted(False)


def halt(bl: Stm32Bootloader, cloud: Cloud):
    bl.halt()
    cloud.send_halted(True)


def send_halted(bl: Stm32Bootloader, cloud: Cloud):
    cloud.send_halted(bl.is_halted())


def send_traceback(exception: Exception, cloud: Cloud):
    from io import StringIO
    from sys import print_exception

    traceback_str = StringIO()
    print_exception(exception, traceback_str)
    cloud.send_error(traceback_str.getvalue())


def read_node_id_from_file() -> int | None:
    try:
        with open(NODE_ID_FILE, "rb") as f:
            return int.from_bytes(f.read(), "little")
    except OSError:
        return None


def write_node_id_to_file(node_id: int):
    with open(NODE_ID_FILE, "w") as f:
        f.write(node_id.to_bytes(4, "little"))


def connect_to_cloud(cloud: Cloud, poller, watchdog: Watchdog):
    print(f"connecting to cloud ({cloud.cloud_addr})...")
    while True:
        try:
            cloud.connect()
            poller.register(cloud._sock, select.POLLIN)
            print("connected to cloud")
            break
        except OSError as e:
            print("could not connect to cloud:", e)
            # clean up failed connection
            gc.collect()
            watchdog.feed(cloud)
            time.sleep(WATCHDOG_TIMEOUT_MS // 1000 // 2)


def handle_cloud_data(
    cloud: Cloud, bootloader: Stm32Bootloader, data: memoryview[bytes]
):
    try:
        message = cbor2.loads(data)
    except CBORDecodeError as e:
        cloud.send_error(f"could not decode cloud message: {e}")
        raise

    if isinstance(message, str):
        message_type = message
    elif isinstance(message, dict):
        message_type = list(message)[0]
    else:
        cloud.send_error(f"unexpected message from cloud: {message}")
        return

    # print("message from cloud:", message_type)

    if message_type == "Reset":
        reset(bootloader, cloud)
    elif message_type == "Halt":
        halt(bootloader, cloud)
    elif message_type == "GetInfo":
        cloud.send_node_info()
    elif message_type == "InitFwUpdate":
        print("updating firmware...")
        bootloader.enter()
        bootloader.cmd_mass_erase_memory()
    elif message_type == "FwChunk":
        offset = message[message_type]["offset"]
        fw_chunk = memoryview(message[message_type]["data"])
        bootloader.cmd_write_memory(
            FLASH_BASE_ADDRESS + offset,
            fw_chunk,
        )
    elif message_type == "FinishFwUpdate":
        cloud.node.flash_crc = bootloader.get_flash_checksum()
        cloud.send_node_info()
        halt(bootloader, cloud)
    else:
        cloud.send_error(f"unhandled message: {message}")


def main(
    poller, bootloader: Stm32Bootloader, cloud: Cloud, uart: UART, watchdog: Watchdog
):
    receive_buffer = bytearray()
    while True:
        for result in poller.poll(WATCHDOG_TIMEOUT_MS // 2):
            obj, event = result[0:2]
            if event & select.POLLHUP or event & select.POLLERR:
                poller.unregister(obj)
                raise Exception("poll() returned with POLLHUP or POLLERR for", obj)
            elif event & select.POLLIN:
                if obj is uart:
                    # read argument limits maximum size of resulting CBOR message
                    # TODO: only remove data from buffer when sent successfully
                    log_data = obj.read(2048)
                    flash_led()
                    try:
                        cloud.send_log(log_data)
                    except OSError as e:
                        print("could not send log to cloud:", e)
                        connect_to_cloud(cloud, poller, watchdog)
                        send_halted(bootloader, cloud)
                        continue
                elif obj is cloud._sock:
                    data = obj.recv(2048)

                    if not data:
                        print("TCP connection terminated")
                        poller.unregister(obj)
                        connect_to_cloud(cloud, poller, watchdog)
                        send_halted(bootloader, cloud)
                        continue

                    receive_buffer.extend(data)

                    while True:
                        try:
                            data, n_consumed = Cobs.decode(receive_buffer)
                        except EOFError:
                            # COBS frame incomplete more data needed
                            break
                        receive_buffer = receive_buffer[n_consumed:]
                        try:
                            handle_cloud_data(cloud, bootloader, memoryview(data))
                        except CBORDecodeError:
                            pass
                else:
                    raise Exception(f"unhandled poll object: {obj}")
            else:
                raise Exception(f"unhandled poll event from {obj}: {event}")
        watchdog.feed(cloud)
        # print(f"free RAM: {gc.mem_free():,} bytes")


try:
    watchdog = Watchdog(WATCHDOG_TIMEOUT_MS, enable=True)

    bootloader = Stm32Bootloader(**BOOTLOADER_PINS)

    # TODO: also write flash crc to file? this way node does not need to be reset on startup
    node = Node(read_node_id_from_file(), None)

    hostname = HOSTNAME_PREFIX + f"{node.id:08x}" if node.id is not None else "UNKNOWN"
    net = Network(hostname=hostname)
    net.connect_wlan(
        ssid=WLAN_SSID,
        key=WLAN_PASS,
        watchdog=watchdog,
        timeout=WLAN_CONNECT_TIMEOUT_MS,
    )
    # net.set_time_via_ntp()

    poller = select.poll()

    cloud = Cloud(
        node=node,
        host=CLOUD_HOST,
        port=CLOUD_PORT,
        socket_timeout_s=SOCKET_TIMEOUT_S,
    )
    connect_to_cloud(cloud, poller, watchdog)

    bootloader.enter()

    node_id = bootloader.get_devnum()
    if node_id != node.id:
        write_node_id_to_file(node_id)
        node.id = node_id
    node.flash_crc = bootloader.get_flash_checksum()

    cloud.send_node_info()

    print(node)

    halt(bootloader, cloud)

    uart = init_uart(poller)

    watchdog.feed(cloud)
except Exception as e:
    print("error during initialization:", e)
    try:
        send_traceback(e, cloud)
    except Exception as e:
        print("could not send error message:", e)
        cloud = None
    # wait a bit as error occurred during initialization
    for _ in range(60):
        watchdog.feed(cloud)
        time.sleep(1)
    raise

try:
    main(poller, bootloader, cloud, uart, watchdog)
except Exception as e:
    try:
        send_traceback(e, cloud)
    except Exception as e:
        print("could not send error message:", e)
    raise
