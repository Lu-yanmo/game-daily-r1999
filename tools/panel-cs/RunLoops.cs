// tools/panel-cs/RunLoops.cs —— 批次 7：活段监控循环的"判据段"本体(ark 一路，verb：decide-maa-run-tick)。
// 口径全文见 docs\ps-migration-live-caliber.md(剧本注入回放 + 同 tick effect 迷你循环)。
// 基线真身 = lib\maa.ps1 Invoke-MaaRun 行 296-433 的判据链(钉 commit c498da0)。
// 文案必须与 PS 原话逐字符一致：tools\probe-live-golden.ps1 拿基线做全字面对拍，
// 有差异回修这里，禁止改期望、禁止放宽比对(docs\ps-migration-batch0.md §7)。
// 改判据/话术改本文件并跑 tools\probe-live-golden.ps1，不要往 lib\maa.ps1 的壳里加规则。
//
// 一次 tick 的纯判据：读数快照(now/live/flags/cfg) + 四键小状态 → 动作 + 新状态。
// C# 绝不碰挂钟/盘/进程/模型 —— elapsedSec/quietSec/grabAgoSec 由壳算好当数喂进来；
// 副作用(抢前台/视觉兜底/救火记账)全以 effect 挂回壳执行，回执(effectsResolved)带回来在
// **同一 tick** 里续判(旧代码兜完当场决定 continue/return，隔一轮再判就不是逐字不变)。
// 判据顺序与 maa.ps1 逐档对应：
//   1 finished→tail  2 unknown-format  3 not-started  4 stuck(抢前台→兜底→落穿续判 / 60 秒仍原地→stalled)
//   5 drained(兜一次→无条件早退)  6 stallWarn 中止  7 timeout  8 quiet(兜成→重置计时续判；否则 stalled)
//   9 MAA 进程没了(收尾或 error)  10 allDone+静默→收尾；都没有→loop。
// tail(收尾)整段留壳：旧代码出循环后**重新取一次挂钟**算 $dur、Round($dur/60,1)、打 OK 日志 —— 挂钟量，拍不了也不该拍。
//
// 挂起点编码在 state.phase："" / stuck-grab / stuck-rescue / stuck-record /
// drain-rescue / drain-record / quiet-rescue / quiet-record。tag 前缀与 phase 不是一套词：
// 静默档的兜底调用在旧代码里就叫 "stall$N"，照抄。
// RecordTag 比调用 Tag 大 1 是旧代码的错位(调用取 Count+1、记账在 += 之后再取 Count+1，疑似笔误)：
// 按纪律原样复刻不顺手修，要修另开一批。
// Detail 是 tick 开头快照(旧代码 return 拼的是镜像段算好的 $detail 串，档内兜底新落的那条不进
// 当 tick 结论句)：probe-live-golden 的 drain-first 场景钉着，实现见 Execute 的 tickDetail。
//
// PS 等值细节：hashtable 大小写不敏感 → completed/expectedChains 一律 OrdinalIgnoreCase；
// [int]$x 是银行家舍入 → 一律 Gd.Int(Convert.ToInt32 同口径)；[int]($quiet/60) 同理走 ToEven。

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;

namespace GdPanel
{
    static class RunLoops
    {
        // ==================== verb: decide-maa-run-tick ====================
        internal static object MaaTick(Dictionary<string, object> inp)
        {
            return new MaaTickRun(inp).Execute();
        }

        // 一次 tick 判定的全部输入/输出累加。写成小类只为把七八个桶收拢，没有任何外部状态。
        sealed class MaaTickRun
        {
            // —— 读数(now / live / flags / cfg)，全是壳采好的标量 ——
            double elapsed, quiet;
            bool hasGrabAgo; double grabAgo;
            bool finished, seenGame, drained, maaRunning, rescueAvail, grabAvail, abort;
            int markers, logChars, stuckExec, chanClicks;
            string stallWarn, asstHint, stuckTask, guiLogPath;
            Dictionary<string, object> completed;   // OrdinalIgnoreCase(PS hashtable 口径)
            List<string> chainKeys;                 // expectedChains 去重(保首个写法，对齐 $expectedCount)
            object[] errors;
            double startTimeout, stall, timeout, idleConfirm;
            object startTimeoutMinRaw, timeoutMinRaw, rescueMaxRaw;

            // —— C# 持有的四键小状态(壳 opaque 中继) ——
            string phase, grabNote;
            int rescueLeft;
            List<string> notes;
            // Detail 快照：旧代码每 tick 开头(基线 298-305 镜像段)算好 $detail，档内兜底新落的
            // $rc.Detail **不重算** —— drain/quiet 首兜当 tick 早退的结论句里没有"视觉兜底试过 N 次"。
            // 对拍门 tools\probe-live-golden.ps1 的 drain-first 场景抓出的口径：快照在此，逐档复用。
            string tickDetail;

            // —— 本 tick 已执行 effect 的回执(旧→新追加) ——
            List<object> resolved;

            // —— 输出累加 ——
            List<object> effects = new List<object>();
            List<object> logs = new List<object>();
            bool grabNow, resetLastGrowth, clearStallWarn;
            string action = "loop";
            bool ret; string retStatus, retDetail; object[] retErrors;

            // 兜底回执合成出的 rc(与旧代码 catch 分支同款三字段)
            sealed class Rc { public bool Tried; public bool Done; public string Detail; }

            internal MaaTickRun(Dictionary<string, object> inp)
            {
                Dictionary<string, object> now = As(Gd.Get(inp, "now"));
                Dictionary<string, object> live = As(Gd.Get(inp, "live"));
                Dictionary<string, object> flags = As(Gd.Get(inp, "flags"));
                Dictionary<string, object> cfg = As(Gd.Get(inp, "cfg"));

                elapsed = Dbl(Gd.Get(now, "elapsedSec"));
                quiet = Dbl(Gd.Get(now, "quietSec"));
                object ga = Gd.Get(now, "grabAgoSec");
                // 对齐 $null -eq $chanGrabAt：没抢过时值是 null —— ConvertTo-Json 会把它落成 JSON null
                // 或直接省键，两种都得判成"还没抢"(键在但为 null 不算，批次 6 的裸开关教训反向用)。
                hasGrabAgo = ga != null && !(ga is string && ((string)ga).Length == 0);
                grabAgo = Dbl(ga);

                finished = Gd.Truthy(Gd.Get(flags, "finished"));
                seenGame = Gd.Truthy(Gd.Get(flags, "seenGame"));
                drained = Gd.Truthy(Gd.Get(flags, "drained"));
                maaRunning = Gd.Truthy(Gd.Get(live, "maaRunning"));
                rescueAvail = Gd.Truthy(Gd.Get(live, "rescueAvailable"));
                grabAvail = Gd.Truthy(Gd.Get(live, "grabAvailable"));
                markers = Gd.Int(Gd.Get(flags, "markers"), 0);
                logChars = Gd.Int(Gd.Get(flags, "logChars"), 0);
                stuckExec = Gd.Int(Gd.Get(flags, "stuckExec"), 0);
                chanClicks = Gd.Int(Gd.Get(flags, "chanClicks"), 0);
                stallWarn = Gd.Str(Gd.Get(flags, "stallWarn"));
                asstHint = Gd.Str(Gd.Get(flags, "asstHint"));
                stuckTask = Gd.Str(Gd.Get(flags, "stuckTask"));
                errors = Gd.List(Gd.Get(flags, "errors"));

                completed = new Dictionary<string, object>(StringComparer.OrdinalIgnoreCase);
                Dictionary<string, object> cm = Gd.Map(Gd.Get(flags, "completed"));
                if (cm != null) foreach (KeyValuePair<string, object> kv in cm) completed[kv.Key] = kv.Value;

                chainKeys = new List<string>();
                HashSet<string> seenChains = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
                foreach (object c in Gd.List(Gd.Get(flags, "expectedChains")))
                {
                    string s = Gd.Str(c);
                    if (seenChains.Add(s)) chainKeys.Add(s);
                }

                startTimeout = Dbl(Gd.Get(cfg, "startTimeoutSec"));
                timeout = Dbl(Gd.Get(cfg, "timeoutSec"));
                stall = Dbl(Gd.Get(cfg, "stallSec"));
                idleConfirm = Dbl(Gd.Get(cfg, "idleConfirmSec"));
                startTimeoutMinRaw = Gd.Get(cfg, "startTimeoutMin");
                timeoutMinRaw = Gd.Get(cfg, "timeoutMin");
                abort = Gd.Truthy(Gd.Get(cfg, "abortOnMaaStallWarning"));
                guiLogPath = Gd.Str(Gd.Get(cfg, "guiLogPath"));
                rescueMaxRaw = Gd.Get(cfg, "rescueMaxRaw");

                resolved = new List<object>(Gd.List(Gd.Get(inp, "effectsResolved")));

                Dictionary<string, object> st = Gd.Map(Gd.Get(inp, "state"));
                if (st == null)
                {
                    // $rescueLeft = [int]$S.runtime.visionMaxRescue; if ($rescueLeft -le 0) { $rescueLeft = 2 }
                    // 的初始化就发生在首轮(旧代码在循环外做一次，效果同一 tick 内不会重来)。
                    phase = ""; grabNote = ""; notes = new List<string>(); tickDetail = "";
                    rescueLeft = Gd.Int(rescueMaxRaw, 0);
                    if (rescueLeft <= 0) rescueLeft = 2;
                }
                else
                {
                    phase = Gd.Str(Gd.Get(st, "phase"));
                    grabNote = Gd.Str(Gd.Get(st, "grabNote"));
                    tickDetail = Gd.Str(Gd.Get(st, "tickDetail"));
                    rescueLeft = Gd.Int(Gd.Get(st, "rescueLeft"), 0);
                    notes = new List<string>();
                    foreach (object n in Gd.List(Gd.Get(st, "rescueNotes"))) notes.Add(Gd.Str(n));
                }
            }

            internal object Execute()
            {
                if (phase.Length > 0)
                {
                    // 真壳永远原样回传我们出的 state(含 tickDetail)；手写续判 state(--looptest/对拍门)可能没带 ——
                    // 此刻 notes 还是 state 带来的历史账、本 tick 无新落，补算与真快照同值。
                    if (tickDetail.Length == 0) tickDetail = DetailText();
                    Resume();
                }
                else
                {
                    // 新 tick：先按旧代码镜像段的口径把 $detail 快照定下(此时 notes 还是上 tick 的账)，
                    // 之后档内兜底落多少条都不回头改它 —— 挂进 state 供同 tick 重入续用。
                    tickDetail = DetailText();
                    Ladder(1);
                }
                return Emit();
            }

            // ---------- 挂起点续判：本 tick 内壳执行完 effect 带着回执重入 ----------
            void Resume()
            {
                if (phase == "stuck-grab")
                {
                    Dictionary<string, object> r = Last("grab");
                    if (r != null && Gd.Truthy(Gd.Get(r, "exception")))
                        grabNote = "抢前台抛异常：" + Gd.Str(Gd.Get(r, "message"));
                    else if (r != null) grabNote = Gd.Str(Gd.Get(r, "detail"));
                    LogStuckWarn();
                    phase = "";
                    if (StartRescue("stuck", WhyStuck())) return;
                    Ladder(5);   // 旧代码 if/elseif 之后不 return：stuck 首轮兜完照旧落穿跑后面的档
                    return;
                }
                if (phase == "stuck-rescue")
                {
                    Rc rc = TakeRescue();
                    notes.Add(rc.Detail);   // $rescues += $rc.Detail —— 记账的 Count+1 在这之后，错位由此来
                    AddRecord("stuck", rc);
                    phase = "stuck-record";
                    return;
                }
                if (phase == "stuck-record")
                {
                    LogRescueResult();
                    phase = "";
                    Ladder(5);
                    return;
                }
                if (phase == "drain-rescue")
                {
                    Rc rc = TakeRescue();
                    notes.Add(rc.Detail);
                    AddRecord("drain", rc);
                    phase = "drain-record";
                    return;
                }
                if (phase == "drain-record")
                {
                    LogRescueResult();
                    phase = "";
                    Ret("stalled", DrainSentence() + tickDetail, errors);   // 兜过兜成都立刻早退；Detail 用 tick 开头快照(基线 305 行为：本 tick 新落的兜底不进结论句)
                    return;
                }
                if (phase == "quiet-rescue")
                {
                    Rc rc = TakeRescue();
                    notes.Add(rc.Detail);
                    AddRecord("stall", rc);   // 静默档的调用 tag 在旧代码里就叫 stall，照抄
                    phase = "quiet-record";
                    return;
                }
                if (phase == "quiet-record")
                {
                    LogRescueResult();
                    phase = "";
                    if (LastRescueDone())
                    {
                        // 挡路的东西拿掉了，停滞计时从头算，给 MAA 机会继续；也不让它下轮被停滞告警提前判死。
                        resetLastGrowth = true; clearStallWarn = true;
                        return;   // Action=loop(旧代码是 continue，跳过本档后面两查，等价)
                    }
                    Ret("stalled", "MAA 日志 " + IntStr(Math.Round(quiet / 60.0, MidpointRounding.ToEven))
                        + " 分钟无输出，判定卡住。" + tickDetail, errors);   // 快照口径：本 tick 新落的那条不进结论句(基线 422 在 += 之后拼的仍是 305 的串)
                    return;
                }
                Ladder(1);   // 不认识的挂起点：按全新 tick 判(壳中继不该走到这)
            }

            // ---------- 判据梯 ----------
            void Ladder(int from)
            {
                if (from <= 1 && finished) { action = "tail"; return; }

                if (from <= 2 && markers == 0 && (elapsed > startTimeout || quiet > stall))
                {
                    string why = logChars == 0
                        ? "本轮没有读到任何新增日志内容(可能 MAA 换了日志路径或文件名)"
                        : "日志在涨(新增 " + logChars + " 字符)但一条认识的标志都没有(可能 MAA 改了文案)";
                    Ret("unknown-format",
                        "MAA " + why + " → 这轮结果不可信，不要按它判定理智是否清完。先看 status.ps1 的 MAA 段，再对着 "
                        + guiLogPath + " 更新 lib\\maa.ps1 的判定正则", errors);
                    return;
                }
                if (from <= 3 && !seenGame && completed.Count == 0 && elapsed > startTimeout)
                {
                    Ret("not-started",
                        IntStr(Gd.Int(startTimeoutMinRaw, 0))
                        + " 分钟内端游没被拉起、也没有任何链路完成 → 大概率是权限/自动登录/客户端路径问题(MAA 连端游需要管理员)",
                        errors);
                    return;
                }
                if (from <= 4 && stuckExec >= 60 && completed.Count == 0 && !finished)
                {
                    if (!hasGrabAgo)
                    {
                        // 4a 首次：旧代码第一件事就是 $chanGrabAt = Get-Date —— 抢不抢得动都要给这一轮记上起点。
                        grabNow = true;
                        if (grabAvail)
                        {
                            effects.Add(Gd.Out("Kind", "grab", "Tag", "", "Context", ""));
                            phase = "stuck-grab";
                            return;
                        }
                        grabNote = "这棵树上没有抢前台的函数，只能等停滞判定";
                        LogStuckWarn();
                        if (StartRescue("stuck", WhyStuck())) return;
                    }
                    else if (grabAgo > 60)
                    {
                        Ret("stalled",
                            "点击没被端游吃进去：" + stuckTask + " 已重复 " + stuckExec + " 次(整轮 " + chanClicks
                            + " 次点击)且一条链路都没完成，抢前台后 60 秒仍在原地。抢前台时看到的点击环境：" + grabNote
                            + "。" + tickDetail, errors);
                        return;
                    }
                }
                if (from <= 5 && drained && !allDone)
                {
                    if (StartRescue("drain",
                            "MAA 自己说这一轮跑完了但报错，只完成 " + DoneCount() + "/" + chainKeys.Count + " 条链路，它不会再自己动"))
                        return;
                    Ret("stalled", DrainSentence() + tickDetail, errors);
                    return;
                }
                if (from <= 6 && stallWarn.Length > 0 && abort)
                {
                    List<object> errs = new List<object>();
                    errs.Add(stallWarn);
                    errs.AddRange(errors);
                    Ret("stalled", "MAA 自身停滞告警(已触发)。" + tickDetail, errs.ToArray());
                    return;
                }
                if (from <= 7 && elapsed > timeout)
                {
                    Ret("timeout",
                        "超过 " + IntStr(Gd.Int(timeoutMinRaw, 0)) + " 分钟未完成。" + tickDetail, errors);
                    return;
                }
                if (from <= 8 && quiet > stall)
                {
                    if (StartRescue("stall", "")) return;
                    Ret("stalled", "MAA 日志 " + IntStr(Math.Round(quiet / 60.0, MidpointRounding.ToEven))
                        + " 分钟无输出，判定卡住。" + tickDetail, errors);
                    return;
                }
                if (from <= 9 && !maaRunning)
                {
                    if (finished || allDone) { action = "tail"; return; }
                    Ret("error", "MAA 进程提前退出。" + tickDetail, errors);
                    return;
                }
                if (from <= 10 && allDone && quiet > idleConfirm) { action = "tail"; return; }
                action = "loop";
            }

            // ---------- 兜底链的公共件 ----------
            bool StartRescue(string tagPrefix, string context)
            {
                if (rescueLeft > 0 && rescueAvail)
                {
                    rescueLeft--;
                    effects.Add(Gd.Out("Kind", "rescue", "Tag", tagPrefix + (notes.Count + 1), "Context", context));
                    phase = tagPrefix == "stuck" ? "stuck-rescue" : (tagPrefix == "drain" ? "drain-rescue" : "quiet-rescue");
                    return true;
                }
                return false;
            }

            Rc TakeRescue()
            {
                Rc rc = new Rc();
                Dictionary<string, object> r = Last("rescue");
                if (r != null && Gd.Truthy(Gd.Get(r, "exception")))
                {
                    // 旧代码 catch：Tried/$Done 都给 $false(哪怕确实动过手)—— 照抄。
                    rc.Tried = false; rc.Done = false;
                    rc.Detail = "兜底抛异常：" + Gd.Str(Gd.Get(r, "message"));
                }
                else if (r != null)
                {
                    rc.Tried = Gd.Truthy(Gd.Get(r, "tried"));
                    rc.Done = Gd.Truthy(Gd.Get(r, "done"));
                    rc.Detail = Gd.Str(Gd.Get(r, "detail"));
                }
                return rc;
            }

            void AddRecord(string tagPrefix, Rc rc)
            {
                // RecordTag 比调用 Tag 大 1：$rescues 已 += 过，再取 Count+1 —— 旧代码原样(疑似笔误，不顺手修)。
                effects.Add(Gd.Out(
                    "Kind", "record", "Tag", tagPrefix + (notes.Count + 1),
                    "Tried", rc.Tried, "Done", rc.Done, "Detail", rc.Detail,
                    "Context", ""));
                phase = tagPrefix == "stuck" ? "stuck-record" : (tagPrefix == "drain" ? "drain-record" : "quiet-record");
            }

            void LogRescueResult()
            {
                string last = notes.Count > 0 ? notes[notes.Count - 1] : "";
                logs.Add(Gd.Out("Level", LastRescueDone() ? "OK" : "WARN", "Text", "视觉兜底结果：" + last));
            }

            void LogStuckWarn()
            {
                logs.Add(Gd.Out("Level", "WARN", "Text",
                    "端游没把点击吃进去(" + stuckTask + " 已重复 " + stuckExec + " 次、整轮 " + chanClicks + " 次点击)：" + grabNote));
            }

            string WhyStuck()
            {
                return "自动化在同一个子任务[" + stuckTask + "]上重复点了 " + stuckExec + " 次，一条链路都没完成";
            }

            string DrainSentence()
            {
                return "MAA 队列已跑空但只完成 " + DoneCount() + "/" + chainKeys.Count
                    + " 条链路，它不会再自己动 → 不等静默判定，当场兜底过一次就直接早退(外层会重试一次)。";
            }

            // ---------- $detail 拼串(四段，与旧代码逐字一致) ----------
            bool allDone { get { return DoneCount() >= chainKeys.Count && chainKeys.Count > 0; } }

            string DetailText()
            {
                string d = "完成链路 " + DoneCount() + "/" + chainKeys.Count;
                string chainText = string.Join(", ", completed.Keys.OrderBy(k => k, StringComparer.Ordinal));
                if (chainText.Length > 0) d += "：" + chainText;
                if (asstHint.Length > 0) d += "；" + asstHint;
                if (notes.Count > 0) d += "；视觉兜底试过 " + notes.Count + " 次，最后一次：" + notes[notes.Count - 1];
                return d;
            }

            int DoneCount()
            {
                // $doneCount += [Math]::Min([int]$completed[$k], [int]$expectedCount[$k]) —— 缺失键 [int]$null=0。
                int sum = 0;
                foreach (string k in chainKeys)
                {
                    object v;
                    int have = completed.TryGetValue(k, out v) ? Gd.Int(v, 0) : 0;
                    sum += Math.Min(have, 1);   // expectedCount 每条恒 1(旧代码逐字)
                }
                return sum;
            }

            // ---------- 回执工具：链每步只追加一条回执，按 kind 从尾部找自己那一条 ----------
            Dictionary<string, object> Last(string kind)
            {
                for (int i = resolved.Count - 1; i >= 0; i--)
                {
                    Dictionary<string, object> r = Gd.Map(resolved[i]);
                    if (r != null && Gd.Str(Gd.Get(r, "kind")) == kind) return r;
                }
                return null;
            }

            bool LastRescueDone()
            {
                Dictionary<string, object> r = Last("rescue");
                return r != null && !Gd.Truthy(Gd.Get(r, "exception")) && Gd.Truthy(Gd.Get(r, "done"));
            }

            // ---------- 出参 ----------
            void Ret(string status, string detail, object[] errs)
            {
                action = "return"; ret = true; retStatus = status; retDetail = detail; retErrors = errs;
            }

            object Emit()
            {
                Dictionary<string, object> d = Gd.Out(
                    "Action", action,
                    "Effects", effects.ToArray(),
                    "Directives", Gd.Out(
                        "GrabNow", grabNow, "ResetLastGrowth", resetLastGrowth, "ClearStallWarn", clearStallWarn),
                    "Logs", logs.ToArray(),
                    "State", Gd.Out(
                        "phase", phase, "rescueLeft", rescueLeft,
                        "rescueNotes", notes.ToArray(), "grabNote", grabNote,
                        "tickDetail", tickDetail));
                if (ret)
                {
                    d["Status"] = retStatus;
                    d["Detail"] = retDetail;
                    d["Errors"] = retErrors ?? new object[0];
                }
                return d;
            }

            // ---------- 输入小工具 ----------
            static Dictionary<string, object> As(object v)
            {
                Dictionary<string, object> d = Gd.Map(v);
                return d ?? new Dictionary<string, object>();
            }

            static double Dbl(object v)
            {
                if (v == null) return 0.0;
                try { return Convert.ToDouble(v, CultureInfo.InvariantCulture); } catch { return 0.0; }
            }

            static string IntStr(long l) { return l.ToString(CultureInfo.InvariantCulture); }
            static string IntStr(double d) { return ((long)d).ToString(CultureInfo.InvariantCulture); }
        }
    }
}
