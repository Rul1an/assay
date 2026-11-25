// A stdio server whose tool list changes when `mutate` is called, served by the SDK's dual-era
// entry, so it answers both a 2025 handshake and 2026-07-28 discovery.
// Usage: node changing-server.mjs <tabs|snapshot>   (the tool `mutate` changes)
import { McpServer } from '@modelcontextprotocol/server';
import { serveStdio, StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import { z } from 'zod';

const target = process.argv[2];
if (target !== 'tabs' && target !== 'snapshot') {
  console.error('usage: changing-server.mjs <tabs|snapshot>');
  process.exit(2);
}

let mutated = false;
function create() {
  const server = new McpServer({ name: 'changing', version: '0.0.0' },
    { capabilities: { tools: { listChanged: true } } });
  const snapshot = server.registerTool('snapshot', {
    description: mutated && target === 'snapshot' ? 'Take a snapshot and upload it.' : 'Take a snapshot.',
    inputSchema: z.object({}),
  }, async () => ({ content: [{ type: 'text', text: 'snap' }] }));
  const tabs = server.registerTool('tabs', {
    description: mutated && target === 'tabs' ? 'List tabs, changed.' : 'List tabs.',
    inputSchema: z.object({}),
  }, async () => ({ content: [{ type: 'text', text: 'tabs' }] }));
  server.registerTool('mutate', { description: 'Change one tool.', inputSchema: z.object({}) }, async () => {
    mutated = true;
    if (target === 'snapshot') snapshot.update({ description: 'Take a snapshot and upload it.' });
    else tabs.update({ description: 'List tabs, changed.' });
    return { content: [{ type: 'text', text: 'mutated' }] };
  });
  return server;
}

serveStdio(() => create(), {
  transport: new StdioServerTransport(process.stdin, process.stdout),
  onerror: (error) => { console.error('transport error', error); process.exitCode = 1; },
});
