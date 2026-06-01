/**
 * Skills bridge (§2A.3). Re-exports from `lib/tauri-bridge.ts`; names + payloads
 * unchanged (contract preserved). Drives the v1 skill-recall chip surface (F4).
 */
export {
  listSkills,
  listLearnedSkills,
  getLearnedSkill,
  toggleLearnedSkill,
  deleteLearnedSkill,
  setSkillLifecycle,
  updateLearnedSkill,
  recordSkillCited,
  backfillSkillKeywords,
  proposeSkillConsolidation,
  cancelSkillConsolidation,
  applySkillConsolidation,
  getSkillVersions,
  createUserSkill,
  deleteUserSkill,
  installSkillFromMarketplace,
} from '../tauri-bridge'
