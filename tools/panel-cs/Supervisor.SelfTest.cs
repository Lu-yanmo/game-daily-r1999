// tools/panel-cs/Supervisor.SelfTest.cs —— --svtest：批次 6(Supervisor.cs)的无头自检。
// 期望串逐字抄自 lib\supervisor.ps1 的旧实现(四句 Why、现值头、拍行、"读不到/没问/none/原因"话术，
// 与 tools\test-modules.ps1 [sv] 块同一批口径) —— 这里红了就是移植走样，改 Supervisor.cs，
// 不改这里的期望(§7 同一条纪律)。
// 批次纪律：不起进程、不读盘、不写文件 —— 证据/状态/画面/文档全部由场景直接喂进来。
// 已知局限(与 Supervisor.cs 头注同一条)：beats/games 缺失形状旧侧 @($null) 会多出一个空元素，
// 真实 supervision.json 恒为数组，场景全部用数组形状。

using System;
using System.Collections.Generic;

namespace GdPanel
{
    static class GdSvSelfTest
    {
        sealed class Box
        {
            public int Total = 0;
            public int Fails = 0;
            public void Chk(string name, Func<bool> test)
            {
                Total++;
                bool ok = false;
                string detail = null;
                try { ok = test(); }
                catch (Exception ex) { ok = false; detail = ex.GetType().Name + ": " + ex.Message; }
                if (!ok)
                {
                    Fails++;
                    GdLog.Line("FAIL " + name + (detail != null ? "：" + detail : ""), "ERROR");
                }
            }
            public int Result(string tag)
            {
                if (Fails == 0) GdLog.Line(tag + " 全绿：" + Total + "/" + Total + " 项", "OK");
                else GdLog.Line(tag + " 有红项：" + Fails + "/" + Total, "ERROR");
                return Fails == 0 ? 0 : 1;
            }
        }

        static Dictionary<string, object> D(object v) { return v as Dictionary<string, object>; }

        static Dictionary<string, object> Inp(params object[] kv)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            for (int i = 0; i + 1 < kv.Length; i += 2) d[(string)kv[i]] = kv[i + 1];
            return d;
        }

        static object Ev(object done, object alive, object asstAge, object dailyAge)
        {
            return Inp("Done", done, "AssistantAlive", alive, "AsstAgeSec", asstAge, "DailyAgeSec", dailyAge);
        }

        static object Scr(object ok, object blocking, object what, object why)
        {
            return Inp("Ok", ok, "Blocking", blocking, "What", what, "Why", why);
        }

        static string KindOf(object r) { return Gd.Str(Gd.Get(D(r), "Kind")); }
        static string WhyOf(object r) { return Gd.Str(Gd.Get(D(r), "Why")); }
        static string StateOf(object r) { return Gd.Str(Gd.Get(D(r), "State")); }

        static List<string> LinesOf(object r)
        {
            List<string> outL = new List<string>();
            foreach (object x in Gd.List(Gd.Get(D(r), "Lines"))) outL.Add(Gd.Str(x));
            return outL;
        }
        static bool HasLine(List<string> ls, string frag)
        {
            foreach (string s in ls) if (s.Contains(frag)) return true;
            return false;
        }
        static int LineCountWith(List<string> ls, string frag)
        {
            int n = 0;
            foreach (string s in ls) if (s.Contains(frag)) n++;
            return n;
        }

        public static int SvTest()
        {
            Box b = new Box();
            SettingsGroup(b);
            StateGroup(b);
            ActionGroup(b);
            DetailGroup(b);
            return b.Result("--svtest");
        }

        // ---------- eval-supervisor-settings：没显式 false 就是开 + 三档钳制 ----------
        static void SettingsGroup(Box b)
        {
            object d = Supervisor.Settings(Inp());
            b.Chk("缺省：开/60s/30分/额度 1", () =>
                (bool)Gd.Get(D(d), "Enabled") && Gd.Int(Gd.Get(D(d), "IntervalSec"), -1) == 60 &&
                Gd.Int(Gd.Get(D(d), "QuietSec"), -1) == 1800 && Gd.Int(Gd.Get(D(d), "MaxInterventions"), -1) == 1);

            d = Supervisor.Settings(Inp("supervisor", false));
            b.Chk("supervisor=false 才关", () => !(bool)Gd.Get(D(d), "Enabled"));
            d = Supervisor.Settings(Inp("supervisor", true));
            b.Chk("supervisor=true 是开", () => (bool)Gd.Get(D(d), "Enabled"));
            d = Supervisor.Settings(Inp("supervisor", 0));
            b.Chk("数值 0 按 PS 等值(x -ne $false)算关", () => !(bool)Gd.Get(D(d), "Enabled"));
            d = Supervisor.Settings(Inp("supervisor", "False"));
            b.Chk("字符串 'False' 是关", () => !(bool)Gd.Get(D(d), "Enabled"));
            d = Supervisor.Settings(Inp("supervisor", "false"));
            b.Chk("字符串 'false' 大小写不敏感也是关", () => !(bool)Gd.Get(D(d), "Enabled"));
            d = Supervisor.Settings(Inp("supervisor", "yes"));
            b.Chk("其余字符串按 PS 等值算开(只认 \"False\")", () => (bool)Gd.Get(D(d), "Enabled"));

            d = Supervisor.Settings(Inp("intervalSeconds", 5));
            b.Chk("间隔<15s 归 60(拍太快没意义)", () => Gd.Int(Gd.Get(D(d), "IntervalSec"), -1) == 60);
            d = Supervisor.Settings(Inp("intervalSeconds", 90));
            b.Chk("间隔 90s 原样", () => Gd.Int(Gd.Get(D(d), "IntervalSec"), -1) == 90);
            d = Supervisor.Settings(Inp("intervalSeconds", 15));
            b.Chk("间隔踩线 15s 不钳(<15 才钳)", () => Gd.Int(Gd.Get(D(d), "IntervalSec"), -1) == 15);
            d = Supervisor.Settings(Inp("intervalSeconds", 0));
            b.Chk("间隔 0 是假值 → 走缺省 60", () => Gd.Int(Gd.Get(D(d), "IntervalSec"), -1) == 60);

            d = Supervisor.Settings(Inp("quietMinutes", 1));
            b.Chk("静止阈值下限 5 分钟(=300 秒)", () => Gd.Int(Gd.Get(D(d), "QuietSec"), -1) == 300);
            d = Supervisor.Settings(Inp("quietMinutes", 45));
            b.Chk("静止阈值 45 分 → 2700 秒", () => Gd.Int(Gd.Get(D(d), "QuietSec"), -1) == 2700);

            d = Supervisor.Settings(Inp("maxInterventions", 0));
            b.Chk("额度可为 0(纯报告模式，0 不是 null)", () => Gd.Int(Gd.Get(D(d), "MaxInterventions"), -1) == 0);
            d = Supervisor.Settings(Inp("maxInterventions", -3));
            b.Chk("负额度钳 0", () => Gd.Int(Gd.Get(D(d), "MaxInterventions"), -1) == 0);
            d = Supervisor.Settings(Inp("maxInterventions", 2));
            b.Chk("额度 2 原样", () => Gd.Int(Gd.Get(D(d), "MaxInterventions"), -1) == 2);
        }

        // ---------- parse-supervisor-state：done/dead/quiet/busy，判据顺序是认真的 ----------
        static void StateGroup(Box b)
        {
            b.Chk("完成线压死一切：进程没了也判 done", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(true, false, 9999, 9999), "everAlive", true))) == "done");
            b.Chk("曾活着+现在没了+没完成 = dead", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(false, false, 10, 10), "everAlive", true))) == "dead");
            b.Chk("助手从没活着过 ≠ dead(开轮太早)", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(false, false, 9999, 10), "everAlive", false))) == "busy");
            b.Chk("两本日志都超阈值才 quiet", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(false, true, 9999, 9999), "quietSec", 1800))) == "quiet");
            b.Chk("只有助手日志老 = 不算静止", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(false, true, 9999, 10), "quietSec", 1800))) == "busy");
            b.Chk("只有主日志老 = 不算静止", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(false, true, 10, 9999), "quietSec", 1800))) == "busy");
            b.Chk("刚好踩在阈值上不算静止(严格大于)", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(false, true, 1800, 1800), "quietSec", 1800))) == "busy");
            b.Chk("ages 缺失按 0 收 → busy", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(false, true, null, null)))) == "busy");
            b.Chk("Decimal 年龄(信封里小数)也参与比较", () =>
                StateOf(Supervisor.State(Inp("evidence", Ev(false, true, 1800.5m, 1801.25m), "quietSec", 1800))) == "quiet");
        }

        // ---------- resolve-supervisor-action：决策表 + 四句 Why 逐字 ----------
        static void ActionGroup(Box b)
        {
            b.Chk("busy 不动", () => KindOf(Supervisor.Action(Inp("state", "busy"))) == "none");
            b.Chk("done 不动", () => KindOf(Supervisor.Action(Inp("state", "done"))) == "none");
            b.Chk("DONE 大写也不动(PS -eq 大小写不敏感)", () => KindOf(Supervisor.Action(Inp("state", "DONE"))) == "none");
            object r = Supervisor.Action(Inp("state", "dead"));
            b.Chk("dead 通知一次，话术逐字", () =>
                KindOf(r) == "notify" && WhyOf(r) == "助手进程不见了，而这一轮还没宣布完成");
            b.Chk("dead 通知过就闭嘴", () =>
                KindOf(Supervisor.Action(Inp("state", "dead", "alreadyNotified", true))) == "none");
            b.Chk("静止一拍不动手(可能是抖动)", () =>
                KindOf(Supervisor.Action(Inp("state", "quiet", "quietStreak", 1, "screen", Scr(true, false, "x", ""), "canRestart", true))) == "none");
            r = Supervisor.Action(Inp("state", "quiet", "quietStreak", 2, "screen", Scr(true, true, "活动弹窗", ""), "canRestart", true));
            b.Chk("blocking 交给主流程兜底，话术逐字", () =>
                KindOf(r) == "none" && WhyOf(r) == "画面被「活动弹窗」挡着，交给主流程兜底");
            r = Supervisor.Action(Inp("state", "quiet", "quietStreak", 2, "screen", Scr(true, false, "日常界面", ""), "canRestart", true,
                "interventions", 0, "maxInterventions", 1));
            b.Chk("静止+画面干净+许重启 = restart_assistant，话术逐字", () =>
                KindOf(r) == "restart_assistant" && WhyOf(r) == "连续静止且画面没有挡路的东西(日常界面)，判定层没醒，重启助手");
            r = Supervisor.Action(Inp("state", "quiet", "quietStreak", 2, "screen", Scr(true, false, "日常界面", ""), "canRestart", true,
                "interventions", 1, "maxInterventions", 1));
            b.Chk("额度用完退回 notify，话术逐字", () =>
                KindOf(r) == "notify" && WhyOf(r) == "连续静止、画面没有弹窗，但这一路不许自动重启(或额度用完)");
            b.Chk("这一路不许重启 → 只 notify", () =>
                KindOf(Supervisor.Action(Inp("state", "quiet", "quietStreak", 2, "screen", Scr(true, false, "日常界面", ""), "canRestart", false))) == "notify");
            r = Supervisor.Action(Inp("state", "quiet", "quietStreak", 2, "screen", Scr(false, false, "", "截图失败")));
            b.Chk("看不清画面只 notify，话术逐字带上下文", () =>
                KindOf(r) == "notify" && WhyOf(r) == "连续静止，且看不清画面(截图失败)");
            r = Supervisor.Action(Inp("state", "quiet", "quietStreak", 2, "screen", Scr(false, false, "", "")));
            b.Chk("没上下文就不缀括号", () => KindOf(r) == "notify" && WhyOf(r) == "连续静止，且看不清画面");
            b.Chk("notify 说过不再重复轰炸", () =>
                KindOf(Supervisor.Action(Inp("state", "quiet", "quietStreak", 3, "screen", Scr(false, false, "", ""), "alreadyNotified", true))) == "none");
            b.Chk("未知状态按旧实现 fall-through 走 quiet 流程(一拍仍不动)", () =>
                KindOf(Supervisor.Action(Inp("state", "whatever", "quietStreak", 1))) == "none");
        }

        // ---------- compose-supervisor-detail：假文档钉整段话术 ----------
        static void DetailGroup(Box b)
        {
            b.Chk("喂 null(从没监督过)返回空数组而不是抛", () => LinesOf(Supervisor.Detail(Inp())).Count == 0);

            object beat = Inp("ts", "2026-09-26 13:05:00", "games", new object[] {
                Inp("game", "t1", "state", "quiet", "alive", true, "asst_age", 1810, "done", false,
                    "screen", "clear", "action", "notify", "why", "连续静止，额度用完"),
                Inp("game", "t2", "state", "dead", "alive", false, "asst_age", -1, "done", true,
                    "screen", null, "action", "none", "why", "") });
            object doc = Inp("pid", 4321, "owner", 1, "started", "2026-09-26 13:00:00", "games", "t1+t2",
                "updated", "2026-09-26 13:05:00", "stopped", "", "beats", new object[] { beat }, "actions", new object[0]);
            List<string> ls = LinesOf(Supervisor.Detail(Inp("sv", doc, "date", "20260926")));
            b.Chk("现值头把 PID/路线/上岗/最后一拍念全", () =>
                ls.Count > 0 && ls[0] == "现值：PID 4321 · 盯的路线 t1+t2 · 上岗 2026-09-26 13:00:00 · 最后一拍 2026-09-26 13:05:00");
            b.Chk("没收工不许出现「收工」二字", () => !string.Join(" ", ls.ToArray()).Contains("收工"));
            b.Chk("流水行走注入的日期戳", () => HasLine(ls, @"流水：logs\supervision-20260926.jsonl(一行一拍，留 7 天)"));
            b.Chk("一拍两路 → 展开成两行(谁都没被合并)", () => LineCountWith(ls, "  13:05:00 ") == 2);
            b.Chk("asst_age=-1 说成\"读不到\"，不是\"-1秒\"", () =>
                HasLine(ls, "t2=dead 助手日志静默 读不到") && !HasLine(ls, "-1秒"));
            b.Chk("画面没问过说\"没问\"", () => HasLine(ls, "画面 没问"));
            b.Chk("数值 age 后缀\"秒\"", () => HasLine(ls, "助手日志静默 1810秒"));
            b.Chk("没动作说 none，有动作原样念", () => HasLine(ls, "动作 none") && HasLine(ls, "动作 notify"));
            b.Chk("why 为空的那一路不缀\"原因 \"", () => LineCountWith(ls, "原因 ") == 1);
            b.Chk("没介入过 → 明说没有", () => HasLine(ls, "这一轮没有介入过(它唯一的干预是重启助手，重启过会记在这里)。"));
            b.Chk("整段输出不含 '|'(面板协议分隔符)", () => !string.Join("\n", ls.ToArray()).Contains("|"));

            object docStopped = Inp("pid", 4321, "owner", 1, "started", "2026-09-26 13:00:00", "games", "t1+t2",
                "updated", "2026-09-26 13:05:00", "stopped", "2026-09-26 13:20:00", "beats", new object[0],
                "actions", new object[] { Inp("ts", "13:06:00", "game", "t1", "kind", "restart_assistant", "ok", false, "why", "重启没成") });
            ls = LinesOf(Supervisor.Detail(Inp("sv", docStopped, "date", "20260926")));
            b.Chk("收工时间真收工时出现在头行", () => ls[0].EndsWith(" · 收工 2026-09-26 13:20:00"));
            b.Chk("拍被挤掉仍报\"还没落下第一拍\"，且不吞介入记录", () =>
                HasLine(ls, "这一枚监督进程还没落下第一拍(起拍间隔默认 60 秒)。") &&
                HasLine(ls, "介入记录(1 次)：") && HasLine(ls, "  13:06:00 t1 restart_assistant 没成 — 重启没成"));

            object b1 = Inp("ts", "2026-09-26 13:01:00", "games", new object[] { Inp("game", "t1", "state", "busy", "alive", true, "asst_age", 5, "done", false, "screen", "", "action", "none", "why", "") });
            object b2 = Inp("ts", "2026-09-26 13:02:00", "games", new object[] { Inp("game", "t1", "state", "busy", "alive", true, "asst_age", 5, "done", false, "screen", "", "action", "none", "why", "") });
            object b3 = Inp("ts", "2026-09-26 13:03:00", "games", new object[] { Inp("game", "t1", "state", "quiet", "alive", true, "asst_age", 30, "done", false, "screen", "", "action", "none", "why", "") });
            ls = LinesOf(Supervisor.Detail(Inp("sv", Inp("pid", 1, "games", "t1", "started", "s", "updated", "u",
                "beats", new object[] { b1, b2, b3 }, "actions", new object[0]), "maxBeats", 2, "date", "20260926")));
            b.Chk("MaxBeats=2 → 只念最后两拍，头行说\"最近 2 拍\"", () =>
                HasLine(ls, "最近 2 拍(字段：状态 / 助手日志静默 / 进程 / 完成线 / 画面 / 动作)：") &&
                LineCountWith(ls, "  13:0") == 2 && HasLine(ls, "13:02:00") && HasLine(ls, "13:03:00") && !HasLine(ls, "13:01:00"));
            ls = LinesOf(Supervisor.Detail(Inp("sv", Inp("pid", 1, "games", "t1", "started", "s", "updated", "u",
                "beats", new object[] { b1 }, "actions", new object[0]), "maxBeats", 0, "date", "20260926")));
            b.Chk("MaxBeats=0 钳到 1 拍", () => HasLine(ls, "最近 1 拍"));
            b.Chk("screen 空串也走\"没问\"", () => HasLine(ls, "画面 没问"));
        }
    }
}
