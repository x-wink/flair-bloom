// 真机验证用的 CDP 小工具：连上 WebView2 的远程调试端口操作面板窗口。
//
// 前置：应用要带远程调试端口启动（需要管理员的路径就在管理员终端里起，应用直接继承权限，不走提权重启）
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm dev
//
// 用法（Node 24 直接跑 .mts，不需要 tsx）：
//   node scripts/cdp.mts eval "<js 表达式>"   在面板页里求值，支持 async IIFE，结果按值返回
//   node scripts/cdp.mts key Escape           发一次真实按键（keyDown + keyUp）
//   node scripts/cdp.mts shot out.png         截图落盘
//
// 元素定位一律用 [data-tour="..."]——引导教程的锚点同时也是测试锚点。
// 点击用 eval 里的 element.click()：它不受 CSS pointer-events 影响，hover 才显形的按钮也点得到。
// 看可访问性树、按元素点这类操作更适合走 .mcp.json 里的 Playwright（同一个端口），本脚本留给精确取值。

interface CdpPage {
  url: string;
  webSocketDebuggerUrl: string;
}

interface CdpResponse {
  id: number;
  result?: Record<string, unknown>;
  error?: unknown;
}

const PORT = process.env.FB_CDP_PORT ?? '9222';
const [, , cmd, arg = ''] = process.argv;

let list: CdpPage[];
try {
  list = (await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json()) as CdpPage[];
} catch {
  console.error(`连不上 127.0.0.1:${PORT}，应用是不是没带 --remote-debugging-port 启动？`);
  process.exit(1);
}
// 同一端口下还有浮窗 panel-float.html，按 url 后缀挑面板
const page = list.find((p) => p.url.endsWith('/panel.html'));
if (!page) {
  console.error('找不到 panel.html 页面，应用起来了吗？');
  process.exit(1);
}

const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const pending = new Map<
  number,
  { resolve: (v: Record<string, unknown>) => void; reject: (e: Error) => void }
>();

function send(
  method: string,
  params: Record<string, unknown> = {},
): Promise<Record<string, unknown>> {
  const msgId = ++id;
  ws.send(JSON.stringify({ id: msgId, method, params }));
  return new Promise((resolve, reject) => {
    pending.set(msgId, { resolve, reject });
    setTimeout(() => reject(new Error(`${method} 超时`)), 15000);
  });
}

ws.addEventListener('message', (e) => {
  const msg = JSON.parse(String(e.data)) as CdpResponse;
  const slot = pending.get(msg.id);
  if (!slot) return;
  pending.delete(msg.id);
  if (msg.error) slot.reject(new Error(JSON.stringify(msg.error)));
  else slot.resolve(msg.result ?? {});
});

await new Promise((r) => ws.addEventListener('open', r));

if (cmd === 'eval') {
  const res = await send('Runtime.evaluate', {
    expression: arg,
    awaitPromise: true,
    returnByValue: true,
  });
  const details = res.exceptionDetails as { exception?: { description?: string } } | undefined;
  if (details) {
    console.error('异常：', details.exception?.description ?? JSON.stringify(details));
    process.exitCode = 1;
  } else {
    console.log(JSON.stringify((res.result as { value?: unknown }).value, null, 2));
  }
} else if (cmd === 'key') {
  // 必须走 Input.dispatchKeyEvent：合成 KeyboardEvent 直接 dispatch 到 window 会让 capture 与
  // bubble 监听器在 at-target 阶段按注册顺序跑，「capture 先行截断」这类语义就验不出来。
  const base = { key: arg, code: arg, windowsVirtualKeyCode: arg === 'Escape' ? 27 : 0 };
  await send('Input.dispatchKeyEvent', { type: 'keyDown', ...base });
  await send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
  console.log('sent', arg);
} else if (cmd === 'shot') {
  const res = await send('Page.captureScreenshot', { format: 'png' });
  const { writeFileSync } = await import('node:fs');
  writeFileSync(arg, Buffer.from(res.data as string, 'base64'));
  console.log('saved', arg);
} else {
  console.error('用法：node scripts/cdp.mts <eval|key|shot> <参数>');
  process.exitCode = 1;
}

ws.close();
