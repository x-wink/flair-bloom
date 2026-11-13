import type { TourDef } from '../types';

export const groups: TourDef = {
  id: 'groups',
  title: '规则组',
  summary: '同组只运行一条',
  steps: [
    {
      id: 'add',
      target: 'add-group',
      title: '创建规则组',
      body: ['这里添加规则组', '同组只运行一条'],
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
        host.setFilter('all');
        host.openCreateMenu();
      },
    },
    {
      id: 'rules',
      target: 'group-latest',
      title: '设置组内规则',
      body: ['这里设置组内规则', '启用后才会运行'],
      prepare: (host) => {
        host.closeMenus();
        return host.snapshot.rules.some((rule) => rule.group) ? undefined : 'skip';
      },
    },
    {
      id: 'run',
      target: 'global',
      title: '按键切换',
      body: ['开始后按键切换', '原规则自动停止'],
      prepare: (host) => host.closeMenus(),
    },
  ],
};
