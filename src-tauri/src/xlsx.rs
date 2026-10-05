//! 极简 xlsx 生成器（零依赖）。
//!
//! xlsx 本质是一个 ZIP 包，内含若干固定的 XML。这里只用 STORE（不压缩），
//! 所有单元格走 inlineStr，省去 sharedStrings 的复杂度。
//! 只服务「导出软件清单」这一种用途，不追求通用。

/// 生成单表 xlsx 的字节内容。`rows[0]` 视为表头；`title` / `meta` 渲染在表头之上。
pub fn build(
    sheet_name: &str,
    title: Option<&str>,
    meta: &[String],
    rows: &[Vec<String>],
    col_widths: &[f64],
) -> Vec<u8> {
    let mut zip = ZipWriter::new();
    zip.add("[Content_Types].xml", CONTENT_TYPES.as_bytes());
    zip.add("_rels/.rels", ROOT_RELS.as_bytes());
    zip.add("xl/workbook.xml", workbook_xml(sheet_name).as_bytes());
    zip.add("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.as_bytes());
    zip.add("xl/styles.xml", STYLES.as_bytes());
    zip.add(
        "xl/worksheets/sheet1.xml",
        sheet_xml(title, meta, rows, col_widths).as_bytes(),
    );
    zip.finish()
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

const WORKBOOK_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;

// 四个字体（常规 / 加粗 / 加粗大号 / 灰色小号）、三个填充（none、gray125、表头灰底）——OOXML 要求前两个填充固定。
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="4"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="14"/><name val="Calibri"/></font><font><sz val="10"/><color rgb="FF808080"/><name val="Calibri"/></font></fonts><fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FFEDF1F3"/><bgColor indexed="64"/></patternFill></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="4"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1"/><xf numFmtId="0" fontId="2" fillId="0" borderId="0" xfId="0" applyFont="1"/><xf numFmtId="0" fontId="3" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#;

fn workbook_xml(sheet_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{}" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        esc(sheet_name)
    )
}

fn sheet_xml(title: Option<&str>, meta: &[String], rows: &[Vec<String>], col_widths: &[f64]) -> String {
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let mut x = String::with_capacity(rows.len() * ncols * 48 + 1024);
    x.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    x.push_str(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#);

    // 表头之上：标题 + 元信息。冻结这部分与表头，数据区自动筛选。
    let prefix = title.is_some() as usize + meta.len();
    let header_row = prefix + 1;
    let data_start = header_row + 1;
    x.push_str(&format!(
        r#"<sheetViews><sheetView workbookViewId="0"><pane ySplit="{}" topLeftCell="A{}" activePane="bottomLeft" state="frozen"/><selection pane="bottomLeft" activeCell="A{}" sqref="A{}"/></sheetView></sheetViews>"#,
        header_row, data_start, data_start, data_start
    ));
    x.push_str(r#"<sheetFormatPr defaultRowHeight="15"/>"#);
    if !col_widths.is_empty() {
        x.push_str("<cols>");
        for (i, w) in col_widths.iter().enumerate() {
            x.push_str(&format!(
                r#"<col min="{}" max="{}" width="{}" customWidth="1"/>"#,
                i + 1,
                i + 1,
                w
            ));
        }
        x.push_str("</cols>");
    }

    x.push_str("<sheetData>");
    let mut r = 1usize;
    if let Some(t) = title {
        x.push_str(&cell_row(r, &[(0, t, 2)]));
        r += 1;
    }
    for m in meta {
        x.push_str(&cell_row(r, &[(0, m.as_str(), 3)]));
        r += 1;
    }
    // 表头：即使某列标题为空也写
    let header: &[String] = rows.first().map(|v| v.as_slice()).unwrap_or(&[]);
    let header_cells: Vec<(usize, &str, u32)> =
        header.iter().enumerate().map(|(i, v)| (i, v.as_str(), 1)).collect();
    x.push_str(&cell_row(r, &header_cells));
    r += 1;
    // 数据：跳过空单元格，减小体积
    for row in rows.iter().skip(1) {
        let cells: Vec<(usize, &str, u32)> = row
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.is_empty())
            .map(|(i, v)| (i, v.as_str(), 0))
            .collect();
        x.push_str(&cell_row(r, &cells));
        r += 1;
    }
    x.push_str("</sheetData>");
    if rows.len() > 1 {
        x.push_str(&format!(
            r#"<autoFilter ref="A{}:{}{}"/>"#,
            header_row,
            col_name(ncols - 1),
            r - 1
        ));
    }
    x.push_str("</worksheet>");
    x
}

/// 序列化一行；`cells` 为 (0 基列号, 文本, 样式索引)。
fn cell_row(row: usize, cells: &[(usize, &str, u32)]) -> String {
    let mut s = format!(r#"<row r="{}">"#, row);
    for (ci, val, style) in cells {
        let sattr = if *style == 0 { String::new() } else { format!(r#" s="{}""#, style) };
        s.push_str(&format!(
            r#"<c r="{}{}"{} t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
            col_name(*ci),
            row,
            sattr,
            esc(val)
        ));
    }
    s.push_str("</row>");
    s
}

/// 0 基列号 → 列名：0→A，25→Z，26→AA。
fn col_name(mut i: usize) -> String {
    let mut s = String::new();
    i += 1;
    while i > 0 {
        let rem = (i - 1) % 26;
        s.insert(0, (b'A' + rem as u8) as char);
        i = (i - 1) / 26;
    }
    s
}

/// XML 文本转义，并剔除 XML 1.0 不允许的控制字符（Excel 会因此拒绝打开）。
fn esc(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 最小 ZIP 写入（仅 STORE）
// ---------------------------------------------------------------------------

const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = 22561; // 2024-01-01，固定值即可

struct ZipWriter {
    local: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl ZipWriter {
    fn new() -> Self {
        Self { local: Vec::new(), central: Vec::new(), count: 0 }
    }

    fn add(&mut self, name: &str, data: &[u8]) {
        let crc = crc32(data);
        let size = data.len() as u32;
        let name_bytes = name.as_bytes();
        let offset = self.local.len() as u32;

        // 本地文件头
        self.local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        self.local.extend_from_slice(&20u16.to_le_bytes()); // 解压所需版本
        self.local.extend_from_slice(&0x0800u16.to_le_bytes()); // 文件名 UTF-8
        self.local.extend_from_slice(&0u16.to_le_bytes()); // 压缩方法 STORE
        self.local.extend_from_slice(&DOS_TIME.to_le_bytes());
        self.local.extend_from_slice(&DOS_DATE.to_le_bytes());
        self.local.extend_from_slice(&crc.to_le_bytes());
        self.local.extend_from_slice(&size.to_le_bytes());
        self.local.extend_from_slice(&size.to_le_bytes());
        self.local.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        self.local.extend_from_slice(&0u16.to_le_bytes()); // extra 长度
        self.local.extend_from_slice(name_bytes);
        self.local.extend_from_slice(data);

        // 中央目录
        self.central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        self.central.extend_from_slice(&20u16.to_le_bytes()); // 创建版本
        self.central.extend_from_slice(&20u16.to_le_bytes()); // 解压所需版本
        self.central.extend_from_slice(&0x0800u16.to_le_bytes());
        self.central.extend_from_slice(&0u16.to_le_bytes());
        self.central.extend_from_slice(&DOS_TIME.to_le_bytes());
        self.central.extend_from_slice(&DOS_DATE.to_le_bytes());
        self.central.extend_from_slice(&crc.to_le_bytes());
        self.central.extend_from_slice(&size.to_le_bytes());
        self.central.extend_from_slice(&size.to_le_bytes());
        self.central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        self.central.extend_from_slice(&0u16.to_le_bytes()); // extra
        self.central.extend_from_slice(&0u16.to_le_bytes()); // 注释
        self.central.extend_from_slice(&0u16.to_le_bytes()); // 起始磁盘
        self.central.extend_from_slice(&0u16.to_le_bytes()); // 内部属性
        self.central.extend_from_slice(&0u32.to_le_bytes()); // 外部属性
        self.central.extend_from_slice(&offset.to_le_bytes());
        self.central.extend_from_slice(name_bytes);

        self.count += 1;
    }

    fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.local.len() as u32;
        let cd_size = self.central.len() as u32;
        self.local.extend_from_slice(&self.central);
        self.local.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.local.extend_from_slice(&0u16.to_le_bytes()); // 本磁盘号
        self.local.extend_from_slice(&0u16.to_le_bytes()); // 中央目录磁盘号
        self.local.extend_from_slice(&self.count.to_le_bytes());
        self.local.extend_from_slice(&self.count.to_le_bytes());
        self.local.extend_from_slice(&cd_size.to_le_bytes());
        self.local.extend_from_slice(&cd_offset.to_le_bytes());
        self.local.extend_from_slice(&0u16.to_le_bytes()); // 注释长度
        self.local
    }
}

/// CRC-32（IEEE 802.3），ZIP 校验用。
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn col_names() {
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(25), "Z");
        assert_eq!(col_name(26), "AA");
        assert_eq!(col_name(27), "AB");
    }

    #[test]
    fn escape_control_chars() {
        assert_eq!(esc("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
        assert_eq!(esc("x\u{0}y"), "x y");
        assert_eq!(esc("换\n行"), "换\n行");
    }

    #[test]
    fn crc32_known_value() {
        // "123456789" 的标准 CRC-32 值
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    /// xlsx 必须是一个结构完整的 zip：EOCD 收尾、条目数与中央目录一致。
    #[test]
    fn build_zip_structure() {
        let rows = vec![
            vec!["名称".into(), "版本".into()],
            vec!["A&B".into(), "1.0".into()],
        ];
        let bytes = build("软件清单", None, &[], &rows, &[20.0, 10.0]);
        // 文件头魔数
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        // EOCD 魔数出现在结尾 22 字节内
        let tail = &bytes[bytes.len() - 22..];
        assert_eq!(&tail[..4], b"PK\x05\x06");
        // 条目数（本文件共 6 个 part）
        let entries = u16::from_le_bytes([tail[10], tail[11]]);
        assert_eq!(entries, 6);
    }

    /// 生成样例文件到临时目录，供人工用 Excel / Python 验证。
    /// 运行：`cargo test -- --ignored write_sample_for_manual_check`
    #[test]
    #[ignore]
    fn write_sample_for_manual_check() {
        let rows = vec![
            vec![
                "软件名称".into(), "分类".into(), "版本号".into(), "形态".into(), "所在机器".into(),
                "安装路径".into(), "恢复意愿".into(), "处置方式".into(), "准备进度".into(), "精选".into(),
                "官网 / 下载链接".into(), "备份备忘与配置说明".into(),
            ],
            vec![
                "Visual Studio Code".into(), "开发工具".into(), "1.90".into(), "常规安装".into(),
                "主力台式机\n便携本".into(),
                "C:\\Program Files\\Microsoft VS Code\nD:\\VS Code".into(),
                "🔴 必须恢复 (Must Restore)".into(), "☁️ 账号登录同步".into(), "已就绪".into(),
                "★".into(), "https://code.visualstudio.com/".into(),
                "配置在 %APPDATA%\\Code\\User".into(),
            ],
        ];
        let path = std::env::temp_dir().join("ledger_sample.xlsx");
        std::fs::write(
            &path,
            build(
                "软件清单",
                Some("软件备份台账 · 软件清单"),
                &[format!("导出时间：{}    软件版本：v{}    共 {} 条", "2026-10-05 12:00:00", "2.1.0", rows.len() - 1)],
                &rows,
                &[22.0, 14.0, 12.0, 10.0, 26.0, 34.0, 20.0, 22.0, 12.0, 8.0, 36.0, 46.0],
            ),
        )
        .unwrap();
        // 特殊字符与多行
        let rows2 = vec![vec!["A&B".into(), "<x>\"q\"".into(), "一\n二".into()]];
        let path2 = std::env::temp_dir().join("ledger_sample2.xlsx");
        std::fs::write(&path2, build("s", None, &[], &rows2, &[10.0, 10.0, 10.0])).unwrap();
        eprintln!("wrote {:?} and {:?}", path, path2);
    }
}
