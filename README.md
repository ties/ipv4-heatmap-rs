# IPv4 heatmap

rust variation (with the goal of creating python bindings more easily) of
[ipv4-heatmap](https://github.com/measurement-factory/ipv4-heatmap) by Duane
Wessels.


## How to run

```
curl https://www.ris.ripe.net/dumps/riswhoisdump.IPv4.gz | gunzip - | awk '{print $2 " " $3 }' | grep -E '[0-9]+\.[0-9]+\..*' | cargo run -- --curve logarithmic --accumulate
```
