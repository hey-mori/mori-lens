# Benchmarks

**Status · empty. Nothing has been benchmarked yet.**

Numbers here are measured on a named machine, with a named build, at a
named date. A number without those three is not a benchmark, it is a
recollection.

## What gets measured

**Acquisition** (from M1), per application:

- element resolution latency, P50 and P95
- text and bounds extraction latency, P50 and P95
- hover-to-target-ready, end to end
- stale rate: results discarded because the pointer moved first
- idle CPU with the probe running and the pointer parked

**Translation** (from M3):

- cold and warm first-token latency, per model
- total latency for the quick path
- cache hit rate over a realistic reading session
- deep analysis latency, separately, to prove it does not touch the
  quick path

## Format

One file per run: `YYYY-MM-DD-<subject>.md`, holding the environment
block, the raw numbers, and a one-paragraph reading of what changed.

```
machine       <cpu> · <ram> · <gpu>
os            Windows 11 26200
build         <git sha>
scaling       150%
ollama        <version>, models: <list>
```

## Rules

- Never compare across machines. A regression is a change on the *same*
  machine between two builds.
- Record the failure and outlier counts, not only the percentiles. A P50
  computed over the runs that succeeded is a lie if a third of them
  failed.
- A performance claim in a pull request cites a file here or it is not a
  performance claim.
- The targets in
  [`../research/hover-text-acquisition.md`](../research/hover-text-acquisition.md)
  §8 are design goals. They are never quoted as results.
