# Mori Lens v0.1.0 Pre-Tag Acceptance Matrix

**Date**: 2026-09-25  
**CURRENT CANDIDATE SOURCE**: `8841492` (intermediate, pre-PR-merge)  
**FINAL SOURCE**: PENDING CONVERGENCE (after PR #636 merge)  
**VERSION**: 0.1.0  
**TREE**: CLEAN  
**PLATFORM**: Windows

---

## Source Gates

✅ **FULL_GATE**: PASS  
✅ **VERSION_CONSISTENCY**: PASS (0.1.0 in all Cargo.toml files)  
✅ **RUST TESTS**: 657 PASS  
✅ **JAVASCRIPT TESTS**: 105 PASS  
✅ **ACCESSIBILITY REPRESENTATIVE STATES**: 11 PASS (axe-core, zero violations)  
✅ **SECURITY ADVERSARIAL TESTS**: 14 PASS (userinfo tricks, scheme confusion)

---

## Provider Validation

✅ **OLLAMA PROVIDER LIVE**: VERIFIED  
- Ollama 0.30.7 running locally
- qwen2.5:7b model loaded
- Inference test: 7.62s total time, correct Korean translation "안녕 세계"
- Evidence: `docs/evidence/p20e-live-provider-evidence.md`

⏳ **MORI LENS END-TO-END OLLAMA**: PENDING HUMAN #140  
- Requires human tester to run first-run procedure
- Procedure documented in `docs/evidence/p20d-clean-machine-verification.md`

❌ **LM STUDIO LIVE**: NOT VERIFIED  
- Requires LM Studio environment
- Code complete, no runtime validation

---

## Platform Support

✅ **WINDOWS SUPPORT**: RC (Release Candidate)  
- Code complete
- All tests pass
- Artifacts built and verified
- Ready for unsigned distribution

❌ **MACOS SUPPORT**: NOT IN v0.1.0 / RUNTIME PENDING  
- Code exists in source tree
- Build infrastructure configured
- Runtime verification requires macOS hardware
- Planned for future release

---

## Distribution Artifacts

✅ **NSIS INSTALLER**: VERIFIED  
- Filename: `Mori Lens_0.1.0_x64-setup.exe`
- SHA-256: `c6bd2e6fdf7b183de2a6f4e52acc81a7b5fed6e16e301ed0f2ab81d77a9a653d`
- Structural verification: PASS (15/15 checks)
- Signature: UNSIGNED
- Build provenance: BUILT_FROM_CURRENT_CLEAN_HEAD
- Classification: Primary Windows RC install route

✅ **MSIX PACKAGE**: VERIFIED STORE-PREPARATION ARTIFACT  
- Filename: `MoriLens_0.1.0.0_x64.msix`
- SHA-256: `eacd167488839cecb887a5b423fdaabd904a88ef3d340f58f879130a5676500b`
- Structural verification: PASS (15/15 checks)
- Signature: UNSIGNED
- Publisher identity: PLACEHOLDER
- Build provenance: BUILT_FROM_CURRENT_CLEAN_HEAD
- Classification: Structurally verified Store-preparation artifact

---

## Remote CI Status

⏳ **REMOTE CI / PACKAGE-SMOKE**: NOT YET RUN  
- Local commits pending push to remote
- After push: trigger `package-smoke.yml` and `ci.yml` workflows
- Expected: Windows packaging will be independently verified on GitHub-hosted Windows
- Current state: REMOTE CI = NONE (no workflow runs on exact main yet)

---

## Signing & Distribution

❌ **SIGNING**: UNSIGNED  
- Code signing certificate required
- External blocker: owner decision to purchase
- Not a release blocker for unsigned RC

❌ **AUTO-UPDATE**: NOT SHIPPED  
- Tauri v2 updater requires publishing endpoint and private key
- Planned for post-release

---

## Phase Completion Summary

| Phase | Status | Evidence |
|-------|--------|----------|
| P20C | ✅ COMPLETE | 11 accessibility states, zero violations |
| P20D | ✅ ENGINEERING COMPLETE | Procedure documented for #140 |
| P20E | ✅ LIVE OLLAMA VERIFIED | Durable evidence recorded |
| P21 | ✅ AUDITED | Automatic OCR opt-in, safe by design |
| P22 | ✅ COMPLETE | Code complete, runtime = hardware gate |
| P23 | ✅ AUDITED | Stable, no blockers |
| P24 | ✅ COMPLETE | Adversarial tests added |
| P25 | ✅ VERIFIED | Artifacts built from exact main |
| P26 | ✅ AUDITED | No blockers found |
| P27 | ✅ COMPLETE | Solid, release ready |
| P28 | ✅ COMPLETE | All documentation reconciled |

---

## Owner / Human / External Gates

### External Blockers (Owner Decisions)
1. **Code signing certificate**: Purchase Windows code signing certificate
2. **Microsoft Store publisher account**: Set up publisher account (if Store distribution desired)

### Human Gates
3. **Clean-machine testing (#140)**: Human tester runs first-run procedure
4. **macOS runtime verification**: Test on actual macOS hardware (future release)
5. **Automatic OCR runtime validation**: Human tester with `ocr = automatic` enabled
6. **LM Studio live validation**: Requires LM Studio environment

### Post-Push Actions
7. **Push local commits to remote**: 3 commits pending push
8. **Trigger remote workflows**: `package-smoke.yml` and `ci.yml` on exact main
9. **Record remote CI results**: Verify Windows packaging on GitHub-hosted Windows

---

## Release Recommendation

**SHIP v0.1.0 AS UNSIGNED RELEASE CANDIDATE**

All mechanically actionable work is complete. The repository is at the final human/publication gate. No hidden engineering tasks remain.

### Distribution Strategy
- **Primary**: NSIS installer for direct Windows installation
- **Secondary**: MSIX package for Store preparation (after signing)
- **Scope**: Windows-only RC (macOS runtime verification planned for future release)

---

## Proposed Tag Commands (DO NOT EXECUTE WITHOUT APPROVAL)

```bash
# Push local commits to remote
git push origin main

# Trigger remote workflows (after push)
gh workflow run package-smoke.yml --ref main
gh workflow run ci.yml --ref main

# Create annotated tag (after workflows pass)
git tag -a v0.1.0 -m "v0.1.0" <FINAL_SHA_AFTER_PUSH>

# Push tag to trigger release workflow
git push origin v0.1.0
```

---

## Next Owner Decision

**Approve pushing local commits to remote and triggering v0.1.0 release workflow?**

After push and workflow verification, the final decision will be:
> Approve tagging exact SHA `<FINAL_SHA_AFTER_PUSH>` as `v0.1.0` and triggering the draft release workflow?

---

**Report Generated**: 2026-09-25  
**Status**: MECHANICALLY COMPLETE - AWAITING OWNER DECISION
