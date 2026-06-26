import { createServer } from 'http';
import { readFile } from 'fs/promises';
import { resolve } from 'path';

const PORT = process.env.PORT || 3000;
const staticRoot = resolve(process.cwd(), 'web', 'public');

const server = createServer(async (req, res) => {
    const url = req.url ?? '/';
    const urlPath = url === '/' ? '/index.html' : url;
    const filePath = resolve(staticRoot, '.' + urlPath!);
    try {
        const data = await readFile(filePath);
        // Simple content type handling for a few extensions
        const ext = filePath.split('.').pop();
        const mime: Record<string, string> = {
            html: 'text/html',
            css: 'text/css',
            js: 'application/javascript',
        };
        const contentType = mime[ext ?? ''] || 'application/octet-stream';
        res.writeHead(200, { 'Content-Type': contentType });
        res.end(data);
    } catch (e) {
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('Not found');
    }
});

server.listen(PORT, () => {
    console.log(`⚡️  MegaGate core server listening on http://localhost:${PORT}`);
});
