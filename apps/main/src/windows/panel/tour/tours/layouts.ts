import type { TourDef } from '../types';

export const layouts: TourDef = {
  id: 'layouts',
  title: '布局与浮窗',
  summary: '横竖切换与浮窗',
  available: (snapshot) => (snapshot.coincidentToggle ? undefined : '此模式不支持横版'),
  steps: [
    {
      id: 'toggle',
      target: 'layout-toggle',
      title: '切换布局',
      body: ['这里切换横竖版', '按钮显示切换目标'],
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
      },
    },
    {
      id: 'keyboard',
      target: 'hkb-keyboard',
      title: '横版设按键',
      body: ['横版直接设置按键'],
      prepare: (host) => host.setLayout('horizontal'),
    },
    {
      id: 'float',
      target: 'minimize',
      title: '收起为浮窗',
      body: ['这里收起为浮窗', '浮窗也能开始运行'],
      prepare: (host) => host.setLayout('vertical'),
    },
  ],
};
