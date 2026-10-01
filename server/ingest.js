// server/ingest.js
import fs from 'node:fs';
import path from 'node:path';

const EVIDENCE_DIR = path.resolve(import.meta.dir, '../evidence');
const DATA_DIR = path.resolve(import.meta.dir, '../data');
const SOFTWARE_FILE = path.join(DATA_DIR, 'software.json');

// 噪声与卸载程序过滤规则
const NOISE_NAME_REGEX = /^(update|uninstall|unins\d*|卸载|microsoft visual c\+\+ \d{4}-\d{4} redistributable|windows sdk|microsoft\.net|directx|vulkan run time)/i;
const NOISE_PATH_REGEX = /(unins\d*\.exe|uninstall\.exe|helper\.exe|crashpad_handler\.exe)$/i;

// 常见软件正规化规则 (映射前缀或关键词到通用名称与分类)
const NORMALIZATION_RULES = [
  { match: /7-zip/i, name: '7-Zip', category: '系统工具', type: 'desktop', url: 'https://www.7-zip.org/' },
  { match: /visual studio code|vscode/i, name: 'Visual Studio Code', category: '开发工具', type: 'desktop', url: 'https://code.visualstudio.com/' },
  { match: /antigravity ide/i, name: 'Antigravity IDE', category: '开发工具', type: 'desktop' },
  { match: /git/i, name: 'Git', category: '开发工具', type: 'cli', url: 'https://git-scm.com/' },
  { match: /firefox/i, name: 'Firefox', category: '浏览器与网络', type: 'desktop', url: 'https://www.mozilla.org/firefox/' },
  { match: /google chrome|chrome/i, name: 'Google Chrome', category: '浏览器与网络', type: 'desktop', url: 'https://www.google.com/chrome/' },
  { match: /node\.js|nodejs/i, name: 'Node.js', category: '开发工具', type: 'runtime', url: 'https://nodejs.org/' },
  { match: /python/i, name: 'Python', category: '开发工具', type: 'runtime', url: 'https://www.python.org/' },
  { match: /bun/i, name: 'Bun', category: '开发工具', type: 'runtime', url: 'https://bun.sh/' },
  { match: /rust/i, name: 'Rust (rustup/cargo)', category: '开发工具', type: 'runtime', url: 'https://www.rust-lang.org/' },
  { match: /everything/i, name: 'Everything', category: '系统工具', type: 'desktop', url: 'https://www.voidtools.com/' },
  { match: /potplayer/i, name: 'PotPlayer', category: '媒体娱乐', type: 'desktop' },
  { match: /vlc/i, name: 'VLC Media Player', category: '媒体娱乐', type: 'desktop', url: 'https://www.videolan.org/' },
  { match: /mpc-be/i, name: 'MPC-BE', category: '媒体娱乐', type: 'desktop' },
  { match: /honeyview/i, name: 'Honeyview', category: '媒体娱乐', type: 'desktop' },
  { match: /snipaste/i, name: 'Snipaste', category: '系统工具', type: 'desktop', url: 'https://zh.snipaste.com/' },
  { match: /pixpin/i, name: 'PixPin', category: '系统工具', type: 'desktop' },
  { match: /obsidian/i, name: 'Obsidian', category: '办公与笔记', type: 'desktop', url: 'https://obsidian.md/' },
  { match: /notion/i, name: 'Notion', category: '办公与笔记', type: 'desktop', url: 'https://www.notion.so/' },
  { match: /cherry-studio/i, name: 'Cherry Studio', category: '开发工具', type: 'desktop' },
  { match: /dbeaver/i, name: 'DBeaver', category: '开发工具', type: 'desktop', url: 'https://dbeaver.io/' },
  { match: /navicat/i, name: 'Navicat', category: '开发工具', type: 'desktop' },
  { match: /docker/i, name: 'Docker Desktop', category: '开发工具', type: 'desktop', url: 'https://www.docker.com/' },
  { match: /qbittorrent/i, name: 'qBittorrent', category: '浏览器与网络', type: 'desktop', url: 'https://www.qbittorrent.org/' },
  { match: /steam/i, name: 'Steam', category: '媒体娱乐', type: 'desktop', url: 'https://store.steampowered.com/' },
  { match: /wechat|微信/i, name: '微信 (WeChat)', category: '通讯与社交', type: 'desktop' },
  { match: /telegram/i, name: 'Telegram', category: '通讯与社交', type: 'desktop', url: 'https://telegram.org/' },
  { match: /qq/i, name: 'QQ', category: '通讯与社交', type: 'desktop' },
  { match: /图吧工具箱/i, name: '图吧工具箱', category: '系统工具', type: 'portable' },
  { match: /hibit uninstaller/i, name: 'HiBit Uninstaller', category: '系统工具', type: 'desktop' },
  { match: /angry ip scanner/i, name: 'Angry IP Scanner', category: '系统工具', type: 'desktop' },
  { match: /afterchat/i, name: 'AfterChat', category: '开发工具', type: 'desktop' }
];

// 智能判断类别
function guessCategory(name) {
  const n = name.toLowerCase();
  if (n.includes('sdk') || n.includes('compiler') || n.includes('git') || n.includes('code') || n.includes('ide') || n.includes('node') || n.includes('python')) {
    return '开发工具';
  }
  if (n.includes('player') || n.includes('media') || n.includes('music') || n.includes('video') || n.includes('audio') || n.includes('game')) {
    return '媒体娱乐';
  }
  if (n.includes('browser') || n.includes('torrent') || n.includes('download') || n.includes('network') || n.includes('ssh') || n.includes('ftp')) {
    return '浏览器与网络';
  }
  if (n.includes('note') || n.includes('office') || n.includes('pdf') || n.includes('doc') || n.includes('excel')) {
    return '办公与笔记';
  }
  return '系统工具';
}

// 规范化名称提取
function normalizeName(rawName) {
  let clean = rawName
    .replace(/\s*\(x64\)|\s*\(64-bit\)|\s*\(32-bit\)|\s*\(User\)|\s*版本\s*[\d\.]+/gi, '')
    .trim();

  for (const rule of NORMALIZATION_RULES) {
    if (rule.match.test(clean)) {
      return {
        name: rule.name,
        category: rule.category,
        type: rule.type || 'desktop',
        download_url: rule.url || ''
      };
    }
  }

  return {
    name: clean,
    category: guessCategory(clean),
    type: 'desktop',
    download_url: ''
  };
}

export function runIngest() {
  if (!fs.existsSync(EVIDENCE_DIR)) {
    return { success: false, message: 'Evidence directory does not exist' };
  }

  // 1. 读取既有数据 (保留用户已经做过的决策)
  let existingItems = [];
  if (fs.existsSync(SOFTWARE_FILE)) {
    try {
      existingItems = JSON.parse(fs.readFileSync(SOFTWARE_FILE, 'utf-8'));
    } catch (e) {
      console.error('Error reading existing software.json:', e);
    }
  }

  // 创建以统一小写名称为索引的映射表
  const softwareMap = new Map();
  for (const item of existingItems) {
    softwareMap.set(item.name.toLowerCase().trim(), item);
  }

  const machineDirs = fs.readdirSync(EVIDENCE_DIR, { withFileTypes: true })
    .filter(d => d.isDirectory())
    .map(d => d.name);

  let nextIdCounter = existingItems.reduce((max, item) => {
    const num = parseInt((item.id || '').replace('SW-', ''), 10);
    return isNaN(num) ? max : Math.max(max, num);
  }, 0);

  // 2. 遍历各机器证据
  for (const machineId of machineDirs) {
    const mDir = path.join(EVIDENCE_DIR, machineId);

    // A. 注册表安装项
    const regFile = path.join(mDir, 'registry-apps.json');
    if (fs.existsSync(regFile)) {
      try {
        const apps = JSON.parse(fs.readFileSync(regFile, 'utf-8'));
        for (const app of apps) {
          if (!app.name || NOISE_NAME_REGEX.test(app.name)) continue;
          if (app.uninstall_string && NOISE_PATH_REGEX.test(app.uninstall_string) && !app.install_location) {
            // 如果仅有卸载程序且没有安装目录，检查名称
          }
          addOrUpdateItem(app.name, {
            machine_id: machineId,
            form: 'installed',
            version: app.version || '',
            install_location: app.install_location || '',
            publisher: app.publisher || ''
          });
        }
      } catch (e) {
        console.error(`Failed to parse ${regFile}:`, e);
      }
    }

    // B. 便携软件扫描
    const portableFile = path.join(mDir, 'portable-apps.json');
    if (fs.existsSync(portableFile)) {
      try {
        const portables = JSON.parse(fs.readFileSync(portableFile, 'utf-8'));
        for (const port of portables) {
          if (!port.name) continue;
          addOrUpdateItem(port.name, {
            machine_id: machineId,
            form: 'portable',
            version: port.version || '',
            install_location: port.folder_path || '',
            main_exe: port.main_exe || ''
          }, 'portable');
        }
      } catch (e) {}
    }

    // C. 桌面与开始菜单快捷方式
    const shortcutFile = path.join(mDir, 'shortcuts.json');
    if (fs.existsSync(shortcutFile)) {
      try {
        const shortcuts = JSON.parse(fs.readFileSync(shortcutFile, 'utf-8'));
        for (const sc of shortcuts) {
          if (!sc.name || !sc.target_path) continue;
          if (NOISE_PATH_REGEX.test(sc.target_path) || NOISE_NAME_REGEX.test(sc.name)) continue;
          if (sc.name.startsWith('卸载') || sc.name.toLowerCase().includes('uninstall')) continue;

          addOrUpdateItem(sc.name, {
            machine_id: machineId,
            form: sc.target_path.toLowerCase().includes('portable') ? 'portable' : 'shortcut',
            install_location: sc.target_path,
            link_file: sc.link_file
          });
        }
      } catch (e) {}
    }
  }

  function addOrUpdateItem(rawName, machineDetail, forcedType = null) {
    const norm = normalizeName(rawName);
    const key = norm.name.toLowerCase().trim();

    let item = softwareMap.get(key);
    if (!item) {
      nextIdCounter++;
      item = {
        id: `SW-${String(nextIdCounter).padStart(3, '0')}`,
        name: norm.name,
        category: norm.category,
        type: forcedType || norm.type,
        version: machineDetail.version || '',
        machines: [],
        restore_intent: 'unreviewed', // must | should | on_demand | drop | unreviewed
        backup_strategy: 'none',      // copy_dir | copy_config | redownload | sync_account | none
        download_url: norm.download_url || '',
        config_notes: '',
        is_awesome: false,
        awesome_role: '',
        created_at: new Date().toISOString()
      };
      softwareMap.set(key, item);
    } else if (!item.version && machineDetail.version) {
      item.version = machineDetail.version;
    }

    // 检查该机器是否已记录
    const existingM = item.machines.find(m => m.machine_id === machineDetail.machine_id);
    if (!existingM) {
      item.machines.push(machineDetail);
    } else {
      // 补充缺失的路径或版本
      if (!existingM.version && machineDetail.version) existingM.version = machineDetail.version;
      if (!existingM.install_location && machineDetail.install_location) existingM.install_location = machineDetail.install_location;
      if (machineDetail.form === 'portable') existingM.form = 'portable';
    }
  }

  // 3. 排序并写入 software.json
  const finalItems = Array.from(softwareMap.values()).sort((a, b) => {
    // 必须恢复排前面，然后按类别、名称排序
    const intentOrder = { must: 1, should: 2, on_demand: 3, unreviewed: 4, drop: 5 };
    const diff = (intentOrder[a.restore_intent] || 4) - (intentOrder[b.restore_intent] || 4);
    if (diff !== 0) return diff;
    return a.name.localeCompare(b.name, 'zh-CN');
  });

  if (!fs.existsSync(DATA_DIR)) {
    fs.mkdirSync(DATA_DIR, { recursive: true });
  }

  fs.writeFileSync(SOFTWARE_FILE, JSON.stringify(finalItems, null, 2), 'utf-8');
  console.log(`[Ingest] Ingested ${finalItems.length} software entries into ${SOFTWARE_FILE}`);

  return {
    success: true,
    total: finalItems.length,
    machines: machineDirs
  };
}

if (import.meta.main) {
  runIngest();
}
