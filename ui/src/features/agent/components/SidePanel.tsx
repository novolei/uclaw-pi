/**
 * WorkspaceFilesView — RightSidePanel 的 Files tab 内容渲染
 *
 * 单一区域: FilesRail (W3+) 承担 workspace / session / attached-dir 三合一展示。
 * 原顶层 "附加目录" UI 已移入 FilesRail 的 WorkspacePanelFooter (Task 11)。
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { FilesRail } from '@/components/files-rail'
import type { MountRoot } from '@/atoms/files-rail-atoms'
import { openPreviewTabAction } from '@/atoms/preview-panel-atoms'
import { addPendingAttachmentAction } from '@/atoms/preview-chip-atoms'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'
import {
  agentSessionsAtom,
  agentSidePanelOpenMapAtom,
  workspaceFilesVersionAtom,
  currentAgentWorkspaceIdAtom,
} from '@/atoms/agent-atoms'

interface WorkspaceFilesViewProps {
  sessionId: string
  sessionPath: string | null
}

export function WorkspaceFilesView({ sessionId, sessionPath }: WorkspaceFilesViewProps): React.ReactElement {
  const setSidePanelOpenMap = useSetAtom(agentSidePanelOpenMapAtom)
  const openPreview = useSetAtom(openPreviewTabAction)
  const addAttachment = useSetAtom(addPendingAttachmentAction)

  // filesVersion is still observed here so the auto-open effect below can
  // detect agent edits and pop the side panel open. The new FilesRail uses
  // notify events instead of polling, so we no longer bump filesVersion from
  // this component — readers elsewhere may still rely on it.
  const filesVersion = useAtomValue(workspaceFilesVersionAtom)

  const agentSessions = useAtomValue(agentSessionsAtom)
  const sessionWorkspaceId = agentSessions.find((s) => s.id === sessionId)?.workspaceId
  const globalWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const currentWorkspaceId = sessionWorkspaceId ?? globalWorkspaceId

  // Auto-open right panel when files change (Phase 1 behavior preserved).
  const prevFilesVersionRef = React.useRef(filesVersion)
  React.useEffect(() => {
    if (filesVersion > prevFilesVersionRef.current && sessionPath) {
      setSidePanelOpenMap((prev) => {
        const map = new Map(prev)
        map.set(sessionId, true)
        return map
      })
    }
    prevFilesVersionRef.current = filesVersion
  }, [filesVersion, sessionPath, sessionId, setSidePanelOpenMap])

  return (
    <div className="h-full flex flex-col">
      {currentWorkspaceId ? (
        <div className="flex-1 min-h-0 flex flex-col">
          {/* ===== Files Rail (W3) ===== */}
          {/* PreviewPanel lived here in W4a (b2b9bcf); moved to MainArea.tsx
              in the W4a follow-up so chat + preview share the central panel
              as a horizontal split (matches Proma layout). FilesRail still
              dispatches `openPreview` — MainArea picks it up via the shared
              atom. */}
          <div className="flex-1 min-h-0 flex flex-col">
            <FilesRail
              sessionId={sessionId}
              onFileClick={(mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => {
                if (node.kind === 'directory') return // directories expand, not preview
                if (event.shiftKey) {
                  void addAttachment({
                    mountId: mount.id,
                    relPath: node.relPath,
                    name: node.name,
                    sessionId,
                    absolutePath: `${mount.path}/${node.relPath}`,
                  })
                  return
                }
                if (event.metaKey || event.ctrlKey) return // reserved for W5
                openPreview({
                  target: {
                    mountId: mount.id,
                    relPath: node.relPath,
                    name: node.name,
                    sessionId,
                    absolutePath: `${mount.path}/${node.relPath}`,
                  },
                  source: 'manual',
                })
              }}
            />
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground">
          请选择工作区
        </div>
      )}
    </div>
  )
}
