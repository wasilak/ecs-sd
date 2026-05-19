# Project State: ecs-sd

**Project:** ecs-sd — AWS ECS HTTP Service Discovery for Prometheus/VictoriaMetrics  
**Current Phase:** Not started  
**Last Updated:** 2026-05-19  

---

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-05-19)

**Core value:** Zero-config metrics discovery for ECS containers — automatic discovery of metrics endpoints with configurable metadata

**Current focus:** Project scope clarified — ready for Phase 1 planning

---

## Phase Status

| Phase | Name | Status | Progress |
|-------|------|--------|----------|
| 1 | Core Discovery & HTTP API | ○ Pending | 0% |
| 2 | Metadata Labels | ○ Pending | 0% |
| 3 | Caching & Configuration | ○ Pending | 0% |
| 4 | Observability & Logging | ○ Pending | 0% |
| 5 | Packaging & CI/CD | ○ Pending | 0% |

---

## Active Work

_None — project scope just clarified from Obsidian spec_

---

## Blockers

_None_

---

## Recent Decisions

1. **Project Type**: HTTP server (not CLI) — provides Prometheus http_sd_configs endpoint
2. **Launch Type**: EC2 only — Fargate excluded from v1 scope
3. **Target Criteria**: Containers with docker label `metrics_port`
4. **Address Format**: EC2 instance private IP + metrics_port value
5. **Cache Strategy**: Stale-while-revalidate — always serve cached, refresh in background
6. **Metadata Levels**: 5 levels (container → task → service → cluster → aws), configurable

---

## Next Actions

1. Run `/gsd-discuss-phase 1` to gather context for Phase 1
2. Or run `/gsd-plan-phase 1` to skip discussion and plan directly

---

*State updated: 2026-05-19*
