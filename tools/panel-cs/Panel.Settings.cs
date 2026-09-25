// 配置读写：settings.json 的行内替换(只动那一个键所在的一行)、改前备份、每文件留 20 份。
// 这一份是 partial class 的一段；文件级用法/编译/自检动词的说明在 Panel.cs 顶部。

using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Windows.Forms;

namespace GdPanel
{
    // ---------- 主窗体(这一份装的成员见本文件头) ----------
    sealed partial class PanelForm : Form
    {
        // ---------- 实验功能：面板唯一直接读写 settings.json 的地方(只碰一个布尔键) ----------
        // 为什么敢让 C# 直接碰配置：这不是判定 —— 用不用视觉兜底仍由 lib\vision-rescue.ps1 读同一个键决定，
        // 面板只是替玩家拧开关。写法用"整文件行内替换"：只换 true/false 那几个字节，
        // 别的内容(含中文说明、注释性键)一个字节不动，改前照 PS 那边的规矩备份 + 每文件只留 20 份。
        static readonly Regex ExpKeyRe = new Regex("(\"arkVisionRescue\"\\s*:\\s*)(true|false)");
        string settingsPathOverride;       // 只有 --consoletest 用它：把读写指到一份临时副本上
        string SettingsPath { get { return string.IsNullOrEmpty(settingsPathOverride) ? Path.Combine(root, @"config\settings.json") : settingsPathOverride; } }
        bool ExpImplemented() { return File.Exists(Path.Combine(root, @"lib\vision-rescue.ps1")); }

        bool? ReadExpFlag()
        {
            try
            {
                string path = SettingsPath;
                if (!File.Exists(path)) return null;
                Match m = ExpKeyRe.Match(File.ReadAllText(path));
                if (!m.Success) return null;
                return m.Groups[2].Value == "true";
            }
            catch { return null; }
        }

        string WriteExpFlag(bool on)
        {
            string path = SettingsPath;
            if (!ExpImplemented()) return "这一仓没有实验功能的实现库，这个按钮不该出现。";
            if (!File.Exists(path)) return "还没有 config\\settings.json —— 先跑 setup.ps1 生成配置，再回来开/关。";
            try
            {
                string text = File.ReadAllText(path);
                if (!ExpKeyRe.IsMatch(text))
                    return "settings.json 里没有 arkVisionRescue 这一行(多半是老模板)，面板改不动 —— 重跑 setup.ps1 或手动补一行再试。";
                string bak = BackupSettings(path);
                File.WriteAllText(path, ExpKeyRe.Replace(text, "$1" + (on ? "true" : "false")), new UTF8Encoding(false));
                return "AI 辅助(视觉兜底)已" + (on ? "打开" : "关掉") + "(写进 config\\settings.json" +
                       (bak != null ? "，旧配置已备份到 backups\\" : "，注意：备份没成") + ")。";
            }
            catch (Exception ex) { return "写配置失败：" + ex.Message; }
        }

        string BackupSettings(string path)
        {
            try
            {
                string dir = Path.Combine(root, "backups");
                if (!Directory.Exists(dir)) Directory.CreateDirectory(dir);
                string dest = Path.Combine(dir, "settings.json." + DateTime.Now.ToString("yyyyMMdd-HHmmss") + ".bak");
                File.Copy(path, dest, true);
                List<FileInfo> olds = new List<FileInfo>();
                foreach (string f in Directory.GetFiles(dir, "settings.json.*.bak"))
                { try { olds.Add(new FileInfo(f)); } catch { } }
                olds.Sort(delegate(FileInfo a, FileInfo b) { return b.LastWriteTime.CompareTo(a.LastWriteTime); });
                for (int i = 20; i < olds.Count; i++) { try { olds[i].Delete(); } catch { } }
                return dest;
            }
            catch { return null; }
        }

        // ===================== 主控台四页：配置 / 定时 / 历史 / 诊断 =====================
        // 这一段的定位和「实验功能」卡一样：面板是"把手"，不是"判定"。
        //   读 —— 路径/时间点/视觉项的现值 = 行内读 settings.json(只取那一个键)；
        //          「这一路到底就绪没有」「下一个时间点是几点」这类结论一律转达 -DumpSummary 已有的
        //          PROBE / TIME 行原文，面板不自己算；历史表 = 直读 state\history.csv(判定层写的那几列)。
        //   写 —— 路径改动代跑 setup.ps1(合并、备份、校验的规矩都留在那儿，面板不复制一套)；
        //          视觉模型/地址/Webhook/vmIndex 这类标量走行内替换(和 arkVisionRescue 同一套：备份 + 每文件留 20 份)；
        //          装卸任务、换时间点代跑 install-task.ps1 / reschedule-daily.ps1，那两步会弹一次 UAC，是脚本本来的行为。
        // 例外一条：**视觉 Key 永远不进命令行参数**(进程命令行在别的用户/工具面前是明文)，只在面板里行内写。
        static readonly string[] ConsoleIds = new[] { "cfg", "sched", "hist", "diag" };
        static bool IsConsolePage(string id)
        {
            if (id == null) return false;
            foreach (string s in ConsoleIds) if (s == id) return true;
            return false;
        }

        Panel consoleHost;                 // 盖住"四卡 + 动作区"那块地，只在主控台四页可见
        string consoleBuilt = null;        // consoleHost 里现在装的是哪一页(同页不重建，免得把人家正敲的字抹掉)
        Label consoleNote;                 // 右上角一句话说明
        readonly Dictionary<string, TextBox> boxByKey = new Dictionary<string, TextBox>();
        readonly Dictionary<string, string> loadedVal = new Dictionary<string, string>();
        Label cfgProbeArk, cfgProbeR1999;
        Label schedReadout;
        Panel histHost;
        Label histHint;
        int histRows;                      // 这次真的铺了几行，版式自检要用它区分"没历史"和"铺不出来"
        string lastDump = null;            // 最近一次 -DumpSummary 原文，诊断页直接把它当"当前读数"存档
        bool diagBusy = false;
        string lastPackError = "";         // 打包失败的人话原因，自检要拿它说话

        sealed class CfgField
        {
            public string Key = "";
            public string Title = "";
            public bool Dir;             // true = 目录(浏览用 FolderBrowserDialog)
            public bool NoPick;          // true = 不给浏览按钮(数字序号这类，选不了文件)
            public bool Secret;          // true = 不进命令行、框里打码
            public string SetupArg = ""; // 非空 = 改动通过 setup.ps1 的这个参数下发
            public string Game = "";     // 归属哪一路(""=两路都用)
            public string Help = "";
        }

        // 按"这棵树上真的有这一路"给字段：单游戏仓库里不该出现另一路的路径框
        List<CfgField> CfgFields()
        {
            List<CfgField> f = new List<CfgField>();
            bool ark = HasGame("ark"), r1999 = HasGame("r1999");
            if (ark)
            {
                f.Add(new CfgField { Key = "maaDir", Title = "方舟助手(MAA)目录", Dir = true, SetupArg = "-MaaDir", Game = "ark", Help = "整个程序目录，不是 MAA.exe 本身" });
                f.Add(new CfgField { Key = "arknightsExe", Title = "方舟端游程序", Dir = false, SetupArg = "-ArkExe", Game = "ark", Help = "指到 Arknights.exe" });
            }
            if (r1999)
            {
                f.Add(new CfgField { Key = "m9aDir", Title = "1999 助手(M9A)目录", Dir = true, SetupArg = "-M9aDir", Game = "r1999", Help = "M9A-win-x86_64-… 那一层" });
                f.Add(new CfgField { Key = "mumuDir", Title = "MuMu 管理器目录", Dir = true, SetupArg = "-MuMuDir", Game = "r1999", Help = "下面要有 nx_main\\MuMuManager.exe" });
                f.Add(new CfgField { Key = "vmIndex", Title = "MuMu 虚拟机序号", Dir = false, NoPick = true, SetupArg = "-VmIndex", Game = "r1999", Help = "只有一台时 setup.ps1 会自动填；多台必须指对" });
            }
            // 视觉兜底是方舟那一路的东西(lib\vision-rescue.ps1 只被 ark 调)，1999 单仓里连键都没有，
            // 摆两行填了也存不进 settings.json —— 按这一仓有没有 ark 决定要不要给这两行。
            if (ark)
            {
                f.Add(new CfgField { Key = "visionModel", Title = "视觉模型", Dir = false, Game = "", Help = "只在 MAA 判到卡住时用来认弹窗，正常一轮一个请求都不发" });
                f.Add(new CfgField { Key = "visionApiKey", Title = "视觉 Key", Dir = false, Secret = true, Game = "", Help = "留空 = 这一项不改；面板不把它带上命令行" });
            }
            f.Add(new CfgField { Key = "webhook", Title = "通知 Webhook", Dir = false, Game = "", Help = "留空 = 不发通知" });
            return f;
        }

        bool HasGame(string id)
        {
            foreach (string[] g in games) if (g[0] == id) return true;
            return false;
        }

        // ---------- settings.json 的标量读写：只换那一个键所在的一行，别的一个字节不动 ----------
        static string JsonEscape(string s)
        {
            if (s == null) return "";
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < s.Length; i++)
            {
                char ch = s[i];
                if (ch == '\\') sb.Append("\\\\");
                else if (ch == '"') sb.Append("\\\"");
                else if (ch == '\t') sb.Append("\\t");
                else if (ch == '\r' || ch == '\n') sb.Append(' ');
                else sb.Append(ch);
            }
            return sb.ToString();
        }

        static string JsonUnescape(string s)
        {
            if (s == null) return "";
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < s.Length; i++)
            {
                char ch = s[i];
                if (ch != '\\' || i + 1 >= s.Length) { sb.Append(ch); continue; }
                char n = s[++i];
                if (n == '\\') sb.Append('\\');
                else if (n == '"') sb.Append('"');
                else if (n == '/') sb.Append('/');
                else if (n == 't') sb.Append('\t');
                else if (n == 'n') sb.Append('\n');
                else if (n == 'r') sb.Append('\r');
                else sb.Append(n);          // \u 之类这台机器的 settings.json 里没有，原样留着别乱猜
            }
            return sb.ToString();
        }

        string ReadSettingString(string key)
        {
            try
            {
                string path = SettingsPath;
                if (!File.Exists(path)) return null;
                Match m = new Regex("\"" + Regex.Escape(key) + "\"\\s*:\\s*\"((?:[^\"\\\\]|\\\\.)*)\"").Match(File.ReadAllText(path));
                if (!m.Success) return null;
                return JsonUnescape(m.Groups[1].Value);
            }
            catch { return null; }
        }

        // 返回 null = 改好了；否则是人话版的没改成
        string WriteSettingString(string key, string value)
        {
            string path = SettingsPath;
            if (!File.Exists(path)) return "还没有 config\\settings.json —— 先点「自动扫描」或跑 setup.ps1 生成配置。";
            try
            {
                string text = File.ReadAllText(path);
                Regex re = new Regex("(\"" + Regex.Escape(key) + "\"\\s*:\\s*\")((?:[^\"\\\\]|\\\\.)*)(\")");
                if (!re.IsMatch(text))
                    return "settings.json 里没有 " + key + " 这一行(多半是老模板)，面板改不动 —— 重跑 setup.ps1 补出这一键再改。";
                string bak = BackupSettings(path);
                string repl = re.Replace(text, "$1" + MatcherEscape(JsonEscape(value)) + "$3");
                File.WriteAllText(path, repl, new UTF8Encoding(false));
                return bak != null ? "" : "写好了，但备份没成(backups\\ 没写进去) —— 这一项改动没法回退，留意一下。";
            }
            catch (Exception ex) { return "写 " + key + " 失败：" + ex.Message; }
        }

        static string MatcherEscape(string s)
        {
            // Regex.Replace 的替换串里 $ 和 \ 有特殊含义，路径里的反斜杠已经转义过了，$ 还得再挡一道
            return s.Replace("$", "$$");
        }

        string[] ReadTimes()
        {
            try
            {
                if (!File.Exists(SettingsPath)) return new string[0];
                Match m = new Regex("\"times\"\\s*:\\s*\\[([^\\]]*)\\]").Match(File.ReadAllText(SettingsPath));
                if (!m.Success) return new string[0];
                List<string> outList = new List<string>();
                foreach (Match q in Regex.Matches(m.Groups[1].Value, "\"([^\"]*)\"")) outList.Add(q.Groups[1].Value);
                return outList.ToArray();
            }
            catch { return new string[0]; }
        }

    }
}