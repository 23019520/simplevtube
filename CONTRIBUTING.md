# Contributing

SimpleVTube is a personal project, built for one specific streaming setup — but it's public, and issues, suggestions, and pull requests are genuinely welcome.

A few notes if you'd like to contribute:

- **Scope matters.** Check [`docs/v2-roadmap.md`](docs/v2-roadmap.md) before proposing a large feature — some things (animated GIF/APNG support, OBS WebSocket integration, a plugin system) are deliberately deferred, not forgotten, for reasons explained there.
- **This repo is spec-first.** The original requirements and architecture live in [`docs/SimpleVTube_SRS_Architecture.md`](docs/SimpleVTube_SRS_Architecture.md), including the exact file-to-file communication contracts the codebase follows. Changes that fit that architecture (one event bus, settings flow through `settings_manager.rs` only, etc.) are much easier to review than changes that route around it.
- **Small, focused PRs** are easier to review than large ones. If you're planning something big, open an issue first to talk through the approach.
- **Bug reports** are extremely welcome — include your OS, what you expected, and what actually happened. If it's audio-related, your microphone model helps too (mic gain behavior varies a lot).

## Development setup

See the [README](README.md#getting-started) for prerequisites and first-run steps.
