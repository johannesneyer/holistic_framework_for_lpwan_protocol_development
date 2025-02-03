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
import json
import xml.etree.ElementTree
from ast import literal_eval

import matplotlib.pyplot as plt
import networkx as nx
import PIL

from lightning_event_log_analysis import (
    Action,
    ActionKind,
    EventKind,
    Node,
    NodeId,
    UptimeMs,
    parse_events,
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stop", help="ignore data after STOP minutes")
    parser.add_argument("-o", "--output", help="render graph to this file")
    parser.add_argument("--dpi")
    parser.add_argument("--fig_size", help="size of figure, e.g.: 8,8")
    parser.add_argument("--floor_svg", help="floor plan to use as background")
    parser.add_argument("--floor_png", help="floor plan to use as background")
    parser.add_argument(
        "-m",
        "--sim_metadata_file_path",
        help="read simulation metadata from this json file",
    )
    parser.add_argument(
        "-i",
        "--interactive",
        action="store_true",
        help="interactively step through events",
    )
    parser.add_argument(
        "-f",
        "--follow",
        action="store_true",
        help="keep reading from event file",
    )
    parser.add_argument(
        "--no_num",
        action="store_true",
        help="don't draw node numbers in graph",
    )
    parser.add_argument("event_file_path", help="read events from this csv file")
    args = parser.parse_args()

    stop: UptimeMs | None = args.stop

    if args.stop is not None:
        stop = float(args.stop) * 60_000

    metadata: tuple[int, dict[NodeId, tuple[int, int]]] | None = None
    if (meta_file_path := args.sim_metadata_file_path) is not None:
        metadata = json.load(open(meta_file_path))

    if args.interactive:
        print("press enter to go to next event")

    if args.dpi is not None:
        plt.figure(dpi=int(args.dpi))

    if args.fig_size is not None:
        fig_size = literal_eval(args.fig_size)
    elif metadata is None:
        fig_size = (3, 3)
    else:
        fig_size = (8, 8)
    plt.gcf().set_size_inches(fig_size)

    if (floor_png := args.floor_png) is not None:
        floor_img = PIL.Image.open(floor_png)
        plt.gca().set_axis_off()
        plt.tight_layout(pad=0)
        draw_img(floor_img)

    nodes: dict[NodeId, Node] = {}
    graph = nx.DiGraph()

    axis = plt.gca()
    if metadata is not None:
        axis.set_aspect("equal")

    update = False
    for node_id, uptime_ms, kind, content in parse_events(
        args.event_file_path, args.follow
    ):
        if stop is not None and uptime_ms > stop:
            break
        match kind:
            case EventKind.MESSAGE:
                if content["kind"] == "data":
                    data_source_ids = list(
                        map(lambda nd: nd["source"], content["data"])
                    )
                    if nodes[node_id].is_sink:
                        nodes[node_id].uplink_data.update(data_source_ids)
                    # update = True
            case EventKind.RESET:
                for edge in graph.edges:
                    if edge[0] == node_id:
                        graph.remove_edge(*edge)
                        break
                node = Node(node_id, content["is_sink"], None, None, set())
                nodes[node.id] = node
                print_node_event(node_id, uptime_ms, "reset")
                graph.add_node(
                    node.id,
                    style="filled",
                    label=hex(node.id)[-2:] if node.id > 0xFF else f"{node.id:x}",
                    # https://graphviz.org/doc/info/colors.html
                    color="1" if node.is_sink else "2",
                    # only relevant when rendering the graph with graphviz
                    colorscheme="accent8",
                )
                update = True
            case EventKind.NEW_CHILD:
                child_id = int(content)
                graph.add_edge(child_id, node_id)
                print(f"[{child_id:x}] -> [{node_id:x}]")
                update = True
            case EventKind.STATE:
                new_state = content
                print_node_event(
                    node_id, uptime_ms, f"{nodes[node_id].state} -> {new_state}"
                )
                nodes[node_id].state = new_state
            case EventKind.ACTION:
                action = Action(content, uptime_ms, nodes[node_id].state)
                match action.kind:
                    case ActionKind.RECEIVE:
                        print_node_event(
                            node_id,
                            uptime_ms,
                            f"receiving for {content['duration']} ms",
                        )
                    case ActionKind.TRANSMIT:
                        print_node_event(node_id, uptime_ms, "transmitting")
                    case ActionKind.WAIT:
                        print_node_event(
                            node_id, uptime_ms, f"waiting for {content['duration']} ms"
                        )
                    case ActionKind.NONE:
                        pass
                    case other:
                        raise Exception(f"unhandled action kind: {other}")
                nodes[node_id].action = action
                # update = True
            case other:
                raise Exception(f"unhandled event kind: {other}")

        if (args.interactive or args.follow) and update:
            update = False
            axis.clear()
            if floor_png is not None:
                draw_img(floor_img)
            draw_graph(graph, axis, metadata, not args.no_num)
            if (out := args.output) is not None:
                save_figure(out, fig_size, args.floor_svg)
            else:
                plt.show(block=False)
            if args.interactive:
                input()

    print("parsed all events")

    print("uplink data:")
    for node in filter(lambda n: n.is_sink, nodes.values()):
        print(f"{node.id:08x}: {', '.join(f'{id:08x}' for id in node.uplink_data)}")

    if not args.interactive:
        draw_graph(graph, axis, metadata, not args.no_num)

    if (out := args.output) is not None:
        save_figure(out, fig_size, args.floor_svg)
    else:
        plt.show()

    return 0


def save_figure(out: str, fig_size: (float, float), background_svg: None | str):
    plt.tight_layout(pad=0)
    if background_svg is not None:
        save_svg_with_background(out, background_svg)
    else:
        plt.gcf().set_size_inches(fig_size)
        plt.savefig(out)
    print(f"graph written to '{out}'")


def save_svg_with_background(out: str, background_svg: str):
    from matplotlib.backends.backend_svg import FigureCanvasSVG

    mpl_svg_dpi = FigureCanvasSVG.fixed_dpi

    xml.etree.ElementTree.register_namespace("", "http://www.w3.org/2000/svg")
    background_svg = xml.etree.ElementTree.parse(background_svg)
    floor_svg_root = background_svg.getroot()

    svg_width = float(floor_svg_root.attrib["width"])
    svg_height = float(floor_svg_root.attrib["height"])
    plt.xlim(xmin=0, xmax=svg_width)
    plt.ylim(ymin=0, ymax=svg_height)
    plt.gcf().set_size_inches(svg_width / mpl_svg_dpi, svg_height / mpl_svg_dpi)
    plt.savefig(out, transparent=True)

    et2 = xml.etree.ElementTree.parse(out)
    for e in et2.getroot()[1:]:
        if e.tag == "{http://www.w3.org/2000/svg}metadata":
            # skip metadata
            continue
        floor_svg_root.append(e)

    background_svg.write(out)


def draw_img(img):
    # floor plan png is four times the size of the svg for it to have a high resolution
    width = img.width / 4
    height = img.height / 4
    plt.xlim(xmin=0, xmax=width)
    plt.ylim(ymin=0, ymax=height)
    plt.imshow(img, extent=(0, width, 0, height))


def print_node_event(node_id: int, uptime_ms: int, event: str):
    minutes = uptime_ms // 60_000
    seconds = uptime_ms % 60_000 / 1000
    print(f"[{node_id:08x}] {minutes:4}min {seconds:6}s {event}")


def draw_graph(
    graph,
    axis,
    meta: tuple[int, dict[NodeId, tuple[int, int]]] | None,
    draw_numbers=True,
):
    if meta is not None:
        node_pos = {
            int(n["id"]): (int(n["location"]["x"]), int(n["location"]["y"]))
            for n in meta["nodes"]
        }
        if (node_range := meta.get("node_range")) is not None:
            for pos in node_pos.values():
                axis.add_patch(plt.Circle(pos, node_range, alpha=0.2, color="C0"))
                axis.add_patch(
                    plt.Circle(pos, node_range, alpha=0.1, fill=False, color="C0")
                )
    else:
        node_pos = nx.nx_agraph.graphviz_layout(graph, prog="dot")

    unknown_nodes: list[NodeId] = list(
        map(lambda n: n[0], filter(lambda n: not n[1], graph.nodes.data()))
    )
    if unknown_nodes:
        print(f"unkown nodes: {', '.join(f'{id:x}' for id in unknown_nodes)}")
    graph.remove_nodes_from(unknown_nodes)

    nx.draw(
        graph,
        labels={n[0]: n[1]["label"] for n in graph.nodes.data()},
        node_color=[f"C{n[1]['color']}" for n in graph.nodes.data()],
        with_labels=draw_numbers,
        font_family="monospace",
        edgecolors="black",
        linewidths=1,
        pos=node_pos,
    )

    plt.tight_layout(pad=0)
