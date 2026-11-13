import StatusLogo from './StatusLogo';
import { ChainIcon, TargetLogo } from './WindowBindingControl';
import { bindingDescription, targetIsForeground, type WindowBindingStatus } from '../windowBinding';

export default function BindingLogos({
  src,
  status,
  enabled,
  pulse,
}: {
  src: string;
  status: WindowBindingStatus | undefined;
  enabled: boolean;
  pulse: boolean;
}) {
  if (!status?.supported || !status.binding)
    return (
      <StatusLogo className="fb-float-logo" src={src} active={enabled} pulse={pulse} dragRegion />
    );
  return (
    <span
      className="binding-logos"
      title={bindingDescription(status, enabled)}
      role="img"
      aria-label={bindingDescription(status, enabled)}
      data-tauri-drag-region
    >
      <StatusLogo className="fb-float-logo" src={src} active={enabled} pulse={pulse} />
      <span className="binding-chain">
        <ChainIcon />
      </span>
      <TargetLogo
        icon={status.target?.icon}
        active={targetIsForeground(status)}
        pulse={pulse && enabled}
      />
    </span>
  );
}
