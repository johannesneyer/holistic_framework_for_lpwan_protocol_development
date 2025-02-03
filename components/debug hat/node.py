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

from collections import namedtuple


class Node(namedtuple("Node", ["id", "flash_crc"])):
    def __str__(self):
        id_str = f"0x{self.id:08x}" if self.id is not None else "unkown"
        crc_str = f"0x{self.flash_crc:08x}" if self.flash_crc is not None else "unkown"
        return f"Node {{ id: {id_str}, flash_crc: {crc_str} }}"
