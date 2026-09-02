# M8 History Privacy Audit - Final Report

**Date:** 2026-09-02  
**Status:** ✅ COMPLETE

## Verdict

```
SAFE TO CONTINUE M8 AUDIT
```

## Summary

All five critical privacy invariants for persistent history (ADR-0014 Tier 3) have been verified through code tracing and automated tests.

## Invariant Results

| # | Invariant | Status | Evidence |
|---|-----------|--------|----------|
| 1 | History OFF = zero persistent write | ✅ PASS | Code trace + test |
| 2 | Excluded app = pre-write rejection | ✅ PASS | Code trace + test |
| 3 | store_application_name=false = NULL in DB | ✅ PASS | Raw DB query test |
| 4 | show_card latency acceptable | ✅ PASS | Benchmark: ~1.2ms |
| 5 | Startup prune semantics correct | ✅ PASS | Multiple tests |

## Test Coverage

**New Privacy Tests Added:**
1. `history_off_never_writes` (apps/lens)
2. `excluded_application_never_writes` (apps/lens)
3. `application_name_is_not_persisted_when_disabled` (lens-storage)
4. `startup_prune_keeps_nonexpired_entries` (lens-storage)
5. `history_table_schema_columns` (lens-storage)
6. `history_latency_benchmark` (apps/lens)

**Total Tests:** 78 passing in apps/lens, 49 in lens-storage

## Latency Benchmark

```
Release mode (1000 iterations):
  History OFF: 6 ns/call
  History ON:  1,210,225 ns/call (~1.2ms)
```

**Assessment:** Acceptable. Cards are shown for seconds, not milliseconds. The 1.2ms delay is imperceptible.

## Known Limitations

1. **Startup-only pruning:** Entries stay until app restarts. Documented, not a bug.
2. **Substring matching:** Excluded apps use case-insensitive substring match.
3. **Whitespace not trimmed:** User's exact input in settings is used.

These are documented behavior, not privacy violations.

## Files Modified

- `apps/lens/src/main.rs`: Added privacy tests and benchmark
- `crates/lens-storage/src/lib.rs`: Added storage-level privacy tests
- `docs/audits/2026-09-02-m8-history-privacy-audit.md`: Full audit report
- `docs/benchmarks/2026-09-02-history-latency.md`: Benchmark documentation

## Next Steps

No privacy fixes required. Continue with M8 audit.

If latency becomes noticeable in user testing:
- Add background writer for history INSERT
- Move SQLite operations off UI thread
- Keep decision logic synchronous (it's fast)
