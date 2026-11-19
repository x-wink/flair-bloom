import type { TourDef } from '../types';

export const gameMode: TourDef = {
  id: 'game-mode',
  title: '游戏模式与驱动',
  summary: '选择适合的输入模式',
  steps: [
    {
      id: 'mode',
      target: 'input-mode',
      title: '选择输入模式',
      body: ['网吧模式即开即用', '剑三推荐游戏模式'],
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
      },
    },
    {
      id: 'install',
      target: 'input-mode',
      title: '首次安装',
      body: ['首次使用需安装', '安装完成需重启', '启用时自动选槽'],
    },
    {
      id: 'run',
      target: 'global',
      title: '开始使用',
      body: ['重启后再开始运行', '无输出时打开诊断修复', '导出诊断报告'],
    },
  ],
};
