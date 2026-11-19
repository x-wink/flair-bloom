import { emit } from '@tauri-apps/api/event';

export type SampleWindow = { name: string; start: number; end: number };
export type DiagnosticNotice = string | { reply: string; samples: SampleWindow[] };
type Kind = 'down' | 'up' | 'character' | 'wheel' | 'blur';
type Seen = { at: number; kind: Kind };

export function summarizeSamples(samples: SampleWindow[], seen: Seen[]) {
  const groups = new Map<
    string,
    { name: string; attempts: number; keyHits: number; wheelHits: number } & Record<Kind, number>
  >();
  for (const { name, start, end } of samples) {
    const row = groups.get(name) ?? {
      name,
      attempts: 0,
      keyHits: 0,
      wheelHits: 0,
      down: 0,
      up: 0,
      character: 0,
      wheel: 0,
      blur: 0,
    };
    const before = { ...row };
    row.attempts++;
    for (const event of seen) if (event.at >= start && event.at <= end) row[event.kind]++;
    if (
      row.down - before.down === 1 &&
      row.up - before.up === 1 &&
      row.character - before.character === 1
    )
      row.keyHits++;
    if (row.wheel > before.wheel) row.wheelHits++;
    groups.set(name, row);
  }
  return [...groups.values()];
}

export function observeDiagnosticInput() {
  const seen: Seen[] = [];
  const remember = (kind: Kind) => seen.push({ at: Date.now(), kind });
  const field = (target: EventTarget | null) =>
    target instanceof HTMLInputElement && target.hasAttribute('data-diagnostic-input');
  const key = (e: KeyboardEvent) => {
    if (e.isTrusted && field(e.target) && e.code === 'Digit1')
      remember(e.type === 'keydown' ? 'down' : 'up');
  };
  const input = (e: Event) => {
    if (e.isTrusted && field(e.target) && (e as InputEvent).data === '1') remember('character');
  };
  const wheel = (e: WheelEvent) => {
    if (e.isTrusted && e.deltaY < 0) remember('wheel');
  };
  const blur = () => {
    remember('blur');
  };
  window.addEventListener('keydown', key, true);
  window.addEventListener('keyup', key, true);
  window.addEventListener('input', input, true);
  window.addEventListener('wheel', wheel, true);
  window.addEventListener('blur', blur);
  return {
    report: (notice: Exclude<DiagnosticNotice, string>) =>
      emit(notice.reply, summarizeSamples(notice.samples, seen)),
    stop() {
      window.removeEventListener('keydown', key, true);
      window.removeEventListener('keyup', key, true);
      window.removeEventListener('input', input, true);
      window.removeEventListener('wheel', wheel, true);
      window.removeEventListener('blur', blur);
    },
  };
}
