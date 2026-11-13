import menuSvg from '../../../assets/icons/menu.svg?raw';
import minimizeSvg from '../../../assets/icons/minimize.svg?raw';
import closeSvg from '../../../assets/icons/close.svg?raw';
import chevronSvg from '../../../assets/icons/chevron.svg?raw';
import editSvg from '../../../assets/icons/edit.svg?raw';
import volumeSvg from '../../../assets/icons/volume.svg?raw';
import updateSvg from '../../../assets/icons/update.svg?raw';
import recordSvg from '../../../assets/icons/record.svg?raw';
import stopSvg from '../../../assets/icons/stop.svg?raw';
import SvgIcon from './SvgIcon';

type IconProps = { size?: number; className?: string };

export const MenuIcon = (p: IconProps) => <SvgIcon src={menuSvg} {...p} />;
export const MinimizeIcon = (p: IconProps) => <SvgIcon src={minimizeSvg} {...p} />;
export const CloseIcon = (p: IconProps) => <SvgIcon src={closeSvg} {...p} />;
export const ChevronIcon = (p: IconProps) => <SvgIcon src={chevronSvg} {...p} />;
export const EditIcon = (p: IconProps) => <SvgIcon src={editSvg} {...p} />;
export const VolumeIcon = (p: IconProps) => <SvgIcon src={volumeSvg} {...p} />;
export const UpdateIcon = (p: IconProps) => <SvgIcon src={updateSvg} {...p} />;
export const RecordIcon = (p: IconProps) => <SvgIcon src={recordSvg} {...p} />;
export const StopIcon = (p: IconProps) => <SvgIcon src={stopSvg} {...p} />;
