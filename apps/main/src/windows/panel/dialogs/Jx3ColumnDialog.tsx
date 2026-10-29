import { invoke } from '@tauri-apps/api/core';
import { type ReactNode, useEffect, useMemo, useState } from 'react';
import Button from '../components/Button';
import { useToast } from '../components/Toast';
import {
  type Authorization,
  cloudMacroName,
  isAuthorized,
  type MacroKind,
  type School,
  validateColumn,
} from '../jx3/column';
import {
  type ColumnResult,
  defaultSchoolId,
  entriesFor,
  schoolColor,
  schoolGroups,
  sourceBrief,
  sourceLine,
  sourceSite,
  splitByAuthorization,
} from '../jx3/view';
import DialogShell from './DialogShell';
import './Jx3ColumnDialog.css';

const MACRO_KIND_LABELS: Record<MacroKind, string> = {
  'one-key': '一键宏',
  'multi-stage': '多段宏',
};

interface Props {
  /** 当前主题色，据此默认选中对应门派 */
  themeColor: string;
  onClose: () => void;
}

function SchoolDot({ id }: { id: string }) {
  const color = schoolColor(id);
  return (
    <span
      className={`jx3-dot${color ? '' : ' jx3-dot--hollow'}`}
      style={color ? { background: color } : undefined}
      aria-hidden="true"
    />
  );
}

/** 条目右下的来源标记：授权的写「作者授权」，其余写来源站点 */
function Provenance({ entry }: { entry: Authorization & { url: string } }) {
  return isAuthorized(entry) ? (
    <span className="jx3-badge" title={entry.authorized_note}>
      ✓ 作者授权
    </span>
  ) : (
    <span className="jx3-origin">来源 {sourceSite(entry.url)}</span>
  );
}

/**
 * 一段：作者授权的常驻展示；公开收录的热门折叠在后，展开后写明来源与下架方式。
 * 换心法时按 key 重新挂载，折叠状态随之复位。
 */
function Section<T extends Authorization>({
  tour,
  title,
  items,
  what,
  onContact,
  render,
}: {
  /** 新手教程的锚点 */
  tour: string;
  title: string;
  items: T[];
  /** 空状态里的称呼，如「花间游白皮书」 */
  what: string;
  onContact: () => void;
  render: (e: T) => ReactNode;
}) {
  const [expanded, setExpanded] = useState(false);
  const { authorized, others } = splitByAuthorization(items);
  return (
    <section className="jx3-section" data-tour={tour}>
      <h3 className="jx3-section__title">
        {title}
        <span className="jx3-section__count">{authorized.length}</span>
      </h3>
      {authorized.length > 0 ? (
        <ul className="jx3-list">{authorized.map(render)}</ul>
      ) : (
        <div className="jx3-empty">
          <span>还没有作者授权的{what}。写过攻略或宏？欢迎授权收录。</span>
          <Button variant="link" size="sm" onClick={onContact}>
            联系我们
          </Button>
        </div>
      )}
      {others.length > 0 && (
        <div className="jx3-others">
          <button
            type="button"
            className="jx3-others__toggle"
            aria-expanded={expanded}
            onClick={() => setExpanded((v) => !v)}
          >
            <span className="jx3-others__caret" aria-hidden="true">
              {expanded ? '▾' : '▸'}
            </span>
            公开收录的热门 · {others.length} 条
          </button>
          {expanded && (
            <>
              <p className="jx3-others__note">
                从原站公开帖子里收录的热门内容，以原站为准。作者不希望收录的，
                <button type="button" className="jx3-inline-link" onClick={onContact}>
                  联系我们
                </button>
                即下架。
              </p>
              <ul className="jx3-list">{others.map(render)}</ul>
            </>
          )}
        </div>
      )}
    </section>
  );
}

export default function Jx3ColumnDialog({ themeColor, onClose }: Props) {
  const toast = useToast();
  const [result, setResult] = useState<ColumnResult | undefined>(undefined);
  /** 后端只认格式版本与骨架，字段不对的数据在这里拦下，宁可提示也不能让面板白屏 */
  const [broken, setBroken] = useState(false);
  const [loading, setLoading] = useState(false);
  const [schoolId, setSchoolId] = useState<string | undefined>(undefined);
  const [specName, setSpecName] = useState<string | undefined>(undefined);
  const [picking, setPicking] = useState(false);

  async function load(refresh: boolean) {
    setLoading(true);
    try {
      const next = await invoke<ColumnResult>('get_jx3_column', { refresh });
      const problems = validateColumn(next.data);
      if (problems.length > 0) {
        invoke('log_from_frontend', {
          level: 'warn',
          message: `剑网三专栏数据校验失败（${next.source}）：${problems.slice(0, 5).join('；')}`,
        }).catch(() => {});
        setBroken(true);
        setResult(undefined);
        toast.error('专栏列表的数据有误，请稍后刷新重试或更新应用');
        return;
      }
      setBroken(false);
      setResult(next);
      if (refresh) {
        if (next.source === 'remote') toast.success('列表已是最新');
        else toast.warning(sourceLine(next));
      }
    } catch (e) {
      toast.error(`读取剑网三专栏失败：${e}`);
    } finally {
      setLoading(false);
    }
  }

  // 打开时取一次（多半直接拿到启动时预取的那份）；刷新走按钮
  useEffect(() => {
    void load(false);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const column = result?.data;
  const groups = useMemo(() => (column ? schoolGroups(column) : undefined), [column]);
  const school = column?.schools.find((s) => s.id === schoolId);

  // 数据到了再定默认门派；刷新后所选门派不在了也重新定，否则选择器跟着消失
  useEffect(() => {
    if (column && !column.schools.some((s) => s.id === schoolId))
      setSchoolId(defaultSchoolId(column, themeColor));
  }, [column, schoolId, themeColor]);
  // 换了门派心法回到第一个；刷新只换了数据对象，所选心法还在就不动（心法名全表唯一）
  useEffect(() => {
    setSpecName((prev) =>
      school?.specs.some((s) => s.name === prev) ? prev : school?.specs[0]?.name,
    );
  }, [school]);

  const openLink = (url: string) =>
    invoke('open_jx3_link', { url }).catch((e) => toast.error(`打开失败：${e}`));
  const openTarget = (target: 'jx3box' | 'support') =>
    invoke('open_link', { target }).catch((e) => toast.error(`打开失败：${e}`));
  const contact = () => void openTarget('support');

  async function copy(text: string, done: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(done);
    } catch {
      toast.error('复制失败，请手动选中复制');
    }
  }

  const pick = (s: School) => {
    setSchoolId(s.id);
    setPicking(false);
  };

  const papers = column ? entriesFor(column.whitepapers, specName) : [];
  const macros = column ? entriesFor(column.macros, specName) : [];

  return (
    <DialogShell
      className="jx3-card"
      labelId="jx3-column-title"
      headerContent={
        <div className="jx3-header">
          <div>
            <h2 id="jx3-column-title" className="fb-dialog__title">
              剑网三专栏
            </h2>
            <p className="fb-dialog__subtitle">心法白皮书与宏，作者授权的优先展示</p>
          </div>
          <Button variant="link" size="sm" onClick={() => void openTarget('jx3box')}>
            JX3Box ↗
          </Button>
        </div>
      }
      subheader={
        school && (
          <div className="jx3-pickers" data-tour="jx3-pickers">
            <button
              type="button"
              className="jx3-school"
              aria-expanded={picking}
              onClick={() => setPicking((v) => !v)}
            >
              <SchoolDot id={school.id} />
              {school.name}
              <span className="jx3-school__caret" aria-hidden="true">
                ▾
              </span>
            </button>
            {school.specs.length > 1 ? (
              <div className="jx3-segmented" role="group" aria-label="心法">
                {school.specs.map((s) => (
                  <button
                    key={s.name}
                    type="button"
                    className="jx3-segmented__item"
                    aria-pressed={s.name === specName}
                    onClick={() => setSpecName(s.name)}
                  >
                    {s.name}
                  </button>
                ))}
              </div>
            ) : (
              <span className="jx3-spec">{specName}</span>
            )}
          </div>
        )
      }
      footerAlign="spread"
      footer={
        <>
          <span className="jx3-source" title={result ? sourceLine(result) : undefined}>
            {result ? sourceBrief(result) : broken ? '列表数据有误' : '正在读取列表…'}
            <button
              type="button"
              className="jx3-refresh"
              disabled={loading}
              onClick={() => void load(true)}
              aria-label="刷新列表"
              title="刷新列表"
            >
              ⟳
            </button>
          </span>
          <Button onClick={onClose}>关闭</Button>
        </>
      }
    >
      {broken ? (
        <div className="jx3-empty">
          <span>专栏列表的数据有误，暂时显示不了。点底部 ⟳ 重试；一直这样请更新应用。</span>
        </div>
      ) : picking && groups ? (
        <div className="jx3-grid-wrap">
          {[
            { label: '门派', list: groups.sects },
            { label: '流派', list: groups.genres },
          ]
            .filter((g) => g.list.length > 0)
            .map((g) => (
              <div key={g.label} className="jx3-grid-group">
                <p className="jx3-grid-label">{g.label}</p>
                <div className="jx3-grid">
                  {g.list.map((s) => (
                    <button
                      key={s.id}
                      type="button"
                      className={`jx3-chip${s.id === schoolId ? ' jx3-chip--active' : ''}`}
                      onClick={() => pick(s)}
                    >
                      <SchoolDot id={s.id} />
                      {s.name}
                    </button>
                  ))}
                </div>
              </div>
            ))}
        </div>
      ) : (
        <>
          <Section
            key={`papers-${specName}`}
            tour="jx3-papers"
            title="白皮书"
            items={papers}
            what={`${specName ?? ''}白皮书`}
            onContact={contact}
            render={(p) => (
              <li key={p.url} className="jx3-item">
                <button
                  type="button"
                  className="jx3-item__main"
                  onClick={() => void openLink(p.url)}
                >
                  <span className="jx3-item__title">{p.title}</span>
                  <span className="jx3-item__meta">
                    {p.author}
                    <Provenance entry={p} />
                  </span>
                </button>
                <span className="jx3-item__go" aria-hidden="true">
                  ↗
                </span>
              </li>
            )}
          />

          <Section
            key={`macros-${specName}`}
            tour="jx3-macros"
            title="宏"
            items={macros}
            what={`${specName ?? ''}宏`}
            onContact={contact}
            render={(m) => (
              <li key={m.url} className="jx3-item jx3-item--post">
                <div className="jx3-item__row">
                  <button
                    type="button"
                    className="jx3-item__main"
                    onClick={() => void openLink(m.url)}
                  >
                    <span className="jx3-item__title">
                      {m.title}
                      <span className="jx3-tag">{MACRO_KIND_LABELS[m.kind] ?? m.kind}</span>
                    </span>
                    <span className="jx3-item__meta">
                      {m.author}
                      <Provenance entry={m} />
                    </span>
                  </button>
                  <span className="jx3-item__go" aria-hidden="true">
                    ↗
                  </span>
                </div>
                {m.items.some((i) => i.body === undefined) && (
                  <div className="jx3-macros" role="group" aria-label="复制云端宏名">
                    {m.items
                      .filter((i) => i.body === undefined)
                      .map((i) => {
                        const cloud = cloudMacroName(m.author, i.name);
                        return (
                          <button
                            key={i.name}
                            type="button"
                            className="jx3-copy"
                            title={`复制 ${cloud}，在游戏插件里粘贴即可同步云端宏`}
                            onClick={() =>
                              void copy(cloud, `已复制「${cloud}」，在游戏插件里粘贴即可同步云端宏`)
                            }
                          >
                            <span className="jx3-copy__icon" aria-hidden="true">
                              ⧉
                            </span>
                            {i.name}
                          </button>
                        );
                      })}
                  </div>
                )}
                {m.items.map(
                  (i) =>
                    i.body !== undefined && (
                      <div key={i.name} className="jx3-code">
                        <div className="jx3-code__head">
                          <span className="jx3-code__name">{i.name}</span>
                          <button
                            type="button"
                            className="jx3-copy"
                            title="复制宏正文，在游戏的宏编辑里粘贴"
                            onClick={() =>
                              void copy(i.body!, `已复制「${i.name}」，在游戏的宏编辑里粘贴即可`)
                            }
                          >
                            <span className="jx3-copy__icon" aria-hidden="true">
                              ⧉
                            </span>
                            复制
                          </button>
                        </div>
                        <pre className="jx3-code__body">{i.body}</pre>
                      </div>
                    ),
                )}
              </li>
            )}
          />
        </>
      )}
    </DialogShell>
  );
}
