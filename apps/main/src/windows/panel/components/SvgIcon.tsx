interface Props {
  src: string;
  size?: number;
  className?: string;
}

export default function SvgIcon({ src, size, className }: Props) {
  return (
    <span
      aria-hidden="true"
      className={className}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        ...(size !== undefined && { fontSize: size }),
      }}
      dangerouslySetInnerHTML={{ __html: src }}
    />
  );
}
