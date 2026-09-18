import type { TourDef } from '../types';
import { gameMode } from './game-mode';
import { gettingStarted } from './getting-started';
import { layouts } from './layouts';
import { profiles } from './profiles';
import { rules } from './rules';
import { settings } from './settings';

/** 目录里的显示顺序；id 即文件名，scripts/check-skills.ts 据此与说明书比对。 */
export const TOURS: TourDef[] = [gettingStarted, gameMode, rules, layouts, profiles, settings];

export function findTour(id: string): TourDef | undefined {
  return TOURS.find((t) => t.id === id);
}
