"""python -m gdagent <verb> ... —— KB 命令行入口。stdout 只出 JSON 信封，日志走 stderr。

动词：import / search / stats / drop / compact / rebuild / reindex / rollback / probe
（M2 会在此加对话回路 `chat`，并把 set-config/preview/run-verb 白名单从 PS 迁来。）
"""
import argparse
import sys

from . import config as CFG
from . import envelope as ENV
from .kb import chunk as CK
from .kb import manager as M
from .kb import store as ST
from .kb import embed as E


def build_parser():
    p = argparse.ArgumentParser(prog="gdagent", description="game-daily 游戏知识库 (M1)")
    sub = p.add_subparsers(dest="verb", required=True)

    s = sub.add_parser("import", help="导入 md 文件或目录")
    s.add_argument("target")
    s.add_argument("--game", choices=list(CK.GAMES))
    s.add_argument("--source", dest="source_prefix")

    s = sub.add_parser("search")
    s.add_argument("query")
    s.add_argument("--game")
    s.add_argument("--top", type=int, default=6)
    s.add_argument("--rerank", choices=["on", "off"], default="on")

    sub.add_parser("stats")

    s = sub.add_parser("drop")
    s.add_argument("--game", required=True)
    s.add_argument("--yes", action="store_true")

    sub.add_parser("compact")
    sub.add_parser("rebuild")
    sub.add_parser("reindex")
    sub.add_parser("rollback")
    sub.add_parser("probe")
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    cfg = CFG.load()
    store = ST.Store(CFG.ROOT / "state" / "kb")
    store.sweep_partial()

    if args.verb == "import":
        res = M.import_paths(store, cfg, CFG.ROOT, args.target, game=args.game, source_prefix=args.source_prefix)
    elif args.verb == "search":
        res = M.do_search(store, cfg, args.query, game=args.game, top=args.top, rerank_on=(args.rerank == "on"))
    elif args.verb == "stats":
        res = dict(ok=True, why="", **M.stats(store))
    elif args.verb == "drop":
        res = M.drop_game(store, cfg, args.game, yes=args.yes)
    elif args.verb == "compact":
        res = M.compact(store, cfg)
    elif args.verb == "rebuild":
        res = M.rebuild(store, cfg)
    elif args.verb == "reindex":
        res = M.reindex(store, cfg)
    elif args.verb == "rollback":
        okv, info = store.rollback()
        res = ({"ok": True, "why": "", "rolled_back_to": info} if okv
               else {"ok": False, "why": info})
    elif args.verb == "probe":
        res = _probe(cfg)
    else:
        res = ENV.fail("未知动词：%s" % args.verb)

    ENV.emit(res)
    return 0 if res.get("ok") else 1


def _probe(cfg):
    if not cfg.has_key():
        return ENV.fail("无 embed key(环境变量 KB_API_KEY/DASHSCOPE_API_KEY)，probe 需真实 key；跳过不算失败",
                        skipped=True)
    try:
        vec = E.embed_texts(["知识库连通性探针"], cfg)[0]
        return ENV.ok(model=cfg.embed_model, dim=len(vec), endpoint=cfg.base_url,
                      why="embed 连通，可用此响应钉 golden")
    except E.EmbedError as e:
        return ENV.fail("embed 探测失败：%s" % e)


if __name__ == "__main__":
    sys.exit(main())
