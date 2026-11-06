# Requirements Document

## Introduction

Label new issues automatically, so maintainers triage faster.

## Glossary

- **Triage_Bot**: the service that labels new issues

## Requirements

### Requirement 1: Automatic labels

**User Story:** As a maintainer, I want new issues labelled automatically, so that triage is faster.

#### Acceptance Criteria

1. WHEN a new issue mentions "crash", THE Triage_Bot SHALL add the label `bug`
2. WHEN a new issue mentions "please add", THE Triage_Bot SHALL add the label `enhancement`
3. IF no rule matches, THEN THE Triage_Bot SHALL add the label `needs-triage`

### Requirement 2: Audit log

**User Story:** As a maintainer, I want every label recorded, so that I can see why an issue has it.

#### Acceptance Criteria

1. WHEN THE Triage_Bot adds a label, THE Triage_Bot SHALL record the issue number and the label
