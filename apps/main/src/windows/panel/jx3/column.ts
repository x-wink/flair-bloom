// 剑网三专栏的数据格式与校验。数据由产品站托管（apps/site/public/jx3/column.json），
// 应用、校验测试与采集脚本共用这里的类型和 validateColumn。

/** 本版应用认得的数据格式版本；线上更高时应用回落缓存或快照，并提示更新 */
export const COLUMN_SCHEMA = 1;

export type SchoolKind = 'sect' | 'genre';
export type SpecRole = 'dps' | 'tank' | 'healer';
export type MacroKind = 'one-key' | 'multi-stage';

export interface Spec {
  /** JX3Box 的心法编号 */
  id: number;
  /** 心法名，条目按它引用，与 JX3Box 帖子的 post_subtype 一致 */
  name: string;
  role: SpecRole;
}

export interface School {
  /** 门派用主题预设的 id（门派拼音），打开专栏时据此默认选中 */
  id: string;
  /** sect 是门派；genre 是流派，不属于任何门派、所有玩家都能选作第二心法 */
  kind: SchoolKind;
  name: string;
  specs: Spec[];
}

/**
 * 原作者的授权记录，随条目存档，有争议时能查到来源。两项要么都有、要么都没有：
 * 有授权的常驻展示，没有的是按热度精选的公开帖，界面上折叠并写明来源。
 */
export interface Authorization {
  /** 授权日期 YYYY-MM-DD */
  authorized_at?: string;
  /** 谁授权、怎么转达的 */
  authorized_note?: string;
}

export const isAuthorized = (e: Authorization) => e.authorized_at !== undefined;

export interface Whitepaper extends Authorization {
  spec: string;
  title: string;
  author: string;
  url: string;
}

export interface MacroItem {
  name: string;
  /**
   * 宏正文，只有作者授权的条目可以带（如白皮书里贴的宏，没有云端宏可同步）。
   * 带正文的复制正文、粘贴进游戏宏编辑；不带的复制云端宏名。
   */
  body?: string;
}

/**
 * 一篇宏帖子，或白皮书里贴出的一组宏；作者常把起手、循环、爆发几个宏放在同一篇里，
 * 复制时逐个复制。
 */
export interface Macro extends Authorization {
  spec: string;
  kind: MacroKind;
  title: string;
  author: string;
  url: string;
  items: MacroItem[];
}

export interface Column {
  schema: number;
  updated_at: string;
  schools: School[];
  whitepapers: Whitepaper[];
  macros: Macro[];
}

const SCHOOL_KINDS: readonly string[] = ['sect', 'genre'];
const SPEC_ROLES: readonly string[] = ['dps', 'tank', 'healer'];
const MACRO_KINDS: readonly string[] = ['one-key', 'multi-stage'];

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
const isText = (v: unknown): v is string => typeof v === 'string' && v.trim() !== '';
const isDate = (v: unknown) =>
  typeof v === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(v) && !Number.isNaN(Date.parse(v));

function isHttps(v: unknown): boolean {
  if (typeof v !== 'string') return false;
  try {
    return new URL(v).protocol === 'https:';
  } catch {
    return false;
  }
}

/** 游戏插件里同步云端宏用的名字，与 JX3Box 宏页面的复制按钮一致 */
export const cloudMacroName = (author: string, name: string) => `${author}#${name}`;

/**
 * 逐项检查数据，返回全部问题（空数组即通过）。不在第一处出错就停：采集脚本和人工编辑
 * 一次能看全要改的地方。只查本版用到的字段，多出来的字段放过，站点可以先于应用加字段。
 */
export function validateColumn(raw: unknown): string[] {
  const errors: string[] = [];
  if (!isRecord(raw)) return ['顶层必须是对象'];
  if (raw.schema !== COLUMN_SCHEMA) errors.push(`schema 应为 ${COLUMN_SCHEMA}`);
  if (!isDate(raw.updated_at)) errors.push('updated_at 应为 YYYY-MM-DD');

  const specNames = new Set<string>();
  const specIds = new Set<number>();
  const schoolIds = new Set<string>();
  const schools = Array.isArray(raw.schools) ? raw.schools : [];
  if (schools.length === 0) errors.push('schools 不能为空');
  schools.forEach((s, i) => {
    const at = `schools[${i}]`;
    if (!isRecord(s)) return void errors.push(`${at} 必须是对象`);
    if (typeof s.id !== 'string' || !/^[a-z]+$/.test(s.id)) errors.push(`${at}.id 应为小写拼音`);
    else if (schoolIds.has(s.id)) errors.push(`${at}.id 重复：${s.id}`);
    else schoolIds.add(s.id);
    if (!SCHOOL_KINDS.includes(s.kind as string)) errors.push(`${at}.kind 应为 sect 或 genre`);
    if (!isText(s.name)) errors.push(`${at}.name 不能为空`);
    const specs = Array.isArray(s.specs) ? s.specs : [];
    if (specs.length === 0) errors.push(`${at}.specs 不能为空`);
    specs.forEach((p, j) => {
      const sat = `${at}.specs[${j}]`;
      if (!isRecord(p)) return void errors.push(`${sat} 必须是对象`);
      if (!Number.isInteger(p.id)) errors.push(`${sat}.id 应为 JX3Box 心法编号`);
      else if (specIds.has(p.id as number)) errors.push(`${sat}.id 重复：${p.id}`);
      else specIds.add(p.id as number);
      if (!isText(p.name)) errors.push(`${sat}.name 不能为空`);
      else if (specNames.has(p.name)) errors.push(`${sat}.name 重复：${p.name}`);
      else specNames.add(p.name);
      if (!SPEC_ROLES.includes(p.role as string))
        errors.push(`${sat}.role 应为 dps、tank 或 healer`);
    });
  });

  const checkEntry = (e: unknown, at: string, seenUrls: Set<string>) => {
    if (!isRecord(e)) return void errors.push(`${at} 必须是对象`);
    if (!isText(e.title)) errors.push(`${at}.title 不能为空`);
    if (!isText(e.spec) || !specNames.has(e.spec))
      errors.push(`${at}.spec 不是表里的心法：${String(e.spec)}`);
    if (!isText(e.author)) errors.push(`${at}.author 不能为空`);
    if (!isHttps(e.url)) errors.push(`${at}.url 必须是 https 链接`);
    const hasDate = e.authorized_at !== undefined;
    const hasNote = e.authorized_note !== undefined;
    if (hasDate !== hasNote)
      errors.push(`${at} 授权记录不完整：authorized_at 与 authorized_note 要么都写、要么都不写`);
    if (hasDate && !isDate(e.authorized_at)) errors.push(`${at}.authorized_at 应为 YYYY-MM-DD`);
    if (hasNote && !isText(e.authorized_note))
      errors.push(`${at}.authorized_note 不能为空：写明谁授权、怎么转达的`);
    const url = String(e.url);
    if (seenUrls.has(url)) errors.push(`${at} 重复：${url}`);
    seenUrls.add(url);
  };

  const papers = Array.isArray(raw.whitepapers) ? raw.whitepapers : undefined;
  if (!papers) errors.push('whitepapers 应为数组');
  const paperUrls = new Set<string>();
  papers?.forEach((e, i) => checkEntry(e, `whitepapers[${i}]`, paperUrls));

  const macros = Array.isArray(raw.macros) ? raw.macros : undefined;
  if (!macros) errors.push('macros 应为数组');
  const macroUrls = new Set<string>();
  // 云端宏名按 作者#宏名 认，同一作者的两篇帖子里也不能撞名
  const cloudNames = new Set<string>();
  macros?.forEach((e, i) => {
    const at = `macros[${i}]`;
    checkEntry(e, at, macroUrls);
    if (!isRecord(e)) return;
    if (!MACRO_KINDS.includes(e.kind as string))
      errors.push(`${at}.kind 应为 one-key 或 multi-stage`);
    const items = Array.isArray(e.items) ? e.items : [];
    if (items.length === 0) errors.push(`${at}.items 不能为空`);
    const itemNames = new Set<string>();
    items.forEach((m, j) => {
      const iat = `${at}.items[${j}]`;
      if (!isRecord(m)) return void errors.push(`${iat} 必须是对象`);
      if (!isText(m.name)) return void errors.push(`${iat}.name 不能为空`);
      if (itemNames.has(m.name)) errors.push(`${iat}.name 重复：${m.name}`);
      itemNames.add(m.name);
      if (m.body === undefined) {
        const cloud = cloudMacroName(String(e.author), m.name);
        if (cloudNames.has(cloud)) errors.push(`${iat} 云端宏名重复：${cloud}`);
        cloudNames.add(cloud);
      } else if (!isText(m.body)) errors.push(`${iat}.body 不能为空`);
      else if (e.authorized_at === undefined)
        errors.push(`${iat}.body 只有作者授权的条目才能收宏正文`);
    });
  });

  return errors;
}
