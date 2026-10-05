#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""一次性迁移：把旧版 data/（v1.x）转成 v2（uuid 内部标识）。

旧 -> 新：
- software.json：补 `uuid`；`restore_intent` should→on_demand；`backup_strategy` copy_config→copy_dir。
- extensions.json：旧按扩展 id 索引 `{"<ext_id>": {...}}` -> 新按 uuid 索引且内嵌 `(browser_id, profile, ext_id)`；
  browser_id / profile 会尽量从 evidence/*/browser-extensions.json 里按扩展 id 反查补全，查不到就留空。
- vault/<主机名>/<kind>/<id>/ -> vault/<kind>/<uuid>/（soft 用 SW-ID 映射，ext 用扩展 id 映射）。
- 新建空的 browsers.json（旧版没有浏览器级标注）。
- config.json / ignored.json / evidence/ / icons/ 原样保留（程序按 `icon_file` 字段读图标，无需改名）。

用法：
  python scripts/migrate_to_v2.py <旧 data 目录>              # 原地迁移，先备份为 <目录>.v1bak
  python scripts/migrate_to_v2.py <旧 data 目录> --out <新目录>  # 输出到新目录，不动原目录
"""

import argparse
import json
import shutil
import sys
import uuid
from pathlib import Path

KIND_DIRS = {"soft", "browser", "ext"}


def load_json(path, default):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return default


def dump_json(path: Path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=2)


def collect_ext_catalog(evidence_dir: Path):
    """扫描证据里的 browser-extensions.json，返回 {ext_id: (machine_id, browser_id, profile)}。

    v1 的 extensions.json 只按扩展 ID 索引，没有机器维度；这里反查到第一个
    (machine, browser, profile) 组合就挂上去（有损，但比丢掉好）。
    """
    catalog = {}
    if not evidence_dir.is_dir():
        return catalog
    for machine_dir in evidence_dir.iterdir():
        if not machine_dir.is_dir():
            continue
        data = load_json(machine_dir / "browser-extensions.json", None)
        if not isinstance(data, dict):
            continue
        machine_id = data.get("machine_id") or machine_dir.name
        for browser in data.get("browsers") or []:
            # 证据里浏览器字段是 id（旧字段名 label）；不是 browser_id。
            browser_id = browser.get("id") or browser.get("label") or ""
            for profile in browser.get("profiles") or []:
                profile_name = profile.get("profile", "")
                for ext in profile.get("extensions") or []:
                    ext_id = ext.get("id", "")
                    if ext_id and ext_id not in catalog:
                        catalog[ext_id] = (machine_id, browser_id, profile_name)
    return catalog


def migrate_software(data_dir: Path):
    path = data_dir / "software.json"
    software = load_json(path, None)
    if not isinstance(software, list):
        return {}
    id_to_uuid = {}
    stats = {"added_uuid": 0, "intent": 0, "strategy": 0}
    for item in software:
        if not isinstance(item, dict):
            continue
        uid = item.get("uuid")
        if not uid:
            uid = str(uuid.uuid4())
            item["uuid"] = uid
            stats["added_uuid"] += 1
        if item.get("id"):
            id_to_uuid[item["id"]] = uid
        if item.get("restore_intent") == "should":
            item["restore_intent"] = "on_demand"
            stats["intent"] += 1
        if item.get("backup_strategy") == "copy_config":
            item["backup_strategy"] = "copy_dir"
            stats["strategy"] += 1
    dump_json(path, software)
    return {"id_to_uuid": id_to_uuid, **stats}


def migrate_extensions(data_dir: Path, evidence_dir: Path):
    old = load_json(data_dir / "extensions.json", None)
    if not isinstance(old, dict):
        return {}
    catalog = collect_ext_catalog(evidence_dir)
    new_map = {}
    ext_id_to_uuid = {}
    created = 0
    for key, fields in old.items():
        fields = fields if isinstance(fields, dict) else {}
        # 已是新格式（按 uuid 索引且内嵌匹配键）：原样保留
        if fields.get("ext_id") and fields.get("browser_id") is not None:
            new_map[key] = fields
            ext_id_to_uuid[fields.get("ext_id", "")] = key
            continue
        machine_id, browser_id, profile = catalog.get(key, ("", "", ""))
        entry = {
            "machine_id": machine_id,
            "browser_id": browser_id,
            "profile": profile,
            "ext_id": key,
        }
        entry.update(fields)
        entry.pop("id", None)
        new_map[str(uuid.uuid4())] = entry
        ext_id_to_uuid[key] = list(new_map.keys())[-1]
        created += 1
    dump_json(data_dir / "extensions.json", new_map)
    return {"ext_id_to_uuid": ext_id_to_uuid, "ext_created": created}


def migrate_vault(data_dir: Path, id_to_uuid, ext_id_to_uuid):
    vault = data_dir / "vault"
    if not vault.is_dir():
        return {"vault_moved": 0, "vault_skipped": 0}
    moved = skipped = 0
    for host in list(vault.iterdir()):
        if not host.is_dir() or host.name in KIND_DIRS:
            continue  # 已经是新格式（vault/<kind>/...）
        for kind_dir in list(host.iterdir()):
            if not kind_dir.is_dir():
                continue
            kind = kind_dir.name
            for id_dir in list(kind_dir.iterdir()):
                if not id_dir.is_dir():
                    continue
                if kind == "soft":
                    new_id = id_to_uuid.get(id_dir.name)
                elif kind == "ext":
                    new_id = ext_id_to_uuid.get(id_dir.name)
                else:
                    new_id = None
                if not new_id:
                    skipped += 1
                    continue
                dest = vault / kind / new_id
                dest.parent.mkdir(parents=True, exist_ok=True)
                if dest.exists():
                    shutil.rmtree(dest)
                shutil.move(str(id_dir), str(dest))
                moved += 1
        try:
            if not any(host.iterdir()):
                host.rmdir()
        except OSError:
            pass
    return {"vault_moved": moved, "vault_skipped": skipped}


def migrate(src: Path, out: Path):
    if not (src / "software.json").exists():
        sys.exit(f"[错误] 找不到 {src / 'software.json'}，确认这是旧版数据目录")

    if out.resolve() == src.resolve():
        backup = src.with_name(src.name + ".v1bak")
        if not backup.exists():
            shutil.copytree(src, backup)
        print(f"[i] 已备份原目录 -> {backup}")

    sw = migrate_software(out)
    ex = migrate_extensions(out, out / "evidence")
    va = migrate_vault(out, sw.get("id_to_uuid", {}), ex.get("ext_id_to_uuid", {}))
    if not (out / "browsers.json").exists():
        dump_json(out / "browsers.json", {})

    print(f"[OK] 迁移完成: {out}")
    print(f"    软件补 uuid: {sw.get('added_uuid', 0)} 条；"
          f"should→on_demand: {sw.get('intent', 0)}；copy_config→copy_dir: {sw.get('strategy', 0)}")
    print(f"    扩展迁移: {ex.get('ext_created', 0)} 条；保管箱搬迁: {va.get('vault_moved', 0)} 个"
          f"（跳过 {va.get('vault_skipped', 0)} 个找不到映射的）")


def main():
    ap = argparse.ArgumentParser(description="旧版 data/ 迁移到 v2（uuid 内部标识）")
    ap.add_argument("data", help="旧版 data 目录")
    ap.add_argument("--out", help="输出目录（默认原地迁移，先备份为 <目录>.v1bak）")
    args = ap.parse_args()
    src = Path(args.data).resolve()
    out = Path(args.out).resolve() if args.out else src
    if out.resolve() != src.resolve():
        if out.exists():
            shutil.rmtree(out)
        shutil.copytree(src, out)
    migrate(src, out)


if __name__ == "__main__":
    main()
