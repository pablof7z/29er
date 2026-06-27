---
title: Excluding .claude/ from Version Control
slug: claude-directory-gitignore
topic: git-configuration
summary: The `.claude/` directory contains local Claude-related state and configuration and should not be committed to version control.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-27
updated: 2026-06-27
verified: 2026-06-27
compiled-from: conversation
sources:
  - session:2d0bcf1a-c9a6-4929-99bd-7956ebfabf16
---

# Excluding .claude/ from Version Control

## Overview

The `.claude/` directory contains local Claude-related state and configuration and should not be committed to version control. <!-- [^2d0bc-352a8] -->

## Add to .gitignore

Add the following line to your `.gitignore` file to exclude the `.claude/` directory from Git:

```
.claude/
``` <!-- [^2d0bc-352a8] -->
