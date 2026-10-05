#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""一台机器的「母本台账」+ 另一台机器的 data/ -> 一份三台都能用的 v2 data/。

典型场景：A 机已升 v2 并打好了标（母本），B 机还是 v1。想把 B 机的软件并进母本，
产出一份 `data/`，拷到 A / B / C 都能直接用——就像「先在 A 打标，再把 data 拷去 B、拷去 C」。

- **第一个参数是锚点（母本）**：它的决策与主观字段优先，A 机应放第一个。
- 任一端的 data 若是 v1（条目缺 uuid），会自动先做 v1→v2 迁移（补 uuid、should→on_demand、
  copy_config→copy_dir、vault/<主机名>/<kind>/<id>/→vault/<kind>/<uuid>/、扩展标注重挂），
  迁移在临时副本上进行，**不改动原始目录**。
- 合并规则与后端 `merge_software` 一致：**锚点为准；事实/资产并入或补齐，决策与主观永不并入**。
  - software.json：按软件名（trim + 小写）配对。配上的并入锚点：machines 按 `machine_id` 去重
    （同机补齐 install_location / version、portable 升级）；补齐 version / download_url；
    has_config 取或；config_notes 追加；restore_intent / backup_strategy / category / type / notes
    等锚点已有值保持不动。配不上的原样追加，并重排 SW-ID 避免两本台账撞号。
  - icons/<uuid>.png、vault/<kind>/<uuid>/：被并项的图标/归档并入锚点（同名文件锚点优先，
    冲突者进 data/trash/）。追加项整体搬入。
  - extensions.json / browsers.json：按 uuid 合并，并按匹配键 `(browser_id, profile, ext_id)` /
    `browser_id` 去重（锚点优先，只补缺失字段）。
  - evidence/：整目录拷贝，同名主机目录不覆盖 base 已有文件。
  - ignored.json：墓碑并集去重；config.json 保留锚点。

用法：
  python scripts/merge_data.py <A机 data（锚点/母本）> <B机 data> --out <产出 data>
  python scripts/merge_data.py <A机 data> <B机 data>          # 原地并入 A，先备份 A.v1bak
"""

import argparse
import json
import shutil
import sys
import tempfile
import uuid
from datetime import datetime
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
    ap = argparse.ArgumentParser(description="母本台账 + 另一台 data -> 三台可用的 v2 data")
    ap.add_argument("base", help="锚点/母本 data 目录（通常放 A 机）")
    ap.add_argument("incoming", help="要并入的 data 目录（v1 会自动迁移）")
    ap.add_argument("--out", help="产出目录（默认原地并入 base，先备份 base.v1bak）")
    args = ap.parse_args()
    base = Path(args.base).resolve()
    inc = Path(args.incoming).resolve()
    if not has_software(base):
        sys.exit(f"[错误] 锚点里没有 software.json: {base}")
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
                print(f"[i] 已备份锚点 -> {backup}")
            if is_v1(base):
                migrate_inplace(base)
            target = base

        stats = merge(target, src)
    finally:
        if src_tmp:
            shutil.rmtree(src_tmp, ignore_errors=True)

    print(f"[OK] 产出 v2 data: {target}")
    print(f"    软件: 并入 {stats['merged']} 条 / 追加 {stats['appended']} 条")
    print(f"    扩展新增 {stats['ext_new']} 条；浏览器新增 {stats['browser_new']} 条；"
          f"证据补齐 {stats['evidence_files']} 个文件")
    print("    把它拷到 A / B / C 的 exe 旁即可（各机重扫会刷新自己的证据）。")


if __name__ == "__main__":
    main()
