// tools/panel-cs/Decisions.SelfTest.cs —— --dectest：批次 1 判定层的无头自检。
// 用例镜像 tools\test-ark.ps1 [t7] / test-r1999 的判定场景，但全部喂临时树 + 内存假数据：
// 不读真配置、不碰 state\ 里的确认门(那套 Save/Restore 隔离是 PS 壳侧的事，C# 侧本来就无状态)。
// 断言里的中文串是从 lib\ark-decisions.ps1 / lib\r1999-decisions.ps1 原话抄来的 ——
// 这里红了就是 C# 移植走样，改 Decisions.cs，不改这里的期望串。

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Text;

namespace GdPanel
{
    static class GdDecisionsSelfTest
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

        // ---------- 假数据小工具 ----------
        static long Ms(string wall) { return Gd.Utc8ToMs(wall).Value; }

        static Dictionary<string, object> Stage(string value, string drop)
        {
            return Gd.Out("Value", value, "Drop", drop);
        }

        static Dictionary<string, object> Activity(string tip, string start, string end)
        {
            return Gd.Out("Tip", tip, "UtcStartTime", start, "UtcExpireTime", end);
        }

        // 一个"分组项"：{ Activity = …, Stages = […] }，与 MAA 缓存同构
        static Dictionary<string, object> EventObj(string tip, string start, string end, params Dictionary<string, object>[] stages)
        {
            return Gd.Out("Activity", Activity(tip, start, end), "Stages", stages);
        }

        static Dictionary<string, object> ArkIn(object data, long now, string source,
            Dictionary<string, object> ack, Dictionary<string, object> shopDone, int dow)
        {
            if (ack == null) ack = new Dictionary<string, object>();
            if (shopDone == null) shopDone = new Dictionary<string, object>();
            Dictionary<string, object> ark = Gd.Out(
                "defaultStage", "CE-5,LS-5",
                "planByWeekday", Gd.Out("3", "LS-6"),
                "eventRequiresAck", true);
            return Gd.Out(
                "ark", ark, "dow", dow, "now", now, "data", data, "source", source,
                "ack", ack, "shopDone", shopDone, "verbAck", "-Ack", "verbShopReset", "-ShopReset");
        }

        static Dictionary<string, object> SideStory(object groups)
        {
            return Gd.Out("Official", Gd.Out("sideStory", groups));
        }

        // ============================ --dectest ============================
        public static int DecTest()
        {
            GdCli.TryAttachConsole();
            Box b = new Box();
            string tmp = Path.Combine(Path.GetTempPath(),
                "gd-dec-" + Process.GetCurrentProcess().Id + "-" + DateTime.Now.ToString("yyyyMMdd-HHmmss") + "-"
                + Guid.NewGuid().ToString("N").Substring(0, 6));
            Directory.CreateDirectory(tmp);
            try
            {
                GdEnv.InitForTest(tmp);

                // 固定的时间线：所有期望串里的 "MM-dd HH:mm" 都从这几个墙上时刻来
                long start = Ms("2026-09-20 04:00:00");
                long end = Ms("2026-09-26 18:00:00");
                long now = Ms("2026-09-23 12:00:00");

                // ---------- Gd 底座语义 ----------
                b.Chk("Truthy：null/空串/0/空集合假，其余真", delegate
                {
                    Dictionary<string, object> empty = new Dictionary<string, object>();
                    return !Gd.Truthy(null) && !Gd.Truthy("") && !Gd.Truthy(0) && !Gd.Truthy(0L)
                        && !Gd.Truthy(empty) && !Gd.Truthy(new object[0])
                        && Gd.Truthy(true) && Gd.Truthy("x") && Gd.Truthy(3) && Gd.Truthy(new object[] { 1 });
                });
                b.Chk("Utc8ToMs：/ 换 -、多空格压缩、坏串 null", delegate
                {
                    return Gd.Utc8ToMs("2026/09/20 04:00:00") == start
                        && Gd.Utc8ToMs("2026-09-20   04:00:00") == start
                        && Gd.Utc8ToMs("2026-09-26") == Ms("2026-09-26 00:00:00")
                        && Gd.Utc8ToMs("") == null && Gd.Utc8ToMs(null) == null && Gd.Utc8ToMs("乱写的") == null;
                });
                b.Chk("Fmt：0/假值 → '?'，UTC+8 展示", delegate
                {
                    return Gd.Fmt(start) == "09-20 04:00" && Gd.Fmt(0) == "?" && Gd.Fmt(null) == "?"
                        && Gd.Fmt(start, "yyyy-MM-dd") == "2026-09-20";
                });
                b.Chk("Str：null→\"\"，bool→True/False(PS 口径)", delegate
                {
                    return Gd.Str(null) == "" && Gd.Str(true) == "True" && Gd.Str(false) == "False" && Gd.Str(12) == "12";
                });
                b.Chk("Out 保序(键顺序即对拍协议)", delegate
                {
                    List<string> keys = new List<string>(Gd.Out("b", 1, "a", 2, "c", 3).Keys);
                    return keys[0] == "b" && keys[1] == "a" && keys[2] == "c";
                });

                // ---------- decide-ark：长草 / 确认门 / 领空 / 提示语 ----------
                b.Chk("ark 无活动 → 长草计划(planByWeekday 没有的 dow 回落 defaultStage)", delegate
                {
                    Dictionary<string, object> a5 = Gd.Map(Decisions.Ark(ArkIn(SideStory(Gd.Out()), now, "假数据", null, null, 5)));
                    Dictionary<string, object> a1 = Gd.Map(Decisions.Ark(ArkIn(SideStory(Gd.Out()), now, "假数据", null, null, 1)));
                    return (string)a5["Mode"] == "idle" && (string)a5["Stage"] == "CE-5,LS-5" && (bool)a5["ClearSanity"]
                        && (string)a5["Reason"] == "无可刷活动关卡，按长草计划刷 CE-5,LS-5(数据源：假数据)"
                        && (string)a1["Stage"] == "CE-5,LS-5";   // dow=1 表里没有 → defaultStage
                });
                b.Chk("ark 周六命中 planByWeekday['3'] 之外的键就用默认(此路只测 3)", delegate
                {
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(Gd.Out()), now, "假数据", null, null, 3)));
                    return (string)r["Stage"] == "LS-6"
                        && (string)r["Reason"] == "无可刷活动关卡，按长草计划刷 LS-6(数据源：假数据)";
                });
                b.Chk("ark source 不给 → '无数据'", delegate
                {
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(Gd.Out()), now, "", null, null, 5)));
                    return ((string)r["Reason"]).EndsWith("(数据源：无数据)");
                });
                Dictionary<string, object> oneGrp = Gd.Out("zztest-event",
                    EventObj("测试活动", "2026-09-20 04:00:00", "2026-09-26 18:00:00",
                        Stage("AI-8", "10001"), Stage("CE-5", "3004")));
                b.Chk("ark 未确认 → event-pending：不烧理智、活动关只是提示、提示语 -Ack", delegate
                {
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(oneGrp), now, "假数据", null, null, 5)));
                    return (string)r["Mode"] == "event-pending" && !(bool)r["ClearSanity"]
                        && (string)r["Stage"] == "CE-5,LS-5" && (string)r["StageHint"] == "AI-8"
                        && (string)r["Reason"] == "活动[测试活动](sideStory/zztest-event，至 09-26 18:00) 进行中但你还没确认可自动刷 → 本轮不消耗理智。确认后运行 daily-clear.ps1 -Ack"
                        && (string)r["EventKey"] == "zztest-event" && !(bool)r["ShopDone"];
                });
                b.Chk("ark 已确认 → event：理智全刷代币关", delegate
                {
                    Dictionary<string, object> ack = Gd.Out("zztest-event", true);
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(oneGrp), now, "假数据", ack, null, 5)));
                    return (string)r["Mode"] == "event" && (bool)r["ClearSanity"] && (string)r["Stage"] == "AI-8"
                        && (string)r["Reason"] == "活动[测试活动]已确认自动刷 → 理智全刷 AI-8(09-20 04:00 ~ 09-26 18:00)";
                });
                b.Chk("ark eventRequiresAck=false → 未确认也直接刷", delegate
                {
                    Dictionary<string, object> inp = ArkIn(SideStory(oneGrp), now, "假数据", null, null, 5);
                    Gd.Map(inp["ark"])["eventRequiresAck"] = false;
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(inp));
                    return (string)r["Mode"] == "event" && (string)r["Stage"] == "AI-8";
                });
                b.Chk("ark 唯一活动已领空 → 回落长草 + ShopDone + 恢复提示 -ShopReset", delegate
                {
                    Dictionary<string, object> ack = Gd.Out("zztest-event", true);
                    Dictionary<string, object> sd = Gd.Out("zztest-event", true);
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(oneGrp), now, "假数据", ack, sd, 5)));
                    return (string)r["Mode"] == "idle" && (bool)r["ShopDone"] && (bool)r["ClearSanity"]
                        && (string)r["EventKey"] == "zztest-event"   // 领空后仍能报出活动 id(供撤销)
                        && (string)r["Reason"] == "活动[测试活动]商店已标记领空 → 剩余理智改刷长草计划 CE-5,LS-5(要恢复刷活动：daily-clear.ps1 -ShopReset)(数据源：假数据)";
                });
                b.Chk("ark 两活动：A 领空 + B 未确认 → pending 且带'另有 1 个'尾巴", delegate
                {
                    Dictionary<string, object> grp = Gd.Out(
                        "zztest-event", EventObj("测试活动A", "2026-09-20 04:00:00", "2026-09-26 18:00:00", Stage("AI-8", "10001")),
                        "zztest-event2", EventObj("测试活动B", "2026-09-20 04:00:00", "2026-09-29 04:00:00", Stage("LS-5", "3004")));
                    Dictionary<string, object> sd = Gd.Out("zztest-event", true);
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(grp), now, "假数据", null, sd, 5)));
                    return (string)r["Mode"] == "event-pending" && (string)r["EventKey"] == "zztest-event2"
                        && ((string)r["Reason"]).EndsWith("daily-clear.ps1 -Ack；另有 1 个活动已标记领空");
                });
                b.Chk("ark 两活动：A 领空 + B 已确认 → 自动改刷 B 并带跳过说明", delegate
                {
                    Dictionary<string, object> grp = Gd.Out(
                        "zztest-event", EventObj("测试活动A", "2026-09-20 04:00:00", "2026-09-26 18:00:00", Stage("AI-8", "10001")),
                        "zztest-event2", EventObj("测试活动B", "2026-09-20 04:00:00", "2026-09-29 04:00:00", Stage("LS-5", "3004")));
                    Dictionary<string, object> ack = Gd.Out("zztest-event2", true);
                    Dictionary<string, object> sd = Gd.Out("zztest-event", true);
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(grp), now, "假数据", ack, sd, 5)));
                    return (string)r["Mode"] == "event" && (string)r["EventKey"] == "zztest-event2" && (string)r["Stage"] == "LS-5"
                        && ((string)r["Reason"]).EndsWith("；另有 1 个活动商店已领空，已跳过");
                });
                b.Chk("ark 多活动按结束先后排队(先刷快结束的)", delegate
                {
                    Dictionary<string, object> grp = Gd.Out(
                        "late", EventObj("晚结束", "2026-09-20 04:00:00", "2026-09-28 04:00:00", Stage("LS-6", "10002")),
                        "early", EventObj("早结束", "2026-09-20 04:00:00", "2026-09-25 04:00:00", Stage("AI-8", "10001")));
                    Dictionary<string, object> ack = Gd.Out("late", true, "early", true);
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(grp), now, "假数据", ack, null, 5)));
                    return (string)r["EventKey"] == "early" && (string)r["Stage"] == "AI-8";
                });
                b.Chk("ark 代币关挑选：Drop 非纯数字的跳过；全非数字取第一个", delegate
                {
                    Dictionary<string, object> grp = Gd.Out(
                        "e1", EventObj("挑关", "2026-09-20 04:00:00", "2026-09-26 18:00:00",
                            Stage("NT-1", "task_flag"), Stage("AI-8", "10003"), Stage("CE-5", "10004")),
                        "e2", EventObj("全坏", "2026-09-20 04:00:00", "2026-09-25 18:00:00",
                            Stage("LS-5", "task_flag"), Stage("SK-2", "")));
                    Dictionary<string, object> r1 = Gd.Map(Decisions.Ark(ArkIn(SideStory(grp), now, "假数据", Gd.Out("e1", true, "e2", true), null, 5)));
                    // e2 结束更早 → 排序在前；其 Stages 无数字 Drop → 取第一个 LS-5
                    Dictionary<string, object> r2 = Gd.Map(Decisions.Ark(ArkIn(SideStory(Gd.Out("e1",
                        EventObj("挑关", "2026-09-20 04:00:00", "2026-09-26 18:00:00",
                            Stage("NT-1", "task_flag"), Stage("AI-8", "10003"), Stage("CE-5", "10004")))),
                        now, "假数据", Gd.Out("e1", true), null, 5)));
                    return (string)r1["EventKey"] == "e2" && (string)r1["Stage"] == "LS-5"
                        && (string)r2["Stage"] == "AI-8";   // 有数字 Drop → 跳过 NT-1 取 AI-8
                });
                b.Chk("ark 分组值是数组(item0…)也能吃；无 Activity 包装也能吃", delegate
                {
                    Dictionary<string, object> bare = Gd.Out(
                        "Tip", "裸活动", "UtcStartTime", "2026-09-20 04:00:00", "UtcExpireTime", "2026-09-26 18:00:00",
                        "Stages", new object[] { Stage("AI-8", "10001") });
                    Dictionary<string, object> data = Gd.Out("Official", Gd.Out("sideStory", new object[] { bare }));
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(data, now, "假数据", Gd.Out("item0", true), null, 5)));
                    return (string)r["Mode"] == "event" && (string)r["EventKey"] == "item0" && (string)r["Stage"] == "AI-8";
                });
                b.Chk("ark 时间窗外的活动不参与(结束在昨天/开始在未来)", delegate
                {
                    Dictionary<string, object> grp = Gd.Out(
                        "past", EventObj("已结束", "2026-09-10 04:00:00", "2026-09-15 04:00:00", Stage("AI-8", "10001")),
                        "future", EventObj("未开始", "2026-09-28 04:00:00", "2026-09-30 04:00:00", Stage("LS-6", "10002")));
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(grp), now, "假数据", Gd.Out("past", true, "future", true), null, 5)));
                    return (string)r["Mode"] == "idle";
                });
                b.Chk("ark 空 Stages 的活动不算可刷", delegate
                {
                    Dictionary<string, object> grp = Gd.Out("e", EventObj("没关", "2026-09-20 04:00:00", "2026-09-26 18:00:00"));
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(SideStory(grp), now, "假数据", null, null, 5)));
                    return (string)r["Mode"] == "idle" && (string)r["EventKey"] == null;
                });
                b.Chk("ark 材料加倍期 → 长草 Reason 追加提示", delegate
                {
                    // 真 MAA 数据里 resourceCollection 直接带 Tip/UtcStartTime/UtcExpireTime(它不是"分组→项"两层结构)
                    Dictionary<string, object> data = Gd.Out("Official", Gd.Out(
                        "sideStory", new Dictionary<string, object>(),
                        "resourceCollection", Activity("材料加倍", "2026-09-22 04:00:00", "2026-09-24 04:00:00")));
                    Dictionary<string, object> r = Gd.Map(Decisions.Ark(ArkIn(data, now, "假数据", null, null, 5)));
                    return (string)r["Mode"] == "idle"
                        && (string)r["Reason"] == "无可刷活动关卡，按长草计划刷 CE-5,LS-5(数据源：假数据)；正逢[材料加倍]材料加倍期(至 09-24 04:00)，刷材料本效率翻倍";
                });

                // ---------- decide-r1999 ----------
                Func<Dictionary<string, object>, Dictionary<string, object>, Dictionary<string, object>, bool, string, string, Dictionary<string, object>> rIn =
                    delegate(Dictionary<string, object> data, Dictionary<string, object> ack, Dictionary<string, object> sd, bool requiresAck, string vAck, string vReset)
                    {
                        return Gd.Out(
                            "re1999", Gd.Out("mainStoryRequiresAck", requiresAck),
                            "now", now, "data", data, "ack", ack ?? new Dictionary<string, object>(),
                            "shopDone", sd ?? new Dictionary<string, object>(), "verbAck", vAck, "verbShopReset", vReset);
                    };
                Func<long, long, string, object, Dictionary<string, object>> ver =
                    delegate(long vs, long ve, string name, object act)
                    {
                        return Gd.Out("start_time", vs, "end_time", ve, "version_name", name, "activity", act);
                    };
                long vStart = Ms("2026-09-15 04:00:00"), vEnd = Ms("2026-09-29 04:00:00");
                Dictionary<string, object> combatMain = Gd.Out("start_time", start, "end_time", end, "event_type", "MainStory");
                Dictionary<string, object> dataMain = Gd.Out("1.9", ver(vStart, vEnd, "版本名", Gd.Out("combat", combatMain)));
                b.Chk("r1999 不在版本窗口 → 长草 + 数据过期提醒", delegate
                {
                    Dictionary<string, object> past = Gd.Out("1.8", ver(vStart - 86400000L * 20, Ms("2026-09-20 04:00:00"), "旧版本", Gd.Out()));
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999(rIn(past, null, null, true, "-AckR1999", "-ShopResetR1999")));
                    return (string)r["Mode"] == "idle" && (string)r["Instance"] == "idle"
                        && (string)r["Reason"] == "不在任何已记录版本的窗口内 → 长草；活动数据只记录到 2026-09-20，可能未更新";
                });
                b.Chk("r1999 主线进行中未确认 → pending(Instance=idle)", delegate
                {
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999(rIn(dataMain, null, null, true, "-AckR1999", "-ShopResetR1999")));
                    return (string)r["Mode"] == "event-pending" && (string)r["Instance"] == "idle"
                        && (string)r["EventKey"] == "1.9-combat" && (string)r["EventTip"] == "版本名"
                        && (string)r["Reason"] == "版本 1.9[版本名]主线活动进行中(至 09-26 18:00)，未确认手动通关 → 本轮跑长草方案；确认后运行 daily-clear.ps1 -AckR1999";
                });
                b.Chk("r1999 确认后(或 requireAck=false) → event(Instance=event)", delegate
                {
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999(rIn(dataMain, Gd.Out("1.9-combat", true), null, true, "-AckR1999", "-ShopResetR1999")));
                    Dictionary<string, object> r2 = Gd.Map(Decisions.R1999(rIn(dataMain, null, null, false, "-AckR1999", "-ShopResetR1999")));
                    return (string)r["Mode"] == "event" && (string)r["Instance"] == "event"
                        && (string)r["Reason"] == "版本 1.9[版本名]活动进行中(09-20 04:00 ~ 09-26 18:00)"
                        && (string)r2["Mode"] == "event";
                });
                b.Chk("r1999 非主线活动不受确认门管", delegate
                {
                    Dictionary<string, object> d = Gd.Out("1.9", ver(vStart, vEnd, "版本名", Gd.Out(
                        "combat", Gd.Out("start_time", start, "end_time", end, "event_type", "Activity"))));
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999(rIn(d, null, null, true, "-AckR1999", "-ShopResetR1999")));
                    return (string)r["Mode"] == "event";
                });
                b.Chk("r1999 商店领空 → idle 但保留 EventKey/EventTip", delegate
                {
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999(rIn(dataMain, Gd.Out("1.9-combat", true), Gd.Out("1.9-combat", true), true, "-AckR1999", "-ShopResetR1999")));
                    return (string)r["Mode"] == "idle" && (bool)r["ShopDone"] && (string)r["EventKey"] == "1.9-combat"
                        && (string)r["Reason"] == "版本 1.9[版本名]活动商店已标记领空 → 本轮跑长草方案(要恢复：daily-clear.ps1 -ShopResetR1999)";
                });
                b.Chk("r1999 复刻窗口 → rerun", delegate
                {
                    Dictionary<string, object> d = Gd.Out("1.9", ver(vStart, vEnd, "版本名", Gd.Out(
                        "re-release", Gd.Out("start_time", Ms("2026-09-21 00:00:00"), "end_time", Ms("2026-09-25 00:00:00")))));
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999(rIn(d, null, null, true, "-AckR1999", "-ShopResetR1999")));
                    return (string)r["Mode"] == "rerun" && (string)r["Instance"] == "rerun" && (string)r["EventTip"] == "版本名 复刻"
                        && (string)r["Reason"] == "版本 1.9 复刻进行中(09-21 00:00 ~ 09-25 00:00)";
                });
                b.Chk("r1999 版本窗口内无活动 → 长草(带版本窗口说明)", delegate
                {
                    Dictionary<string, object> d = Gd.Out("1.9", ver(vStart, vEnd, "版本名", Gd.Out()));
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999(rIn(d, null, null, true, "-AckR1999", "-ShopResetR1999")));
                    return (string)r["Mode"] == "idle"
                        && (string)r["Reason"] == "版本 1.9[版本名]当前无进行中活动(版本窗口 09-15 04:00 ~ 09-29 04:00)→ 长草";
                });
                b.Chk("r1999 空数据也不炸", delegate
                {
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999(rIn(new Dictionary<string, object>(), null, null, true, "-Ack", "-ShopReset")));
                    return (string)r["Mode"] == "idle" && ((string)r["Reason"]).StartsWith("不在任何已记录版本的窗口内 → 长草");
                });

                // ---------- check-stages ----------
                b.Chk("check-stages：混合分隔符拆分+去重前空格、理智合计、未知关卡", delegate
                {
                    Dictionary<string, object> table = Gd.Out("CE-5", 6, "LS-5", 6, "SR-8", 10);
                    Dictionary<string, object> r = Gd.Map(Decisions.CheckStages(Gd.Out("table", table, "stageText", "CE-5, LS-5、SR-8；CE-5  ")));
                    object[] list = (object[])r["Stages"];
                    return list.Length == 4 && (int)r["Cost"] == 28 && ((object[])r["Bad"]).Length == 0
                        && (string)r["Note"] == "共 4 关，单圈理智 28";
                });
                b.Chk("check-stages：未知关卡点名；空表说拿不到", delegate
                {
                    Dictionary<string, object> r = Gd.Map(Decisions.CheckStages(Gd.Out("table", Gd.Out("CE-5", 6), "stageText", "CE-5,XX-9")));
                    object[] bad = (object[])r["Bad"];
                    Dictionary<string, object> r2 = Gd.Map(Decisions.CheckStages(Gd.Out("table", new Dictionary<string, object>(), "stageText", "CE-5")));
                    return bad.Length == 1 && (string)bad[0] == "XX-9" && (string)r["Note"] == "未知关卡：XX-9"
                        && (int)r["Cost"] == 6
                        && (string)r2["Note"] == "拿不到 MAA 关卡表，未校验";
                });

                // ---------- r1999-instance ----------
                b.Chk("r1999-instance：按实例名找文件、Tasks 取 <||> 末段、找不到抛原话", delegate
                {
                    string dir = Path.Combine(tmp, "instances");
                    Directory.CreateDirectory(dir);
                    GdJson.Write(Path.Combine(dir, "a1.json"), Gd.Out(
                        "InstanceName", "别的", "CurrentTasks", new object[] { "x<||>y<||>无关任务" }));
                    GdJson.Write(Path.Combine(dir, "b2.json"), Gd.Out(
                        "InstanceName", "日常", "CurrentTasks", new object[] { "root<||>每日<||>清体力" }));
                    Dictionary<string, object> r = Gd.Map(Decisions.R1999Instance(Gd.Out("dir", dir, "want", "日常", "planKey", "event")));
                    bool ok = (string)r["Id"] == "b2" && (string)r["Name"] == "日常" && ((object[])r["Tasks"]).Length == 1
                        && (string)((object[])r["Tasks"])[0] == "清体力";
                    string msg = null;
                    try { Decisions.R1999Instance(Gd.Out("dir", dir, "want", "没有这个", "planKey", "idle")); }
                    catch (GdRuleException ex) { msg = ex.Message; }
                    return ok && msg == "M9A 中找不到名为[没有这个]的实例(计划 idle)";
                });

                // ---------- verb 接线(信封/退出码) ----------
                b.Chk("verb decide-ark：--in/--out 回环，data 即判定对象", delegate
                {
                    string ip = Path.Combine(tmp, "in1.json"), op = Path.Combine(tmp, "out1.json");
                    GdJson.Write(ip, ArkIn(SideStory(oneGrp), now, "假数据", null, null, 5));
                    int code = GdCli.Run("decide-ark", new string[] { "--in=" + ip, "--out=" + op });
                    Dictionary<string, object> env = Gd.Map(GdJson.Read(op));
                    return code == 0 && (bool)env["ok"] && (int)env["exit"] == 0 && (int)env["proto"] == 1
                        && (string)Gd.Map(env["data"])["Mode"] == "event-pending";
                });
                b.Chk("verb 缺 --in → 1；顶层非对象 → 1；未知参数 → 1", delegate
                {
                    string op = Path.Combine(tmp, "out2.json");
                    if (GdCli.Run("decide-ark", new string[] { "--out=" + op }) != 1) return false;
                    string bad = Path.Combine(tmp, "in2.json");
                    File.WriteAllText(bad, "[1,2]", new UTF8Encoding(false));
                    if (GdCli.Run("check-stages", new string[] { "--in=" + bad, "--out=" + op }) != 1) return false;
                    return GdCli.Run("decide-r1999", new string[] { "--nope", "--out=" + op }) == 1;
                });
                b.Chk("verb r1999-instance 找不到 → 退出码 2 + error 是原话", delegate
                {
                    string ip = Path.Combine(tmp, "in3.json"), op = Path.Combine(tmp, "out3.json");
                    string dir = Path.Combine(tmp, "instances");
                    Directory.CreateDirectory(dir);
                    GdJson.Write(ip, Gd.Out("dir", dir, "want", "不存在", "planKey", "event"));
                    int code = GdCli.Run("r1999-instance", new string[] { "--in=" + ip, "--out=" + op });
                    Dictionary<string, object> env = Gd.Map(GdJson.Read(op));
                    return code == 2 && !(bool)env["ok"] && (int)env["exit"] == 2
                        && (string)env["error"] == "M9A 中找不到名为[不存在]的实例(计划 event)";
                });
            }
            catch (Exception ex)
            {
                b.Chk("dectest 整体没炸", delegate { throw ex; });
            }
            finally
            {
                try { Directory.Delete(tmp, true); } catch { }
            }
            return b.Result("dectest");
        }
    }
}
