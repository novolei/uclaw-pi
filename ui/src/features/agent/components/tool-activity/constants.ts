/**
 * Shared sizing/layout constants for the tool-activity row family
 * (ActivityRow / ActivityGroupRow / ToolActivityList). Extracted from
 * ToolActivityItem.tsx during the features/agent migration split.
 */

export const SIZE = {
  icon: 'size-3',
  spinner: 'size-2.5',
  row: 'py-1',
  staggerLimit: 10,
  autoScrollThreshold: 6,
  rowHeight: 26,
} as const
