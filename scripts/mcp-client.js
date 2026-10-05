#!/usr/bin/env node
// 迷你 MCP 客户端：零依赖，用来本地验收软件备份台账的 --mcp 服务端。
//
// 用法：
//   node scripts/mcp-client.js                 # 连上并跑一遍演示（机器 / must 软件 / 开发环境）
//   node scripts/mcp-client.js tools           # 列出全部工具
//   node scripts/mcp-client.js call list_machines
//   node scripts/mcp-client.js call search_software '{"intent":"must","limit":5}'
//   node scripts/mcp-client.js call get_dev_env_file '{"machine":"REDMIBOOK","file":"vscode-extensions.txt"}'
//
// 指定 exe：环境变量 MCP_EXE，或第一个参数 --exe <路径>。

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

function findExe() {
  if (process.env.MCP_EXE) return process.env.MCP_EXE;
  const root = path.resolve(__dirname, '..');
  const candidates = [
    path.join(root, 'src-tauri', 'target', 'release', 'windows-software-ledger.exe'),
    path.join(root, 'src-tauri', 'target', 'debug', 'windows-software-ledger.exe'),
  ].filter((c) => fs.existsSync(c));
  if (!candidates.length) throw new Error('找不到 exe，请先构建，或用 MCP_EXE 指定路径');
  // 取修改时间最新的那份：避免误用没有 --mcp 的旧 release 构建。
  candidates.sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs);
  return candidates[0];
}

class McpClient {
  constructor(exe) {
    this.child = spawn(exe, ['--mcp'], { stdio: ['pipe', 'pipe', 'pipe'] });
    this.buf = '';
    this.pending = new Map();
    this.nextId = 1;
    this.child.stdout.on('data', (d) => this._onData(d.toString('utf8')));
    this.child.stderr.on('data', (d) => process.stderr.write('[server] ' + d.toString('utf8')));
    this.child.on('error', (e) => {
      console.error('无法启动 MCP 服务端:', e.message);
      process.exit(1);
    });
  }

  _onData(chunk) {
    this.buf += chunk;
    let idx;
    // MCP stdio：一行一个 JSON-RPC 消息
    while ((idx = this.buf.indexOf('\n')) >= 0) {
      const line = this.buf.slice(0, idx).trim();
      this.buf = this.buf.slice(idx + 1);
      if (!line) continue;
      let msg;
      try { msg = JSON.parse(line); } catch { continue; }
      if (msg.id !== undefined && this.pending.has(msg.id)) {
        const { resolve, reject, timer } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        clearTimeout(timer); // 关键：不清掉的话 Node 会一直挂到超时
        if (msg.error) reject(new Error(msg.error.message));
        else resolve(msg.result);
      }
    }
  }

  request(method, params, timeoutMs) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error('请求超时: ' + method));
        }
      }, timeoutMs || 30000);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params: params || {} }) + '\n');
    });
  }

  notify(method, params) {
    this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method, params: params || {} }) + '\n');
  }

  close() {
    try { this.child.stdin.end(); } catch {}
    try { this.child.kill(); } catch {}
  }
}

// 工具的返回是 { content: [{type:'text', text}], isError }。把 JSON 文本解析回来，方便断言。
function unwrap(result) {
  const text = (result && result.content && result.content[0] && result.content[0].text) || '';
  if (result && result.isError) return { error: text };
  try { return JSON.parse(text); } catch { return { text }; }
}

async function callTool(client, name, args) {
  const res = await client.request('tools/call', { name, arguments: args || {} });
  return { raw: res, data: unwrap(res) };
}

function describeClient(init) {
  const cap = Object.keys(init.capabilities || {}).join(', ') || '无';
  console.log(`✔ 已连接：${init.serverInfo.name} v${init.serverInfo.version}｜协议 ${init.protocolVersion}｜能力 ${cap}`);
}

async function demo(client, init) {
  if (init.instructions) {
    console.log('\n— 服务端给 agent 的说明（节选）—');
    console.log(init.instructions.split('\n').slice(0, 2).join('\n'));
  }

  console.log('\n— 1. list_machines —');
  const machines = (await callTool(client, 'list_machines')).data;
  console.log(`当前机器：${machines.current_machine}｜共 ${machines.total_software} 条软件`);
  for (const m of machines.machines || []) {
    console.log(`  ${m.is_current ? '▶' : ' '} ${m.alias || m.machine} (${m.machine}) — ${m.software_count} 条，必须恢复 ${m.must_count}`);
  }

  console.log('\n— 2. search_software { intent: "must", limit: 5 } —');
  const must = (await callTool(client, 'search_software', { intent: 'must', limit: 5 })).data;
  for (const s of must.software || []) {
    const url = s.download_url ? '有链接' : '无链接';
    console.log(`  ${s.name} [${s.type}] 处置=${s.backup_strategy} ${url}`);
  }
  if (must.truncated) console.log('  …（已截断）');

  console.log('\n— 3. 本机缺失但别的机器有的软件（not_on_machine）—');
  const missing = (await callTool(client, 'search_software', {
    not_on_machine: machines.current_machine,
    intent: 'must',
    limit: 8,
  })).data;
  for (const s of missing.software || []) {
    const where = (s.machines || []).map((x) => x.alias || x.machine).join('/');
    console.log(`  ${s.name}  ← 装在 ${where}`);
  }

  console.log('\n— 4. list_dev_env（本机）—');
  const dev = (await callTool(client, 'list_dev_env', { machine: machines.current_machine })).data;
  for (const mac of dev.machines || []) {
    for (const p of mac.providers || []) {
      if (!p.available) continue;
      const files = (p.files || []).map((f) => f.name).join(', ');
      console.log(`  [${p.id}] ${p.summary}${files ? '  files: ' + files : ''}`);
      for (const c of p.restore_commands || []) console.log(`      $ ${c}`);
    }
  }

  console.log('\n全部通过 ✅ —— 这就是 agent 接入后会看到的原始信息。');
}

async function main() {
  const argv = process.argv.slice(2);
  // 允许 `--exe <path>` 出现在任意位置
  const exeFlag = argv.indexOf('--exe');
  let exe;
  if (exeFlag >= 0) {
    exe = argv[exeFlag + 1];
    argv.splice(exeFlag, 2);
  } else {
    exe = findExe();
  }

  const client = new McpClient(exe);
  try {
    let init;
    try {
      init = await client.request('initialize', {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: { name: 'mcp-client.js', version: '0' },
      }, 10000);
    } catch (e) {
      throw new Error(
        `${e.message}。这个 exe 可能是不含 --mcp 的旧版本，请重新构建（scripts\\cargo-msvc.bat build），或用 MCP_EXE 指定新版 exe`
      );
    }
    client.notify('notifications/initialized');
    describeClient(init);

    const [cmd, ...rest] = argv;
    if (cmd === 'tools' || cmd === 'list-tools') {
      const tools = await client.request('tools/list', {});
      console.log('\n可用工具：');
      for (const t of tools.tools || []) console.log(`  • ${t.name} — ${t.description}`);
    } else if (cmd === 'call') {
      const tool = rest[0];
      if (!tool) throw new Error('用法: call <tool> [jsonArgs]');
      const args = rest[1] ? JSON.parse(rest[1]) : {};
      const { raw, data } = await callTool(client, tool, args);
      console.log(`\n${tool} → isError=${!!raw.isError}`);
      console.log(JSON.stringify(data, null, 2));
    } else {
      await demo(client, init);
    }
  } finally {
    client.close();
  }
}

main().catch((e) => {
  console.error('✗ ' + e.message);
  process.exit(1);
});
