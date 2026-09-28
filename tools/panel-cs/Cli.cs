// tools/panel-cs/Cli.cs —— verb 子命令入口(无头、绝不 Show 窗体)。契约全文见 docs\ps-migration-batch0.md §2。
// 批次 0：echo(契约样板)。批次 1：decide-ark / decide-r1999 / check-stages / r1999-instance —— PS 转发壳把
// 判定所需的一切**值**组装成 JSON 送 --in，这边只出结论、不读配置不写盘。
// 批次 2：detect-installs / maa-queue-lines / m9a-instance-lines —— 探测与状态行要读现场(盘/注册表/配置文件)，
// 读的动作也搬了进来：detect 的 roots 可由壳注入(golden 用假树)，两个 *-lines 只按路径读、不写任何东西。
// 批次 3：外部进程层(maa/m9a/mumu)的纯解析/纯规划十件(见 Processes.cs 头注的切分口径)——
// parse-asst-* / plan-maa-queue / m9a-launch-args / parse-m9a-lines / resolve-m9a-status / parse-mumu-* / parse-adb-*。
// 批次 4：视觉兜底层的纯规则段八件(见 Vision.cs 头注)——build-vision-prompt / parse-vision-verdict /
// vision-click-point / resolve-vision-endpoint / shot-changed / parse-vision-reply / rescue-entry /
// compose-ark-desktop-state。截图、点击、起子进程、兜底编排与整个 vision-mumu(定义文本被自检钉死)留 PS。
// 批次 5：编排/排程层的纯规则段七件(见 Orchestration.cs 头注)——parse-farm-target-spec / parse-ark-drop-text /
// resolve-ark-farm-plan / apply-ark-daily-intent / apply-r1999-daily-intent / parse-maa-gui-lines / split-task-args。
// 起进程、监控循环、写文件仍全在 PS 壳；plan-maa-queue 出的 NewConfig 由壳决定写不写。
// 批次 6：监督 agent 纯判定段四件(见 Supervisor.cs 头注)——eval-supervisor-settings / parse-supervisor-state /
// resolve-supervisor-action / compose-supervisor-detail。采集/问画面/重启三个副作用函数与读写盘、起停进程留 PS。
// 批次 7：活段监控循环的 tick 判据(见 RunLoops.cs 头注，口径 docs\ps-migration-live-caliber.md)——
// decide-maa-run-tick 一个 verb：壳每 tick 采读数(now/live/flags/cfg)喂进来，拿回"继续/早退/收尾"+
// 同 tick 挂起的副作用指令(grab/rescue/record)。while+sleep、挂钟、读盘、真兜底真抢前台、记账、收尾时长全留 PS 壳。
// 批次 8：活段第二批(见 RunLoops.cs / Orchestration.cs 头注)——decide-m9a-run-tick 一个 verb：m9a 监控循环的
// 反应段判据搬进 RunLoops.cs；ark/r1999 两份 $retryable 分流搬进 Orchestration.cs(decide-daily-retry)。
// 批次 9：两路监控循环的"事件→状态"回灌段(见 RunLoops.cs 对应段头注)——apply-maa-gui-events /
// finish-maa-run / apply-m9a-events 三个 verb：事件列表+旧状态进来，新状态+日志出去；
// Get-Date/Sort-Object/hashtable 重建这些现场量仍由壳做，Register-M9AEvent 与回灌 switch 只剩转发壳。
// 批次 10(收官)：common.ps1 三段从未搬过的小纯规则——resolve-screensaver-who / resolve-screensaver-verdict
// 两个 verb(见 Decisions.cs 屏保段：注册表读数由壳注入，屏保根因文案在这边拼)与 build-relaunch-args
// (见 Processes.cs 提权段：-Force/-Omit/$Bound 分类全部由壳组装成值进来，只出参数数组)。
// Get-GdScreensaverConfig(真读注册表)与 Get-GdElevateBatchLine(四行小拼接)留壳。
//
// 三通道铁律：
//   * 机读结果 **只走 --out 文件**(winexe 的 stdout 从控制台启动本来就不可靠，见 Panel.cs 头注)；
//   * 退出码 = 结论摘要：0 完成 / 1 我们自己的代码或参数错了 / 2 前置不满足这一轮没跑(Stop-GdBeforeRun 同型) /
//     3 预留给批次 1 的"环境行但不该干"(确认门/拒绝)；
//   * stdout 只是给人看的，AttachConsole 之后才真的可见 —— 双击启动没父控制台，attach 自然失败，忽略。
// 参数规矩：只认 `--键=值` 单 token(值含空格/中文都不拆)，`--键`(无 =)当开关；结构化输入走 --in 文件不走 stdin。
// 信封永远写出：连内部异常也要落成 ok:false + error 再退 —— PS 转发壳只面对"文件在/不在"两种情况。

using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace GdPanel
{
    static class GdCli
    {
        [DllImport("kernel32.dll")] static extern bool AttachConsole(int dwProcessType);
        const int ATTACH_PARENT_PROCESS = -1;

        // 参数/契约类错误(调用方的锅)：退出码 1；配置前置类(环境的锅)：GdPrecondition → 2。
        sealed class GdUsageException : Exception
        {
            public GdUsageException(string msg) : base(msg) { }
        }
        sealed class GdPreconditionException : Exception
        {
            public GdPreconditionException(string msg) : base(msg) { }
        }

        sealed class VerbResult
        {
            public bool Ok = true;
            public int Exit = 0;
            public object Data = null;
            public string Error = null;
        }

        public static void TryAttachConsole()
        {
            try
            {
                if (!AttachConsole(ATTACH_PARENT_PROCESS)) return;
                UTF8Encoding enc = new UTF8Encoding(false);
                Console.SetOut(new StreamWriter(Console.OpenStandardOutput(), enc) { AutoFlush = true });
                Console.SetError(new StreamWriter(Console.OpenStandardError(), enc) { AutoFlush = true });
                try { Console.OutputEncoding = enc; } catch { }
            }
            catch { }
        }

        // Panel.cs 的 Main 看到 argv[0] 不带 -- 就进来，跑完直接 Environment.Exit —— 中间不碰 WinForms。
        public static int Run(string verb, string[] tail)
        {
            TryAttachConsole();
            GdEnv.Init(null);
            Dictionary<string, string> a = null;
            VerbResult r = new VerbResult();
            try
            {
                a = Parse(tail);
                if (Has(a, "--transcript")) GdLog.StartCli(verb);
                if (verb == "echo") r = Echo(a);
                else if (verb == "decide-ark") r = Decide(a, Decisions.Ark);
                else if (verb == "decide-r1999") r = Decide(a, Decisions.R1999);
                else if (verb == "check-stages") r = Decide(a, Decisions.CheckStages);
                else if (verb == "r1999-instance") r = Decide(a, Decisions.R1999Instance);
                else if (verb == "resolve-screensaver-who") r = Decide(a, Decisions.ScreensaverWhoV);
                else if (verb == "resolve-screensaver-verdict") r = Decide(a, Decisions.ScreensaverVerdict);
                else if (verb == "build-relaunch-args") r = Decide(a, Processes.RelaunchArgs);
                else if (verb == "detect-installs") r = Decide(a, Detect.Installs);
                else if (verb == "maa-queue-lines") r = Decide(a, Detect.MaaQueueLines);
                else if (verb == "m9a-instance-lines") r = Decide(a, Detect.M9aInstanceLines);
                else if (verb == "parse-asst-state") r = Decide(a, Processes.AsstState);
                else if (verb == "parse-asst-hint") r = Decide(a, Processes.AsstHint);
                else if (verb == "plan-maa-queue") r = Decide(a, Processes.PlanMaaQueue);
                else if (verb == "m9a-launch-args") r = Decide(a, Processes.M9aLaunchArgs);
                else if (verb == "parse-m9a-lines") r = Decide(a, Processes.M9aLines);
                else if (verb == "resolve-m9a-status") r = Decide(a, Processes.M9aStatus);
                else if (verb == "parse-mumu-state") r = Decide(a, Processes.MumuState);
                else if (verb == "parse-mumu-vms") r = Decide(a, Processes.MumuVms);
                else if (verb == "parse-adb-devices") r = Decide(a, Processes.AdbDevices);
                else if (verb == "parse-adb-pidof") r = Decide(a, Processes.AdbPidof);
                else if (verb == "build-vision-prompt") r = Decide(a, Vision.Prompt);
                else if (verb == "parse-vision-verdict") r = Decide(a, Vision.Verdict);
                else if (verb == "vision-click-point") r = Decide(a, Vision.ClickPoint);
                else if (verb == "resolve-vision-endpoint") r = Decide(a, Vision.Endpoint);
                else if (verb == "shot-changed") r = Decide(a, Vision.ShotChanged);
                else if (verb == "parse-vision-reply") r = Decide(a, Vision.Reply);
                else if (verb == "rescue-entry") r = Decide(a, Vision.Entry);
                else if (verb == "compose-ark-desktop-state") r = Decide(a, Vision.DesktopState);
                else if (verb == "parse-farm-target-spec") r = Decide(a, Orchestration.FarmTargetSpec);
                else if (verb == "parse-ark-drop-text") r = Decide(a, Orchestration.FarmDropText);
                else if (verb == "resolve-ark-farm-plan") r = Decide(a, Orchestration.FarmPlan);
                else if (verb == "apply-ark-daily-intent") r = Decide(a, Orchestration.ArkIntent);
                else if (verb == "apply-r1999-daily-intent") r = Decide(a, Orchestration.R1999Intent);
                else if (verb == "parse-maa-gui-lines") r = Decide(a, Orchestration.MaaGuiLines);
                else if (verb == "split-task-args") r = Decide(a, Orchestration.TaskArgs);
                else if (verb == "eval-supervisor-settings") r = Decide(a, Supervisor.Settings);
                else if (verb == "parse-supervisor-state") r = Decide(a, Supervisor.State);
                else if (verb == "resolve-supervisor-action") r = Decide(a, Supervisor.Action);
                else if (verb == "compose-supervisor-detail") r = Decide(a, Supervisor.Detail);
                else if (verb == "decide-maa-run-tick") r = Decide(a, RunLoops.MaaTick);
                else if (verb == "decide-m9a-run-tick") r = Decide(a, RunLoops.M9aTick);
                else if (verb == "decide-daily-retry") r = Decide(a, Orchestration.DailyRetry);
                else if (verb == "apply-maa-gui-events") r = Decide(a, RunLoops.MaaGuiEvents);
                else if (verb == "finish-maa-run") r = Decide(a, RunLoops.MaaFinish);
                else if (verb == "apply-m9a-events") r = Decide(a, RunLoops.M9aEvents);
                else if (verb == "detect-maa-fight-skip") r = Decide(a, Orchestration.MaaFightSkip);
                else throw new GdUsageException("未知子命令：" + verb + "(现有：echo + 批次 1 判定四件 + 批次 2 探测/状态行 + 批次 3 的 parse-asst-*/plan-maa-queue/m9a-launch-args/parse-m9a-lines/resolve-m9a-status/parse-mumu-*/parse-adb-* + 批次 4 的 build-vision-prompt/parse-vision-verdict/vision-click-point/resolve-vision-endpoint/shot-changed/parse-vision-reply/rescue-entry/compose-ark-desktop-state + 批次 5 的 parse-farm-target-spec/parse-ark-drop-text/resolve-ark-farm-plan/apply-ark-daily-intent/apply-r1999-daily-intent/parse-maa-gui-lines/split-task-args + 批次 6 的 eval-supervisor-settings/parse-supervisor-state/resolve-supervisor-action/compose-supervisor-detail + 批次 7 的 decide-maa-run-tick + 批次 8 的 decide-m9a-run-tick/decide-daily-retry + 批次 9 的 apply-maa-gui-events/finish-maa-run/apply-m9a-events + detect-maa-fight-skip(09-28 认关)；后续批次见 docs\\ps-migration-batch0.md 与 docs\\ps-migration-live-caliber.md)");
            }
            catch (GdUsageException ex)
            {
                r.Ok = false; r.Exit = 1; r.Error = ex.Message;
            }
            catch (GdPreconditionException ex)
            {
                r.Ok = false; r.Exit = 2; r.Error = ex.Message;
            }
            catch (GdRuleException ex)
            {
                // 判定层的"规则说不干"(如实例找不到)：不是代码 bug，原话给 PS 壳继续 throw。
                r.Ok = false; r.Exit = 2; r.Error = ex.Message;
            }
            catch (FileNotFoundException ex)
            {
                // 批次 2 的状态行 verb 读配置文件缺文件：PS 侧原本就是"红字 + 原话"，
                // 不能走下面的通用 catch 被冠上 "FileNotFoundException: " 前缀 —— 原话进 error，退出码 2(前置缺)。
                r.Ok = false; r.Exit = 2; r.Error = ex.Message;
            }
            catch (Exception ex)
            {
                r.Ok = false; r.Exit = 1; r.Error = ex.GetType().Name + ": " + ex.Message;
            }
            if (r.Error != null) GdLog.Line(r.Error, "ERROR");
            try { WriteOut(a, verb, r); }
            catch (Exception ex)
            {
                // 连结果文件都写不出去(盘坏/路径被占)才是真正没法挽回的内部错误：说清楚，仍是 1。
                GdLog.Line("写结果文件失败：" + ex.Message, "ERROR");
                if (r.Ok) r.Exit = 1;
            }
            return r.Exit;
        }

        static void WriteOut(Dictionary<string, string> a, string verb, VerbResult r)
        {
            string outPath = Get(a, "--out");
            if (outPath == null)
            {
                if (!Has(a, "--no-out")) GdLog.Line("未给 --out：机读结果没处写(契约要求 verb 调用必带 --out)", "WARN");
                return;
            }
            if (Has(a, "--garbage"))
            {
                // --no-out / --garbage 是给 --echotest 和探针用的"故意违约"模拟旋钮(§5)：
                // 违约必须能被调用方识别出来，识别不出来才是契约没立住。
                string dir = Path.GetDirectoryName(Path.GetFullPath(outPath));
                if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir)) Directory.CreateDirectory(dir);
                File.WriteAllText(outPath, "{这不是合法的 JSON", new UTF8Encoding(false));
                return;
            }
            if (Has(a, "--no-out")) return;
            GdJson.Write(outPath, GdJson.Envelope(verb, r.Ok, r.Exit, r.Data, r.Error));
        }

        // ---------- echo：契约样板，不含任何业务判定。--exit=N 模拟结论码，--in 指向一份 JSON。 ----------
        static VerbResult Echo(Dictionary<string, string> a)
        {
            RequireKnown(a, "--in", "--out", "--exit", "--no-out", "--garbage", "--transcript");
            object data = new Dictionary<string, object>();
            string inPath = Get(a, "--in");
            if (inPath != null) data = GdJson.Read(inPath);   // 读不到就抛：那是调用方给的参数错 → 1
            int code = 0;
            string es = Get(a, "--exit");
            if (es != null)
            {
                if (!int.TryParse(es, out code)) throw new GdUsageException("--exit 不是整数：" + es);
            }
            VerbResult r = new VerbResult();
            r.Ok = code == 0;
            r.Exit = code;
            r.Data = data;
            r.Error = code == 0 ? null : "(echo) 模拟结论码 " + code;
            return r;
        }

        // ---------- 批次 1 判定 verb 的共同骨架：输入是一份 PS 壳组装好的 JSON(--in)，输出进 data。 ----------
        static VerbResult Decide(Dictionary<string, string> a, Func<Dictionary<string, object>, object> run)
        {
            RequireKnown(a, "--in", "--out", "--no-out", "--garbage", "--transcript");
            string inPath = Get(a, "--in");
            if (inPath == null) throw new GdUsageException("该子命令需要 --in <文件>：判定输入是 PS 转发壳组装的 JSON 对象");
            object input = GdJson.Read(inPath);              // 读不到就抛：那是调用方给的参数错 → 1
            Dictionary<string, object> map = input as Dictionary<string, object>;
            if (map == null) throw new GdUsageException("--in 指向的 JSON 顶层不是对象({})：" + inPath);
            VerbResult r = new VerbResult();
            r.Data = run(map);
            return r;
        }

        // ---------- 参数工具 ----------
        static Dictionary<string, string> Parse(string[] tail)
        {
            Dictionary<string, string> a = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (string t in tail)
            {
                if (t == null || !t.StartsWith("--"))
                    throw new GdUsageException("verb 参数必须以 --键=值 形式给出，收到：" + t);
                int eq = t.IndexOf('=');
                if (eq < 0) a[t] = null;
                else a[t.Substring(0, eq)] = t.Substring(eq + 1);
            }
            return a;
        }

        static void RequireKnown(Dictionary<string, string> a, params string[] known)
        {
            foreach (KeyValuePair<string, string> kv in a)
            {
                bool ok = false;
                foreach (string k in known) if (string.Equals(k, kv.Key, StringComparison.OrdinalIgnoreCase)) { ok = true; break; }
                if (!ok) throw new GdUsageException("未知参数：" + kv.Key + "(这一子命令只认 " + string.Join(" ", known) + ")");
            }
        }

        public static string Get(Dictionary<string, string> a, string k)
        {
            if (a == null) return null;
            string v;
            return a.TryGetValue(k, out v) ? v : null;
        }

        public static bool Has(Dictionary<string, string> a, string k)
        {
            return a != null && a.ContainsKey(k);
        }
    }
}
