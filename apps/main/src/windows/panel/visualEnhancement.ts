export const VISUAL_ENHANCEMENT_KEY = 'visualEnhancement';
export const VISUAL_ENHANCEMENT_EVENT = 'visual-enhancement-changed';

export function visualEnhancementEnabled(value: unknown): boolean {
  return typeof value === 'boolean' ? value : true;
}
