# Where the seconds go, 2026-08-07

A reader's screenshots showed card latencies of **16.6s, 11.2s, 7.7s,
7.3s, 4.5s and 2.9s**, against the warm median of 2,644ms recorded in
[`2026-08-06-model-at-one-size.md`](2026-08-06-model-at-one-size.md).

Two candidates were measured. One was real and is fixed. The other was a
plausible idea that the numbers do not support, recorded here so nobody
spends a second afternoon on it.

## Real: the model lease was taken once and never renewed

`prewarm` sent `keep_alive: "30m"` and nothing else sent it at all, so
the lease Lens ran under was the runtime's default, re-applied by each
hover. On this machine that default happens to be thirty minutes, so it
held by luck; Ollama documents five, and `OLLAMA_KEEP_ALIVE` overrides
it.

The cost when it does not hold, measured by releasing the model and
translating one sentence through the product's own request body:

```
[23350ms · qwen2.5:7b · local]
```

Against the reader's own log: `engine warm after 23548ms`. Same cost,
same cause. Fixed by sending the lease on every request that loads the
model.

## Not real: a stable fence for prompt-prefix reuse

The fence marker is derived per request, so **no two requests share a
system prompt**, and a runtime that reuses a KV cache across requests
cannot reuse anything. Making the marker per **session** rather than per
request would still be unguessable to the threat it exists for, which is
a page the reader hovers over, and would put the whole prompt in a
shared prefix.

Five sentences each way, same model, same options:

| | median prefill | totals |
|---|---:|---|
| a changing fence | 412ms | 2,510 to 26,788ms |
| one fixed fence | 372ms | 1,894 to 17,425ms |

Two of the five fixed-fence runs prefilled in **35.8ms and 69.5ms**,
which is a cache hit and exactly the mechanism the idea predicts. The
other three prefilled at 372 to 405ms, no better than the changing
fence.

**The idea is not wrong, and it is not worth doing.** Prompt evaluation
is 239 tokens and about 400ms. The totals it sits inside range over an
order of magnitude, and they do so under both conditions, so whatever is
producing 26 seconds is not the prefill and would not be improved by
removing it. Changing a per-request nonce to a per-session one is a
change to the injection defence, and a security-relevant design should
not move for something the measurement cannot even see.

Recorded as a negative rather than left as a good-sounding idea for the
next person.

## What the range actually is

The totals above are from a machine running several other model-backed
sessions. That is also the machine the reader's screenshots came from,
which is why their cards took 16 seconds and the benchmark's took 2.6.

Nothing in this product can fix contention for one GPU. What it already
does about it:

- 번역 중 after 180ms, so the wait is visibly a wait
- 모델을 불러오는 중 when the model is not resident, which the lease fix
  makes reachable again
- a 20 second ceiling, after which the card says the engine did not
  answer rather than showing nothing

What it does not do is claim a number it cannot hold. The 2,644ms figure
is a warm median on a quiet machine and is labelled as one.
