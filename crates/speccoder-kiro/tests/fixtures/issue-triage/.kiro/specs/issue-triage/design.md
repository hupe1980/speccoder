# Design Document

## Overview

One function per rule in `src/lib.rs`; the first rule that matches wins.

## Correctness Properties

### Property 1: One label per issue

*For any* issue, exactly one label is added.

**Validates: Requirements 1.1, 1.2, 1.3**
