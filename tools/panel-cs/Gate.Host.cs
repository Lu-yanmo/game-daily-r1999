// tools/panel-cs/Gate.Host.cs —— 宿主批的地盘闸：MuMu 的 adb 端点定档与虚拟机探活。
//
// 这一件的来处是 lib\vision-mumu.ps1 的 Get-GdMuMuAdbEndpoint(lib\mumu.ps1 的 Get-GdMuMu 供路径算术)
// 与 lib\supervisor.ps1 的问画面那一段。搬它的原因是宿主批(Round.Host / Supervisor.Host)不许再经过
// powershell —— 端点取法住在 PS 就等于每一拍问画面都要起一枚 pwsh，而"问一次画面"本身是纯取数。
//
// 三源两正则的**顺序**照真身，不许调：① 调用方给的 Conn(实例配置里那一路真要用的设备)；
// ② MuMuManager 的 `adb -v <index>`；③ `adb devices` 里**恰好只有一台**时的那枚序列号。
// ② 的两副眼镜也照真身：先抠连续的 ip:port，落空再按 "adb_host"/"adb_port" 分字段拼回(09-30 实锤：
// 本机那版回的是分字段形态，只认前一副会把"虚拟机明明在跑"误报成 vm not running)。
// ③ 只在恰好一台时才认：挑错设备会把点击打进另一台模拟器实例，宁可不认。
// 绝不写死 16384/7555 这类端口 —— 换了 MuMu 版本或实例编号就错，本机专属值也不许进源码。
//
// ⚠ 本件只放端点与探活两枚。外发名册里 `updgate`／`audit` 两臂的直身**不在这里**：它们各自的真身
//   (tools\m9a-update.ps1／tools\1999-event-audit.ps1)仍在生产面上有人直接起，宿主批还没接到那一臂，
//   先把名字占在这里等于登记一个没有实现的出口。
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;

namespace GdPanel
{
    static class GateHost
    {
        static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        sealed class ProcResult
        {
            public int Code = 0;
            public string Out = "";
            public string Err = "";
            public bool TimedOut = false;
        }

        // ==================== 公开面 ====================

        // Get-GdMuMuAdbEndpoint 的直身。返回 {Adb;Serial;Why}，Why 是能进日志的一句话。
        // conn 那一源由调用方喂(m9a 的实例配置)，没有就传 null。
        internal static Dictionary<string, object> MuMuEndpoint(Dictionary<string, object> S)
        {
            return MuMuEndpoint(S, null);
        }

        internal static Dictionary<string, object> MuMuEndpoint(Dictionary<string, object> S, Dictionary<string, object> conn)
        {
            string adb = "";
            if (conn != null) adb = Gd.Str(Gd.Get(conn, "adb")) ?? "";
            Dictionary<string, object> mm = MuMuPaths(S);
            if (adb.Length == 0) adb = Gd.Str(Gd.Get(mm, "Adb")) ?? "";
            // 到这儿就没了：paths.mumuAdb 存的是"相对 mumuDir 的文件名"，再兜一层只会把相对路径
            // 当 adb 用、报出一句看不懂的"命令不存在"(真身同一句守卫)。
            if (adb.Length == 0 || !File.Exists(adb))
                return Gd.Out("Adb", "", "Serial", "", "Why", "找不到 MuMu 自带的 adb(先跑 setup.ps1 填 paths.mumuDir)");

            string serial = "";
            if (conn != null) serial = Gd.Str(Gd.Get(conn, "serial")) ?? "";
            if (serial.Length == 0) serial = MuMuManagerSerial(mm);

            if (serial.Length == 0)
            {
                // 最后一次机会：已经连着的设备里挑一个，且只在恰好只有一台时才认(理由见文件头)。
                ProcResult dev = RunProc(adb, new string[] { "devices" }, 20);
                List<string> ons = new List<string>();
                foreach (Match m in Regex.Matches(dev.Out ?? "", @"^(\S+)\s+device\b", RegexOptions.Multiline)) ons.Add(m.Groups[1].Value);
                if (ons.Count == 1) serial = ons[0];
            }
            if (serial.Length == 0)
                return Gd.Out("Adb", adb, "Serial", "", "Why",
                    "拿不到 adb 序列号：虚拟机没在跑，或这版 MuMuManager 的 adb -v 输出形态两种认法都落空(09-30 已兜 JSON 分字段形态)");
            return Gd.Out("Adb", adb, "Serial", serial, "Why", "");
        }

        // 真身 `.Alive` 那三步(vision-mumu.ps1)的直身，与 Live 类内那枚 private 同判据、不同源不复用：
        // 先问 devices，序列号数得出来就直接算在场；数不出来才补一次 connect。
        // 不许合成一枚 —— 合了就是"每次探活都白跑一趟 connect"，而真身在设备本来就列着时一趟都不起。
        internal static bool MuMuVmAlive(string adb, string serial)
        {
            if (adb == null) adb = "";
            if (serial == null) serial = "";
            if (adb.Length == 0 || serial.Length == 0) return false;
            if (!File.Exists(adb)) return false;         // 真身：起不来 → 补 connect → 路径判据抛 → catch 回假
            ProcResult dev = RunProc(adb, new string[] { "devices" }, 20);
            if ((dev.Out ?? "").Contains(serial)) return true;
            ProcResult c = RunProc(adb, new string[] { "connect", serial }, 30);
            return c.Code == 0 && (c.Out ?? "").IndexOf("connected", StringComparison.OrdinalIgnoreCase) >= 0;
        }

        // ==================== 取数段 ====================

        // Get-GdMuMu(lib\mumu.ps1)的直身：四条算术，不起任何进程。
        static Dictionary<string, object> MuMuPaths(Dictionary<string, object> S)
        {
            Dictionary<string, object> paths = Gd.Map(Gd.Get(S, "paths")) ?? new Dictionary<string, object>();
            Dictionary<string, object> mumu = Gd.Map(Gd.Get(S, "mumu")) ?? new Dictionary<string, object>();
            string dir = Gd.Str(Gd.Get(paths, "mumuDir")) ?? "";
            return Gd.Out(
                "Manager", JoinRel(dir, Gd.Get(paths, "mumuManager")),
                "Adb", JoinRel(dir, Gd.Get(paths, "mumuAdb")),
                "Index", Gd.Str(Gd.Get(mumu, "vmIndex")) ?? "",
                "Name", Gd.Str(Gd.Get(mumu, "vmNameHint")) ?? "");
        }

        // paths 里那两枚既写成一枚字符串也写成一段清单都算对(与 Join-GdPath 的 @(...) 同口径)：取第一枚。
        static string JoinRel(string dir, object rel)
        {
            string r = "";
            object[] list = Gd.List(rel);
            if (list != null && list.Length > 0) r = Gd.Str(list[0]) ?? "";
            else r = Gd.Str(rel) ?? "";
            if (dir.Length == 0) return r;
            if (r.Length == 0) return "";
            return GdEnv.JoinPath(dir, r);
        }

        // ② 那一源：MuMuManager `adb -v <index>` 的两副眼镜。拿不到回空串(不抛)。
        static string MuMuManagerSerial(Dictionary<string, object> mm)
        {
            string manager = Gd.Str(Gd.Get(mm, "Manager")) ?? "";
            if (manager.Length == 0 || !File.Exists(manager)) return "";
            ProcResult r = RunProc(manager, new string[] { "adb", "-v", Gd.Str(Gd.Get(mm, "Index")) ?? "" }, 30);
            string txt = (r.Out ?? "") + (r.Err ?? "");
            Match hit = Regex.Match(txt, @"\b\d{1,3}(?:\.\d{1,3}){3}:\d{2,5}\b");
            if (hit.Success) return hit.Value;
            Match h = Regex.Match(txt, @"""adb_host(?:_ip)?""\s*:\s*""([^""]+)""");
            Match p = Regex.Match(txt, @"""adb_port""\s*:\s*""?(\d+)""?");
            if (h.Success && p.Success) return h.Groups[1].Value + ":" + p.Groups[1].Value;
            return "";
        }

        // 起一枚短命令并收三格读数。stdout/stderr 各一根线程收(先挂读手再等退出)，
        // 否则输出满了管道就是死锁 —— PS 侧那两枚现出口防的是同一件事，这里不靠超时侥幸。
        static ProcResult RunProc(string file, string[] args, int timeoutSeconds)
        {
            ProcResult res = new ProcResult();
            try
            {
                ProcessStartInfo psi = new ProcessStartInfo(file);
                // .NET Framework 4.0 的 ProcessStartInfo 没有 ArgumentList(那是后来自带运行时才有的)，
                // 只能手拼这一串：每枚参数一律加双引号，空参数干脆不拼(拼进去会多出一枚空位参数)。
                StringBuilder cl = new StringBuilder();
                for (int i = 0; i < args.Length; i++)
                {
                    string a = args[i] ?? "";
                    if (a.Length == 0) continue;
                    if (cl.Length > 0) cl.Append(' ');
                    cl.Append('"').Append(a.Replace("\"", "\\\"")).Append('"');
                }
                psi.Arguments = cl.ToString();
                psi.UseShellExecute = false;
                psi.RedirectStandardOutput = true;
                psi.RedirectStandardError = true;
                psi.CreateNoWindow = true;
                using (Process p = new Process())
                {
                    p.StartInfo = psi;
                    StringBuilder so = new StringBuilder();
                    StringBuilder se = new StringBuilder();
                    object gate = new object();
                    p.OutputDataReceived += delegate (object s, DataReceivedEventArgs e)
                    {
                        if (e.Data == null) return;
                        lock (gate) so.AppendLine(e.Data);
                    };
                    p.ErrorDataReceived += delegate (object s, DataReceivedEventArgs e)
                    {
                        if (e.Data == null) return;
                        lock (gate) se.AppendLine(e.Data);
                    };
                    p.Start();
                    p.BeginOutputReadLine();
                    p.BeginErrorReadLine();
                    if (!p.WaitForExit(timeoutSeconds * 1000))
                    {
                        res.TimedOut = true;
                        try { p.Kill(); } catch { }
                        res.Code = -1;
                    }
                    else
                    {
                        // WaitForExit(带超时)只保证到"退出信号"，异步读手可能还有尾巴没吐完：
                        // 不带参数的这一次才是"流也收干净了"，只在已经退出的那一支做。
                        p.WaitForExit();
                        res.Code = p.ExitCode;
                    }
                    lock (gate) { res.Out = so.ToString(); res.Err = se.ToString(); }
                }
                return res;
            }
            catch (Exception ex)
            {
                res.Code = -1;
                res.Err = ex.Message;
                return res;
            }
        }
    }
}
