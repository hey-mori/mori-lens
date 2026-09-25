# P20E: Live Provider Evidence

**Date**: 2026-09-25  
**Status**: LIVE OLLAMA PROVIDER VERIFIED

## Summary

Direct Ollama provider verification with durable evidence. Mori Lens end-to-end verification requires human operation (tracked in #140).

## Test Environment

| Field | Value |
|-------|-------|
| **Date/Time (UTC)** | 2026-09-25T02:10:05Z |
| **OS** | Microsoft Windows 11 Home 10.0.26200 |
| **Ollama version** | 0.30.7 |
| **Endpoint** | 127.0.0.1:11434 |
| **Model** | qwen2.5:7b (845dbda0ea48, 4.7 GB) |

## Inference Test

**Prompt**: "Translate to Korean: Hello world"  
**Response**: "안녕 세계" (correct)  
**Total time**: 7.62s (warm start)  
**Eval duration**: 0.10s for 4 tokens

## Verification Status

✅ **LIVE OLLAMA PROVIDER VERIFIED**  
⏳ **MORI LENS END-TO-END OLLAMA VERIFICATION = PENDING** (requires human, tracked in #140)  
❌ **LIVE LM STUDIO VERIFIED = NO** (LM Studio not running)

## Related

- Issue #140: First-run experience verification
- P13 baseline: `docs/benchmarks/p13-quick-baseline-2026-09-21-054306.md`
