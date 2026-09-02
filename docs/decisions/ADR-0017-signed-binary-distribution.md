# ADR-0017 · Signed binary distribution

**Status** · Accepted, 2026-09-03 (owner decision 2026-08-13)

## Context

`docs/mori-lens-project-goal.md` says a signed binary is the whole of the
remaining list before this is something a person installs. It is the only
item on that list and it has never had an issue of its own.

What exists: `scripts/package.sh` runs the gate, builds, and writes a zip
named for the version and the commit with a SHA-256 beside it.
`install.ps1` travels in the zip, is per-user, needs no elevation, and
`uninstall.ps1` reverses it. What a reader gets today is SmartScreen
saying the publisher is unknown, and the goal doc says it is right to.

A certificate is a purchase and a decision. Since 2023 the key must live
on hardware or in a cloud signing service, and a `.pfx` in CI is no
longer possible.

## Decision

**Signing is decided. The question is no longer whether.**

- no public Windows release stays unsigned
- the primary v1 public channel is a Microsoft Store MSIX, which the
  Store signs, so the first release needs no purchased certificate
- a website or GitHub direct download waits for an OV code-signing
  certificate
- EV is not bought for SmartScreen alone
- Azure Artifact Signing Public Trust is not assumed available to a
  South Korean publisher
- the OV vendor is chosen after the legal publishing identity is fixed
- a controlled local research build is not a public release

### What this means for the release pipeline

Two distribution channels, each with its own signing path:

| Channel | Signs with | Needs |
|---|---|---|
| Microsoft Store | the Store's own signature | Store developer account, MSIX package |
| Direct download | an OV code-signing certificate | legal publishing identity, hardware key or cloud HSM |

The Store path is first because it does not require a certificate
purchase. The Store signs the MSIX on behalf of the publisher, so the
reader sees a known publisher and no SmartScreen warning.

### What remains before the first signed release

- Store developer account registration
- MSIX packaging structure (AppxManifest, assets, build step)
- Legal publishing identity (individual or entity)
- For direct download: OV vendor comparison and purchase

None of these are code changes. They are account registrations and
purchases. What this repository can prepare is the MSIX packaging
structure and a verification command.

### What is not implemented yet

This ADR records the decision. The MSIX packaging and Store submission
require the developer account, which is an external dependency. Until
that account exists:

- the existing `scripts/package.sh` continues to produce unsigned zip
  releases with SHA-256 checksums
- SmartScreen warns about unknown publisher, and the warning is honest
- `scripts/verify-signature.ps1` exists to check whether a binary is
  signed, for use when a signed build becomes available

## Consequences

- The first public release goes through the Store, which signs it. No
  certificate purchase is needed for that path.
- A direct download channel waits for an OV certificate and a legal
  identity. Both are external decisions.
- The existing zip installer stays available for development and testing,
  unsigned, with a checksum and a README explaining the warning.
- SmartScreen's "unknown publisher" warning is proportionate and this
  product does not argue with it until there is a signature to show.
