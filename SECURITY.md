# Security Policy

## Reporting a vulnerability

**Do not open a public issue for something exploitable.** A public issue
is a disclosure, and it lands before there is a fix.

Where this repository offers a **private security advisory**, its
*Security* tab, *Report a vulnerability*, use that. It is private end to
end and needs no address.

Where that form is not there, open an issue saying you have found
something and asking how to send it privately, and keep the technical
details out of it. That reaches the maintainer without disclosing
anything.

Expect a response within a few days, even if only to acknowledge
receipt. If the report involves a specific file or line, include the
path and what you think it does.

## What this product touches

Mori Lens reads the text under the pointer on the user's screen. That
reach is the product, and it is also the reason the rules below are
not adjustable defaults.

### Privacy guarantees

The full model, with evidence and how to verify each claim:
[docs/privacy-model.md](docs/privacy-model.md). The short version:

- **Nothing leaves the machine by default.** Translation runs against a
  local Ollama on `127.0.0.1`. A remote address is refused, not warned
  about: the HTTP client has no TLS and no per-provider grant.
- **No screen image is ever written to disk.** Including for debugging.
  An OCR crop lives in memory for the duration of one recognition and is
  dropped.
- **Persistent History is disabled by default.** When enabled, only text
  already shown on screen is stored, in a local SQLite database, with no
  automatic transmission.
- **Explicitly saved Library items are persisted.** The user saves a card
  to the Library with `Ctrl+Alt+S` or the Save button. Each entry is one
  deliberate action.
- **Session Recent is memory-only.** The most recent cards are held in
  memory for the session and discarded on exit.
- **Protected fields are refused before they are read.** `IsPassword` is
  checked before any text is materialised. If the check itself fails,
  the field is treated as protected.
- **The clipboard is written on a keypress and never read.** `Ctrl+Alt+C`
  puts the card on the clipboard. Lens never reads the clipboard back.
- **The engine in use is visible.** The card shows what produced the
  answer, so the user can always tell whether that text left the
  machine.
- **Model output is untrusted.** Nothing a model returns is executed,
  opened, or followed.

### Per-application tiers

Three tiers, per application:

| Tier | Behaviour |
|------|-----------|
| Allow | automatic hover |
| Modifier | only while a modifier key is held |
| Deny | never, including with the modifier |

Deny is the default for password managers, by name. The default for
everything else is `Modifier`: Lens reads nothing anywhere it was not
told about unless a key is held.

### Logging

The rule: log the shape, never the content. Application identity in logs
is a stable hash, not a process name.

### What the user can verify

- The engine is named at startup, in every card's footer, and in the
  tray tooltip.
- `Ctrl+Alt+L` pauses and resumes. Pause means the pointer is not
  sampled at all, which a process monitor can confirm.
- The per-application tier list is in `%APPDATA%\Mori Lens\settings.toml`,
  opened from the tray menu and re-read while Lens runs.
- Network calls to anything other than a granted provider are a defect,
  and the local-only case makes zero of them.

## Known limitations

- A Lens running without elevation cannot read an application running
  elevated, because of UIPI. Lens does not run elevated to fix this.
- Model output is not resistant to prompt injection via screen text; the
  mitigation is that the original text is always shown beside the
  answer.
- Banking and payment applications are not denied by name; they are
  protected by the `Modifier` default.

## Supported versions

Only the latest release is supported. There is no backport process.
Security fixes are applied to the current release and future releases.

## License

See the README for the current license state.
