import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const root = fileURLToPath(new URL('.', import.meta.url));
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.txt': 'text/plain; charset=utf-8' };
http.createServer(async (req, res) => {
  try {
    const pathname = decodeURIComponent(new URL(req.url, 'http://127.0.0.1').pathname);
    const relative = pathname === '/' ? 'index.html' : pathname.slice(1);
    const target = path.resolve(root, relative);
    if (!target.startsWith(root) || relative.includes('node_modules') || !types[path.extname(target)]) throw new Error('Not found');
    const data = await readFile(target);
    res.writeHead(200, { 'Content-Type': types[path.extname(target)], 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
    res.end(data);
  } catch { res.writeHead(404); res.end('Not found'); }
}).listen(Number(process.env.PORT || 0), '127.0.0.1', function () { console.log(`Quantix UI concept: http://127.0.0.1:${this.address().port}`); });
