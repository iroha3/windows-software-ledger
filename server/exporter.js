// server/exporter.js
import fs from 'node:fs';
import path from 'node:path';

const DATA_DIR = path.resolve(import.meta.dir, '../data');
const EXPORTS_DIR = path.resolve(import.meta.dir, '../exports');
const SOFTWARE_FILE = path.join(DATA_DIR, 'software.json');

const INTENT_LABELS = {
  must: '🔴 必须恢复 (Must Restore)',
  should: '🟡 建议恢复 (Should Restore)',
  on_demand: '🔵 用到再装 (On Demand)',
  drop: '⚫ 淘汰弃用 (Drop / Deprecate)',
  unreviewed: '⚪ 待确认 (Unreviewed)'
};

const STRATEGY_LABELS = {
  copy_dir: '📦 保留/压缩整个目录',
  copy_config: '⚙️ 手动导出/备份配置',
  redownload: '🌐 重新下载安装',
  sync_account: '☁️ 账号登录同步',
  none: '➖ 无需操作'
};

export function exportChecklists() {
  if (!fs.existsSync(SOFTWARE_FILE)) {
    return { success: false, message: 'software.json does not exist' };
  }

  const items = JSON.parse(fs.readFileSync(SOFTWARE_FILE, 'utf-8'));
  if (!fs.existsSync(EXPORTS_DIR)) {
    fs.mkdirSync(EXPORTS_DIR, { recursive: true });
  }

  // 1. 生成 RECOVERY_CHECKLIST.md
  const groups = {
    must: [],
    should: [],
    on_demand: [],
    unreviewed: [],
    drop: []
  };

  const backupTasks = [];
  const awesomeItems = [];

  for (const item of items) {
    const intent = item.restore_intent || 'unreviewed';
    if (groups[intent]) {
      groups[intent].push(item);
    } else {
      groups.unreviewed.push(item);
    }

    if (item.backup_strategy === 'copy_dir' || item.backup_strategy === 'copy_config') {
      backupTasks.push(item);
    }

    if (item.is_awesome) {
      awesomeItems.push(item);
    }
  }

  const checklistLines = [
    '# 电脑重装恢复备忘清单 (Recovery Checklist)',
    '',
    `> 生成时间: ${new Date().toLocaleString('zh-CN')}  `,
    `> 统计概览: 软件总数 **${items.length}** | 必须恢复 **${groups.must.length}** | 建议恢复 **${groups.should.length}** | 用到再装 **${groups.on_demand.length}** | 待确认 **${groups.unreviewed.length}** | 待备份资产 **${backupTasks.length}**`,
    '',
    '---',
    '',
    '## ⚠️ 重装前必须备份的资产清单 (Pre-install Backup Tasks)',
    '',
    backupTasks.length === 0
      ? '*暂无标记为需要打包目录或导出配置的软件。在软件备份台账中将处置方式标记为「保留/压缩目录」或「导出配置」后将在此列出。*'
      : '| 软件名称 | 处置方式 | 机器分布与路径 | 备份备忘与配置说明 |\n|---|---|---|---|\n' +
        backupTasks.map(t => {
          const paths = t.machines.map(m => `\`${m.machine_id}\`: ${m.install_location || m.path || '未记录路径'}`).join('<br>');
          return `| **${t.name}** | ${STRATEGY_LABELS[t.backup_strategy] || t.backup_strategy} | ${paths} | ${t.config_notes || '—'} |`;
        }).join('\n'),
    '',
    '---',
    '',
    '## 一、' + INTENT_LABELS.must,
    '',
    groups.must.length === 0 ? '*暂无必须恢复的软件*' : renderGroupChecklist(groups.must),
    '',
    '## 二、' + INTENT_LABELS.should,
    '',
    groups.should.length === 0 ? '*暂无建议恢复的软件*' : renderGroupChecklist(groups.should),
    '',
    '## 三、' + INTENT_LABELS.on_demand,
    '',
    groups.on_demand.length === 0 ? '*暂无*' : renderCompactList(groups.on_demand),
    '',
    '## 四、' + INTENT_LABELS.drop,
    '',
    groups.drop.length === 0 ? '*暂无*' : renderDropList(groups.drop)
  ];

  const checklistPath = path.join(EXPORTS_DIR, 'RECOVERY_CHECKLIST.md');
  fs.writeFileSync(checklistPath, checklistLines.join('\n'), 'utf-8');

  // 2. 生成 AWESOME_LIST.md
  const awesomeLines = [
    '# 个人工作流精选软件资产库 (Awesome Software List)',
    '',
    `> 汇编时间: ${new Date().toLocaleString('zh-CN')}  `,
    `> 精选收录: **${awesomeItems.length}** 款核心工具`,
    '',
    '这份清单记录了深度融入日常开发与生产力工作流的高价值工具。',
    '',
    awesomeItems.length === 0
      ? '*暂无收录。在软件备份台账表格中为认可的软件点亮星标 ★ 即可收录至此。*'
      : renderAwesomeList(awesomeItems)
  ];

  const awesomePath = path.join(EXPORTS_DIR, 'AWESOME_LIST.md');
  fs.writeFileSync(awesomePath, awesomeLines.join('\n'), 'utf-8');

  return {
    success: true,
    checklistPath,
    awesomePath,
    recoveryListPath: checklistPath,
    awesomeListPath: awesomePath,
    checklistFilename: 'RECOVERY_CHECKLIST.md',
    awesomeFilename: 'AWESOME_LIST.md',
    checklistContent: checklistLines.join('\n'),
    awesomeContent: awesomeLines.join('\n'),
    stats: {
      total: items.length,
      must: groups.must.length,
      should: groups.should.length,
      backupTasks: backupTasks.length,
      awesome: awesomeItems.length
    }
  };
}

function renderGroupChecklist(items) {
  // 按分类归类
  const cats = {};
  for (const item of items) {
    const cat = item.category || '未分类';
    if (!cats[cat]) cats[cat] = [];
    cats[cat].push(item);
  }

  const out = [];
  for (const [cat, list] of Object.entries(cats)) {
    out.push(`### 📂 ${cat}`);
    for (const item of list) {
      const verText = item.version ? ` \`${item.version}\`` : '';
      const urlText = item.download_url ? ` [官网/下载](${item.download_url})` : '';
      const machinesText = item.machines.map(m => m.machine_id).join(', ');
      const stratText = item.backup_strategy && item.backup_strategy !== 'none'
        ? ` *(处置: ${STRATEGY_LABELS[item.backup_strategy] || item.backup_strategy})*`
        : '';
      const notesText = item.config_notes ? `\n  - 💡 **备注/配置说明**: ${item.config_notes}` : '';
      const checkMark = item.prep_status === 'ready' ? '[x]' : '[ ]';
      const readyBadge = item.prep_status === 'ready' ? ' *(已就绪)*' : '';

      out.push(`- ${checkMark} **${item.name}**${verText} (${item.type || 'desktop'})${urlText} — 机器: \`${machinesText}\`${stratText}${readyBadge}${notesText}`);
    }
    out.push('');
  }
  return out.join('\n');
}

function renderCompactList(items) {
  return items.map(item => {
    const url = item.download_url ? ` ([链接](${item.download_url}))` : '';
    return `- [ ] **${item.name}** (${item.category})${url}`;
  }).join('\n') + '\n';
}

function renderDropList(items) {
  return items.map(item => `- ❌ ~~${item.name}~~ (${item.category})`).join('\n') + '\n';
}

function renderAwesomeList(items) {
  const cats = {};
  for (const item of items) {
    const cat = item.category || '常用工具';
    if (!cats[cat]) cats[cat] = [];
    cats[cat].push(item);
  }

  const out = [];
  for (const [cat, list] of Object.entries(cats)) {
    out.push(`## 📌 ${cat}\n`);
    for (const item of list) {
      const url = item.download_url ? ` - [官方站点](${item.download_url})` : '';
      const role = item.awesome_role ? `\n> **工作流定位**: ${item.awesome_role}` : '';
      const notes = item.config_notes ? `\n- **实践经验**: ${item.config_notes}` : '';
      out.push(`### ${item.name}${url}${role}${notes}\n`);
    }
  }
  return out.join('\n');
}

if (import.meta.main) {
  const res = exportChecklists();
  console.log('Export result:', res);
}
