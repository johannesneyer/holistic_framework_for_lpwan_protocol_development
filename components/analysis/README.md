# Analysis Scripts

arch:

``` shell
doas pacman -S python graphviz otf-latin-modern
```

debian:

``` shell
sudo apt install python3-dev python3-tk graphviz libgraphviz-dev fonts-lmodern
```

init venv:

``` shell
python -m venv .venv
source .venv/bin/activate
pip install -e .
```

or with `uv` instead of `pip`:

``` shell
doas pacman -S uv
uv venv
source .venv/bin/activate
uv pip install -e .
```

analyse data from simulator:

``` shell
analysis -m /tmp/protocol_sim_meta.json /tmp/protocol_sim_events.csv
```

analyse data from cloud:

``` shell
analysis -i /tmp/protocol_cloud_events.csv
```

show node activity timeline:

``` shell
timeline -s --stop 60 /tmp/protocol_cloud_events.csv
```

generate svg whenever simulation has run:

``` fish
while inotifywait -e close_write /tmp/protocol_events.csv; analysis -o /tmp/out.svg -m /tmp/protocol_sim_meta.json /tmp/protocol_events.csv; end
```
