/**
 * Settings Tab Atom - 设置标签页状态
 *
 * 管理设置面板中当前激活的标签页
 */

import { atom } from 'jotai'

export type SettingsTab =
  | 'connectivity'   // 服务商 + 用量
  | 'intelligence'   // 模型 + Agent + 提示词
  | 'tools'          // 工具 + 权限 + 已学技能
  | 'memoryRecall'   // 记忆（本地检索引擎 + 召回参数）；id 保持不变
  | 'learnedProfile' // openhuman facet store (Sprint 2.2)
  | 'imChannels'     // IM 渠道
  | 'general'        // 通用 + 外观
  | 'stt'            // 语音输入
  | 'shortcuts'
  | 'pet'
  | 'proxy'
  | 'browserRuntime' // Browser Runtime / Startup Doctor
  | 'system'         // 系统诊断
  | 'about'

/** 当前设置标签页（不持久化，每次打开默认显示「服务商与用量」） */
export const settingsTabAtom = atom<SettingsTab>('connectivity')

/** 设置浮窗是否打开 */
export const settingsOpenAtom = atom(false)

/** 渠道创建表单是否有未保存内容（用于拦截导航离开） */
export const channelFormDirtyAtom = atom(false)

/** 外部请求关闭设置面板（如 Cmd+W），SettingsPanel 监听后弹出确认对话框 */
export const settingsCloseRequestedAtom = atom(false)
