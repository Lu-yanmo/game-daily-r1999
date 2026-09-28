# -*- coding: utf-8 -*-
"""逐角色养成数值的只读查询 —— 第五动词 progression-lookup 的数据层(M4 立项、本轮落码)。

数据口径(kb-plan §0 既定)：精确数值不进向量库，走结构化表 state/progression/<game>.json
(实抓 wiki 落成，_meta.updated 记采集日)。本模块是唯一实现，
tools/progression.py 的 CLI 只是它的薄壳；agent 侧经 live.make_prog_fn 进程内直调。
纯本地只读：没有表/没有角色都如实拒，不编数。仓导出环境里没有 state/progression 时
返回 ok:false 的人话拒绝 —— 这是设计行为，不是故障。
"""
import io
import json
import os


def load_table(root, game):
    path = os.path.join(str(root), "state", "progression", "%s.json" % game)
    if not os.path.exists(path):
        return None, "没有 %s 的养成表(%s)，先补真实采集" % (game, path)
    with io.open(path, encoding="utf-8") as f:
        return json.load(f), ""


def lookup(root, game, char, line="", stage=""):
    """返回信封 dict：{ok, why, game, char, meta, lines, gap?} —— 与 CLI stdout 的 JSON 同形状。"""
    game = str(game or "").strip()
    char = str(char or "").strip()
    line = str(line or "").strip()
    stage = str(stage or "").strip()
    if not game:
        return {"ok": False, "why": "没给游戏(表按游戏分档：ark/r1999)"}
    if not char:
        return {"ok": False, "why": "没给角色名(查询是精确全名匹配)"}
    table, why = load_table(root, game)
    if table is None:
        return {"ok": False, "why": why}
    chars = table.get("characters") or {}
    key = char.lower()
    hit = None
    for name, data in chars.items():
        if key in (name.lower(), str(data.get("name", "")).lower()):
            hit = (name, data)
            break
    if hit is None:
        return {"ok": False, "why": "表内无角色「%s」；已收录 %d 名(可按名查，勿凭记忆编)" % (
            char, len(chars))}
    name, data = hit
    out = {"ok": True, "why": "", "game": game, "char": name,
           "meta": table.get("_meta", {})}
    if line:
        lines = {k: v for k, v in data.items() if k not in ("name", "gap")}
        want = line.lower()
        match = {k: v for k, v in lines.items() if k.lower() == want}
        if not match:
            return {"ok": False, "why": "「%s」无养成线「%s」；有：%s" % (
                name, line, "、".join(lines.keys()) or "空")}
        if stage:
            src = match.get(line) or list(match.values())[0]
            line_key = line if line in match else list(match.keys())[0]
            sel = {k: v for k, v in (src or {}).items() if k.lower() == stage.lower()}
            if not sel:
                return {"ok": False, "why": "「%s」线无档「%s」；有：%s" % (
                    line_key, stage, "、".join((src or {}).keys()) or "空")}
            match = {line_key: sel}
        out["lines"] = match
    else:
        out["lines"] = {k: v for k, v in data.items()
                        if k in ("insight", "resonance", "tuning", "elite", "skillup",
                                 "level_breakroom", "level_breakthrough", "shaping")}
    if data.get("gap"):
        out["gap"] = data["gap"]
    return out
