# M8 History Latency Benchmark

Measures the overhead of history recording on the show_card path.

## Method

Simulates the show_card history recording path with history enabled and disabled.
Measures the time to execute the decision logic and (when enabled) the SQLite INSERT.

## Results

Run with: `cargo test -p lens --release history_latency_benchmark -- --nocapture`

(Results will be printed below when run)
