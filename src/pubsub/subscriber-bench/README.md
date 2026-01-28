# Background

A benchmark to measure throughput of a subscriber.

The subscription is hardcoded. All of the parameters are hardcoded.

Note that there is no feedback on whether acks are successful. This is really a
measure of how many acks were accepted by the client library. I sort of need to
check pantheon to see real metrics. :thinking-face:

# Run

```sh
TS=$(date +%s); RUSTFLAGS="-C target-cpu=native" \
  cargo run --release -p subscriber-bench \
  >bm-${TS}.txt 2>bm-${TS}.log </dev/null &
```
