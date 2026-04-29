# Prompt Gap Analysis: Aggressive Mode & MCP Integration

**Context**: This analysis evaluates the gaps in the current NeuroPrompt Aggressive Engine (NPAE) when applied to a Model Context Protocol (MCP) architecture.

## Identified Gaps

### GAP-MCP-01: Destructive Compression of Tool Entities
- **Current State**: Aggressive mode optimizes for Task-Essential Score (TES) by stripping out structural language.
- **The Gap**: MCP clients rely on exact string matches for tool arguments (e.g., file paths, repository names). Aggressive compression may alter or remove these parameters.
- **Proposed Solution**: Implement **Entity Pinning**. The semantic parser must identify and lock specific parameter strings as `MCP_ARGUMENT` types, shielding them from lexical compression.

### GAP-MCP-02: Passive Ambiguity Handling
- **Current State**: When ambiguity > 0.65, the engine generates an array of clarifying questions (`ClarifyingQuestion`).
- **The Gap**: In an MCP flow, returning text questions is an anti-pattern. MCP supports interactive, structured prompts.
- **Proposed Solution**: Refactor ambiguity resolution to emit native **MCP Prompts** with required arguments, allowing the MCP client (like Cursor or Claude Desktop) to render native UI forms to collect the missing context before re-triggering the LLM.

### GAP-MCP-03: Static Context Targets vs Dynamic MCP Limits
- **Current State**: TES attempts to maximize the compression ratio (e.g., scoring 10.0 for maximum compression) without awareness of the LLM's available context window.
- **The Gap**: MCP workflows often chain multiple tool calls, rapidly exhausting the context window.
- **Proposed Solution**: The engine must accept a `max_tokens` or `context_target` parameter from the MCP client. Aggressive mode should dynamically adjust its hysteresis thresholds, aggressively dropping secondary context only when the target context limit is approaching.

### GAP-MCP-04: Markdown Rendering vs Structured JSON
- **Current State**: Aggressive mode ends by calling `render_crisp_prompt` to output a Markdown string.
- **The Gap**: MCP architectures expect structured data. Forcing an LLM to parse a highly compressed Markdown string introduces unnecessary cognitive load.
- **Proposed Solution**: Bypass Markdown rendering for MCP clients. Pass the `StructuredPrompt` object directly as an array of structured JSON objects. LLMs process JSON keys with significantly lower token overhead.

### GAP-MCP-05: Missing Implicit Zero-Shot Directives
- **Current State**: The schema infers missing deliverables (e.g., inferring a "shopping list").
- **The Gap**: MCP needs intent mapped to available tools.
- **Proposed Solution**: Update `StructurerRouter` to inject `SystemDirectives` that explicitly command the LLM to use specific MCP tools based on the inferred deliverables.
