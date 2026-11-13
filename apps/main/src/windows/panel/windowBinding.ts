export interface WindowIcon {
  rgba: number[];
  size: number;
}

export interface WindowBinding {
  exe: string;
  class?: string;
  title_contains?: string;
}

export interface BindableWindow {
  id: number;
  exe?: string;
  class: string;
  title: string;
  icon?: WindowIcon;
}

export interface WindowBindingStatus {
  revision: number;
  supported: boolean;
  binding?: WindowBinding;
  target?: { name: string; icon?: WindowIcon };
  open: boolean;
  foreground?: boolean;
  present: boolean;
  reason: string;
  picking: boolean;
  error?: string;
}

export function bindingDescription(
  status: WindowBindingStatus | undefined,
  enabled: boolean,
): string {
  if (!status?.binding) return '不限制窗口';
  const name = status.target?.name || status.binding.exe;
  const detail = status.picking
    ? '请点击目标窗口，Esc 取消'
    : status.error
      ? status.error
      : !enabled
        ? '未运行'
        : status.reason === 'self'
          ? '助手前台，允许试运行'
          : status.reason === 'matched'
            ? '目标在前台，已就绪'
            : status.reason === 'unknown'
              ? '已暂停，无法识别当前窗口'
              : status.present
                ? '已暂停，等待目标窗口回到前台'
                : '已暂停，目标窗口未打开';
  const location = targetIsForeground(status)
    ? '目标窗口在前台'
    : status.present
      ? '目标窗口不在前台'
      : '目标窗口未打开';
  return `${name}（${status.binding.exe}）\n${location} · ${detail}`;
}

export function targetIsForeground(status: WindowBindingStatus | undefined): boolean {
  return !!status?.binding && (status.foreground ?? status.reason === 'matched');
}

export function filterBindableWindows(
  windows: readonly BindableWindow[],
  query: string,
): BindableWindow[] {
  const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return windows.filter((window) => {
    const text = `${window.title} ${window.exe || ''}`.toLocaleLowerCase();
    return words.every((word) => text.includes(word));
  });
}

const iconCache = new Map<string, string>();
export function windowIconUrl(icon: WindowIcon | undefined): string | undefined {
  if (!icon || icon.size < 1 || icon.size > 256 || icon.rgba.length !== icon.size * icon.size * 4)
    return undefined;
  const key = `${icon.size}:${icon.rgba.join(',')}`;
  const existing = iconCache.get(key);
  if (existing) return existing;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = icon.size;
  const context = canvas.getContext('2d');
  if (!context) return undefined;
  context.putImageData(new ImageData(new Uint8ClampedArray(icon.rgba), icon.size, icon.size), 0, 0);
  const url = canvas.toDataURL('image/png');
  if (iconCache.size >= 64) iconCache.clear();
  iconCache.set(key, url);
  return url;
}
