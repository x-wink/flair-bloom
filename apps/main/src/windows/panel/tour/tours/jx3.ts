import type { TourDef } from '../types';

export const jx3: TourDef = {
  id: 'jx3',
  title: '剑网三专栏',
  summary: '选心法查攻略复制宏',
  steps: [
    {
      id: 'pickers',
      target: 'jx3-pickers',
      title: '选择心法',
      body: ['这里选择门派心法'],
      prepare: (host) => host.openJx3(),
    },
    {
      id: 'papers',
      target: 'jx3-papers',
      title: '查看攻略',
      body: ['这里查看心法攻略'],
      prepare: (host) => host.openJx3(),
    },
    {
      id: 'macros',
      target: 'jx3-macros',
      title: '复制宏',
      body: ['这里复制游戏宏', '复制后粘贴到游戏'],
      prepare: (host) => host.openJx3(),
    },
  ],
};
