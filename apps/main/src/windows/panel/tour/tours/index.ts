import type { TourDef } from '../types';
import { gameMode } from './game-mode.ts';
import { gettingStarted } from './getting-started.ts';
import { groups } from './groups.ts';
import { jx3 } from './jx3.ts';
import { layouts } from './layouts.ts';
import { profiles } from './profiles.ts';
import { rules } from './rules.ts';
import { settings } from './settings.ts';

/** 目录里的显示顺序；id 即文件名，scripts/check-skills.ts 据此与说明书比对。 */
export const TOURS: TourDef[] = [
  gettingStarted,
  gameMode,
  rules,
  groups,
  layouts,
  profiles,
  settings,
  jx3,
];

export function findTour(id: string): TourDef | undefined {
  return TOURS.find((t) => t.id === id);
}
