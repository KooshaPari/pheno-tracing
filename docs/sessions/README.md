# Agent Session Logs — pheno-tracing

_Audit fix for L31 (v37 scorecard). Establishes a committed session corpus so
agents can resume work and reviewers can audit prior changes._

## Convention

Each agent work session that produces committed changes should create a session
log file in this directory using the template at `SESSION_TEMPLATE.md`.

**File naming:** `YYYY-MM-DD-<slug>.md` (e.g. `2026-06-30-v37-overhaul.md`).

**When to create a log:**

- Any session that commits to `main` or opens a PR.
- Any session that performs a non-trivial investigation (even if no commit results).

**When to skip:** Trivial one-liner fixes that are fully self-described by the
commit message.

## Rationale

`AGENTS.md` and `llms.txt` describe *what* the repo does. Session logs record
*what changed and why* across individual work sessions, enabling:

- Resumable agent context (an agent reads prior logs before starting).
- Auditability of autonomous changes.
- Backlog visibility (open-items accumulate across sessions).
