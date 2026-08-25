import type { HotwordState } from "../../domain";

export function mergeHotwords(state: HotwordState) {
  return Array.from(new Set([...state.manual_hotwords, ...state.agent_hotwords])).filter(Boolean);
}
