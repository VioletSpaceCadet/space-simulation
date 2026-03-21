---
title: Iterative FE Design with Visual Companion Mockups
category: patterns
date: 2026-03-21
tags: [frontend, design, ui, brainstorming, visual-companion, mockups]
components: [ui_web]
---

# Iterative FE Design with Visual Companion Mockups

## Context

First FE design session for this project. Needed to redesign the ResearchPanel from a basic debug-style list into a DAG-based tech tree visualization with progressive disclosure.

## Process That Worked

Used the superpowers brainstorming skill with the visual companion (browser-based HTML mockup server) to iterate on the design in real-time. The user reviewed each mockup in the browser and gave immediate feedback, driving 14 iterations from bare concept to approved spec in one session.

### Key workflow steps

1. **Explore existing UI patterns first** — dispatched an agent to catalog the existing panel conventions (colors, typography, component patterns, badge styles) before designing anything. This grounded the mockups in the real design system.

2. **Write full HTML mockups, not wireframes** — instead of abstract option cards or text descriptions, wrote complete styled HTML that matched the actual app aesthetic (dark mission-control theme, monospace font, exact color tokens). This let the user evaluate the real feel, not an approximation.

3. **Ask conceptual questions in the terminal, visual questions in the browser** — architecture decisions (rendering approach, data flow) stayed in text. Layout/styling questions used the mockup. A question *about* UI is not automatically a visual question.

4. **Iterate fast on user feedback** — each round of feedback produced a new HTML file (never reuse filenames). Small fixes (remove abbreviations, drop tooltips) took one edit. Bigger changes (progressive disclosure, mystery nodes) got a full rewrite.

5. **Let the user drive the design** — the most valuable ideas came from the user: progressive disclosure with `???` mystery nodes, DAG edges continuing into the unknown zone, no raw tech IDs, rates shown as `/hr` not `/t`. The designer's job was to render and refine, not dictate.

## Pitfalls Encountered

- **`replace_all` is dangerous on HTML** — doing a global replace of `/t` to `/hr` corrupted `</title>` into `</hritle>`, breaking the page. Always check for collateral damage with global replaces in markup files.

- **SVG connectors with `preserveAspectRatio="none"` distort dash patterns** — use `vector-effect: non-scaling-stroke` or `preserveAspectRatio="xMidYMid meet"` to prevent visual distortion of dashed lines.

- **Absolute-positioned SVG bezier curves are fragile** — hand-tuning SVG path coordinates to match HTML node positions breaks easily when content sizes change. The v7 rewrite to tier-based flexbox layout with simple SVG line connectors was much more robust and maintainable.

- **Visual companion server times out after 30 min of inactivity** — if the conversation pauses, the server dies. Need to restart with `start-server.sh` and copy files to the new session directory. The `</title>` corruption compounded this (server started, page blank, had to debug).

- **Mystery nodes must match tech node dimensions** — if `???` boxes are a different width/height than tech nodes, the SVG connector coordinates don't align. Making them identical dimensions solved the alignment for free.

## When to Use This Pattern

- Any FE component redesign where layout/visual hierarchy matters
- When the user needs to see options to make decisions (not just read about them)
- Complex UI with multiple states (like the 4-state tech tree nodes)
- When the existing codebase has established design patterns to match

## When NOT to Use

- Simple text/data changes that don't affect layout
- Backend-only work
- When the user has provided a Figma mockup (use `figma-design-sync` instead)

## Artifacts Produced

- `docs/superpowers/specs/2026-03-21-research-panel-redesign.md` — approved spec
- `docs/superpowers/specs/2026-03-21-research-panel-mockup.html` — final mockup (v13)
- 4 Linear tickets (RD-07 through RD-10) with detailed acceptance criteria
