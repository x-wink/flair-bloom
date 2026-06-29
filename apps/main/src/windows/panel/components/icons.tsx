import menuSvg from '../../../assets/icons/menu.svg?raw';
import minimizeSvg from '../../../assets/icons/minimize.svg?raw';
import closeSvg from '../../../assets/icons/close.svg?raw';
import chevronSvg from '../../../assets/icons/chevron.svg?raw';
import editSvg from '../../../assets/icons/edit.svg?raw';
import volumeSvg from '../../../assets/icons/volume.svg?raw';
import SvgIcon from './SvgIcon';

type IconProps = { size?: number; className?: string };

export const MenuIcon = (p: IconProps) => <SvgIcon src={menuSvg} {...p} />;
export const MinimizeIcon = (p: IconProps) => <SvgIcon src={minimizeSvg} {...p} />;
export const CloseIcon = (p: IconProps) => <SvgIcon src={closeSvg} {...p} />;
export const ChevronIcon = (p: IconProps) => <SvgIcon src={chevronSvg} {...p} />;
export const EditIcon = (p: IconProps) => <SvgIcon src={editSvg} {...p} />;
export const VolumeIcon = (p: IconProps) => <SvgIcon src={volumeSvg} {...p} />;
