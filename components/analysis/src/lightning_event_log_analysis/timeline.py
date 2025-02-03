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

import argparse
from ast import literal_eval

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
from matplotlib import patches

from lightning_event_log_analysis import (
    Action,
    ActionKind,
    EventKind,
    Node,
    NodeId,
    UptimeMs,
    parse_events,
)

# import matplotlib
# matplotlib.rcParams['figure.dpi'] = 200

PlotOffset = int

# STM32WL55 datasheet (DS13293) section 5.3.3 Sub-GHz radio characteristics (table 28 and 29)
rx_power_w = 5.46e-3 * 3.3
tx_power_w = 23.5e-3 * 3.3
# time on air of beacon in test network
time_on_air_s = 74e-3
# Battery capacity in joule (2 x alkaline AA battery with 3.9 Wh each)
battery_capacity_j = 2 * 3.9 * 3600
# idle_time_power_consumption_w = 10e-6


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("-o", "--output", help="render graph to this file")
    parser.add_argument("event_file_path", help="read events from this csv file")
    parser.add_argument("--start", help="ignore data before START minutes")
    parser.add_argument("--stop", help="ignore data after STOP minutes")
    parser.add_argument(
        "--until_stable",
        action="store_true",
        help="stop when all nodes joined the network",
    )
    parser.add_argument("--node_order", help="order in which to plot the nodes")
    parser.add_argument(
        "-s", "--print_stats", action="store_true", help="print statistics"
    )
    parser.add_argument("--no_plot", action="store_true", help="don't create plot")
    parser.add_argument("--no_draw_connect_arrows", action="store_true")
    parser.add_argument("--dpi")
    args = parser.parse_args()

    start: UptimeMs | None = args.start
    stop: UptimeMs | None = args.stop

    if args.start is not None:
        start = float(args.start) * 60_000
    if args.stop is not None:
        stop = float(args.stop) * 60_000

    if start is not None and stop is not None:
        is_zoomed_in = (stop - start) / (60 * 1000) < 2
    else:
        is_zoomed_in = False

    if args.dpi is not None:
        plt.figure(dpi=int(args.dpi))

    plt.ylabel("node ID")

    axis = plt.gca()

    axis.spines["right"].set_visible(False)
    axis.spines["top"].set_visible(False)
    axis.spines["left"].set_visible(False)
    axis.spines["bottom"].set_visible(False)
    for spine in axis.spines.values():
        spine.set_position(("outward", 10))
    axis.margins(x=0, y=0)
    axis.tick_params(left=False)

    spacing = 2.5
    width = 1
    next_plot_offset: PlotOffset = -spacing

    node_order: None | [NodeId] = (
        literal_eval(args.node_order) if args.node_order is not None else None
    )

    nodes: dict[NodeId, Node] = {}

    for node_id, uptime_ms, kind, content in parse_events(args.event_file_path):
        if kind == EventKind.RESET:
            if not nodes.get(node_id):
                node = Node(node_id, content["is_sink"], None, None, set())
                node.connected = False
                node.receive_ms = 0
                node.transmit_msgs = 0
                node.plot_offset = None
                if node_order is not None:
                    node.plot_offset = (
                        (len(node_order) - node_order.index(node_id) - 1)
                        * (width + spacing)
                        - spacing
                        if node_id in node_order
                        else None
                    )
                else:
                    node.plot_offset = next_plot_offset
                    next_plot_offset += width + spacing
                nodes[node.id] = node
            continue
        elif kind == EventKind.STATE:
            nodes[node_id].state = content
            if nodes[node_id].state == "Idle":
                nodes[node_id].connected = True
            if (
                args.until_stable
                and uptime_ms > 60_000
                and all(map(lambda n: n.connected, nodes.values()))
            ):
                break
            continue

        if start is not None and uptime_ms < start:
            continue
        if stop is not None and uptime_ms > stop:
            break

        if kind == EventKind.ACTION:
            node = nodes[node_id]
            if node.action is not None:
                # A node stops receiving or transmitting when current action is
                # receive/transmit and another action occurs. This is not super
                # accurate as processing time of firmware (most notably sending log
                # messages over UART) is also counted as receive or transmit time.
                match node.action.kind:
                    case ActionKind.RECEIVE:
                        receive_duration_ms = uptime_ms - node.action.start
                        assert receive_duration_ms > 0
                        node.receive_ms += receive_duration_ms
                        if node.plot_offset is not None:
                            offset = node.plot_offset
                            # is_data = node.action.state == "SendData"
                            # is_ack = node.state == ""
                            if is_zoomed_in or uptime_ms - node.action.start > 1_000:
                                plt.fill_betweenx(
                                    y=[offset + width * 0.1, offset + width * 0.9],
                                    x1=node.action.start,
                                    x2=uptime_ms,
                                    color="C0",
                                    linewidth=0.4,
                                )
                            elif node.action.state.endswith("Ack"):
                                # ignore ack messages
                                pass
                            elif node.action.state in [
                                "ListenForData",
                                "ListenForConnect",
                                "ListenForBestBeacon",
                            ]:
                                plt.plot(
                                    uptime_ms,
                                    offset,
                                    color="C0",
                                    marker="2",
                                    linewidth=0.75,
                                )
                            else:
                                print("unhandled receive action:", node.action.state)
                    case ActionKind.TRANSMIT:
                        node.transmit_msgs += 1
                        if node.plot_offset is not None:
                            offset = node.plot_offset
                            if is_zoomed_in:
                                plt.fill_betweenx(
                                    y=[offset + width * 0.1, offset + width * 0.9],
                                    x1=node.action.start + node.action.delay_ms,
                                    x2=uptime_ms,
                                    color=(
                                        "C1"
                                        if node.action.state == "SendBeacon"
                                        else "C2"
                                    ),
                                    zorder=(
                                        2 if node.action.state == "SendBeacon" else 1
                                    ),
                                    linewidth=0.4,
                                )
                            elif node.action.state.endswith("Ack"):
                                # ignore ack messages
                                pass
                            elif node.action.state == "SendConnect":
                                plt.plot(
                                    uptime_ms,
                                    offset + width / 2,
                                    color="C3",
                                    marker="3",
                                    linewidth=0.75,
                                )
                            elif node.action.state == "SendData":
                                plt.plot(
                                    uptime_ms,
                                    offset + width / 2,
                                    color="C2",
                                    marker="4",
                                    linewidth=0.75,
                                )
                            elif node.action.state == "SendBeacon":
                                plt.plot(
                                    uptime_ms,
                                    offset + width,
                                    color="C1",
                                    marker="1",
                                    linewidth=0.75,
                                )
                            else:
                                print("unhandled transmit action:", node.action.state)
            node.action = Action(content, uptime_ms, node.state)
            continue
        elif kind == EventKind.NEW_CHILD:
            parent = nodes[node_id]
            child = nodes[int(content)]
            if (
                not args.no_draw_connect_arrows
                and parent.plot_offset is not None
                and child.plot_offset is not None
            ):
                arrow_offset = 0.2
                if child.plot_offset < parent.plot_offset:
                    child_y = child.plot_offset + width + arrow_offset
                    parent_y = parent.plot_offset - arrow_offset
                else:
                    child_y = child.plot_offset - arrow_offset
                    parent_y = parent.plot_offset + width + arrow_offset
                axis.add_patch(
                    patches.ConnectionPatch(
                        (uptime_ms, child_y),
                        (uptime_ms, parent_y),
                        coordsA=axis.transData,
                        coordsB=axis.transData,
                        arrowstyle="-|>",
                        color="C3",
                        zorder=2,
                    )
                )
            continue

    if start is None:
        start = 0
    if stop is None:
        stop = uptime_ms

    if args.print_stats:
        duration_s = (stop - start) / 1000

        print("total time: ", end="")
        if duration_s / 60 / 60 < 10:
            print(f"{duration_s / 60:.2f} minutes")
        elif duration_s / 60 / 60 / 24 < 4:
            print(f"{duration_s / 60 / 60:.2f} hours")
        else:
            print(f"{duration_s / 60 / 60 / 24:.2f} days")

        print(
            "id       | time in rx / ms | msgs transmitted | energy used / J | 2*AA battery lifetime / years"
        )
        print(
            "---------+-----------------+------------------+-----------------+------------------------------"
        )

        for node in (
            [nodes[node_id] for node_id in node_order]
            if node_order is not None
            else nodes.values()
        ):
            energy_consumption_j = (
                node.receive_ms / 1000 * rx_power_w
                + node.transmit_msgs * time_on_air_s * tx_power_w
            )

            lifetime_years = (
                battery_capacity_j / energy_consumption_j * duration_s / 3600 / 24 / 365
            )

            print(
                f"{node.id:08x} | {node.receive_ms:15} | {node.transmit_msgs:16} | {energy_consumption_j:15.2f} | {lifetime_years:29.2f}"
            )

        print()
        print("id       | duty cycle / %")
        print("---------+-----------------")

        for node in (
            [nodes[node_id] for node_id in node_order]
            if node_order is not None
            else nodes.values()
        ):
            time_transmitting = node.transmit_msgs * time_on_air_s
            print(f"{node.id:08x} | {time_transmitting / duration_s * 100:.2f}")

    for label in axis.get_yticklabels():
        label.set_fontname("monospace")

    filtered_nodes = list(filter(lambda n: n.plot_offset is not None, nodes.values()))
    axis.yaxis.set_major_locator(
        ticker.FixedLocator(
            list(map(lambda n: n.plot_offset + width / 2, filtered_nodes))
        )
    )
    axis.set_yticklabels(map(lambda n: f"{n.id & 0xFF:x}", filtered_nodes))

    if start is not None:
        axis.set_xlim(xmin=start)
    else:
        axis.set_xlim(xmin=0)

    if stop is not None:
        axis.set_xlim(xmax=stop)

    # increase distance to legend and x axis
    axis.set_ylim(ymin=axis.get_ylim()[0] - 0.3, ymax=axis.get_ylim()[1] + 1.5)

    if is_zoomed_in:
        plt.xlabel("time / ms")
        axis.xaxis.set_major_locator(ticker.MultipleLocator(100))
        axis.xaxis.set_major_formatter(
            ticker.FuncFormatter(lambda x, _pos: f"{int(x)}")
        )
    else:
        plt.xlabel("time / min")
        axis.xaxis.set_major_locator(ticker.MultipleLocator(5 * 60 * 1000))
        axis.xaxis.set_major_formatter(
            ticker.FuncFormatter(lambda x, _pos: f"{int(x / 60_000)}")
        )

    # hack for legend:
    if is_zoomed_in:
        plt.fill_betweenx(
            y=[0, 0],
            x1=0,
            x2=0,
            color="C0",
            label="receive",
        )
        plt.fill_betweenx(
            y=[0, 0],
            x1=0,
            x2=0,
            color="C1",
            label="transmit beacon",
        )
        plt.fill_betweenx(
            y=[0, 0],
            x1=0,
            x2=0,
            color="C2",
            label="transmit",
        )
    else:
        plt.fill_betweenx(
            y=[0, 0],
            x1=0,
            x2=0,
            color="C0",
            label="listen for beacon",
        )
        plt.scatter(-10, -10, color="C0", marker="2", label="receive", linewidth=0.75)
        plt.scatter(
            -10, -10, color="C2", marker="4", label="transmit data", linewidth=0.75
        )
        plt.scatter(
            -10, -10, color="C3", marker="3", label="transmit connect", linewidth=0.75
        )
        plt.scatter(
            -10, -10, color="C1", marker="1", label="transmit beacon", linewidth=0.75
        )
    axis.legend(frameon=False, ncols=3, loc="lower center", bbox_to_anchor=(0.5, 1))

    if (out := args.output) is not None:
        plt.gcf().set_size_inches(8, 0.8 + 0.35 * len(filtered_nodes))
        plt.tight_layout(pad=0)
        plt.savefig(out)
        print(f'plot written to "{out}"')
    elif not args.no_plot:
        plt.show()
