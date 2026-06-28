# Prometheus TDD Protocol (Strict)

This document defines the required development loop for all new feature work.

## 1) Non-negotiable TDD loop

For every behavior change, follow this order only:

1. **Red**: write or extend a failing test first.
2. **Green**: implement the smallest change to pass the test.
3. **Refactor**: improve code structure while keeping tests green.

No production code is added before at least one failing test exists for that behavior.

## 2) Test levels and naming

- `unit`: fast, deterministic, no network, no filesystem side effects.
- `integration`: local process/database integration with controlled fixtures.
- `system`: end-to-end path, minimal critical flows only.

Suggested naming:

- Unit tests: `module_behavior_expected_result`
- Integration tests: `when_condition_then_observable_outcome`
- System tests: `user_path_primary_goal`

## 3) Scope discipline

- One TDD cycle should target one behavior.
- Avoid mixing unrelated refactors with behavior changes.
- If a task is large, split into multiple red-green-refactor slices.

## 4) Quality gates per change

Each change must satisfy:

1. New/updated tests fail before implementation (captured in local run notes).
2. New/updated tests pass after implementation.
3. Existing relevant test suites remain green.
4. No new warnings in changed modules.

## 5) Anti-patterns (disallowed)

- Writing tests only after code is complete.
- Changing assertions to match buggy behavior.
- Using broad end-to-end tests to replace missing unit tests.
- Adding hidden feature logic without test coverage.

## 6) Practical workflow for this repository

For each task:

1. Add tests in the corresponding module first.
2. Confirm failure reason matches expected behavior gap.
3. Implement minimal code path.
4. Re-run targeted tests.
5. Refactor internals and keep tests green.
6. Run module-level and workspace-level checks before merge.

## 7) References used

- Martin Fowler: Test-Driven Development
  - https://martinfowler.com/bliki/TestDrivenDevelopment.html
- Martin Fowler: Unit Test
  - https://martinfowler.com/bliki/UnitTest.html
- Martin Fowler: Integration Test
  - https://martinfowler.com/bliki/IntegrationTest.html
- Google Testing Blog: Test Sizes
  - https://testing.googleblog.com/2010/12/test-sizes.html
