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


class Cobs:
    @staticmethod
    def encode(source: memoryview[bytes]) -> memoryview[bytearray]:
        """COBS encode using 0x00 as sentinel value"""
        if not source:
            return b""
        source_len = len(source)
        max_encoded_len = (
            source_len + 1 + source_len // 254 + (1 if source_len % 254 > 0 else 0)
        )
        encoded = memoryview(bytearray(max_encoded_len))
        sentinel_index = 0
        encoded_index = 1
        for source_index, source_byte in enumerate(source):
            if source_byte != 0x00:
                if encoded_index - sentinel_index == 0xFF:
                    encoded[sentinel_index] = 0xFF
                    sentinel_index = encoded_index
                    encoded_index += 1
                encoded[encoded_index] = source_byte
            else:
                encoded[sentinel_index] = encoded_index - sentinel_index
                sentinel_index = encoded_index
            encoded_index += 1
        encoded[sentinel_index] = encoded_index - sentinel_index
        encoded[encoded_index] = 0x00
        encoded_index += 1
        return encoded[:encoded_index]

    @staticmethod
    def decode(source: memoryview[bytes]) -> (memoryview[bytearray], int):
        """COBS decode using 0x00 as sentinel value

        returns decoded data and number of consumed bytes
        """
        if not source:
            raise EOFError("source buffer empty")
        if source[0] == 0x00:
            return b"", 1
        decoded = memoryview(bytearray(len(source)))
        sentinel_offset = source[0]
        skip_next = sentinel_offset == 0xFF
        decoded_index = 0
        for source_index, source_byte in enumerate(source[1:]):
            if source_byte == 0x00:
                break
            sentinel_offset -= 1
            if sentinel_offset != 0:
                decoded[decoded_index] = source_byte
                decoded_index += 1
            else:
                if not skip_next:
                    decoded[decoded_index] = 0x00
                    decoded_index += 1
                sentinel_offset = source_byte
                skip_next = sentinel_offset == 0xFF
        else:
            raise EOFError("more data needed")
        return decoded[:decoded_index], source_index + 2
