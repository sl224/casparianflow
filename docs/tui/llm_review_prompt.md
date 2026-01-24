# LLM TUI Review Prompt

You are reviewing a Terminal UI (TUI) using the attached snapshot bundle.
The bundle contains:
- Plain text frames
- Background masks (focus/selection hints)
- Layout tree JSON (rects + focus)
- Optional tmux captures (real interaction frames)

## Your Task

Provide actionable UX feedback for the TUI. Be specific, reference frame names,
point to misalignments, focus ambiguity, or missing affordances.

## Rubric (Must Cover)

1. **Critical path mapping**
   - Can a first-time user complete the core flow without guessing?
2. **Focus/escape consistency**
   - Is it clear where focus is? Does Esc/back behave consistently?
3. **Density at 80x24**
   - Are key elements readable at the smallest size?
4. **Discoverability vs clutter**
   - Are key actions visible without overwhelming the layout?
5. **Trust signals**
   - Do labels and hints match actual keybinds and behavior?

## Output Format (Required)

1. **Top Issues (ranked by severity)**
   - Severity: Critical / High / Medium / Low
   - Evidence: reference specific frames (e.g., `discover_rule_builder__80x24__plain`)
   - Impact: what breaks or confuses the user
   - Fix: concrete change suggestion

2. **Wireframe Proposals**
   - Provide ASCII wireframes for:
     - 80x24
     - 120x40
   - Focus on structure and spacing, not colors.

3. **Navigation + Focus Model**
   - Describe the focus rules and Esc/back behavior.
   - Highlight any conflicting shortcuts.

4. **Copy / Affordance Issues**
   - Point out misleading labels or missing hints.

5. **Quick Wins (1 week)**
   - Small changes with high impact.

## Constraints

- Do not invent keybindings not shown in the frames.
- Do not assume hidden UI elements beyond what is visible.
- If something is unclear, call it out explicitly.
