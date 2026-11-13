import './StatusLogo.css';

export default function StatusLogo({
  src,
  active,
  pulse,
  className,
  dragRegion = false,
}: {
  src: string;
  active: boolean;
  pulse: boolean;
  className?: string;
  dragRegion?: boolean;
}) {
  return (
    <span
      className={`status-logo ${active ? 'is-running' : 'is-idle'}${pulse ? ' is-enhanced' : ''}${className ? ` ${className}` : ''}`}
      data-tauri-drag-region={dragRegion || undefined}
    >
      {active && pulse && (
        <>
          <img
            className="status-logo-echo status-logo-echo--first"
            src={src}
            alt=""
            draggable={false}
          />
          <img
            className="status-logo-echo status-logo-echo--second"
            src={src}
            alt=""
            draggable={false}
          />
        </>
      )}
      <img className="status-logo-image" src={src} alt="" draggable={false} />
    </span>
  );
}
