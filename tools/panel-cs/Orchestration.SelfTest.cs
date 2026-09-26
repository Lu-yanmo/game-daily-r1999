// tools/panel-cs/Orchestration.SelfTest.cs —— --orchtest：批次 5(Orchestration.cs)的无头自检。
// 期望串逐字抄自 lib\ark-farm.ps1 / lib\ark-daily.ps1 / lib\r1999-daily.ps1 / lib\maa.ps1 / run-via-task.ps1
// 的旧实现(tools\test-ark.ps1 的 farm 段口径同一批) —— 这里红了就是移植走样，改 Orchestration.cs，
// 不改这里的期望(§7 同一条纪律)。
// 批次纪律：不起进程、不读盘、不写文件 —— 目标清单/账本/日志片段/任务参数串全部由场景直接喂进来。
// 已知局限(与 Orchestration.cs 头注同一条)：挑关排序在两键全并列时旧侧本就不可押，场景全部避开全并列。

using System;
using System.Collections.Generic;

namespace GdPanel
{
    static class GdOrchSelfTest
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

        static Dictionary<string, object> Obj(params object[] kv) { return Inp(kv); }

        static bool Bool(object v) { return Gd.Truthy(v); }
        static int IntOf(object v)
        {
            try { return Convert.ToInt32(v, System.Globalization.CultureInfo.InvariantCulture); } catch { return int.MinValue; }
        }

        public static int OrchTest()
        {
            Box b = new Box();
            SpecGroup(b);
            DropGroup(b);
            PlanGroup(b);
            ArkIntentGroup(b);
            R1999Group(b);
            MaaGuiGroup(b);
            TaskArgsGroup(b);
            return b.Result("--orchtest");
        }

        // ---------- 目标清单解析(原 Parse-GdArkFarmTargetSpec) ----------
        static void SpecGroup(Box b)
        {
            Dictionary<string, object> p1 = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", "D32钢=30@CE-5")), "Spec"));
            b.Chk("名=数@关 拆得开", () => Bool(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", "D32钢=30@CE-5")), "Found"))
                && (string)p1["Key"] == "D32钢" && IntOf(p1["Target"]) == 30
                && Gd.List(p1["Stages"]).Length == 1 && Gd.Str(p1["Bad"]) == "");
            Dictionary<string, object> p2 = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", "30135=12@CE-5,CE-4")), "Spec"));
            b.Chk("代号+多关(逗号/顿号分隔)拆得开", () => (string)p2["Key"] == "30135" && Gd.List(p2["Stages"]).Length == 2);
            Dictionary<string, object> p2b = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", "30135=12@CE-5、CE-4；LS-5")), "Spec"));
            b.Chk("顿号/分号/空白都是关卡分隔", () => Gd.List(p2b["Stages"]).Length == 3);
            Dictionary<string, object> p3 = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw",
                Obj("item", "D32钢", "target", 20, "stages", new object[] { "CE-5" }))), "Spec"));
            b.Chk("json 对象写法也吃", () => (string)p3["Key"] == "D32钢" && IntOf(p3["Target"]) == 20 && Gd.Str(p3["Bad"]) == "");
            Dictionary<string, object> p3b = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw",
                Obj("item", "D32钢", "target", 20, "stages", new object[] { "CE-5，CE-4" }))), "Spec"));
            b.Chk("stages 只给一条串时按分隔符再切(旧 $s.Count -eq 1 分支)", () => Gd.List(p3b["Stages"]).Length == 2);
            Dictionary<string, object> p3c = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw",
                Obj("name", "或铝石", "count", 7))), "Spec"));
            b.Chk("name/count 别名也吃，没关卡就是空数组", () => (string)p3c["Key"] == "或铝石" && IntOf(p3c["Target"]) == 7
                && Gd.List(p3c["Stages"]).Length == 0);
            Dictionary<string, object> p4 = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", "想刷点火腿")), "Spec"));
            b.Chk("看不懂的行给 Bad 说明而不是丢掉", () => Gd.Str(p4["Bad"]).Contains("材料名=数量") && (string)p4["Raw"] == "想刷点火腿");
            Dictionary<string, object> p5 = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", Obj("item", "D32钢"))), "Spec"));
            b.Chk("对象缺数量也给 Bad(Raw 回显键名)", () => Gd.Str(p5["Bad"]).Contains("目标数量") && (string)p5["Raw"] == "D32钢");
            Dictionary<string, object> p6 = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", Obj("target", 5))), "Spec"));
            b.Chk("对象连 item 都没有 → Bad 里点名(item)，Raw 回压缩 JSON", () =>
                (string)p6["Bad"] == "条目里没有材料名/代号(item)" && (string)p6["Raw"] == "{\"target\":5}");
            b.Chk("raw 缺失/空串 → Found:false(旧实现返回 $null)", () =>
                !Bool(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", null)), "Found")) &&
                !Bool(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", "   ")), "Found")));
            Dictionary<string, object> p7 = D(Gd.Get(Orchestration.FarmTargetSpec(Inp("raw", "D32钢＝30 @ CE-5")), "Spec"));
            b.Chk("全角等号与 @ 前后空格都认(旧正则 [=＝] 与 \\s*@\\s*)", () =>
                IntOf(p7["Target"]) == 30 && Gd.List(p7["Stages"]).Length == 1);
        }

        // ---------- 掉落文本解析(原 Parse-GdArkDropText) ----------
        static void DropGroup(Box b)
        {
            string sample = "[08:50:26] CE-5 掉落统计:\nD32钢 : 2 (+2)   或铝石 : 1 (+1)   当前次数 : 3\n"
                + "[08:52:10] LS-5 掉落统计:\n采购凭证 : 30 (30)";
            object[] blks = Gd.List(Gd.Get(Orchestration.FarmDropText(Inp("text", sample)), "Blocks"));
            b.Chk("两个块、关卡代号抓得到", () => blks.Length == 2
                && (string)Gd.Get(blks[0], "Stage") == "CE-5" && (string)Gd.Get(blks[1], "Stage") == "LS-5");
            object[] items0 = Gd.List(Gd.Get(blks[0], "Items"));
            b.Chk("时间戳前缀不影响识别；「当前次数」被拒绝表挡掉", () => items0.Length == 2
                && (string)Gd.Get(items0[0], "Name") == "D32钢" && IntOf(Gd.Get(items0[0], "Delta")) == 2);
            object[] items1 = Gd.List(Gd.Get(blks[1], "Items"));
            b.Chk("只按 (+N) 记增量，\"(30)\"这种总数不当增量", () =>
                items1.Length == 1 && IntOf(Gd.Get(items1[0], "Delta")) == 0);
            b.Chk("空文本给空数组不抛", () =>
                Gd.List(Gd.Get(Orchestration.FarmDropText(Inp("text", "")), "Blocks")).Length == 0);
            object[] b2 = Gd.List(Gd.Get(Orchestration.FarmDropText(Inp("text", "<12> PR-B-2 掉落统计:术师芯片组 : 2 (+2)")), "Blocks"));
            b.Chk("行首 <n> 前缀剥掉后块头照样认", () => b2.Length == 1 && (string)Gd.Get(b2[0], "Stage") == "PR-B-2");
            object[] b3 = Gd.List(Gd.Get(Orchestration.FarmDropText(Inp(
                "text", "CE-4 掉落统计:\nD32钢 : 1 (+1)\n这是一句普通说明\n30135 : 9 (+9)")), "Blocks"));
            b.Chk("出现不像配额的行就算块结束(后面不再入账)", () => b3.Length == 1 && Gd.List(Gd.Get(b3[0], "Items")).Length == 1);
        }

        // ---------- 补刷判表(原 Resolve-ArkFarmPlan) ----------
        static readonly Dictionary<string, object> N2I = Obj("D32钢", "30135", "或铝石", "30115");
        static readonly Dictionary<string, object> I2N = Obj("30135", "D32钢", "30115", "或铝石");
        static readonly Dictionary<string, object> Known = Obj("CE-5", true, "CE-4", true);
        static readonly Dictionary<string, object> DT = Obj("30135", Obj(
            "CE-5", Obj("runs", 5, "qty", 12),
            "CE-4", Obj("runs", 3, "qty", 3)));

        static Dictionary<string, object> Spec(string key, int target, params object[] stages)
        {
            return Obj("Key", key, "Target", target, "Stages", stages, "Raw", key + "=" + target, "Bad", "");
        }

        static void PlanGroup(Box b)
        {
            Dictionary<string, object> r0 = D(Orchestration.FarmPlan(Inp("targets", new object[0])));
            b.Chk("空清单不炸，理由说\"没配 farmTargets\"", () =>
                !Bool(r0["Active"]) && Gd.Str(r0["Reason"]).Contains("farmTargets"));

            Dictionary<string, object> inp1 = Obj("targets", new object[] { Spec("D32钢", 30, "CE-5") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj("30135", 18),
                "origin", Obj(), "dropTable", DT, "knownStages", Known, "maxTimes", 12);
            Dictionary<string, object> rf = D(Orchestration.FarmPlan(inp1));
            b.Chk("缺口=目标-现有；显式填关优先；次数按学习掉落率折算", () =>
                Bool(rf["Active"]) && IntOf(rf["Gap"]) == 12 && (string)Gd.List(rf["Stages"])[0] == "CE-5"
                && IntOf(rf["Times"]) == 5);
            b.Chk("Reason 说清账目(逐字口径：还差 12 / 每场约 2.4)", () => {
                string s = Gd.Str(rf["Reason"]);
                return s.Contains("还差 12") && s.Contains("每场约 2.4") && s.StartsWith("补刷 D32钢×5 @CE-5：目标 30，现有 18");
            });

            Dictionary<string, object> inpCap = Obj("targets", new object[] { Spec("D32钢", 99, "CE-5") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj(),
                "origin", Obj(), "dropTable", DT, "knownStages", Known, "maxTimes", 4);
            Dictionary<string, object> rc = D(Orchestration.FarmPlan(inpCap));
            b.Chk("单轮封顶生效并在 Reason 里说\"明天接着刷\"", () =>
                IntOf(rc["Times"]) == 4 && Gd.Str(rc["Reason"]).Contains("(单轮封顶 4，剩下的明天接着刷)"));

            Dictionary<string, object> inpG = Obj("targets", new object[] { Spec("D32钢", 10, "CE-5") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj("30135", 12),
                "origin", Obj(), "dropTable", DT, "knownStages", Known, "maxTimes", 12);
            Dictionary<string, object> rg = D(Orchestration.FarmPlan(inpG));
            b.Chk("已够的目标不刷，理由里带\"已够(12/10)\"", () =>
                !Bool(rg["Active"]) && Gd.Str(rg["Reason"]).Contains("D32钢 已够(12/10)"));

            Dictionary<string, object> inpO = Obj("targets", new object[] { Spec("30135", 30, "CE-9") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj(),
                "origin", Obj(), "dropTable", Obj(), "knownStages", Known, "maxTimes", 12);
            Dictionary<string, object> ro = D(Orchestration.FarmPlan(inpO));
            b.Chk("只填了不认识的关 → 不刷并说为什么", () =>
                !Bool(ro["Active"]) && Gd.Str(ro["Reason"]).Contains("不被 MAA 关卡表认识"));

            Dictionary<string, object> inpU = Obj("targets", new object[] { Spec("点火腿", 5) },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj(),
                "origin", Obj(), "dropTable", Obj(), "knownStages", Obj(), "maxTimes", 12);
            Dictionary<string, object> ru = D(Orchestration.FarmPlan(inpU));
            b.Chk("材料表查不到的名字跳过并说明(不静默)", () =>
                !Bool(ru["Active"]) && Gd.Str(ru["Reason"]).Contains("item_index"));

            Dictionary<string, object> inpN = Obj("targets", new object[] { Spec("D32钢", 5) },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj(),
                "origin", Obj(), "dropTable", Obj(), "knownStages", Obj(), "maxTimes", 12);
            Dictionary<string, object> rn = D(Orchestration.FarmPlan(inpN));
            b.Chk("没学习数据又没填关 → 提示写成带 @关卡 的形式", () =>
                !Bool(rn["Active"]) && Gd.Str(rn["Reason"]).Contains("@CE-5 这种带关卡的形式"));

            Dictionary<string, object> inpE = Obj("targets", new object[] { Spec("D32钢", 8, "CE-5") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj("30135", 4),
                "origin", Obj("30135", "depot"), "dropTable", Obj(), "knownStages", Obj(), "maxTimes", 12);
            Dictionary<string, object> re = D(Orchestration.FarmPlan(inpE));
            b.Chk("depot 标定的现值明说\"是估算\"；无学习数据按每场 1 估", () => {
                string s = Gd.Str(re["Reason"]);
                return s.Contains("估算") && s.Contains("按每场 1 估") && IntOf(re["Times"]) == 4;
            });

            Dictionary<string, object> inpB = Obj("targets", new object[] { Spec("D32钢", 20, "CE-5"), Spec("或铝石", 100, "CE-4") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj("30135", 18, "30115", 90),
                "origin", Obj(), "dropTable", Obj(), "knownStages", Obj(), "maxTimes", 12);
            Dictionary<string, object> rb = D(Orchestration.FarmPlan(inpB));
            b.Chk("多目标一次只锁缺口最大的那个(或铝石差10)", () =>
                (string)rb["ItemName"] == "或铝石" && IntOf(rb["Gap"]) == 10);

            // 学习表挑关：CE-5 平均 2.4 > CE-4 平均 1，两关都进(最多2)，合计均值 15/8=1.875 → 串面 1.9
            // (1.875 不是 1 位小数的中点[那得是 1.85]，ToEven 不生效 —— 实测 [math]::Round(15/8,1) 两代 PS 都给 1.9)
            Dictionary<string, object> inpL = Obj("targets", new object[] { Spec("D32钢", 30) },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj("30135", 18),
                "origin", Obj(), "dropTable", DT, "knownStages", Known, "maxTimes", 12);
            Dictionary<string, object> rl = D(Orchestration.FarmPlan(inpL));
            b.Chk("没填关 → 学习表挑关(平均掉得多的打头，最多 2 关)", () => {
                object[] st = Gd.List(rl["Stages"]);
                return st.Length == 2 && (string)st[0] == "CE-5" && (string)st[1] == "CE-4" && IntOf(rl["Times"]) == 7;
            });
            b.Chk("合计均值的串面 1.9(1.875 不是中点，ToEven 不生效，实测两代 PS 都给 1.9)", () =>
                Gd.Str(rl["Reason"]).Contains("每场约 1.9"));

            Dictionary<string, object> inpCode = Obj("targets", new object[] { Spec("30115", 20, "CE-4") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj(),
                "origin", Obj(), "dropTable", Obj(), "knownStages", Obj(), "maxTimes", 12);
            Dictionary<string, object> rcode = D(Orchestration.FarmPlan(inpCode));
            b.Chk("键是代号时 ItemName 反查回材料名(Reason 用名不用号)，无学习数据按 1 估并封顶", () =>
                (string)rcode["ItemName"] == "或铝石" && Gd.Str(rcode["Reason"]).StartsWith("补刷 或铝石×12 @CE-4：目标 20，现有 0，还差 20，掉落率未知，按每场 1 估"));

            Dictionary<string, object> inpMix = Obj("targets", new object[] {
                Obj("Key", "点火腿", "Target", 0, "Stages", new object[0], "Raw", "想刷点火腿", "Bad", "看不懂「想刷点火腿」，要写成 材料名=数量[@关卡1,关卡2]"),
                Spec("D32钢", 20, "CE-5") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj("30135", 18),
                "origin", Obj(), "dropTable", Obj(), "knownStages", Obj(), "maxTimes", 12);
            Dictionary<string, object> rmix = D(Orchestration.FarmPlan(inpMix));
            b.Chk("坏行不进方案但 Notes 里留痕(汇总着说，不静默丢)", () =>
                Bool(rmix["Active"]) && Gd.List(rmix["Notes"]).Length == 1
                && Gd.Str(Gd.List(rmix["Notes"])[0]).StartsWith("跳过「想刷点火腿」"));

            Dictionary<string, object> inpZero = Obj("targets", new object[] { Spec("D32钢", 3, "CE-5") },
                "nameToId", N2I, "idToName", I2N, "inventory", Obj(),
                "origin", Obj(), "dropTable", Obj(), "knownStages", Obj(), "maxTimes", 0);
            Dictionary<string, object> rz = D(Orchestration.FarmPlan(inpZero));
            b.Chk("MaxTimes<1 兜成 1(旧 [int] 兜底)，封顶句跟着说 1", () =>
                IntOf(rz["Times"]) == 1 && Gd.Str(rz["Reason"]).Contains("单轮封顶 1"));
        }

        // ---------- 方舟本轮意图(原 Resolve-GdArkDecision 构造段) ----------
        static readonly Dictionary<string, object> IdleArk = Obj("Mode", "idle", "Stage", "CE-5,LS-5", "StageHint", "CE-5,LS-5",
            "ClearSanity", true, "Reason", "长草", "EventKey", null, "EventTip", null, "Source", "weekday");
        static readonly Dictionary<string, object> PendArk = Obj("Mode", "event-pending", "Stage", null, "StageHint", "CT-1",
            "ClearSanity", false, "Reason", "等新活动确认", "EventKey", "act1", "EventTip", "sidestory", "Source", "event");

        static void ArkIntentGroup(Box b)
        {
            Dictionary<string, object> m = D(Gd.Get(Orchestration.ArkIntent(Inp("ark", PendArk, "stage", "OF-1", "force", false, "farm", false)), "Ark"));
            b.Chk("-Stage 手造 manual(逐字：命令行指定刷 OF-1；Source=manual)", () =>
                (string)m["Mode"] == "manual" && (string)m["Stage"] == "OF-1" && (string)m["StageHint"] == "OF-1"
                && (string)m["Reason"] == "命令行指定刷 OF-1" && (string)m["Source"] == "manual" && Bool(m["ClearSanity"]));
            b.Chk("manual 档 EventKey/EventTip 是给过的空值(不是残留活动键)", () =>
                m["EventKey"] == null && m["EventTip"] == null);

            Dictionary<string, object> r = D(Orchestration.ArkIntent(Inp("ark", PendArk, "stage", "", "force", true, "farm", false)));
            Dictionary<string, object> f = D(r["Ark"]);
            b.Chk("-Force 跳确认门 → event 档取 StageHint，Reason 带(-Force)原话", () =>
                (string)f["Mode"] == "event" && (string)f["Stage"] == "CT-1"
                && (string)f["Reason"] == "(-Force) 跳过活动确认门，直接刷 CT-1"
                && (string)f["EventKey"] == "act1" && (string)f["EventTip"] == "sidestory" && (string)f["Source"] == "event");
            b.Chk("-Force 的 WARN 文案原样交给壳去打", () =>
                Gd.List(r["Warnings"]).Length == 1 && Gd.Str(Gd.List(r["Warnings"])[0]) == "-Force：本轮忽略活动确认门");

            Dictionary<string, object> fn = D(Orchestration.ArkIntent(Inp("ark", IdleArk, "stage", "", "force", true, "farm", false)));
            b.Chk("-Force 对非 event-pending 不动手(空 Warn，ark 原样)", () =>
                Gd.List(fn["Warnings"]).Length == 0 && (string)D(fn["Ark"])["Mode"] == "idle"
                && !Bool(fn["HasFarmFields"]));

            Dictionary<string, object> fd = D(Orchestration.ArkIntent(Inp("ark", IdleArk, "stage", "", "force", false, "farm", true,
                "farmOpt", Obj("Stages", new object[] { "PR-D-2", "CE-5" }, "Reason", "补刷 特种芯片组×8 @PR-D-2"))));
            Dictionary<string, object> dd = D(fd["Ark"]);
            b.Chk("专用轮 Mode=farm、Stage 只挂 farm 关(逗号拼)", () =>
                (string)dd["Mode"] == "farm" && (string)dd["Stage"] == "PR-D-2,CE-5"
                && (string)dd["Reason"] == "材料补刷专用轮 — 补刷 特种芯片组×8 @PR-D-2");
            b.Chk("专用轮键抄自原 ark(EventKey/EventTip/Source)", () =>
                dd["EventKey"] == null && (string)dd["Source"] == "weekday" && (string)fd["FarmMode"] == "dedicated"
                && Bool(fd["HasFarmFields"]));

            Dictionary<string, object> fa = D(Orchestration.ArkIntent(Inp("ark", Obj("Mode", "idle", "Stage", "CE-5,LS-5,CE-5",
                "StageHint", "CE-5,LS-5", "ClearSanity", true, "Reason", "长草", "EventKey", null, "EventTip", null, "Source", "weekday"),
                "stage", "", "force", false, "farm", false,
                "farmOpt", Obj("Stages", new object[] { "PR-D-2", "CE-5" }, "Reason", "补刷 特种芯片组×8 @PR-D-2"))));
            Dictionary<string, object> da = D(fa["Ark"]);
            b.Chk("需求优先档：farm 关排最前且去重(CE-5 不重复出现)", () =>
                (string)da["Stage"] == "PR-D-2,CE-5,LS-5" && (string)fa["FarmMode"] == "append");
            b.Chk("append 的 Reason 用「；需求优先 」拼接(逐字)", () =>
                (string)da["Reason"] == "长草；需求优先 补刷 特种芯片组×8 @PR-D-2");
            b.Chk("append 档不动 Mode/StageHint/ClearSanity(只改 Stage/Reason)", () =>
                (string)da["Mode"] == "idle" && (string)da["StageHint"] == "CE-5,LS-5" && Bool(da["ClearSanity"]));

            Dictionary<string, object> fl = D(Orchestration.ArkIntent(Inp("ark", IdleArk, "stage", "", "force", false, "farm", true)));
            b.Chk("-Farm 但本轮没有可补 → 原方案 + FarmMode 空 + WARN 文案", () =>
                (string)fl["FarmMode"] == "" && Bool(fl["HasFarmFields"])
                && Gd.Str(Gd.List(fl["Warnings"])[0]) == "-Farm 本轮没有可补的材料 → 按原方案跑"
                && (string)D(fl["Ark"])["Mode"] == "idle");
        }

        // ---------- 1999 本轮意图(原 Resolve-GdR1999Decision 构造段) ----------
        static readonly Dictionary<string, object> PendR = Obj("Mode", "event-pending", "Instance", "idle", "Version", "v4.7.1",
            "VersionName", "测试版本", "EventKey", "act123", "EventTip", "活动", "Reason", "待确认", "DataAge", null);

        static void R1999Group(Box b)
        {
            Dictionary<string, object> r = D(Orchestration.R1999Intent(Inp("decision", PendR, "plan", "auto", "force", true)));
            Dictionary<string, object> d = D(r["Decision"]);
            b.Chk("-Force → event/event，Reason 与 WARN 逐字", () =>
                (string)d["Mode"] == "event" && (string)d["Instance"] == "event"
                && (string)d["Reason"] == "(-Force) 跳过 1999 活动确认门，直接用活动方案"
                && (string)d["Version"] == "v4.7.1" && (string)d["VersionName"] == "测试版本"
                && (string)d["EventKey"] == "act123" && d["DataAge"] == null
                && Gd.Str(Gd.List(r["Warnings"])[0]) == "-Force：1999 本轮忽略活动确认门");

            Dictionary<string, object> r2 = D(Orchestration.R1999Intent(Inp("decision", PendR, "plan", "idle", "force", true)));
            Dictionary<string, object> d2 = D(r2["Decision"]);
            b.Chk("-Plan≠auto 无条件覆盖(连 Force 的结果也覆盖)：Version=manual/VersionName=命令行指定", () =>
                (string)d2["Mode"] == "idle" && (string)d2["Instance"] == "idle" && (string)d2["Version"] == "manual"
                && (string)d2["VersionName"] == "命令行指定" && (string)d2["Reason"] == "命令行指定 idle"
                && d2["EventKey"] == null && d2["DataAge"] == null);

            Dictionary<string, object> r3 = D(Orchestration.R1999Intent(Inp("decision", PendR, "plan", "auto", "force", false)));
            b.Chk("两样都不给 → decision 原样、零 Warn", () =>
                Gd.List(r3["Warnings"]).Length == 0 && (string)D(r3["Decision"])["Mode"] == "event-pending");
        }

        // ---------- gui.log 行分类(原 Invoke-MaaRun 每行段) ----------
        static void MaaGuiGroup(Box b)
        {
            string text = "[12:00:00] Completed Task Chain: Recruit\n<12> 任务已全部完成\n<12> 用时 3 分钟 (03:12)\n"
                + "<13> 无更新   请检查客户端   <14>\n<15> 任务已完成，但出现错误\n<16> [ERR] 900 连接失败 something\n"
                + "<17> 任务失败：作战\n<18> 一条普通日志\n";
            Dictionary<string, object> res = D(Orchestration.MaaGuiLines(Inp("text", text, "benign", new object[] { @"连接失败 something$" })));
            object[] evs = Gd.List(res["Events"]);
            b.Chk("五种标志各归各类(顺序按行)", () => {
                string[] kinds = new string[evs.Length];
                for (int i = 0; i < evs.Length; i++) kinds[i] = Gd.Str(Gd.Get(evs[i], "Kind"));
                return string.Join(",", kinds) == "chain,finished,stall,drained,benign,error";
            });
            b.Chk("marker 计数=认识的行数(普通行不算)", () => IntOf(res["Markers"]) == 6);
            b.Chk("logChars 只加非空白行的长度", () => {
                int want = 0;
                foreach (string l in text.Split('\n')) if (l.Trim().Length > 0) want += l.TrimEnd('\r').Length;
                return IntOf(res["LogChars"]) == want;
            });
            b.Chk("完成行的下一行抠「用时」并裁尾(旧 $Matches[1].Trim())", () =>
                (string)Gd.Get(evs[1], "Used") == "3 分钟 (03:12");
            b.Chk("停滞行剥 <n> 前缀并把连续空白压成一个空格", () =>
                (string)Gd.Get(evs[2], "Msg") == "无更新 请检查客户端 <14>");
            b.Chk("benign 命中带出所用模式串(壳按它去重)", () =>
                (string)Gd.Get(evs[4], "Pattern") == @"连接失败 something$" && (string)Gd.Get(evs[4], "Msg") == "连接失败 something");
            b.Chk("「任务失败：<中文名>」算异常线索(行首 <n> 前缀照旧留下——旧清洗只剥到第一个 ] )", () =>
                (string)Gd.Get(evs[5], "Msg") == "<17> 任务失败：作战");

            Dictionary<string, object> r2 = D(Orchestration.MaaGuiLines(Inp("text", "[ERR]\n", "benign", new object[0])));
            b.Chk("清洗后 msg 为空：marker 照记、事件不发(旧分支语义)", () =>
                IntOf(r2["Markers"]) == 1 && Gd.List(r2["Events"]).Length == 0);
            Dictionary<string, object> r3 = D(Orchestration.MaaGuiLines(Inp("text", "", "benign", new object[0])));
            b.Chk("空片段全零不抛", () => IntOf(r3["Markers"]) == 0 && IntOf(r3["LogChars"]) == 0);
            Dictionary<string, object> r4 = D(Orchestration.MaaGuiLines(Inp("text", "Completed Task Chain: Recruit5", "benign", new object[0])));
            b.Chk("链路名只吃字母([A-Za-z]+)，尾巴数字不当链名", () => {
                object[] e4 = Gd.List(r4["Events"]);
                return e4.Length == 1 && (string)Gd.Get(e4[0], "Chain") == "Recruit";
            });
        }

        // ---------- 任务参数切词(原 Get-GdTaskArgsWithGames) ----------
        static void TaskArgsGroup(Box b)
        {
            Dictionary<string, object> r = D(Orchestration.TaskArgs(Inp("text",
                "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File \"C:\\a b\\daily-clear.ps1\" -Tag auto -Games ark")));
            object[] p = Gd.List(r["Pieces"]);
            b.Chk("剥掉原有 -Games/-Tag，带空格引号路径整体成一项", () => {
                string[] s = new string[p.Length];
                for (int i = 0; i < p.Length; i++) s[i] = Gd.Str(p[i]);
                return string.Join("|", s) == "-NoProfile|-WindowStyle|Hidden|-ExecutionPolicy|Bypass|-File|C:\\a b\\daily-clear.ps1";
            });
            Dictionary<string, object> r2 = D(Orchestration.TaskArgs(Inp("text",
                "-NoProfile -File \"x.ps1\" -Games \"ark\" -Tag \"manual\"")));
            b.Chk("值带引号的 -Games/-Tag 也剥得掉(旧 \\\"? 可选引号)", () =>
                Gd.List(r2["Pieces"]).Length == 3 && Gd.Str(Gd.List(r2["Pieces"])[2]) == "x.ps1");
            Dictionary<string, object> r3 = D(Orchestration.TaskArgs(Inp("text", "-File x")));
            b.Chk("没有可剥的就原样切(Count 守卫归壳管)", () => Gd.List(r3["Pieces"]).Length == 2);
        }
    }
}
