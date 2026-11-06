# Bugfix Requirements Document

## Introduction

Saving a note with an empty title panics instead of refusing it.

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN a note is saved with an empty title THEN the system
panics

### Expected Behavior (Correct)

2.1 WHEN a note is saved with an empty title THEN the system SHALL refuse it with a message

### Unchanged Behavior (Regression Prevention)

3.1 WHEN a note is saved with a title THEN the system SHALL CONTINUE TO save it
