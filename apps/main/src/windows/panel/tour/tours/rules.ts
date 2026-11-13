import type { TourDef, TourSnapshot } from '../types';
import { p } from './helpers';

function hasHoldRule(snapshot: TourSnapshot): boolean {
  return snapshot.rules.some((r) => r.mode === 'hold');
}

export const rules: TourDef = {
  id: 'rules',
  title: '规则玩法',
  summary: '筛选、规则卡、连发序列与高级设置',
  steps: [
    {
      id: 'filter',
      target: 'filter',
      title: '一个列表，顶部只管筛选',
      body: p(
        '全部、长按、切换、规则组四个标签只筛选显示，不会删改规则；规则组只显示已分组的规则。',
        '长按、切换标签的数字是「已启用 / 总数」，规则组显示组数。长按适合平 A、等 CD 戳键；切换适合挂机、采集、一键宏。',
      ),
      prepare: async (host) => {
        host.closeMenus();
        await host.setLayout('vertical');
        host.setFilter('all');
      },
    },
    {
      id: 'add',
      target: 'add-hold',
      title: '先加一条规则',
      body: p('后面几步要在规则卡上讲，先从「我要新建」菜单选择「长按连发」。'),
      // 前置步：已经有长按规则就不出现，没有就等用户加一条，后面几步才有卡可指
      prepare: (host) => {
        host.setFilter('hold');
        if (hasHoldRule(host.snapshot)) return 'skip';
        host.openCreateMenu();
      },
      done: (now, entered) => now.rules.length > entered.rules.length,
      actionHint: '等你点一下…',
    },
    {
      id: 'card',
      target: 'rule-latest',
      title: '规则卡上有什么',
      body: p(
        '左边色条实心的是长按、描边的是切换；单键显示连发按键和间隔，多步显示连发序列，旁边标有「开关」的是规则启用开关。头部中间的把手能拖动排序。',
        '拖动卡片时，面板底部出现渐变删除区与垃圾桶图标，提示「拖入规则」及 Ctrl 强删；拖入后高亮并显示「松手删除」，按住 Ctrl 改为「松手强制删除」。普通放下仍需确认，放下时按住 Ctrl 则跳过确认直接删除。',
        '正在连发时卡片会描边、呼吸高亮；被同组插队暂停时变淡并停止呼吸。',
        '触发键与目标相同、停止键与触发或目标相同时，该热键及其 label 一起淡化，目标键保持原色；独立热键保持原样。高级规则的设置入口用实心三角标识，单步连发序列章节整体淡化，仍可录制或添加步骤。运行、录键和冲突提示仍优先显示。',
        '多条规则一起连时软件会自动控速，不会越连越快、停不下来。',
      ),
      // 上一步被跳过、仍然没有规则卡：跳过而不是指着空处讲
      prepare: (host) => {
        host.setFilter('hold');
        return hasHoldRule(host.snapshot) ? undefined : 'skip';
      },
    },
    {
      id: 'mode',
      target: 'rule-mode',
      title: '点标签就能换模式',
      body: p(
        '卡片左上角的「长按 / 切换」标签点一下就换成另一种，不用删了重建。',
        '右上角的下拉箭头里还有移入组、删除。',
      ),
      prepare: (host) => {
        host.setFilter('hold');
        return hasHoldRule(host.snapshot) ? undefined : 'skip';
      },
    },
    {
      id: 'queue',
      target: 'rule-latest',
      title: '单键也是一步序列',
      body: p(
        '单键直接在规则卡主行编辑；高级设置中的带图标「录制宏」与「添加步骤」并排，添加第二步后显示列表，可复制、删除、调序并设置各步间隔，修改自动保存。',
        '全局开关开启时，先确认结束运行再录制；未开启则直接录制，没有倒计时。录制浮层遮住面板，实时预览按键和间隔，点「结束录制」或按 F12 结束。',
        '结束后左侧「重录」、右侧「采用」：重录清空预览并开始新录制，采用才替换当前序列。浮层右上角关闭会停止并放弃本次录制，保留原序列。末步回到首步默认 10ms，与连发默认值一致，采用后仍可在步骤列表修改。正常停止后从头开始，同组临时插队会保留步骤和剩余等待。',
      ),
      prepare: (host) => {
        host.setFilter('hold');
        return hasHoldRule(host.snapshot) ? undefined : 'skip';
      },
    },
    {
      id: 'advanced',
      target: 'rule-advanced',
      title: '高级设置：触发与序列',
      body: p(
        '切换连发的启动热键与停止热键都放在高级设置；独立长按键仍在主行，方便看清按住哪个键。这里也能编辑连发序列。',
        '长按键可以与输出按键分开：按住侧键连左键、按 F 连滚轮都行。',
        '切换连发还能再单独设一个停止键。',
        '网吧模式允许启动、停止与连发用同一个键；同键启停异常时可关闭全局开关或正常退出应用，停止连发并释放按键。',
      ),
      prepare: (host) => {
        host.setFilter('hold');
        return hasHoldRule(host.snapshot) ? undefined : 'skip';
      },
    },
    {
      id: 'add-toggle',
      target: 'add-toggle',
      title: '添加切换连发',
      body: p(
        '「我要新建」菜单可以添加长按连发、切换连发和规则组；这一步只看，不用点。',
        '新建切换连发默认用 F：按一次开始连发 F，再按一次停止。',
        '需要同组规则自动切换时，可从「我要新建 → 规则组」开始，默认带两条按键为 1、2 的停用切换规则；另有「规则组」教程专门讲。',
      ),
      prepare: (host) => {
        host.setFilter('all');
        host.openCreateMenu();
      },
    },
    {
      id: 'finish',
      prepare: (host) => host.closeMenus(),
      title: '按键框变色是在提醒你',
      body: p(
        '规则的按键框描边变红或变黄是冲突提醒：热键盖住了触发键、这条的连发键是另一条的启动键之类。',
        '红的会让规则失效，黄的只是可能互相干扰，换个键就恢复。',
      ),
    },
  ],
};
