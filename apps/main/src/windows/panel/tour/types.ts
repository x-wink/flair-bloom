import type { ReactNode } from 'react';
import type { KeyId } from '../components/KeyCapture';
import type { Location } from '../components/Overlay';

/**
 * 首启教程的内容版本，与用户协议的 `AGREEMENT_VERSION` 同一套语义：存进 settings.json 的
 * `tours.introVersion`，对不上就在下次启动自动跑一遍「上手三步」，跑过（或中途退出）记回当前值。
 *
 * 教程内容有实质改动时 bump 它，老用户会再看一次。不按「有没有规则」判新老用户——
 * 新装配置自带两条未启用的出厂规则，那个口径对新用户永远为假。
 */
export const TOUR_INTRO_VERSION = '1';

/** 教程只关心规则的这几个字段；与 PanelApp 的 BurstRule 结构兼容，不反向依赖它。 */
export interface TourRule {
  id: string;
  mode: 'hold' | 'toggle';
  enabled: boolean;
  trigger_key: KeyId;
  target_key: KeyId;
  group: string | null;
}

/** 宿主在每次渲染时给出的状态快照，实操步骤靠它判定「做完了没」。 */
export interface TourSnapshot {
  rules: TourRule[];
  globalEnabled: boolean;
  inputMode: 'sendinput' | 'interception' | 'ddsimple';
  layout: 'vertical' | 'horizontal';
  activeTab: 'hold' | 'toggle';
  /**
   * 每成功录入一次「连发按键」+1。实操判定用它而不是比较键值：新建规则的连发按键默认
   * 就是 Q，用户照着提示按 Q 时键值不变，比较键值会让人永远停在「等你按一个键…」。
   */
  keyCaptureSeq: number;
  settingsOpen: boolean;
  settingsTab: 'general' | 'hotkeys' | 'sound' | 'profiles';
  /** 后端能力位：为 false（DD 系列）时横版键鼠图不可用。 */
  coincidentToggle: boolean;
}

/**
 * 教程能对宿主做的全部动作。引导层不 import PanelApp 的内部状态，PanelApp 也不认识
 * 具体教程，两边只隔这一个接口。
 */
export interface TourHost {
  snapshot: TourSnapshot;
  setActiveTab: (tab: TourSnapshot['activeTab']) => void;
  /** 可能被拒绝（DD 模式切横版），拒绝时宿主自己提示，教程只需继续。 */
  setLayout: (layout: TourSnapshot['layout']) => Promise<void>;
  openSettings: (tab: TourSnapshot['settingsTab']) => void;
  closeSettings: () => void;
  closeMenus: () => void;
}

export type PrepareResult = void | 'skip';

export interface TourStep {
  id: string;
  /** 目标元素的 `data-tour` 属性值；缺省为无目标的居中气泡。 */
  target?: string;
  title: string;
  body: ReactNode;
  /** 缺省按剩余空间自动选一侧。 */
  placement?: Location;
  /** 展示前调用：切页签、开设置等。返回 'skip' 跳过本步。 */
  prepare?: (host: TourHost) => PrepareResult | Promise<PrepareResult>;
  /**
   * 有则为实操步骤：用户真做完才自动前进。`entered` 是进入本步时的快照，
   * 「规则数增加」这类判定要靠它做基线。
   */
  done?: (now: TourSnapshot, entered: TourSnapshot) => boolean;
  /** 实操步骤下方的「等你…」提示。 */
  actionHint?: string;
}

export interface TourDef {
  id: string;
  title: string;
  summary: string;
  steps: TourStep[];
  /** 返回不可用原因；目录里据此禁用入口。 */
  available?: (snapshot: TourSnapshot) => string | undefined;
}

export type TourExitResult = 'completed' | 'skipped';
