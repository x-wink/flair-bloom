function applyBrandColor(color: string) {
  const style = document.documentElement.style;
  for (const [name, value] of Object.entries(brandVariables(color))) {
    style.setProperty(name, value);
  }
}

export function useBrandColor() {
  const color = useState('brand-color', () => defaultBrandColor);

  onMounted(() => {
    try {
      const stored = localStorage.getItem(BRAND_STORAGE_KEY);
      if (isBrandPreset(stored)) color.value = stored;
    } catch {
      // 隐私模式等场景读不到存储，保持默认色
    }
  });

  function select(next: string) {
    if (!isBrandPreset(next)) return;
    color.value = next;
    applyBrandColor(next);
    try {
      localStorage.setItem(BRAND_STORAGE_KEY, next);
    } catch {
      // 存不下只影响下次打开，本次切换已生效
    }
  }

  return { color, select };
}
