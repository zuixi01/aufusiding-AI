# Auto-coding Skill Profile (Local)

This repository does not have a built-in global skill loader named `Auto-coding`.
To keep behavior consistent, this file defines a local equivalent profile for development.

## Activation scope

- Applies to all feature development in this repository.
- Especially applies to Phase 1 execution tasks.

## Required behavior

1. Follow strict TDD: `Red -> Green -> Refactor`.
2. Keep changes in small, verifiable slices.
3. Add or update tests for every behavior change.
4. Keep warnings at zero for touched modules.
5. Prefer deterministic tests; avoid external network dependencies in unit tests.

## Phase 1 execution mode

- Implement by dependency order from `docs/dev-execution-plan.md`.
- Treat key-path tasks as blocking.
- For each task, record:
  - target files
  - failing test first
  - minimal passing implementation
  - refactor notes

## Verification checklist per task

- [ ] failing test exists before implementation
- [ ] targeted tests pass
- [ ] touched module tests pass
- [ ] no new warnings in touched modules
- [ ] task acceptance criteria mapped from execution plan

## Notes

- This profile is repository-local and works as the configured replacement for `Auto-coding`.
