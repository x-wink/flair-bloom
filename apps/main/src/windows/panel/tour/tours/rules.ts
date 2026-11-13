import type { TourDef } from '../types';
import { lastRule } from './helpers.ts';

export const rules: TourDef = {
  id: 'rules',
  title: '规则玩法',
  summary: '选类型再设按键',
  steps: [
    {
      id: 'mode',
      target: 'add-menu',
      title: '选择规则',
      body: ['开关按一次启动', '再按一次就停止', '长按按住才连发'],
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
        host.setFilter('all');
      },
    },
    {
      id: 'card',
      target: 'rule-latest',
      title: '设置规则',
      body: ['按键在这里设置', '启用控制本条规则'],
      prepare: (host) => (lastRule(host.snapshot) ? undefined : 'skip'),
    },
    {
      id: 'global',
      target: 'global',
      title: '全局运行',
      body: ['开始后才会连发', '结束后停止连发'],
    },
  ],
};
