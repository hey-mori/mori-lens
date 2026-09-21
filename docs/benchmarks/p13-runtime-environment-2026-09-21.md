# P13 Runtime Environment Provenance

**Date captured:** 2026-09-21
**Associated canonical artifacts:**
- `p13-quick-baseline-2026-09-21-054306.json`
- `p13-quick-baseline-2026-09-21-054306.md`
- `p13-deep-baseline-2026-09-21-054306.json`
- `p13-deep-baseline-2026-09-21-054306.md`

**PR:** #612
**Source HEAD:** `42100d989330c26db429c84c6e2c28221e277bb7`

---

## Provenance Statement

> **Environment captured after the benchmark run on the same host.**
> These fields were not embedded in `BenchmarkMetadata` at inference time.
> The canonical JSON artifacts record only: model, provider, endpoint, timestamp, runs_per_entry, warm_start, corpus_revision.
> This document supplements that metadata with host-level provenance captured post-hoc.

## Host Environment

| Field | Value |
|-------|-------|
| **OS** | Microsoft Windows 11 Home |
| **OS Build** | 10.0.26200 |
| **System** | ASUS Vivobook K3605ZV |
| **CPU** | 12th Gen Intel(R) Core(TM) i7-12650H |
| **CPU Cores** | 10 physical, 16 logical |
| **Total RAM** | 32 GB (31.6 GiB) |
| **Architecture** | x64 |

## Inference Runtime

| Field | Value |
|-------|-------|
| **Provider** | Ollama |
| **Ollama version** | 0.30.7 |
| **Endpoint** | 127.0.0.1:11434 |
| **Model** | qwen2.5:7b |
| **Model ID** | 845dbda0ea48 |
| **Model size** | 4.7 GB |
| **Corpus revision** | canary-25-v1 |
| **Runs per entry** | 1 |
| **Warm start** | true |

## LM Studio

| Field | Value |
|-------|-------|
| **Status** | NOT RUN |
| **Notes** | Canonical baseline executed against Ollama only. LM Studio measurement is a future comparison point. |

## Notes

- The canonical benchmark artifacts were generated on 2026-09-21 between 05:39 and 05:40 UTC.
- Environment values were captured on the same host on 2026-09-21 but not synchronously during inference.
- The `BenchmarkMetadata` struct does not currently record OS/CPU/RAM/Ollama version. A future schema revision may add structured runtime metadata.
- No model digest/SHA is exposed by the installed Ollama version beyond the model ID shown in `ollama list`.
