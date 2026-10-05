#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把两份 data/ 捏成一份：base = 本机 data（锚点），incoming = 另一台机器的 data。

规则与后端 `merge_software` 一致：**锚点为准；事实/资产并入或补齐，决策与主观永不并入**。

- software.json：按软件名（trim + 小写）配对。
  - 配上的并入锚点：machines 按 `machine_id` 去重（同机补齐 install_location / version、portable 升级）；
    补齐 version / download_url；has_config 取或；config_notes 追加；
    restore_intent / backup_strategy / category / type / notes 等锚点已有值保持不动。
  - 配不上的原样追加，并重排 SW-ID 避免两本台账撞号。
- icons/<uuid>.png、vault/<kind>/<uuid>/：被并项的图标/归档并入锚点（同名文件锚点优先，
  冲突者进 data/trash/）。追加项整体搬入。
- extensions.json / browsers.json：按 uuid 合并，并按匹配键
  `(browser_id, profile, ext_id)` / `browser_id` 去重（锚点优先，只补缺失字段）。
- evidence/：整目录拷贝，同名主机目录不覆盖 base 已有文件。
- ignored.json：墓碑并集去重；config.json 保留 base。

用法：
  python scripts/merge_data.py <base data> <incoming data> --out <目标 data>   # 输出到新目录
  python scripts/merge_data.py <base data> <incoming data>                     # 原地并入，先备份 base.v1bak
"""

import argparse
import json
import shutil
import sys
import uuid
from datetime import datetime
from pathlib import Path


def load_json(path, default):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return default


def dump_json(path, obj):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=2)


def norm_name(s):
    return (s or "").strip().lower()


def numeric_id(s):
    try:
        return int(str(s).replace("SW-", ""))
    except (ValueError, TypeError):
        return 0


def trash_file(base_dir, path):
    """同名冲突时把被并项文件挪进垃圾桶（不物理删除）。"""
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    dest = Path(base_dir) / "trash" / f"{stamp}-{uuid.uuid4().hex[:8]}"
    dest.mkdir(parents=True, exist_ok=True)
    shutil.move(str(path), str(dest / Path(path).name))


def fill_machine(anchor_machines, child_machine):
    mid = child_machine.get("machine_id", "")
    for m in anchor_machines:
        if m.get("machine_id", "") == mid:
            if not m.get("install_location") and child_machine.get("install_location"):
                m["install_location"] = child_machine["install_location"]
            if not m.get("version") and child_machine.get("version"):
                m["version"] = child_machine["version"]
            if child_machine.get("form") == "portable":
                m["form"] = "portable"
            return
    anchor_machines.append(dict(child_machine))


def merge_soft(anchor, child):
    """锚点为准：事实/资产并入或补齐，决策与主观不动。"""
    machines = anchor.setdefault("machines", [])
    for m in child.get("machines") or []:
        fill_machine(machines, m)
    for field in ("version", "download_url"):
        if not anchor.get(field) and child.get(field):
            anchor[field] = child[field]
    if child.get("has_config"):
        anchor["has_config"] = True
    notes = (child.get("config_notes") or "").strip()
    if notes:
        cur = (anchor.get("config_notes") or "").strip()
        anchor["config_notes"] = f"{cur}; {notes}" if cur else notes


def copy_icon(inc_dir, base_dir, icon_file, uuid_):
    """把 incoming 的图标拷成 base 的 <uuid>.png，返回新文件名（拷不到返回空）。"""
    if not icon_file or not uuid_:
        return ""
    src = Path(inc_dir) / "icons" / icon_file
    if not src.is_file():
        return ""
    dst = Path(base_dir) / "icons" / f"{uuid_}.png"
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    return dst.name


def merge_vault_dir(base_dir, kind, target_uuid, inc_dir, src_uuid):
    """incoming 的 vault/<kind>/<src_uuid>/ 并入 vault/<kind>/<target_uuid>/，同名锚点优先。"""
    if not target_uuid or not src_uuid:
        return 0
    src = Path(inc_dir) / "vault" / kind / src_uuid
    if not src.is_dir():
        return 0
    dst = Path(base_dir) / "vault" / kind / target_uuid
    dst.mkdir(parents=True, exist_ok=True)
    moved = 0
    for f in src.iterdir():
        if not f.is_file():
            continue
        dest = dst / f.name
        if dest.exists():
            trash_file(base_dir, f)
        else:
            shutil.move(str(f), str(dest))
        moved += 1
    try:
        src.rmdir()
    except OSError:
        pass
    return moved


def merge_software(base_dir, inc_dir):
    base = load_json(Path(base_dir) / "software.json", [])
    inc = load_json(Path(inc_dir) / "software.json", [])
    base = base if isinstance(base, list) else []
    inc = inc if isinstance(inc, list) else []
    by_name = {}
    for it in base:
        key = norm_name(it.get("name"))
        if key:
            by_name.setdefault(key, it)
    max_id = max((numeric_id(it.get("id")) for it in base), default=0)
    stats = {"merged": 0, "appended": 0}
    for child in inc:
        key = norm_name(child.get("name"))
        anchor = by_name.get(key) if key else None
        if anchor is not None:
            merge_soft(anchor, child)
            if not anchor.get("icon_file") and child.get("icon_file"):
                name = copy_icon(inc_dir, base_dir, child["icon_file"], anchor.get("uuid", ""))
                if name:
                    anchor["icon_file"] = name
            merge_vault_dir(base_dir, "soft", anchor.get("uuid", ""), inc_dir, child.get("uuid", ""))
            stats["merged"] += 1
        else:
            max_id += 1
            child["id"] = f"SW-{max_id:03}"
            name = copy_icon(inc_dir, base_dir, child.get("icon_file"), child.get("uuid", ""))
            if name:
                child["icon_file"] = name
            merge_vault_dir(base_dir, "soft", child.get("uuid", ""), inc_dir, child.get("uuid", ""))
            base.append(child)
            if key:
                by_name[key] = child
            stats["appended"] += 1
    dump_json(Path(base_dir) / "software.json", base)
    return stats


def merge_map(base_dir, inc_dir, filename, key_fields, vault_kind):
    """合并 uuid 索引的标注文件（extensions.json / browsers.json），按匹配键去重。"""
    path = Path(base_dir) / filename
    base = load_json(path, {})
    inc = load_json(Path(inc_dir) / filename, {})
    base = base if isinstance(base, dict) else {}
    inc = inc if isinstance(inc, dict) else {}
    key_index = {}
    for uid, val in base.items():
        k = tuple((val or {}).get(f, "") for f in key_fields)
        key_index.setdefault(k, uid)
    created = 0
    for uid, val in inc.items():
        val = val if isinstance(val, dict) else {}
        k = tuple(val.get(f, "") for f in key_fields)
        target = key_index.get(k)
        if target is None:
            target = uid if uid not in base else str(uuid.uuid4())
            base[target] = dict(val)
            key_index[k] = target
            created += 1
        else:
            dst = base[target]
            for fk, fv in val.items():
                if fk not in dst or dst[fk] in ("", None, [], {}):
                    dst[fk] = fv
        if vault_kind:
            merge_vault_dir(base_dir, vault_kind, target, inc_dir, uid)
    if base or inc or path.exists():
        dump_json(path, base)
    return created


def merge_evidence(base_dir, inc_dir):
    src_root = Path(inc_dir) / "evidence"
    if not src_root.is_dir():
        return 0
    copied = 0
    for host in src_root.iterdir():
        if not host.is_dir():
            continue
        for f in host.rglob("*"):
            if not f.is_file():
                continue
            out = Path(base_dir) / "evidence" / host.name / f.relative_to(host)
            if out.exists():
                continue
            out.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(f, out)
            copied += 1
    return copied


def merge_ignored(base_dir, inc_dir):
    base = load_json(Path(base_dir) / "ignored.json", [])
    inc = load_json(Path(inc_dir) / "ignored.json", [])
    base = base if isinstance(base, list) else []
    inc = inc if isinstance(inc, list) else []
    seen = {(t.get("name"), tuple(t.get("paths") or [])) for t in base}
    for t in inc:
        k = (t.get("name"), tuple(t.get("paths") or []))
        if k not in seen:
            base.append(t)
            seen.add(k)
    if base or inc:
        dump_json(Path(base_dir) / "ignored.json", base)


def merge_trash(base_dir, inc_dir):
    src = Path(inc_dir) / "trash"
    if not src.is_dir():
        return
    for d in src.iterdir():
        dst = Path(base_dir) / "trash" / d.name
        if not dst.exists():
            shutil.copytree(d, dst)


def merge_config(base_dir, inc_dir):
    dst = Path(base_dir) / "config.json"
    if not dst.exists():
        src = Path(inc_dir) / "config.json"
        if src.is_file():
            shutil.copy2(src, dst)


def merge(base_dir, inc_dir):
    stats = {}
    stats.update(merge_software(base_dir, inc_dir))
    stats["ext_new"] = merge_map(
        base_dir, inc_dir, "extensions.json", ("browser_id", "profile", "ext_id"), "ext"
    )
    stats["browser_new"] = merge_map(base_dir, inc_dir, "browsers.json", ("browser_id",), "browser")
    stats["evidence_files"] = merge_evidence(base_dir, inc_dir)
    merge_ignored(base_dir, inc_dir)
    merge_trash(base_dir, inc_dir)
    merge_config(base_dir, inc_dir)
    return stats


def main():
    ap = argparse.ArgumentParser(description="把两份 data/ 捏成一份（base 为锚点）")
    ap.add_argument("base", help="本机 data 目录（锚点）")
    ap.add_argument("incoming", help="另一台机器的 data 目录")
    ap.add_argument("--out", help="输出目录（默认原地并入 base，先备份 base.v1bak）")
    args = ap.parse_args()
    base = Path(args.base).resolve()
    inc = Path(args.incoming).resolve()
    if not (base / "software.json").exists():
        sys.exit(f"[错误] base 里没有 software.json: {base}")
    if not (inc / "software.json").exists():
        sys.exit(f"[错误] incoming 里没有 software.json: {inc}")

    if args.out:
        out = Path(args.out).resolve()
        if out in (base, inc):
            sys.exit("[错误] --out 不能等于 base 或 incoming")
        if out.exists():
            shutil.rmtree(out)
        shutil.copytree(base, out)
        target = out
    else:
        backup = base.with_name(base.name + ".v1bak")
        if not backup.exists():
            shutil.copytree(base, backup)
            print(f"[i] 已备份 base -> {backup}")
        target = base

    stats = merge(target, inc)
    print(f"[OK] 合并完成: {target}")
    print(f"    软件: 并入 {stats['merged']} 条 / 追加 {stats['appended']} 条")
    print(f"    扩展新增 {stats['ext_new']} 条；浏览器新增 {stats['browser_new']} 条；"
          f"证据补齐 {stats['evidence_files']} 个文件")


if __name__ == "__main__":
    main()
