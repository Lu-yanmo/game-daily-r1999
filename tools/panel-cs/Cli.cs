// tools/panel-cs/Cli.cs —— verb 子命令入口(无头、绝不 Show 窗体)。契约全文见 docs\ps-migration-batch0.md §2。
// 批次 0：echo(契约样板)。批次 1：decide-ark / decide-r1999 / check-stages / r1999-instance —— PS 转发壳把
// 判定所需的一切**值**组装成 JSON 送 --in，这边只出结论、不读配置不写盘。
// 批次 2：detect-installs / maa-queue-lines / m9a-instance-lines —— 探测与状态行要读现场(盘/注册表/配置文件)，
// 读的动作也搬了进来：detect 的 roots 可由壳注入(golden 用假树)，两个 *-lines 只按路径读、不写任何东西。
// 批次 3：外部进程层(maa/m9a/mumu)的纯解析/纯规划十件(见 Processes.cs 头注的切分口径)——
// parse-asst-* / plan-maa-queue / m9a-launch-args / parse-m9a-lines / resolve-m9a-status / parse-mumu-* / parse-adb-*。
// 起进程、监控循环、写文件仍全在 PS 壳；plan-maa-queue 出的 NewConfig 由壳决定写不写。
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
                else throw new GdUsageException("未知子命令：" + verb + "(现有：echo + 批次 1 判定四件 + 批次 2 探测/状态行 + 批次 3 的 parse-asst-*/plan-maa-queue/m9a-launch-args/parse-m9a-lines/resolve-m9a-status/parse-mumu-*/parse-adb-*；后续批次见 docs\\ps-migration-batch0.md)");
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
