import type { DragEvent, ReactNode } from 'react';
import { ChevronIcon, EditIcon } from './components/icons';
import GradientDropZone from './components/GradientDropZone';

interface StatusItem {
  id: string;
  label: string;
}

interface Props {
  name: string;
  /**
   * 教程聚焦的组，承载锚点 `group-latest` / `group-header`：有示例组时是示例组（规则组教程
   * 讲的就是它），否则是最后一个组。
   */
  isLatest: boolean;
  collapsed: boolean;
  /** 正在重命名时的草稿；undefined 表示未在编辑。 */
  draft: string | undefined;
  dragActive: boolean;
  appendActive: boolean;
  globalEnabled: boolean;
  showDropZone: boolean;
  onAppendDragOver: () => void;
  onDragLeave: () => void;
  /** 隐藏部分成员时不接受拖入，避免放置位置不可预期。 */
  acceptDrop: boolean;
  /** 组内在跑 / 暂停的规则；同组可能有两条同键位规则，故带 id 作 key。 */
  running: StatusItem[];
  paused: StatusItem[];
  /** 被筛选隐藏的成员数。 */
  hiddenCount: number;
  children: ReactNode;
  onToggleCollapse: () => void;
  onStartRename: () => void;
  onDraftChange: (draft: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onDisband: () => void;
  onDragOver: () => void;
  onDrop: (e: DragEvent<HTMLDivElement>) => void;
}

export default function RuleGroup({
  name,
  isLatest,
  collapsed,
  draft,
  dragActive,
  appendActive,
  globalEnabled,
  showDropZone,
  onAppendDragOver,
  onDragLeave,
  acceptDrop,
  running,
  paused,
  hiddenCount,
  children,
  onToggleCollapse,
  onStartRename,
  onDraftChange,
  onCommitRename,
  onCancelRename,
  onDisband,
  onDragOver,
  onDrop,
}: Props) {
  const editing = draft !== undefined;
  const dropHandlers = acceptDrop
    ? {
        onDragOver: (e: DragEvent<HTMLDivElement>) => {
          e.preventDefault();
          e.stopPropagation();
          onDragOver();
        },
        onDrop: (e: DragEvent<HTMLDivElement>) => {
          e.preventDefault();
          e.stopPropagation();
          onDrop(e);
        },
      }
    : {};

  return (
    <div
      className={`rule-group-container${dragActive && acceptDrop ? ' drag-active' : ''}`}
      data-tour={isLatest ? 'group-latest' : undefined}
      {...dropHandlers}
      onDragLeave={(event) => {
        const hovered = document.elementFromPoint(event.clientX, event.clientY);
        if (!hovered || !event.currentTarget.contains(hovered)) onDragLeave();
      }}
    >
      <div className="rule-group-heading" data-tour={isLatest ? 'group-header' : undefined}>
        <div
          className={`rule-group-header${collapsed ? ' collapsed' : ''}`}
          onClick={() => !editing && onToggleCollapse()}
        >
          {editing ? (
            <input
              className="group-name-edit"
              autoFocus
              value={draft}
              onChange={(e) => onDraftChange(e.target.value)}
              onBlur={onCommitRename}
              onKeyDown={(e) => {
                if (e.key === 'Enter') onCommitRename();
                else if (e.key === 'Escape') onCancelRename();
              }}
              onClick={(e) => e.stopPropagation()}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => e.preventDefault()}
            />
          ) : (
            <div className={`group-collapse-indicator${collapsed ? ' collapsed' : ''}`}>
              <ChevronIcon size={12} className="group-chevron" />
              <span className="group-name-text">{name}</span>
            </div>
          )}
          {globalEnabled && (
            <div className="group-status" aria-live="polite">
              {running.length === 0 && paused.length === 0 ? (
                <span className="group-status-idle">未在运行</span>
              ) : (
                <>
                  {running.map(({ id, label }) => (
                    <span key={id} className="group-status-item is-running" title={`${label} 在跑`}>
                      ● {label} 在跑
                    </span>
                  ))}
                  {paused.map(({ id, label }) => (
                    <span key={id} className="group-status-item is-paused" title={`${label} 暂停`}>
                      ⏸ {label} 暂停
                    </span>
                  ))}
                </>
              )}
            </div>
          )}
          {!editing && (
            <button
              className="group-edit-btn"
              onClick={(e) => {
                e.stopPropagation();
                onStartRename();
              }}
              title="重命名"
            >
              <EditIcon size={12} />
            </button>
          )}
          <button
            className="disband-btn"
            onClick={(e) => {
              e.stopPropagation();
              onDisband();
            }}
            title="解散规则组（可选择同时删除组内规则）"
          >
            解散
          </button>
        </div>
      </div>
      {!collapsed && (
        <div className="group-body">
          {children}
          {hiddenCount > 0 && <div className="group-hidden-note">另有 {hiddenCount} 条已隐藏</div>}
          {acceptDrop && showDropZone && (
            <GradientDropZone
              active={appendActive}
              label={dragActive ? '松手放置' : '拖入规则'}
              ariaLabel={`将规则放到「${name}」末尾`}
              size="sm"
              onDragOver={onAppendDragOver}
              onDrop={onDrop}
            />
          )}
        </div>
      )}
    </div>
  );
}
