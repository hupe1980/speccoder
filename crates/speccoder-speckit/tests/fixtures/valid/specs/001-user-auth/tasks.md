---
description: "Task list for user authentication"
---

# Tasks: User Authentication

**Input**: Design documents from `/specs/001-user-auth/`

## Format: `[ID] [P?] [Story] Description`

<!--
  Sample tasks are replaced by /speckit-tasks:
  - [ ] T900 A commented-out task
-->

```text
- [ ] T901 A task inside a code fence
```

## Phase 1: Setup

- [x] T001 Create project structure per implementation plan
- [X] T002 Initialize Rust project with clap dependencies
- [ ] T003 [P] Configure linting and formatting tools

## Phase 2: Foundational

- [ ] T004 Setup database schema and migrations framework

## Phase 3: User Story 1 - Log in with email (Priority: P1) 🎯 MVP

### Tests for User Story 1

- [ ] T005 [P] [US1] Contract test for POST /login in tests/contract/login.rs

### Implementation for User Story 1

- [ ] T006 [P] [US1] Create User model in src/models/user.rs
- [ ] T007 [US1] Implement AuthService in src/services/auth.rs (depends on T006)

## Dependencies & Execution Order

- Setup → Foundational → stories
