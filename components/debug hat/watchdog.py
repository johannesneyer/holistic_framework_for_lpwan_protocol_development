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

from machine import WDT


class Watchdog:
    def __init__(self, timeout_ms: int, enable: bool = True):
        self._timeout_ms = timeout_ms
        self.wdt = WDT(timeout=self._timeout_ms) if enable else None
        self.last_fed = time.ticks_ms()

    def feed(self, cloud: Cloud | None):
        if self.wdt:
            self.wdt.feed()
        now = time.ticks_ms()
        diff = now - self.last_fed
        self.last_fed = now
        if diff > self._timeout_ms * 0.65:
            msg = f"time since last feed() call: {now - self.last_fed} ms"
            if cloud:
                cloud.send_error(msg)
            else:
                print(msg)
