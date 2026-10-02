// scripts/dev-server.mjs
// 仅供 `tauri dev` 使用的静态服务器：把 client/ 跑在 1420 端口，并在改动时
// 通过 SSE 通知所有页面自动刷新。这样改前端不用重新编译 Rust。
//   启动：bun scripts/dev-server.mjs
// release 构建不使用本文件（release 走 tauri.conf.json 的 frontendDist 内嵌）。

import { watch } from 'node:fs';
import { stat, readFile } from 'node:fs/promises';
import { extname, join, normalize, resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '..');
const CLIENT = join(ROOT, 'client');
const PORT = Number(process.env.DEV_PORT || 1420);

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.map': 'application/json; charset=utf-8',
};

const RELOAD_SNIPPET =
  '<script>try{const es=new EventSource("/__livereload");es.onmessage=()=>location.reload();}catch(e){}</script>';

// 记录每个打开的页面，用于广播刷新
const clients = new Set();

function broadcast() {
  for (const ctrl of clients) {
    try { ctrl.enqueue('data: reload\n\n'); } catch { /* 已断开 */ }
  }
}

const server = (() => {
  try {
    return Bun.serve({
      port: PORT,
      hostname: '127.0.0.1',
      async fetch(req) {
        const url = new URL(req.url);

        if (url.pathname === '/__livereload') {
          let ctrl;
          const stream = new ReadableStream({
            start(c) { ctrl = c; clients.add(ctrl); c.enqueue(': connected\n\n'); },
            cancel() { clients.delete(ctrl); },
          });
          return new Response(stream, {
            headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', Connection: 'keep-alive' },
          });
        }

        let pathname = decodeURIComponent(url.pathname);
        if (pathname.endsWith('/')) pathname += 'index.html';
        const file = normalize(join(CLIENT, pathname));
        if (!file.startsWith(CLIENT)) return new Response('forbidden', { status: 403 });

        try {
          const info = await stat(file);
          if (info.isDirectory()) return Response.redirect(url.pathname + '/', 302);
          let body = await readFile(file);
          if (extname(file).toLowerCase() === '.html') {
            const html = body.toString('utf8');
            body = html.includes('__livereload') ? html : html.replace(/<\/body>/i, RELOAD_SNIPPET + '</body>');
          }
          return new Response(body, {
            headers: { 'Content-Type': MIME[extname(file).toLowerCase()] || 'application/octet-stream', 'Cache-Control': 'no-store' },
          });
        } catch {
          return new Response('404 Not Found', { status: 404, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
        }
      },
    });
  } catch (e) {
    // 端口被占用：多半是上一轮 dev 的 server 还在。直接复用它，让 tauri dev 继续。
    if (e && (e.code === 'EADDRINUSE' || String(e).includes('EADDRINUSE'))) {
      console.log(`[dev] port ${PORT} already in use — reusing the running dev server, nothing to do.`);
      process.exit(0);
    }
    throw e;
  }
})();

watch(CLIENT, { recursive: true }, (event, filename) => {
  if (filename && /(^|[\\/])\./.test(filename)) return; // 忽略隐藏文件
  console.log(`[dev] changed: ${filename || '(unknown)'} -> reload`);
  broadcast();
});

console.log(`[dev] serving ${CLIENT}`);
console.log(`[dev] http://127.0.0.1:${server.port}  (edit client/ → auto reload, no rebuild)`);
