#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把两份 data/ 合成一份：A（母本）+ B 全量并列，**软件条目不做任何自动合并**。

典型场景：A 机已升 v2，B 机还是 v1。想要一份含两台机器数据的 `data/`，拷到 A / B / C 都能用。
- 任一端若是 v1（条目缺 uuid），自动先做 v1→v2 迁移（在临时副本上进行，**不改动原目录**）。
- software.json：两份条目**原样并列**（只按 uuid 去重，uuid 相同才算同一条）。被并进来的条目
  重排 SW-ID，避免两本台账撞号。**同名条目不会自动合并**——要不要合并、并哪几条，由你在 app
  里用「同名合并」自己决定；分开保留两条也完全正常。
- icons / vault / evidence：按 uuid / 主机名并集拷入，已存在的不覆盖。
- extensions.json / browsers.json：与软件同样，**纯并集、不按匹配键合并**，只按 uuid 去重。
  同一条扩展分别装在两台机器上就保留为两条，各自独立。
- ignored.json：墓碑并集去重；config.json 保留 A。

用法：
  python scripts/combine_data.py <A机 data> <B机 data> --out <产出 data>
  python scripts/combine_data.py <A机 data> <B机 data>          # 原地并入 A，先备份 A.v1bak
"""

import argparse
import json
import shutil
import sys
import tempfile
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from migrate_to_v2 import migrate_extensions, migrate_software, migrate_vault  # noqa: E402


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


def has_software(data_dir):
    return (Path(data_dir) / "software.json").exists()


def is_v1(data_dir):
    """条目里有任何一条缺 uuid，就当作 v1 数据。"""
    sw = load_json(Path(data_dir) / "software.json", [])
    return isinstance(sw, list) and any(isinstance(it, dict) and not it.get("uuid") for it in sw)


def migrate_inplace(data_dir):
    """把 data_dir 就地升级成 v2（调用方负责先备份）。"""
    data_dir = Path(data_dir)
    sw = migrate_software(data_dir)
    ex = migrate_extensions(data_dir, data_dir / "evidence")
    migrate_vault(data_dir, sw.get("id_to_uuid", {}), ex.get("ext_id_to_uuid", {}))
    if not (data_dir / "browsers.json").exists():
        dump_json(data_dir / "browsers.json", {})
    return sw, ex


def prepare(data_dir):
    """v1 -> 临时 v2 副本；已是 v2 则原样返回。返回 (可用目录, 待清理的临时根 或 None)。"""
    data_dir = Path(data_dir)
    if not is_v1(data_dir):
        return data_dir, None
    tmp_root = Path(tempfile.mkdtemp(prefix="ledger-v2-"))
    tmp = tmp_root / "data"
    shutil.copytree(data_dir, tmp)
    sw, ex = migrate_inplace(tmp)
    print(f"[i] 检测到 v1 数据并自动迁移: {data_dir}")
    print(f"    补 uuid {sw.get('added_uuid', 0)} 条；should→on_demand {sw.get('intent', 0)}；"
          f"扩展重挂 {ex.get('ext_created', 0)} 条")
    return tmp, tmp_root


def numeric_id(s):
    try:
        return int(str(s).replace("SW-", ""))
    except (ValueError, TypeError):
        return 0


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


def copy_vault_dir(base_dir, kind, target_uuid, inc_dir, src_uuid):
    """incoming 的 vault/<kind>/<src_uuid>/ 并入 base，已存在同名文件不覆盖。"""
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
        if not dest.exists():
            shutil.move(str(f), str(dest))
        moved += 1
    try:
        src.rmdir()
    except OSError:
        pass
    return moved


def combine_software(base_dir, inc_dir):
    """两份条目原样并列，同名不合并。只按 uuid 去重。"""
    base = load_json(Path(base_dir) / "software.json", [])
    inc = load_json(Path(inc_dir) / "software.json", [])
    base = base if isinstance(base, list) else []
    inc = inc if isinstance(inc, list) else []
    seen = {it.get("uuid") for it in base if it.get("uuid")}
    max_id = max((numeric_id(it.get("id")) for it in base), default=0)
    stats = {"added": 0, "skipped": 0}
    for child in inc:
        uid = child.get("uuid")
        if not uid:
            uid = str(uuid.uuid4())
            child["uuid"] = uid
        if uid in seen:
            stats["skipped"] += 1
            continue
        max_id += 1
        child["id"] = f"SW-{max_id:03}"
        name = copy_icon(inc_dir, base_dir, child.get("icon_file"), uid)
        if name:
            child["icon_file"] = name
        copy_vault_dir(base_dir, "soft", uid, inc_dir, uid)
        base.append(child)
        seen.add(uid)
        stats["added"] += 1
    dump_json(Path(base_dir) / "software.json", base)
    return stats


def combine_map(base_dir, inc_dir, filename, vault_kind):
    """uuid 索引的标注文件（extensions.json / browsers.json）纯并集。

    和软件条目一样：只按 uuid 去重，**不按 (browser/profile/ext) 匹配键合并**。
    同一条扩展在两台机器上各有一条标注时，两条都保留、各自独立。
    """
    path = Path(base_dir) / filename
    base = load_json(path, {})
    inc = load_json(Path(inc_dir) / filename, {})
    base = base if isinstance(base, dict) else {}
    inc = inc if isinstance(inc, dict) else {}
    stats = {"added": 0, "skipped": 0}
    for uid, val in inc.items():
        if uid in base:
            stats["skipped"] += 1
            continue
        base[uid] = dict(val) if isinstance(val, dict) else {}
        if vault_kind:
            copy_vault_dir(base_dir, vault_kind, uid, inc_dir, uid)
        stats["added"] += 1
    if base or inc or path.exists():
        dump_json(path, base)
    return stats


def combine_evidence(base_dir, inc_dir):
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


def combine_ignored(base_dir, inc_dir):
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


def combine_trash(base_dir, inc_dir):
    src = Path(inc_dir) / "trash"
    if not src.is_dir():
        return
    for d in src.iterdir():
        dst = Path(base_dir) / "trash" / d.name
        if not dst.exists():
            shutil.copytree(d, dst)


def combine_config(base_dir, inc_dir):
    dst = Path(base_dir) / "config.json"
    if not dst.exists():
        src = Path(inc_dir) / "config.json"
        if src.is_file():
            shutil.copy2(src, dst)


def combine(base_dir, inc_dir):
    stats = {}
    stats.update(combine_software(base_dir, inc_dir))
    ext = combine_map(base_dir, inc_dir, "extensions.json", "ext")
    browser = combine_map(base_dir, inc_dir, "browsers.json", "browser")
    stats["ext_new"] = ext["added"]
    stats["ext_skipped"] = ext["skipped"]
    stats["browser_new"] = browser["added"]
    stats["browser_skipped"] = browser["skipped"]
    stats["evidence_files"] = combine_evidence(base_dir, inc_dir)
    combine_ignored(base_dir, inc_dir)
    combine_trash(base_dir, inc_dir)
    combine_config(base_dir, inc_dir)
    return stats


def main():
    ap = argparse.ArgumentParser(description="把两份 data/ 合成一份（条目并列，不自动合并）")
    ap.add_argument("base", help="母本 data 目录（通常放 A 机，其数据在前）")
    ap.add_argument("incoming", help="要并进来的 data 目录（v1 会自动迁移）")
    ap.add_argument("--out", help="产出目录（默认原地并入 base，先备份 base.v1bak）")
    args = ap.parse_args()
    base = Path(args.base).resolve()
    inc = Path(args.incoming).resolve()
    if not has_software(base):
        sys.exit(f"[错误] 母本里没有 software.json: {base}")
    if not has_software(inc):
        sys.exit(f"[错误] incoming 里没有 software.json: {inc}")
    out = Path(args.out).resolve() if args.out else None
    if out and out in (base, inc):
        sys.exit("[错误] --out 不能等于 base 或 incoming")

    src, src_tmp = prepare(inc)
    try:
        if out:
            if out.exists():
                shutil.rmtree(out)
            shutil.copytree(base, out)
            if is_v1(out):
                migrate_inplace(out)
            target = out
        else:
            backup = base.with_name(base.name + ".v1bak")
            if not backup.exists():
                shutil.copytree(base, backup)
                print(f"[i] 已备份母本 -> {backup}")
            if is_v1(base):
                migrate_inplace(base)
            target = base

        stats = combine(target, src)
    finally:
        if src_tmp:
            shutil.rmtree(src_tmp, ignore_errors=True)

    print(f"[OK] 产出 v2 data: {target}")
    print(f"    软件: 新增 {stats['added']} 条 / 跳过重复 uuid {stats['skipped']} 条（同名不合并，原样并列）")
    print(f"    扩展: 新增 {stats['ext_new']} 条 / 跳过重复 uuid {stats['ext_skipped']} 条"
          f"（不按匹配键合并）")
    print(f"    浏览器: 新增 {stats['browser_new']} 条 / 跳过 {stats['browser_skipped']} 条；"
          f"证据补齐 {stats['evidence_files']} 个文件")
    print("    把它拷到 A / B / C 的 exe 旁即可；要不要合并条目，在 app 里自己定。")


if __name__ == "__main__":
    main()
