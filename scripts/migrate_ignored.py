#!/usr/bin/env python3
"""把旧版 ignored.json 墓碑迁移到「机器 + 路径」新结构。

旧结构：{match_key, name, paths: [...], deleted_at}
新结构：{match_key, name, machines: [{machine_id, paths: [...]}], deleted_at}

旧墓碑不含机器信息，脚本把所有条目挂到同一台机器（默认反查 config.json
machine_aliases 里别名为 Surface 的机器 id）。无路径的旧墓碑写成「机器 + 同名」墓碑
（`machines[].name_only = true`），重扫时同机同名条目默认不勾选。

用法：
    python scripts/migrate_ignored.py [ignored.json 路径] [--machine DESKTOP-E5EJM94]
默认路径 src-tauri/target/debug/data/ignored.json（相对仓库根）。
原文件会先备份成 ignored.json.bak-<时间戳>。
"""

import argparse
import json
import shutil
import sys
import time
from pathlib import Path


def resolve_machine(data_dir: Path, alias: str):
    """从同目录 config.json 的 machine_aliases 反查合并 id。"""
    cfg = data_dir / "config.json"
    if not cfg.exists():
        return None
    try:
        aliases = json.loads(cfg.read_text(encoding="utf-8")).get("machine_aliases", {})
    except (OSError, json.JSONDecodeError):
        return None
    for mid, name in aliases.items():
        if name == alias:
            return mid
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("path", nargs="?", default="src-tauri/target/debug/data/ignored.json")
    ap.add_argument("--machine", help="机器 id（默认取 config.json 中别名为 --alias 的键）")
    ap.add_argument("--alias", default="Surface", help="用于反查机器 id 的别名，默认 Surface")
    args = ap.parse_args()

    path = Path(args.path)
    if not path.exists():
        sys.exit(f"找不到文件：{path}")
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        sys.exit("ignored.json 顶层不是数组")

    machine = args.machine or resolve_machine(path.parent, args.alias)
    if not machine:
        sys.exit(f"无法从 config.json 反查到别名“{args.alias}”的机器 id，请用 --machine 指定")

    out, with_path, name_only, kept = [], 0, 0, 0
    for g in data:
        if "machines" in g:
            out.append(g)
            kept += 1
            continue
        paths = [p for p in g.get("paths", []) if p]
        entry = {"machine_id": machine, "paths": paths}
        if not paths:
            entry["name_only"] = True
            name_only += 1
        else:
            with_path += 1
        out.append(
            {
                "match_key": g.get("match_key", ""),
                "name": g.get("name", ""),
                "machines": [entry],
                "deleted_at": g.get("deleted_at", ""),
            }
        )

    backup = path.with_name(f"{path.name}.bak-{time.strftime('%Y%m%d-%H%M%S')}")
    shutil.copy2(path, backup)
    path.write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(
        f"机器 {machine}：迁移 {with_path} 条（带路径），{name_only} 条（同名墓碑），"
        f"已是新结构 {kept} 条"
    )
    print(f"备份于 {backup}")
    print(f"写回 {path}")


if __name__ == "__main__":
    main()
