# Bugfix Requirements Document

## Current Behavior (Defect)

1. WHEN a session is idle for 30 minutes THEN the system logs the user out after 5 minutes

## Expected Behavior (Correct)

1. WHEN a session is idle for 30 minutes THEN the system SHALL keep the user logged in

## Unchanged Behavior (Regression Prevention)

1. WHEN a user logs out THEN the system SHALL CONTINUE TO end the session
