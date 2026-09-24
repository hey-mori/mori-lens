# P20D: Clean-Machine First-Run Verification

**Status**: ENGINEERING PREPARATION = COMPLETE, HUMAN EVIDENCE = PENDING #140  
**Phase**: Release Candidate Preparation  
**Owner**: Human Tester (requires clean Windows environment)

## Purpose

Verify that Mori Lens provides a correct, safe, and welcoming first-run experience on a machine that has never had Mori Lens or local inference engines installed.

This is the **only remaining first-run verification gate**. All engineering preparation is complete; the remaining work is human execution on physical hardware.

## Engineering Preparation Status

### ✅ COMPLETE

- **First-run lesson implementation**: `crates/lens-core/src/welcome.rs` provides language-appropriate welcome cards based on system locale
- **Lesson content**: Explains OCR fallback, privacy model, and how to trigger lens capture
- **Lesson trigger logic**: `WelcomeManager` determines first run via persistent state
- **Integration**: Welcome card displays on first activation after install
- **Test coverage**: Unit tests verify lesson content, locale detection, and first-run detection

### ⏳ PENDING: Human Evidence

- **Clean-machine install**: Install Mori Lens on a Windows machine with no prior engines
- **First-run experience**: Verify welcome card appears and is understandable
- **Lesson clarity**: Verify the lesson explains OCR fallback and privacy correctly
- **Engine installation flow**: Verify guided installation of Ollama or LM Studio
- **End-to-end first capture**: Verify user can complete first successful lens capture

**Tracking**: Issue #140 tracks this human verification

## Test Environment Requirements

### Hardware

- Windows 10 or 11 machine (x64)
- No prior Mori Lens installation
- No prior Ollama or LM Studio installation
- Internet connection for engine download
- At least 8 GB RAM (16 GB recommended for LM Studio)
- At least 10 GB free disk space

### Software

- Clean Windows install or user profile
- No local inference engines running
- No conflicting accessibility tools or screen readers (unless testing accessibility)

## Verification Procedure

### Step 1: Install Mori Lens

1. Download the NSIS installer from the release artifacts
2. Run the installer
3. Verify installer completes without errors
4. Launch Mori Lens from Start Menu or desktop shortcut

**Expected**: Mori Lens Control Center opens

### Step 2: Welcome Card

1. Observe the welcome card that appears
2. Verify it explains:
   - What Mori Lens does (accessibility-first text capture)
   - Privacy model (text never leaves the machine)
   - OCR fallback (used only when accessibility fails)
   - How to trigger capture (hover over text)
3. Verify the language matches system locale (Korean on Korean Windows, English otherwise)

**Expected**: Welcome card is clear, accurate, and appropriately localized

### Step 3: Engine Installation Guidance

1. Dismiss the welcome card
2. Navigate to Models section
3. Observe that no engines are detected
4. Follow the guided installation instructions
5. Install Ollama (recommended for most users) or LM Studio

**Expected**: Installation guidance is clear and leads to successful engine install

### Step 4: Model Download

1. After engine installation, verify engine is detected
2. Download a recommended model (e.g., `qwen2.5:7b` for Ollama)
3. Wait for download to complete
4. Verify model status changes to "ready"

**Expected**: Download completes successfully, model becomes ready

### Step 5: First Lens Capture

1. Open a target application with visible text (e.g., Notepad, browser)
2. Hover the pointer over text for ~500ms
3. Observe the lens card appears
4. Verify card shows captured text
5. Verify card is draggable and dismissable
6. Click the card to copy text to clipboard

**Expected**: Text is captured accurately, card appears, copy works

### Step 6: Privacy Verification

1. Verify no network traffic occurs during capture (use Resource Monitor or Wireshark)
2. Verify captured text does not appear in any log files
3. Verify the engine processes text locally (check Ollama/LM Studio logs)

**Expected**: All processing is local, no remote transmission

## Evidence Template

Record the following for Issue #140:

```
## P20D Clean-Machine Verification Report

**Date**: YYYY-MM-DD
**Tester**: [name or handle]
**Windows Version**: [e.g., Windows 11 24H2]
**Mori Lens Version**: [e.g., 0.1.0]
**Engine Installed**: [e.g., Ollama 0.30.7]
**Model Downloaded**: [e.g., qwen2.5:7b]

### Step 1: Install Mori Lens
- [ ] Installer ran without errors
- [ ] Application launched successfully
- Notes: [any issues or observations]

### Step 2: Welcome Card
- [ ] Welcome card appeared
- [ ] Language was correct ([language])
- [ ] Content was clear and accurate
- Notes: [any issues or observations]

### Step 3: Engine Installation Guidance
- [ ] No engines detected initially
- [ ] Installation guidance was clear
- [ ] Successfully installed [engine name]
- Notes: [any issues or observations]

### Step 4: Model Download
- [ ] Engine detected after install
- [ ] Download completed successfully
- [ ] Model became ready
- Notes: [any issues or observations]

### Step 5: First Lens Capture
- [ ] Text captured on hover
- [ ] Lens card appeared
- [ ] Card was interactive (draggable, dismissable)
- [ ] Copy to clipboard worked
- Notes: [any issues or observations]

### Step 6: Privacy Verification
- [ ] No network traffic during capture
- [ ] No text in log files
- [ ] Engine processed locally
- Notes: [any issues or observations]

### Overall Assessment
- First-run experience: [excellent / good / acceptable / poor]
- Issues encountered: [list or "none"]
- Recommendations: [optional]
```

## Acceptance Criteria

P20D is complete when:

1. ✅ Engineering preparation is complete (DONE)
2. ⏳ A human tester completes the verification procedure on a clean machine
3. ⏳ The evidence report is attached to Issue #140
4. ⏳ No critical usability issues are found
5. ⏳ Privacy model is verified (no remote transmission)

## Related

- Issue #140: First-run experience verification
- ADR-0002: Accessibility-first acquisition
- docs/privacy-model.md: Privacy invariant
- crates/lens-core/src/welcome.rs: Welcome lesson implementation
