// scripts/test_llm.js
// 本地 LLM 提示词单测与探测脚本
// 运行方式: bun run scripts/test_llm.js [软件名称] [路径] [LLM地址]
// 示例: bun run scripts/test_llm.js "PotPlayer" "D:\\Tools\\PotPlayer\\PotPlayer64.exe"

const args = process.argv.slice(2);
const softwareName = args[0] || 'PotPlayer';
const softwarePath = args[1] || 'D:\\Software\\PotPlayer\\PotPlayer64.exe';
const llmUrl = args[2] || process.env.LLM_URL || 'https://api.deepseek.com/chat/completions';
const modelName = process.env.LLM_MODEL || args[3] || 'deepseek-flash';
const apiKey = process.env.LLM_API_KEY || args[4] || '';

console.log('==================================================');
console.log(` 测试 LLM 软件分析提示词`);
console.log(` 目标模型: ${modelName} @ ${llmUrl}`);
if (apiKey) console.log(` API Key: 已配置 (${apiKey.slice(0, 4)}...${apiKey.slice(-4)})`);
console.log(` 软件名称: ${softwareName}`);
console.log(` 探测路径: ${softwarePath}`);
console.log('==================================================');

// 提示词模板 (可在此自由调整调优)
const prompt = `你是一个 Windows 软件与系统重装迁移专家。请根据给出的软件名称与路径线索，分析并返回标准的分类与配置建议。
软件名称：${softwareName}
已记录路径：${softwarePath}

请输出严格的 JSON 格式（不要输出任何多余的 Markdown 或前后缀，只返回一个标准 JSON 对象）：
{
  "category": "媒体娱乐",
  "type": "desktop",
  "restore_intent": "must",
  "download_url": "https://potplayer.daum.net/",
  "config_notes": "若为便携版通常配置在同级 PotPlayerMini64.ini，安装版可能在注册表 HKCU\\\\Software\\\\Daum\\\\PotPlayer64"
}

枚举约束说明：
- category: 必须从 [开发工具, 系统工具, 浏览器与网络, 媒体娱乐, 办公与笔记, 通讯与社交, 其他] 中选一个
- type: 必须从 [desktop, portable, cli, runtime] 中选一个
- restore_intent: 必须从 [must, on_demand, drop] 中选一个
- download_url: 软件官网或可靠下载页
- config_notes: 简要说明配置文件通常存放在何处（如 AppData、~/.config 或安装目录），或者是否依赖云同步
`;

async function main() {
  const startTime = performance.now();
  try {
    const fetchHeaders = { 'Content-Type': 'application/json' };
    if (apiKey && apiKey.trim()) {
      fetchHeaders['Authorization'] = `Bearer ${apiKey.trim()}`;
    }

    const requestBody = {
      model: modelName,
      messages: [{ role: 'user', content: prompt }],
      temperature: 0.1
    };
    // DeepSeek 默认开启思考模式；此处显式关闭，仅对 DeepSeek 端点下发，
    // 避免 OpenAI / 本地 LM Studio 因未知参数报错。
    if (llmUrl.includes('api.deepseek.com')) {
      requestBody.thinking = { type: 'disabled' };
    }

    console.log(` 思考模式: ${requestBody.thinking ? '已关闭 (thinking.disabled)' : '未指定'}`);

    const res = await fetch(llmUrl, {
      method: 'POST',
      headers: fetchHeaders,
      body: JSON.stringify(requestBody),
      signal: AbortSignal.timeout(20000)
    });

    const elapsed = Math.round(performance.now() - startTime);

    if (!res.ok) {
      console.error(`❌ HTTP 错误: ${res.status} ${res.statusText}`);
      const errText = await res.text();
      console.error(errText);
      return;
    }

    const data = await res.json();
    const rawContent = data.choices?.[0]?.message?.content || '';
    const cleanJsonStr = rawContent.replace(/```json/g, '').replace(/```/g, '').trim();

    console.log(`⏱️ 响应耗时: ${elapsed} ms`);
    console.log('\n--- 原始回复 ---');
    console.log(rawContent);

    console.log('\n--- JSON 解析结果 ---');
    try {
      const parsed = JSON.parse(cleanJsonStr);
      console.dir(parsed, { depth: null, colors: true });
      console.log('\n✅ 格式校验通过！');
    } catch (e) {
      console.error('❌ JSON 解析失败:', e.message);
    }
  } catch (err) {
    console.error('❌ 请求失败:', err.message);
    console.error('💡 提示: DeepSeek 请确认 API Key 与网络；本地端点请确认服务已启动且模型名正确。');
  }
}

main();
