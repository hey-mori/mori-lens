# The readiness probe over one session, 2026-09-30

The Status section asks the provider whether it is ready. Until this change it
asked four times.

`get_provider_readiness` ran three lookups against one endpoint: a reachability
check, an inventory fetch, and a lifecycle check. For Ollama the first two are the
same request, `GET /api/tags`, and the reachability one threw the body away
(`lib.rs:110`, `let (_status, _body) = ...`). The lifecycle check then called
`model_state`, which fetches `/api/tags` a third time before asking `/api/ps` what
is loaded. For LM Studio all three resolve to `GET /api/v1/models`, so it asked the
identical question three times.

Each lookup opened its own TCP connection, on its own spawned thread.

## The machine, the build, the date

| | |
| --- | --- |
| Machine | 12th Gen Intel Core i7-12650H, 10 physical / 16 logical cores, 32 GB RAM |
| OS | Windows 11 Home 10.0.26200, build 26200 |
| Toolchain | rustc 1.89.0 (29483883e 2025-08-04), `dev` profile |
| Base commit | `4fb79aa` |
| Provider | Ollama 0.30.7 on `127.0.0.1:11434` |
| Installed models | `qwen2.5:7b` (4.68 GB), `translategemma:latest` (3.30 GB) |
| Date | 2026-09-30 |

## What a connection costs, measured before deciding anything

`GET /api/tags` returning a 944-byte body, 40 iterations:

| | median | mean |
| --- | --- | --- |
| TCP handshake alone, connect and close | 0.398 ms | 1.630 ms |
| Full request, a fresh connection each time | 18.972 ms | 32.701 ms |
| Full request, reusing one connection | 9.143 ms | 9.722 ms |

**The handshake is 4% of the difference.** This matters because the `Session` type
being wired in here arrived in #648 with a doc comment claiming each fresh
connection "wastes ~0.1ms per handshake" and that reuse is "a 3-5x improvement".
The handshake is 0.4 ms, not 0.1 ms, and it is not where the time goes. Reuse
saves 9.8 ms per request, which is 25x what a handshake count predicts, because
`Connection: close` also makes the server end each body by closing the socket and
makes every request pay connection teardown.

So the number in that comment was wrong in both directions at once: too small
about the handshake, too vague about the total, and derived from no measurement.
It has been replaced with the table above.

## The whole path, old against new

`the_readiness_probe_agrees_with_the_lookups_it_replaced` in
`crates/lens-translate/src/live_tests.rs`, against the Ollama above. Both paths run
alternately inside one loop, 20 iterations, so a warm cache or a busy moment
cannot favour whichever ran first. Medians:

| | median over 20 runs |
| --- | --- |
| Three lookups, fresh connection each | 54.253 ms |
| One probe, one connection | 21.384 ms |
| Saved | 32.869 ms |

2.5x, and 32.9 ms off every readiness refresh. Two effects, composing: four
requests become two, and the second of those two costs 9 ms instead of 19 ms.

Run it with:

```bash
cargo test -p lens-translate the_readiness_probe_agrees -- --ignored --nocapture
```

## The test asserts agreement, not just speed

A faster path that answers differently is not an optimisation. The live test
compares the probe against the lookups it replaces, on the same endpoint, in the
same run: the inventory must match `list_models_for_provider` element for element,
and `probe.is_loaded(model)` must match whether `ProviderRuntime::model_state`
reports `ModelState::Loaded`. It asserted both on this machine, for
`qwen2.5:7b`, before it reported any timing.

The speed assertion is deliberately weak: `new < old`, not `new < old / 2`. A
threshold tuned to this laptop would fail on a slower one for no reason anybody
could act on.

## What the fake-server tests cannot show, and what covers it

`cargo test` proves the parsing and the connection count against
`FakeServer::start_keep_alive`, which had to be written for this: the existing
`FakeServer` closes after every response, so a client that reuses a connection
and one that does not look identical to it. It counts accepted connections now,
and the unit tests assert one connection for two requests.

None of that shows a real Ollama behaving the same way over a reused socket. Only
the live test above does, which is why the claim in this file rests on it.

## What is not measured here

**LM Studio.** Nothing is listening on `127.0.0.1:1234` on this machine, so the
LM Studio path has fake-server tests and no live run. Its saving should be larger
in proportion, since three identical requests become one, but that is a prediction
and it is not recorded as a result.

**The readiness cache.** #648 added a 3-second TTL cache in front of this path, so
a user refreshing inside that window pays nothing either way. The 32.9 ms is what
a cache miss costs, which is every settings change and every cold open of the
Status section.
