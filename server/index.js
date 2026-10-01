// server/index.js
import fs from 'node:fs';
import path from 'node:path';
import { runIngest } from './ingest.js';
import { exportChecklists } from './exporter.js';

const PORT = 3000;
const ROOT_DIR = path.resolve(import.meta.dir, '..');
const CLIENT_DIR = path.join(ROOT_DIR, 'client');
const DATA_DIR = path.join(ROOT_DIR, 'data');
const SOFTWARE_FILE = path.join(DATA_DIR, 'software.json');
const CONFIG_FILE = path.join(DATA_DIR, 'config.json');
const EVIDENCE_DIR = path.join(ROOT_DIR, 'evidence');

const DEFAULT_CONFIG = {
  llm_url: process.env.LLM_URL || 'http://127.0.0.1:1234/v1/chat/completions',
  llm_model: process.env.LLM_MODEL || 'qwen3.5-4b',
  llm_api_key: process.env.LLM_API_KEY || '',
  scan_directories: [
    'D:\\Portable',
    'D:\\Tools',
    'D:\\Software',
    'E:\\Portable',
    'E:\\Tools',
    'E:\\Software',
    'C:\\Software',
    'C:\\Portable'
  ],
  machine_aliases: {
    'DESKTOP-HEGVCTR': '台式工作站'
  }
};

function getConfig() {
  if (!fs.existsSync(CONFIG_FILE)) {
    return { ...DEFAULT_CONFIG };
  }
  try {
    return { ...DEFAULT_CONFIG, ...JSON.parse(fs.readFileSync(CONFIG_FILE, 'utf-8')) };
  } catch (e) {
    return { ...DEFAULT_CONFIG };
  }
}

function saveConfig(cfg) {
  if (!fs.existsSync(DATA_DIR)) {
    fs.mkdirSync(DATA_DIR, { recursive: true });
  }
  fs.writeFileSync(CONFIG_FILE, JSON.stringify(cfg, null, 2), 'utf-8');
}

function getSoftwareData() {
  if (!fs.existsSync(SOFTWARE_FILE)) return [];
  try {
    return JSON.parse(fs.readFileSync(SOFTWARE_FILE, 'utf-8'));
  } catch (e) {
    console.error('Failed to parse software.json', e);
    return [];
  }
}

function saveSoftwareData(data) {
  if (!fs.existsSync(DATA_DIR)) {
    fs.mkdirSync(DATA_DIR, { recursive: true });
  }
  fs.writeFileSync(SOFTWARE_FILE, JSON.stringify(data, null, 2), 'utf-8');
}

function getMachinesList() {
  const set = new Set();
  if (fs.existsSync(EVIDENCE_DIR)) {
    fs.readdirSync(EVIDENCE_DIR, { withFileTypes: true })
      .filter(d => d.isDirectory())
      .forEach(d => set.add(d.name));
  }
  const software = getSoftwareData();
  for (const s of software) {
    for (const m of s.machines || []) {
      if (m.machine_id) set.add(m.machine_id);
    }
  }
  const aliases = getConfig().machine_aliases || {};
  Object.keys(aliases).forEach(id => set.add(id));
  return Array.from(set);
}

const server = Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);
    const pathname = url.pathname;

    const headers = {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type'
    };

    if (req.method === 'OPTIONS') {
      return new Response(null, { headers });
    }

    try {
      // 获取配置
      if (pathname === '/api/config' && req.method === 'GET') {
        return Response.json(getConfig(), { headers });
      }

      // 更新配置 (如切换局域网 IP / 端口)
      if (pathname === '/api/config' && req.method === 'POST') {
        const body = await req.json();
        const current = getConfig();
        const updated = { ...current, ...body };
        saveConfig(updated);
        return Response.json({ success: true, config: updated }, { headers });
      }

      // 获取状态与汇总
      if (pathname === '/api/status' && req.method === 'GET') {
        const software = getSoftwareData();
        const machines = getMachinesList();
        
        const pathSet = new Set();
        for (const s of software) {
          for (const m of s.machines || []) {
            const p = m.install_location || m.path || '';
            if (p) {
              if (p.includes('\\AppData\\Local')) pathSet.add('AppData\\Local');
              else if (p.includes('\\AppData\\Roaming')) pathSet.add('AppData\\Roaming');
              else if (p.toLowerCase().startsWith('c:\\program files (x86)')) pathSet.add('C:\\Program Files (x86)');
              else if (p.toLowerCase().startsWith('c:\\program files')) pathSet.add('C:\\Program Files');
              else if (p.toLowerCase().startsWith('c:\\software')) pathSet.add('C:\\Software');
              else if (p.startsWith('D:\\')) pathSet.add('D:\\');
              else if (p.startsWith('E:\\')) pathSet.add('E:\\');
              else if (p.startsWith('C:\\')) pathSet.add('C:\\');
            }
          }
        }

        const stats = {
          total: software.length,
          must: software.filter(s => s.restore_intent === 'must').length,
          should: software.filter(s => s.restore_intent === 'should').length,
          on_demand: software.filter(s => s.restore_intent === 'on_demand').length,
          drop: software.filter(s => s.restore_intent === 'drop').length,
          unreviewed: software.filter(s => !s.restore_intent || s.restore_intent === 'unreviewed').length,
          awesome: software.filter(s => s.is_awesome).length,
          backupTasks: software.filter(s => s.backup_strategy === 'copy_dir' || s.backup_strategy === 'copy_config').length,
          ready: software.filter(s => s.prep_status === 'ready').length
        };
        return Response.json({ machines, stats, availablePaths: Array.from(pathSet).sort(), machine_aliases: getConfig().machine_aliases || {} }, { headers });
      }

      if (pathname === '/api/software' && req.method === 'GET') {
        const software = getSoftwareData();
        return Response.json(software, { headers });
      }

      // 单项更新
      if (pathname === '/api/software/update' && req.method === 'POST') {
        const { id, updates } = await req.json();
        const software = getSoftwareData();
        const index = software.findIndex(s => s.id === id);
        if (index === -1) {
          return Response.json({ success: false, message: 'Item not found' }, { status: 404, headers });
        }
        software[index] = { ...software[index], ...updates };
        saveSoftwareData(software);
        return Response.json({ success: true, item: software[index] }, { headers });
      }

      // 批量更新
      if (pathname === '/api/software/batch-update' && req.method === 'POST') {
        const { ids, updates } = await req.json();
        const software = getSoftwareData();
        let count = 0;
        for (const item of software) {
          if (ids.includes(item.id)) {
            Object.assign(item, updates);
            count++;
          }
        }
        saveSoftwareData(software);
        return Response.json({ success: true, count }, { headers });
      }

      // 批量删除
      if (pathname === '/api/software/delete' && req.method === 'POST') {
        const { ids } = await req.json();
        let software = getSoftwareData();
        software = software.filter(s => !ids.includes(s.id));
        saveSoftwareData(software);
        return Response.json({ success: true, remaining: software.length }, { headers });
      }

      // 批量添加软件
      if (pathname === '/api/software/batch-add' && req.method === 'POST') {
        const { names, restore_intent } = await req.json();
        const software = getSoftwareData();
        const maxId = software.reduce((max, s) => {
          const num = parseInt((s.id || '').replace('SW-', ''), 10);
          return isNaN(num) ? max : Math.max(max, num);
        }, 0);

        let currentId = maxId;
        const newItems = [];

        for (const rawName of names) {
          const cleanName = rawName.trim();
          if (!cleanName) continue;
          currentId++;
          const item = {
            id: `SW-${String(currentId).padStart(3, '0')}`,
            name: cleanName,
            version: '',
            category: '系统工具',
            type: 'desktop',
            machines: [{ machine_id: 'DESKTOP-HEGVCTR', form: 'manual' }],
            restore_intent: restore_intent || 'must',
            backup_strategy: 'none',
            prep_status: 'todo',
            has_config: false,
            download_url: '',
            config_notes: '',
            is_awesome: false,
            awesome_role: '',
            is_new: true,
            created_at: new Date().toISOString()
          };
          newItems.push(item);
          software.unshift(item);
        }

        saveSoftwareData(software);
        return Response.json({ success: true, items: newItems }, { headers });
      }

      // 合并多项条目
      if (pathname === '/api/software/merge' && req.method === 'POST') {
        const { targetId, mergeIds } = await req.json();
        let software = getSoftwareData();
        const target = software.find(s => s.id === targetId);
        if (!target) {
          return Response.json({ success: false, message: 'Target not found' }, { status: 404, headers });
        }

        const itemsToMerge = software.filter(s => mergeIds.includes(s.id));
        for (const item of itemsToMerge) {
          for (const m of item.machines) {
            if (!target.machines.some(tm => tm.machine_id === m.machine_id && tm.install_location === m.install_location)) {
              target.machines.push(m);
            }
          }
          if (!target.version && item.version) target.version = item.version;
          if (!target.download_url && item.download_url) target.download_url = item.download_url;
          if (item.has_config) target.has_config = true;
          if (item.config_notes) {
            target.config_notes = (target.config_notes ? target.config_notes + '; ' : '') + item.config_notes;
          }
        }

        software = software.filter(s => !mergeIds.includes(s.id));
        saveSoftwareData(software);
        return Response.json({ success: true, target }, { headers });
      }

      // 本地 LLM 分析辅助 (使用可配置的 LLM 端点)
      if (pathname === '/api/llm/analyze' && req.method === 'POST') {
        const { id, name, paths, category } = await req.json();
        const cfg = getConfig();

        const prompt = `你是一个 Windows 软件与系统重装迁移专家。请根据给出的软件名称与路径线索，分析并返回标准的分类与配置建议。
软件名称：${name}
已记录路径：${paths || '未记录路径'}
当前分类：${category || '未知'}

请输出严格的 JSON 格式（不要输出任何多余的 Markdown 或前后缀，只返回一个标准 JSON 对象）：
{
  "category": "开发工具",
  "type": "desktop",
  "restore_intent": "must",
  "backup_strategy": "redownload",
  "download_url": "https://...",
  "config_notes": "配置位置说明与迁移备忘"
}

枚举约束说明：
- category: 必须从 [开发工具, 系统工具, 浏览器与网络, 媒体娱乐, 办公与笔记, 通讯与社交, 其他] 中选一个
- type: 必须从 [desktop, portable, cli, runtime] 中选一个
- restore_intent: 必须从 [must, should, on_demand, drop] 中选一个
- backup_strategy: 必须从 [copy_dir, copy_config, redownload, sync_account, none] 中选一个
- download_url: 软件官网或可靠下载页
- config_notes: 简要说明配置文件通常存放在何处（如 AppData、~/.config 或安装目录），或者是否依赖云同步
`;

        try {
          // 允许只填服务基址 (如 https://api.deepseek.com)，自动补全为 chat/completions 端点
          let llmEndpoint = (cfg.llm_url || '').trim();
          if (llmEndpoint && !llmEndpoint.includes('/chat/completions')) {
            llmEndpoint = llmEndpoint.replace(/\/+$/, '') + '/chat/completions';
          }

          const fetchHeaders = { 'Content-Type': 'application/json' };
          if (cfg.llm_api_key && cfg.llm_api_key.trim()) {
            fetchHeaders['Authorization'] = `Bearer ${cfg.llm_api_key.trim()}`;
          }

          const llmRes = await fetch(llmEndpoint, {
            method: 'POST',
            headers: fetchHeaders,
            body: JSON.stringify({
              model: cfg.llm_model,
              messages: [{ role: 'user', content: prompt }],
              temperature: 0.1
            }),
            signal: AbortSignal.timeout(18000)
          });

          if (!llmRes.ok) {
            return Response.json({ success: false, error: `LLM 服务返回状态码: ${llmRes.status}` }, { status: 502, headers });
          }

          const llmData = await llmRes.json();
          let rawContent = llmData.choices?.[0]?.message?.content || '{}';
          rawContent = rawContent.replace(/```json/g, '').replace(/```/g, '').trim();

          const parsed = JSON.parse(rawContent);
          return Response.json({ success: true, suggestion: parsed, id }, { headers });
        } catch (err) {
          console.error('[LLM Error]', err);
          return Response.json({ success: false, error: `无法连接 LLM [${cfg.llm_url}]: ` + err.message }, { status: 500, headers });
        }
      }

      // 一键扫描本机
      if (pathname === '/api/scan' && req.method === 'POST') {
        const cfg = getConfig();
        const scriptPath = path.join(ROOT_DIR, 'scripts', 'collect.ps1');
        const customDirs = Array.isArray(cfg.scan_directories) ? cfg.scan_directories.join(',') : '';

        const spawnArgs = ['pwsh', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', scriptPath];
        if (customDirs) {
          spawnArgs.push('-CustomPortableDirs', customDirs);
        }

        const proc = Bun.spawn(spawnArgs, {
          cwd: ROOT_DIR,
          stdout: 'pipe',
          stderr: 'pipe'
        });

        const output = await new Response(proc.stdout).text();
        const error = await new Response(proc.stderr).text();
        const exitCode = await proc.exited;

        if (exitCode !== 0) {
          return Response.json({ success: false, error }, { status: 500, headers });
        }

        const ingestRes = runIngest();
        return Response.json({ success: true, output, ingestRes }, { headers });
      }

      // 导出 Markdown
      if (pathname === '/api/export' && req.method === 'POST') {
        const res = exportChecklists();
        return Response.json(res, { headers });
      }

      // 静态资源托管
      let filePath = pathname === '/' ? path.join(CLIENT_DIR, 'index.html') : path.join(CLIENT_DIR, pathname);
      if (pathname.startsWith('/evidence/')) {
        filePath = path.join(ROOT_DIR, pathname);
      }

      if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
        const file = Bun.file(filePath);
        return new Response(file, {
          headers: {
            ...headers,
            'Cache-Control': 'no-cache, no-store, must-revalidate'
          }
        });
      }

      return new Response('Not Found', { status: 404, headers });
    } catch (err) {
      console.error('[Server Error]', err);
      return Response.json({ success: false, error: err.message }, { status: 500, headers });
    }
  }
});

const currentCfg = getConfig();
console.log(`==================================================`);
console.log(` 软件备份台账已在本地启动: http://localhost:${PORT}`);
console.log(` 数据文件: ${SOFTWARE_FILE}`);
console.log(` LLM 端点: ${currentCfg.llm_url} (模型: ${currentCfg.llm_model})`);
console.log(`==================================================`);
