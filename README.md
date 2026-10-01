# backup-software-list-tools

> 面向 3 台电脑重装与迁移的轻量、低摩擦软件决策中台与清单管理工具。

---

## ⚡ 快速开始

本项目由 **Bun** 驱动，启动耗时仅需数毫秒。

```bash
# 启动本地决策中台服务
bun start
```

启动后在浏览器打开：[http://localhost:3000](http://localhost:3000)

---

## ⌨️ 极速操作与快捷键

表格支持多选后使用**纯键盘流**进行决策打标，杜绝页面卡顿与抖动：

| 快捷键 | 功能 | 说明 |
|---|---|---|
| <kbd>1</kbd> | 🔴 必须恢复 | 将选中的软件标记为必须安装 |
| <kbd>2</kbd> | 🟡 建议恢复 | 标记为建议恢复 |
| <kbd>3</kbd> | 🔵 用到再装 | 标记为按需安装 |
| <kbd>4</kbd> | ⚫ 淘汰弃用 | 标记为淘汰弃用 |
| <kbd>5</kbd> | ⚪ 恢复默认 | 重置所选项为待定状态 |
| <kbd>6</kbd> | ✅ 设为已就绪 | 标记准备进度为已就绪 |
| <kbd>7</kbd> | 🤖 AI 智能预判 | 调用本地 LLM 自动补全分类、意愿与配置说明 |
| <kbd>8</kbd> | 🔗 合并选中项 | 将重复条目归并为一条并汇总各机器路径 |
| <kbd>Del</kbd> × 2 | 🗑️ 快速删除 | **2 秒内连按两次 Delete** 直接删除（无弹窗打扰） |
| <kbd>Esc</kbd> | 取消勾选 | 一键清空当前选择 |

---

## 🤖 本地 LLM 辅助与提示词测试

系统支持调用本地模型（如 LM Studio 运行的 `qwen3.5-4b`）进行智能预填。

### 1. 独立单测与提示词调优脚本
我们提供了一个完全独立的探测脚本 [scripts/test_llm.js](file:///E:/Projects/backup-software-list-tools/scripts/test_llm.js)，方便手动调优提示词：

```bash
# 测试指定软件的 AI 预判
bun run scripts/test_llm.js "PotPlayer" "D:\Software\PotPlayer\PotPlayer64.exe"

# 指定其它局域网 IP / 端口测试
bun run scripts/test_llm.js "Git" "C:\Program Files\Git" "http://192.168.1.100:1234/v1/chat/completions"
```

### 2. 跨机与局域网 LLM 配置
在 Web 界面右上角点击 **「⚙️ LLM 设置」**，可直接修改 LLM API 地址。如果其它没有 GPU 的电脑访问本中台，只需填入主机的局域网 IP（例如 `http://192.168.1.100:1234/v1/chat/completions`）即可共用主机的本地模型。

---

## 🖥️ 3 台电脑的端到端工作流

### 1. 本机（已完成扫描）
- 打开网页中台，点击顶部 **「🔄 扫描本机」** 即可一键重新抓取本机注册表、便携软件与快捷方式并自动入库。

### 2. 另外两台电脑采集
1. 将 [scripts](file:///E:/Projects/backup-software-list-tools/scripts) 目录拷至 U 盘或共享盘。
2. 在目标机器上双击运行 `collect.bat`（零依赖，调用 Windows 原生 PowerShell）。
3. 脚本执行完成后，会在同级生成 `evidence/<电脑名>/` 目录。
4. 将该文件夹复制到本项目的 `evidence/` 目录下。
5. 刷新或点击网页中台，即可在顶部机器 Tabs 中直接切换查看各台机器分布。

---

## 📄 产物导出

在页面右上角点击 **「📄 导出 Markdown 清单」**，将即时生成两份高价值落地文件：

1. **[RECOVERY_CHECKLIST.md](file:///E:/Projects/backup-software-list-tools/exports/RECOVERY_CHECKLIST.md)**:
   - ⚠️ 重装前必须备份的资产汇总表（列出所有需要打包的便携目录与配置文件路径）。
   - 按意愿分组的重装待办清单（已就绪项自动打勾，带官网链接与配置备忘）。
2. **[AWESOME_LIST.md](file:///E:/Projects/backup-software-list-tools/exports/AWESOME_LIST.md)**:
   - 沉淀长期个人精选工作流软件库（点亮每行开头的星标 ★ 即可收录）。
