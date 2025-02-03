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

import time

from machine import SPI, Pin
from micropython import const

FLASH_BASE_ADDRESS = const(0x0800_0000)
FLASH_SIZE_BYTES = const(256_000)
UUID_ADDRESS = const(0x1FFF_7580)

CRC32_POLYNOMIAL = const(0x04C1_1DB7)
CRC32_INIT_VALUE = const(0x0000_0000)

BOOTLOADER_SPI_FREQ = const(4_000_000)

BOOTLOADER_SYNC_BYTE = const(0x5A)
BOOTLOADER_SYNC_OK_BYTE = const(0xA5)
BOOTLOADER_ACK_BYTE = const(0x79)
BOOTLOADER_NACK_BYTE = const(0x1F)
BOOTLOADER_CMD_GET = const(0x00)
BOOTLOADER_CMD_VERSION = const(0x01)
BOOTLOADER_CMD_ID = const(0x02)
BOOTLOADER_CMD_READ_MEMORY = const(0x11)
BOOTLOADER_CMD_WRITE_MEMORY = const(0x31)
BOOTLOADER_CMD_ERASE_MEMORY = const(0x44)
BOOTLOADER_CMD_WRITE_UNPROTECT = const(0x73)
BOOTLOADER_CMD_GET_CHECKSUM = const(0xA1)
BOOTLOADER_WRITE_MAX_SIZE = const(256)


class Stm32BootloaderException(Exception):
    pass


class Stm32Bootloader:
    """Client for STM32 SPI bootloader

    bootloader manual:
    https://www.st.com/resource/en/application_note/dm00081379-spi-protocol-used-in-the-stm32-bootloader-stmicroelectronics.pdf
    """

    def __init__(self, nss_pin: int, boot0_pin: int, nrst_pin: int):
        self._nss_pin = Pin(nss_pin, Pin.OPEN_DRAIN, value=0)
        self._boot0_pin = Pin(boot0_pin, Pin.OUT, value=0)
        self._nrst_pin = Pin(nrst_pin, Pin.OPEN_DRAIN, value=0)  # halt
        self._spi = SPI(0, baudrate=BOOTLOADER_SPI_FREQ)  # SPI0: GP16-19

    def enter(self) -> None:
        print("entering bootloader")
        # minimum bootloader startup time is 0.390ms
        # see AN2606 Table 175 Bootloader startup timings
        self._boot0_pin.on()
        for _ in range(10):
            try:
                self.reset()
                self._bootloader_sync()
                self._boot0_pin.off()
                break
            except Stm32BootloaderException:
                pass
        else:
            self._boot0_pin.off()
            raise Stm32BootloaderException("entering bootloader timed out")

    def reset(self) -> None:
        print("resetting target")
        self._nrst_pin.off()
        # see DS13293 section 5.3.17 NRST pin characteristics
        time.sleep_us(1)
        self._nrst_pin.on()

    def halt(self) -> None:
        print("halting target")
        self._nrst_pin.off()

    def is_halted(self) -> bool:
        return not bool(self._nrst_pin.value())

    def _bootloader_sync(self) -> None:
        for i in range(500):
            rx = self._spi.read(1, BOOTLOADER_SYNC_BYTE)[0]
            if rx == BOOTLOADER_SYNC_OK_BYTE:
                # print(f"bootloader sync after {i}")
                break
        else:
            raise Stm32BootloaderException("no bootloader sync!")
        self._spi.write(bytes([0x00]))
        self._wait_for_ack()

    def _wait_for_ack(self) -> None:
        for i in range(2000):
            rx = self._spi.read(1)[0]
            if rx == BOOTLOADER_ACK_BYTE:
                self._spi.write(bytes([BOOTLOADER_ACK_BYTE]))
                # print(f"bootloader ack after {i}")
                return
            elif rx == BOOTLOADER_NACK_BYTE:
                self._spi.write(bytes([BOOTLOADER_ACK_BYTE]))
                raise Stm32BootloaderException("bootloader nack")
        raise Stm32BootloaderException("no bootloader (n)ack")

    def _calc_cmd_checksum(self, data: bytes) -> bytes:
        if len(data) == 1:
            checksum = data[0] ^ 0xFF
        else:
            checksum = 0
            for x in data:
                checksum = checksum ^ x
        return checksum.to_bytes(1, "big")

    def _write(self, data: bytes) -> None:
        self._spi.write(data + self._calc_cmd_checksum(data))

    def _write_with_ack(self, data: bytes) -> None:
        """write and wait for ack"""
        self._write(data)
        self._wait_for_ack()

    def _read(self, number_of_bytes: int) -> bytes:
        # skip first byte of response as it's a dummy byte
        return self._spi.read(number_of_bytes + 1)[1:]

    def _read_with_ack(self, number_of_bytes: int) -> bytes:
        """read command response and wait for ack"""
        response = self._read(number_of_bytes)
        self._wait_for_ack()
        return response

    def _read_with_ack_using_length_from_response(self) -> bytes:
        """read command response and wait for ack"""
        # first byte of response is a dummy byte
        # second byte of response is length - 1
        response_length = self._spi.read(2)[1] + 1
        response = self._spi.read(response_length)
        self._wait_for_ack()
        return response

    def _send_cmd(self, cmd: int) -> None:
        if cmd > 0xFF:
            raise Stm32BootloaderException(f"invalid command: 0x{cmd:x}")
        self._spi.write(bytes([BOOTLOADER_SYNC_BYTE]))
        self._write_with_ack(cmd.to_bytes(1, "big"))

    def cmd_get(self) -> bytes:
        self._send_cmd(BOOTLOADER_CMD_GET)
        return self._read_with_ack_using_length_from_response()

    def cmd_version(self) -> bytes:
        self._send_cmd(BOOTLOADER_CMD_VERSION)
        return self._read_with_ack(1)

    def cmd_read_memory(self, base_address: int, number_of_bytes: int) -> bytes:
        if base_address > 0xFFFF_FFFF:
            raise Stm32BootloaderException(f"invalid base address: 0x{base_address:x}")
        if number_of_bytes > 256:
            raise Stm32BootloaderException(
                f"number of bytes to read must not be larger than 256 ({number_of_bytes})"
            )
        # print(f"reading {number_of_bytes} bytes from 0x{base_address:08x}")
        self._send_cmd(BOOTLOADER_CMD_READ_MEMORY)
        self._write_with_ack(base_address.to_bytes(4, "big"))
        self._write_with_ack((number_of_bytes - 1).to_bytes(1, "big"))
        return self._read(number_of_bytes)

    def cmd_write_memory(self, base_address: int, data: bytes) -> None:
        # flash is written 64 bits at a time
        if base_address > 0xFFFF_FFFF:
            raise Stm32BootloaderException(f"invalid base address: 0x{base_address:x}")
        if len(data) % 2 != 0 or len(data) > 256:
            raise Stm32BootloaderException(
                f"length of data must not be higher than 256 and must be a multiple of 2 ({len(data)})"
            )
        print(f"writing {len(data)} bytes to 0x{base_address:08x}")
        self._send_cmd(BOOTLOADER_CMD_WRITE_MEMORY)
        self._write_with_ack(base_address.to_bytes(4, "big"))
        self._write_with_ack((len(data) - 1).to_bytes(1, "big") + data)

    def cmd_erase_memory(self, pages: bytes):
        if len(pages) % 2 != 0:
            raise Stm32BootloaderException(f"invalid number of pages ({len(pages)})")
        number_of_pages = len(pages) // 2
        print(f"erasing pages: 0x{pages.hex()}")
        self._send_cmd(BOOTLOADER_CMD_ERASE_MEMORY)
        self._write_with_ack((number_of_pages - 1).to_bytes(2, "big"))
        self._write_with_ack(pages)

    def cmd_mass_erase_memory(self):
        print("mass erasing flash")
        self._send_cmd(BOOTLOADER_CMD_ERASE_MEMORY)
        self._write_with_ack(b"\xff\xff")

    def cmd_get_checksum(self, start_address: int, number_of_words: int) -> int:
        if start_address > 0xFFFF_FFFF:
            raise Stm32BootloaderException(
                f"invalid start address: 0x{start_address:x}"
            )
        if number_of_words > 0xFFFF_FFFF:
            raise Stm32BootloaderException(f"invalid length: {number_of_words}")
        self._send_cmd(BOOTLOADER_CMD_GET_CHECKSUM)
        self._write_with_ack(start_address.to_bytes(4, "big"))
        self._write_with_ack(number_of_words.to_bytes(4, "big"))
        self._write_with_ack(CRC32_POLYNOMIAL.to_bytes(4, "big"))
        self._write_with_ack(CRC32_INIT_VALUE.to_bytes(4, "big"))
        self._wait_for_ack()
        response = self._read(5)
        if self._calc_cmd_checksum(response) != b"\x00":
            raise Stm32BootloaderException("response of checksum cmd is corrupted")
        return int.from_bytes(response[:-1], "big")

    def get_devnum(self) -> int:
        devnum = self.cmd_read_memory(UUID_ADDRESS, 4)
        return int.from_bytes(devnum, "little")

    def get_flash_checksum(self) -> int:
        """Calculate checksum of whole flash."""
        return self.cmd_get_checksum(FLASH_BASE_ADDRESS, FLASH_SIZE_BYTES // 4)

    # def cmd_write_unprotect(self) -> None:
    #     self._send_cmd(BOOTLOADER_CMD_WRITE_UNPROTECT)
    #     self._wait_for_ack()
