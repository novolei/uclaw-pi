/**
 * `shared/tool-rendering` — cross-domain tool-call / content-block rendering layer.
 *
 * Used by BOTH the agent core (`components/agent/*`) and the chat domain
 * (`components/chat/*`), so per the code-organization discipline
 * (docs/adr/2026-05-31-pi-code-organization-discipline.md: "共享只下沉 shared/")
 * it lives here rather than inside a single feature.
 *
 * External consumers import from this barrel only — never deep paths.
 */

// Content-block renderer (paired tool_use ↔ tool_result rendering).
export { NativeBlockRenderer } from './NativeBlockRenderer'
export type { NativeBlockRendererProps } from './NativeBlockRenderer'

// Tool-result renderer dispatcher.
export { ToolResultRenderer } from './tool-renderers'
export type { ToolResultRendererProps } from './tool-renderers'

// Live bash stream view (imported directly by chat + agent today).
export { BashStreamView } from './tool-renderers/BashStreamView'

// Tool metadata helpers (icons, summaries, diff stats, elapsed formatting, …).
export * from './tool-utils'

// Tool phrasing (verb + object for a tool call).
export * from './tool-phrase'
