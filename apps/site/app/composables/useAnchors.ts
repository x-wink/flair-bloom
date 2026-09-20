/** 跳转后还认多久的高度变化；更新公告走代理拉清单，慢的时候两三秒才落地 */
const WATCH_MS = 4000;

/** 这些动作一出现就算用户接管了滚动，别再跟人抢。不监听 scroll：平滑跳转本身就在滚 */
const HANDOVER = ['wheel', 'touchstart', 'pointerdown', 'keydown'] as const;

/**
 * 页内锚点的两件事：
 *
 * 一是跳完纠偏。更新公告是在浏览器里异步读的，读到之前那一片只有一行字，列表渲染出来
 * （或手动展开某条）会把下面的区块整体往下推；原生 hash 跳转只算一次落点，跳完内容再长高
 * 就偏了。所以跳完盯一小会儿，目标位置一变就重新贴齐，超时或用户一动手就撒手。
 *
 * 二是清掉过期的 hash。hash 表达的是「刚才点导航要去哪」的意图，用户自己滚走后意图就过期，
 * 留着只会让刷新跳回一个早已离开的位置；清掉后位置恢复交给浏览器原生的 scrollRestoration。
 * 判据是目标整块滚出视口，不是 @xwink/ui ScrollRail 那种「占住视口中部」——本页最后一节
 * 不满一屏，永远占不到中部，那套判据会在刚跳到就把 hash 抹掉。
 */
export function useAnchors() {
  onMounted(() => {
    let controller: AbortController | undefined;

    /** 目标在文档里的位置（不随滚动变），上面的内容长高它才会变 */
    const documentTop = (target: Element) => target.getBoundingClientRect().top + window.scrollY;

    const align = (target: Element) => {
      controller?.abort();
      controller = new AbortController();
      const { signal } = controller;

      // 逐帧比目标在文档里的位置，而不是观察某个盒子的尺寸：撑开它的可能是任意一层祖先，
      // ResizeObserver 盯 body 盯不到（本页插进 main 里的内容就没让 body 的 content-box 变过）
      let anchored = documentTop(target);
      const tick = () => {
        if (signal.aborted) return;
        const now = documentTop(target);
        if (Math.abs(now - anchored) > 1) {
          anchored = now;
          // 纠偏不是导航，平滑滚会跟正在进行的那次跳转打架，这里直接贴齐
          target.scrollIntoView({ behavior: 'instant', block: 'start' });
        }
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);

      const timer = setTimeout(() => controller?.abort(), WATCH_MS);
      signal.addEventListener('abort', () => clearTimeout(timer));
      for (const type of HANDOVER) {
        window.addEventListener(type, () => controller?.abort(), { signal, passive: true });
      }
    };

    let stale: IntersectionObserver | undefined;

    const watchStale = (target: Element) => {
      stale?.disconnect();
      // 平滑跳转途中目标还没进视口，那会儿的「不可见」不算离开，得先看见过
      let arrived = false;
      stale = new IntersectionObserver(([entry]) => {
        if (entry?.isIntersecting) {
          arrived = true;
          return;
        }
        if (!arrived) return;
        history.replaceState(undefined, '', location.pathname + location.search);
        stale?.disconnect();
        stale = undefined;
      });
      stale.observe(target);
    };

    const follow = () => {
      const hash = location.hash;
      const target = hash.length > 1 ? document.querySelector(hash) : undefined;
      if (!target) return;
      align(target);
      watchStale(target);
    };

    window.addEventListener('hashchange', follow);
    follow();

    onBeforeUnmount(() => {
      window.removeEventListener('hashchange', follow);
      controller?.abort();
      stale?.disconnect();
    });
  });
}
