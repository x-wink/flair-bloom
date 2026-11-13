import type { TourDef } from '../types';
import { p } from './helpers';

export const layouts: TourDef = {
  id: 'layouts',
  title: '布局与浮窗',
  summary: '在键鼠图上直接点键，最小化成浮窗',
  // 横版是单键模型（trigger == target），外设模式建不出切换连发，switchLayout 本来就会拦；
  // 教程不绕过这条互斥，在目录里就说明白。
  available: (snapshot) =>
    snapshot.coincidentToggle ? undefined : '当前输入模式不支持横版·键鼠按键',
  steps: [
    {
      id: 'toggle',
      target: 'layout-toggle',
      title: '两种布局',
      body: p(
        '底部右侧布局按钮显示切换目标的图标和名称：当前竖版时显示「横版」，当前横版时显示「竖版」；☰ → 外观布局也能选择。',
        '竖版适合精细编辑，横版一眼看清哪些键在用。',
        '顶栏、内容区与底栏统一采用紧凑的四边留白，常规控件与面板使用统一圆角，滑动开关保持胶囊；快捷键提示保留小圆角，与筛选数量标记共用浅主色底和主色文字；底部新建、窗口绑定与底栏按钮等高。',
        '布局切换使用弱样式，全局开关始终为主色填充的胶囊按钮，通过「开始运行／结束运行」文字表达当前动作；窗口捕获与底栏下拉沿用表单输入组件的底色和边框。',
        '界面统一使用系统 UI 字体，跟随显示器缩放，不受 Windows 文字大小影响；需要放大时可以拖动面板边缘。',
        '网吧模式可用横版；外设模式暂不支持横版。',
      ),
      // 从竖版起步，下一步切过去才有对照
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
      },
    },
    {
      id: 'click-key',
      target: 'hkb-keyboard',
      title: '直接点键设连发',
      body: p(
        '在键鼠图上点一个键：无 → 切换连发 → 长按连发 → 取消，三态轮着来。',
        '右键点它还有更多选项：启用、改模式、分组、删除。',
      ),
      prepare: (host) => host.setLayout('horizontal'),
    },
    {
      id: 'legend',
      target: 'hbar-legend',
      title: '颜色和角标的意思',
      body: p(
        '实心是长按连发、描边是切换连发、虚线是已停用；数字角标是规则组的编号，实心描边同理。',
        '正在连发的键有明显描边、呼吸光晕和角落亮点；暂停时变淡、停止呼吸并显示 ⏸。',
        '⚠ 表示多步队列、启动与输出不同或同键多规则等复杂配置；点击进入竖版编辑，悬停能看全部输出键的走线。',
      ),
      prepare: (host) => host.setLayout('horizontal'),
    },
    {
      id: 'interval',
      target: 'hbar-interval',
      title: '横版共用一个间隔',
      body: p('横版上点出来的一步规则共用这个间隔，多步队列不受影响；分别设置请在竖版编辑。'),
      prepare: (host) => host.setLayout('horizontal'),
    },
    {
      id: 'float',
      target: 'minimize',
      title: '收进浮窗',
      body: p(
        '点「—」把面板收成一个常驻置顶的小条，想放哪拖到哪，位置会记住。',
        '全局启用时小条有明显描边；正在连发的键会呼吸高亮并带亮点，也能直接启停、点开回主面板。',
        '「设置 → 通用 → 外观」里的视觉强化默认开启：两道等距流光沿内边框每秒绕行一圈，横版、竖版和浮窗自动匹配尺寸与主题；切换布局重新计算完整轨迹。Logo 未运行时灰调，运行时彩色脉冲并扩散虚影，关闭视觉强化会停止流光与 Logo 脉冲虚影，保留原有高亮。',
        '流光头使用面板文字色，细小实心，长尾沿主题色渐隐。',
      ),
      // 教程结束时布局停在竖版，别把用户丢在横版
      prepare: (host) => host.setLayout('vertical'),
    },
    {
      id: 'finish',
      title: '关窗口的行为也能改',
      body: p('点 ✕ 关窗口时是退出还是收进浮窗，在设置 → 通用里选。'),
    },
  ],
};
