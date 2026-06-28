# Engineering Skills Playbook

This file tracks high-value engineering skills and how they apply to Prometheus.

## Core skills (priority order)

1. **Strict TDD execution**
   - Why: reduces regressions while accelerating design feedback.
   - Apply in repo: all core modules (`router`, `session`, `storage`, `sync`).

2. **Test architecture design (small/medium/large)**
   - Why: faster pipelines and better confidence split by risk.
   - Apply in repo: keep most logic in fast unit tests, reserve broad tests for smoke paths.

3. **Refactoring discipline under safety net**
   - Why: keeps architecture evolvable while shipping continuously.
   - Apply in repo: incremental cleanup after each green phase, no large untested rewrites.

4. **ADR-driven architecture decisions**
   - Why: preserves long-term rationale for critical choices.
   - Apply in repo: record key decisions (storage engine, sync model, plugin ABI, routing policy).

5. **Operational engineering baseline**
   - Why: commercial software needs observability and reproducibility.
   - Apply in repo: logging standards, deterministic config, reproducible builds and checks.

## Practical skill adoption rules

- New feature PRs must show at least one red-green-refactor cycle.
- Any architectural change must include an ADR note in `docs/architecture.md` or dedicated ADR docs.
- Test additions should prefer deterministic fixtures over live dependencies.
- Performance-sensitive paths must include micro-benchmarks before optimization refactors.

## Reference set (curated)

- Test-Driven Development (Martin Fowler)
  - https://martinfowler.com/bliki/TestDrivenDevelopment.html
- Unit Test (Martin Fowler)
  - https://martinfowler.com/bliki/UnitTest.html
- Integration Test (Martin Fowler)
  - https://martinfowler.com/bliki/IntegrationTest.html
- Test Sizes (Google Testing Blog)
  - https://testing.googleblog.com/2010/12/test-sizes.html
- Architecture Decision Records (ADR)
  - https://github.com/joelparkerhenderson/architecture-decision-record
- The Twelve-Factor App
  - https://12factor.net/
