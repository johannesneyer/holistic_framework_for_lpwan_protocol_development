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

import socket
import time

import network
import ntptime

import cbor2
from cobs import Cobs
from node import Node
from watchdog import Watchdog


class Network:
    def __init__(self, hostname):
        network.hostname(hostname)
        self._wlan = network.WLAN(network.STA_IF)
        self._wlan.active(False)
        self._wlan.config(pm=network.WLAN.PM_PERFORMANCE)

    def connect_wlan(self, key: str, ssid: str, watchdog: Watchdog, timeout: int):
        self._wlan.active(True)
        print(f'connecting to "{ssid}" WLAN...')
        self._wlan.connect(ssid, key)
        connect_start_time_ms = time.ticks_ms()
        while not self._wlan.isconnected():
            watchdog.feed(None)
            if time.ticks_ms() - connect_start_time_ms > timeout:
                raise Exception("connecting to WLAN timed out")
            time.sleep_ms(100)
        print("WLAN connected, my IP address:", self._wlan.ifconfig()[0])
        print("hostname:", network.hostname())

    def set_time_via_ntp(self):
        ntptime.host = "ch.pool.ntp.org"
        ntptime.timeout = 2
        print(f'fetching time from "{ntptime.host}" using NTP...')
        ntptime.settime()
        print("current time is", pretty_print_time(time.gmtime()))
        # print(f"{time.time_ns() // 1000}us since the unix epoch")


class Cloud:
    def __init__(self, node: Node, host: str, port: int, socket_timeout_s: int):
        self.node = node
        self._socket_timeout_s = socket_timeout_s
        self._sock = None
        try:
            self.cloud_addr = socket.getaddrinfo(host, port)[0][-1]
        except OSError as e:
            print("could not resolve cloud hostname:", e)
            raise

    def connect(self):
        if self._sock is not None:
            self._sock.close()
        self._sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._sock.settimeout(self._socket_timeout_s)
        self._sock.connect(self.cloud_addr)
        if self.node.id:
            self.send_node_info()

    def _write(self, data: memoryview[bytes]):
        """Encode using COBS and write to socket"""
        self._sock.write(Cobs.encode(data))

    def send_node_info(self):
        data = cbor2.dumps(
            {
                "Info": {
                    "id": self.node.id,
                    "crc": self.node.flash_crc,
                }
            }
        )
        self._write(memoryview(data))

    def send_log(self, log_data: bytes):
        data = cbor2.dumps({"Log": log_data})
        self._write(memoryview(data))

    def send_error(self, msg: str):
        print("sending error message: ", msg)
        data = cbor2.dumps({"Error": msg})
        self._write(memoryview(data))

    def send_halted(self, halted: bool):
        self._write(memoryview(cbor2.dumps({"Halted": halted})))


def pretty_print_time(time):
    """Converts output of time.gmtime() to a string"""
    return f"""\
{time[0]}-{time[1]}-{time[2]} \
{time[3]:02}:{time[4]:02}:{time[5]:02} UTC"""
