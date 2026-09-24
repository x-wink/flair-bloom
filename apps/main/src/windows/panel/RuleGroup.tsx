import type { DragEvent, ReactNode } from 'react';
import { ChevronIcon, EditIcon } from './components/icons';

interface Props {
  name: string;
  /** 最后一个组容器，承载教程锚点 `group-latest` / `group-header`。 */
  isLatest: boolean;
  collapsed: boolean;
  /** 正在重命名时的草稿；undefined 表示未在编辑。 */
  draft: string | undefined;
  dragActive: boolean;
  /** 筛选状态下不接受拖入：被隐藏的成员看不见，拖进去的位置不可预期。 */
  acceptDrop: boolean;
  hasHold: boolean;
  running: string[];
  paused: string[];
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
  acceptDrop,
  hasHold,
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
    >
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
        {!editing && (
          <span className="group-rule-text">
            同一时间只跑一条{hasHold ? '，长按松手后恢复' : ''}
          </span>
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
          title="解散分组（规则保留）"
        >
          解散
        </button>
      </div>
      <div
        className="group-status"
        data-tour={isLatest ? 'group-header' : undefined}
        aria-live="polite"
      >
        {running.length === 0 && paused.length === 0 ? (
          <span className="group-status-idle">未在运行</span>
        ) : (
          <>
            {running.map((label) => (
              <span key={`r-${label}`} className="group-status-item is-running">
                ● {label} 在跑
              </span>
            ))}
            {paused.map((label) => (
              <span key={`p-${label}`} className="group-status-item is-paused">
                ⏸ {label} 暂停
              </span>
            ))}
          </>
        )}
      </div>
      {!collapsed && (
        <div className="group-body">
          {children}
          {hiddenCount > 0 && <div className="group-hidden-note">另有 {hiddenCount} 条已隐藏</div>}
          {acceptDrop && (
            <div className={`group-drop-zone${dragActive ? ' drag-active' : ''}`}>拖入规则</div>
          )}
        </div>
      )}
    </div>
  );
}
