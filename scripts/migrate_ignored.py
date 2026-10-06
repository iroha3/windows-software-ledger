#!/usr/bin/env python3
"""把旧版 ignored.json 墓碑迁移到统一的删除记录结构。

当前结构：{uuid, name, deleted_at, machines: [{machine_id, paths: [...], name_only?}]}
  - uuid 供同步合并按 uuid 删除（含「合并条目」被并掉的记录）；
  - machines 的身份键供重扫压制（同机同路径 / 同机同名 name_only）。

历史结构：
  1) {match_key, name, paths: [...], deleted_at}
  2) {match_key, name, machines: [{machine_id, paths: [...]}], deleted_at}

老墓碑不含机器信息，脚本把所有条目挂到同一台机器（默认反查 config.json
machine_aliases 里别名为 Surface 的机器 id）。无路径的旧墓碑写成「机器 + 同名」墓碑
（`machines[].name_only = true`）。缺 uuid 的条目补一个随机 uuid（只保证唯一，
不保证能匹配回已删记录，因此仅参与重扫压制、不参与同步合并）。

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
import uuid
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

    out, with_path, name_only, added_uuid = [], 0, 0, 0
    for g in data:
        entry = {
            "uuid": g.get("uuid") or str(uuid.uuid4()),
            "name": g.get("name", ""),
            "deleted_at": g.get("deleted_at", ""),
            "machines": g.get("machines", []),
        }
        if not g.get("uuid"):
            added_uuid += 1
        if not entry["machines"]:
            paths = [p for p in g.get("paths", []) if p]
            m = {"machine_id": machine, "paths": paths}
            if not paths:
                m["name_only"] = True
                name_only += 1
            else:
                with_path += 1
            entry["machines"] = [m]
        out.append(entry)

    backup = path.with_name(f"{path.name}.bak-{time.strftime('%Y%m%d-%H%M%S')}")
    shutil.copy2(path, backup)
    path.write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(
        f"机器 {machine}：旧结构补机器 {with_path} 条（带路径）+ {name_only} 条（同名墓碑），"
        f"补 uuid {added_uuid} 条，共 {len(out)} 条"
    )
    print(f"备份于 {backup}")
    print(f"写回 {path}")


if __name__ == "__main__":
    main()
