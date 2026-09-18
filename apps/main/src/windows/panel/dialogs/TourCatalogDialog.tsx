import Button from '../components/Button';
import type { TourDef, TourSnapshot } from '../tour/types';
import DialogShell from './DialogShell';
import './TourCatalogDialog.css';

interface Props {
  tours: TourDef[];
  completed: string[];
  snapshot: TourSnapshot;
  onStart: (id: string) => void;
  onClose: () => void;
}

export default function TourCatalogDialog({ tours, completed, snapshot, onStart, onClose }: Props) {
  return (
    <DialogShell
      className="tour-catalog-card"
      title="新手教程"
      subtitle="每组几分钟，随时可退出，走完会打勾"
      labelId="tour-catalog-title"
      footer={<Button onClick={onClose}>关闭</Button>}
    >
      <ul className="tour-catalog-list">
        {tours.map((tour) => {
          const done = completed.includes(tour.id);
          const reason = tour.available?.(snapshot);
          return (
            <li key={tour.id} className={`tour-catalog-row${reason ? ' unavailable' : ''}`}>
              <span className={`tour-catalog-mark${done ? ' done' : ''}`} aria-hidden="true">
                {done ? '✓' : '○'}
              </span>
              <div className="tour-catalog-main">
                <span className="tour-catalog-title">{tour.title}</span>
                <span className={`tour-catalog-desc${reason ? ' reason' : ''}`}>
                  {reason ?? tour.summary}
                </span>
              </div>
              <span className="tour-catalog-steps">{tour.steps.length} 步</span>
              <Button
                variant="outline"
                tone={done ? 'neutral' : 'primary'}
                size="sm"
                // 用 aria-disabled 而非 disabled：不可用的原因已经显示在 summary 位置，
                // 但 disabled 会让按钮连 hover 提示都不给，与面板里布局按钮的做法保持一致。
                aria-disabled={reason !== undefined}
                title={reason}
                onClick={() => {
                  if (reason !== undefined) return;
                  onStart(tour.id);
                }}
              >
                {done ? '重新学习' : '开始'}
              </Button>
            </li>
          );
        })}
      </ul>
    </DialogShell>
  );
}
