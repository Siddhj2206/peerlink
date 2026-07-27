## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the default canonical triage labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout. See `docs/agents/domain.md`.

## Build environment

This is an immutable system. All cargo/rust builds must run inside the `devbox` toolbox container:

```
toolbox run --container devbox <command>
```

For interactive shell sessions, enter with:

```
toolbox enter devbox
```

The `devbox` container has all system dependencies (ALSA, etc.) installed.