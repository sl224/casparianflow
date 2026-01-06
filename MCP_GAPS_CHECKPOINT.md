# MCP Gaps Implementation Checkpoint

**Started:** 2025-01-05
**Status:** IN_PROGRESS

## Workstreams

| ID | Branch | Description | Status | Agent |
|----|--------|-------------|--------|-------|
| W1 | feat/constraint-visibility | Constraint visibility in discover_schemas | RUNNING | a546cdd |
| W2 | feat/refine-parser | refine_parser tool with bounded iteration | RUNNING | aab746f |
| W3 | feat/approval-protocol | Human approval protocol for all tools | RUNNING | a182f13 |
| W4 | (pending) | Constraint flow integration | BLOCKED (needs W1) | - |

## Worktrees

```
/Users/shan/workspace/cf-mcp-w1  -> feat/constraint-visibility
/Users/shan/workspace/cf-mcp-w2  -> feat/refine-parser
/Users/shan/workspace/cf-mcp-w3  -> feat/approval-protocol
```

## Merge Order

1. W1 first (constraint types are foundational)
2. W3 next (approval protocol is independent)
3. W2 next (refine_parser may use constraint types)
4. W4 last (integrates constraint flow)

## Gaps Being Addressed

1. **Constraint Visibility** (W1)
   - discover_schemas returns reasoning (WHY types were inferred)
   - Tracks eliminated types with counter-examples
   - Generates human questions for ambiguous columns
   - Adds schema grouping for bulk approval

2. **Bounded Iteration** (W2)
   - refine_parser tool takes failed code + errors
   - Max 3 attempts before escalating to human
   - Tracks changes made for transparency

3. **Human Approval Protocol** (W3)
   - WorkflowMetadata in all tool responses
   - Phase tracking (Discovery, Schema, Parser, Backtest, Execute)
   - Pending decisions for human input
   - Next actions with approval requirements
   - Bulk approval support for filter-funnel workflow

4. **Constraint Flow** (W4 - pending)
   - Pass constraint object through generate -> backtest -> refine
   - Validate output against constraints
   - Update constraints based on failures

## Notes

- Bulk/chunked approval support for filter-funnel workflow
- Human always in the loop at key decision points
- Bounded iteration prevents infinite loops
