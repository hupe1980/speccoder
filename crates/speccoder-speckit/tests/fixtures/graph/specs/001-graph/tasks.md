# Tasks: Graph

## Phase 1: Setup

- [x] T001 Create project structure
- [x] T002 [P] Configure linting

## Phase 2: Foundational

- [x] T003 Create database schema

## Phase 3: User Story 1 - Login (Priority: P1) 🎯 MVP

### Tests for User Story 1

- [x] T004 [P] [US1] Contract test for login in tests/contract/login.rs
- [ ] T005 [P] [US1] Integration test for login in tests/integration/login.rs

### Implementation for User Story 1

- [ ] T006 [P] [US1] Create User model in src/models/user.rs
- [ ] T007 [P] [US1] Create Session model in src/models/session.rs
- [ ] T008 [US1] Implement AuthService in src/services/auth.rs

## Phase 4: User Story 2 - Profile (Priority: P2)

### Implementation for User Story 2

- [ ] T009 [P] [US2] Create Profile model in src/models/profile.rs
- [ ] T010 [US2] Implement profile endpoint in src/api/profile.rs (depends on T008)

## Phase 5: Polish & Cross-Cutting Concerns

- [ ] T011 Documentation updates in docs/
