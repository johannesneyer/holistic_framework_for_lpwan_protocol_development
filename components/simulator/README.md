# Simulator

run for 120 minutes of simulated time with a bunch of RNG seeds

``` fish
for seed in (seq 0 256); cargo r -- --seed $seed --time_min 120 || break; end; echo $seed
```
