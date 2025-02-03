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

import json
from collections.abc import Iterable
from dataclasses import dataclass
from enum import Enum

import matplotlib.pyplot as plt

NodeId = int
UptimeMs = int

EXPECTED_CSV_HEADER = ["uptime", "node_id", "kind", "content"]


class Action:
    def __init__(self, d: dict, start: UptimeMs, state: str):
        self.kind = ActionKind(d.pop("kind"))
        self.start = start
        self.state = state
        match self.kind:
            case ActionKind.RECEIVE:
                self.duration = d["duration"]
                self.channel = d["channel"]
            case ActionKind.TRANSMIT:
                self.channel = d["channel"]
                self.delay_ms = d["delay_ms"]
            case ActionKind.WAIT:
                self.duration = d["duration"]
            case ActionKind.NONE:
                pass
            case other:
                raise Exception(f"unhandled action kind: {other}")

    def __str__(self):
        return str(self.kind) + " action"


@dataclass
class Node:
    id: NodeId
    is_sink: bool
    state: None | str
    action: None | Action
    uplink_data: set[NodeId]


class EventKind(Enum):
    ACTION = "action"
    NEW_CHILD = "new_child"
    MESSAGE = "message"
    RESET = "reset"
    STATE = "state"


class ActionKind(Enum):
    NONE = "none"
    RECEIVE = "receive"
    TRANSMIT = "transmit"
    WAIT = "wait"


def parse_events(
    file_path, follow=False
) -> Iterable[tuple[NodeId, UptimeMs, EventKind, dict]]:
    def read_csv_line(f):
        line = f.readline()
        if not line:
            return None
        return line.strip().split(";")

    with open(file_path, newline="") as f:
        assert read_csv_line(f) == EXPECTED_CSV_HEADER

        while True:
            event = read_csv_line(f)
            if not event:
                if follow:
                    plt.pause(1)
                    continue
                else:
                    break
            yield (
                NodeId(event[1]),
                UptimeMs(event[0]),
                EventKind(event[2]),
                json.loads(event[3]),
            )
