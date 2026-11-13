import './RuleWarning.css';

export default function RuleWarning({
  title,
  className = '',
}: {
  title: string;
  className?: string;
}) {
  return (
    <span className={`rule-warning ${className}`} role="img" aria-label={title} title={title}>
      ⚠️
    </span>
  );
}
