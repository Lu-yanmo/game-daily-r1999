// 读数：起一次 -DumpSummary、按协议行解析(含 PROTO 版本对不上时要说的那句话)，
// 再把 state\history.csv 里判定层写下的原因挂到卡片行上。Row 是这一层的数据结构。
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
    // ---------- 协议行 ----------
    sealed class Row
    {
        public string Kind;   // ok / warn / err / dim / info
        public string Game;   // 空 = 不属于任何一路(分页时永远保留)
        public string Text;
        // 完整原因(来自 state\history.csv 的 detail 列)。非空 = 这一行可以点。
        // 为什么单开一个字段而不是塞进 Text：卡片一行只有 272px，判定层那句几百字的
        // "M9A 连模拟器这一步就失败了…" 会把整张「上一轮」卡撑成省略号，反而什么都看不见。
        public string Detail = "";   // 长文本原因(不参与卡片渲染，点这一行才展开)；没有就是空串
        // 原文出处：点了那一行之后，日志区那一句抬头要说的"这段话是从哪儿抄来的"。
        // 空串 = 走默认那套(「上一轮」的行，出处是 state\history.csv 的 detail 列)；
        // SV 行喂的是 state\supervision.json，抬头不说清楚就会让人去翻历史 CSV 找它。
        public string Source = "";
        public Row(string kind, string game, string text) { Kind = kind; Game = game; Text = text; }
        public Row(string kind, string game, string text, string detail) { Kind = kind; Game = game; Text = text; Detail = detail ?? ""; }
    }

    // ---------- 主窗体(这一份装的成员见本文件头) ----------
    sealed partial class PanelForm : Form
    {
        // ---------- 引擎 / 数据源 ----------
        // engine 为空(=这台机器没有 pwsh 7)时子进程全起不来：这句话给各调用点拼错误消息用。
        const string NoEngineHint = "这台机器上没有 pwsh.exe(PowerShell 7)——本程序只支持 PowerShell 7，请先安装：winget install Microsoft.PowerShell";

        static string PickEngine()
        {
            // 面板起的每个子进程都要一个 PowerShell。本仓库只支持 PowerShell 7(2026-10-06 C 轮退役 5.1)：
            // 这里只找 pwsh.exe，全落空了回空串(调用点各自把"没引擎"报成一句人话 NoEngineHint，不再静默回落 5.1)。
            // 旧版把商店版(WindowsApps 别名)整个跳过 —— 那套是跟着 panel.ps1 的 RunAs 顾虑抄来的；
            // 但面板自己从不 RunAs(全是读状态 / dry-run / 触发计划任务)，而本机实测(2026-10-06)只有商店版，
            // 照跳等于这台机器直接没引擎 —— 所以只保留"安装版优先"，商店版进末档。
            string path = Environment.GetEnvironmentVariable("PATH") ?? "";
            string storeAlias = null;
            foreach (string dir in path.Split(';'))
            {
                if (dir.Trim().Length == 0) continue;
                string cand;
                try { cand = Path.Combine(dir.Trim(), "pwsh.exe"); } catch { continue; }
                if (!File.Exists(cand)) continue;
                if (cand.IndexOf("WindowsApps", StringComparison.OrdinalIgnoreCase) >= 0) { if (storeAlias == null) storeAlias = cand; continue; }
                return cand;   // PATH 里的安装版：正统(和面板 PS 侧的"安装版优先"同口径)
            }
            if (storeAlias == null)
                storeAlias = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), @"Microsoft\WindowsApps\pwsh.exe");
            if (File.Exists(storeAlias)) return storeAlias;
            string prog = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), @"PowerShell\7\pwsh.exe");
            if (File.Exists(prog)) return prog;
            return "";
        }

        string RunDumpSummary()
        {
            if (engine.Length == 0) throw new InvalidOperationException(NoEngineHint);
            ProcessStartInfo psi = new ProcessStartInfo();
            psi.FileName = engine;
            psi.Arguments = "-NoProfile -ExecutionPolicy Bypass -File \"" + Path.Combine(root, @"tools\panel.ps1") + "\" -DumpSummary";
            psi.WorkingDirectory = root;
            psi.UseShellExecute = false;
            psi.CreateNoWindow = true;
            psi.RedirectStandardOutput = true;
            psi.StandardOutputEncoding = Encoding.UTF8;
            psi.RedirectStandardError = true;
            Process p = Process.Start(psi);
            string stdout = p.StandardOutput.ReadToEnd();
            string stderr = p.StandardError.ReadToEnd();
            if (!p.WaitForExit(60000)) { try { p.Kill(); } catch { } throw new TimeoutException("panel.ps1 -DumpSummary 60 秒没返回"); }
            if (string.IsNullOrEmpty(stdout) && !string.IsNullOrEmpty(stderr)) throw new Exception(stderr.Trim());
            return stdout;
        }

        void ParseSummary(string dump)
        {
            lastDump = dump;    // 诊断页把这份协议原文当"当前读数"存档，不另起一次子进程
            // 刷新会再解析一遍：这些列表必须先清空，否则"上一轮/门/环境"每刷新一次翻一倍
            games.Clear(); rowsLast.Clear(); rowsGates.Clear(); rowsEnv.Clear();
            probesByGame.Clear(); helpRows.Clear(); timesByPage.Clear(); elevNames.Clear();
            protoVer = -1;
            string[] lines = dump.Replace("\r\n", "\n").Split('\n');
            foreach (string raw in lines)
            {
                string line = raw.Trim();
                if (line.Length == 0 || line.IndexOf('|') < 0) continue;
                string[] f = line.Split('|');
                switch (f[0])
                {
                    case "PROTO":
                        int v;
                        if (f.Length > 1 && int.TryParse(f[1], out v)) protoVer = v;
                        break;
                    case "HEADER":
                        if (f.Length > 2) entryName = f[2];
                        break;
                    case "GAME":
                        if (f.Length > 2) games.Add(new[] { f[1], f[2] });
                        break;
                    case "TIME": Add(timesByPage, "all", RowOf(f, false)); break;
                    case "LAST": rowsLast.Add(RowOf(f, true)); break;
                    case "GATE": rowsGates.Add(RowOf(f, true)); break;
                    case "ENV": rowsEnv.Add(RowOf(f, false)); break;
                    case "SV":
                        // SV|kind|一行文本|详情行1|详情行2…  (v2)：环境卡里那一行"监督：…"，正文之后的每段是一行展开。
                        // 外壳在这里一个新判定都不加：kind 和那一句都是判定层(lib\supervisor.ps1 的读数)算好的，
                        // 详情段是它逐拍写下的原文 —— 这里只按固定字段位次取文本、拼详情，然后交给 RenderRows。
                        if (f.Length > 2)
                        {
                            Row sv = RowOf(f, false);            // f[2..] 全拼回文本，先把详情段摘掉
                            StringBuilder sd = new StringBuilder();
                            for (int i = 3; i < f.Length; i++)
                            {
                                if (sd.Length > 0) sd.Append('\n');
                                sd.Append(f[i]);
                            }
                            sv.Text = f[2];
                            sv.Detail = sd.ToString();
                            sv.Source = "state\\supervision.json 的现值与逐拍记录(监督 agent 自己写的原文)";
                            rowsEnv.Add(sv);
                        }
                        break;
                    case "PROBE":
                        if (f.Length > 3)
                        {
                            List<Row> l;
                            if (!probesByGame.TryGetValue(f[2], out l)) { l = new List<Row>(); probesByGame[f[2]] = l; }
                            l.Add(RowOf(f, true));   // PROBE|kind|game|text —— 同 LAST/GATE 的字段位次
                        }
                        break;
                    case "HELP":
                        if (f.Length > 2) helpRows.Add(new[] { f[1], f[2] });
                        break;
                    case "FLAGS":
                        if (f.Length > 4)
                        {
                            ackFlagName = f[1]; shopFlagName = f[2];
                            flagsGames = f[3] == "yes"; flagsViaTask = f[4] == "yes";
                            if (f.Length > 5 && f[5].Length > 0) elevNames.AddRange(f[5].Split(','));
                        }
                        break;
                }
            }
            // 对不上协议就说一次，对得上什么都不说(别每 45 秒刷一条横幅盖住真读数)。
            if (protoVer != protoReported)
            {
                protoReported = protoVer;
                string pw = ProtoMismatch();
                if (pw.Length > 0) AppendLog("协议对不上：" + pw);
            }
            // 结论行到位了再补"为什么"：这一步读的是判定层自己写的 history.csv，不加任何新判断。
            LoadLastDetails();
            AttachDetails();
        }

        // 判定层和这个面板对不上协议时该说的那句话；对得上返回空串。
        // 单拆成一个纯函数，是为了让自测能直接问"这份读数你认不认"，不用去翻日志区里的那一条。
        string ProtoMismatch()
        {
            if (protoVer == ProtoKnown) return "";
            if (protoVer < 0) return "判定层给的读数里没有协议版本号(它比这个面板还旧)，按 v" + ProtoKnown + " 尽力渲染 —— 有格子空着先更新仓库";
            if (protoVer > ProtoKnown) return "判定层协议 v" + protoVer + "，这个面板只认 v" + ProtoKnown + " —— 某些格子可能是空的或错位的，重编一次：tools\\panel-cs\\build.ps1";
            return "判定层协议 v" + protoVer + "，比这个面板认的 v" + ProtoKnown + " 还旧 —— 重编一次：tools\\panel-cs\\build.ps1";
        }

        static Row RowOf(string[] f, bool hasGame)
        {
            // 协议：TAG|kind|[game]|文本 —— kind 恒为 f[1]；正文允许再带 '|'，把字段之后全部拼回去
            string kind = f.Length > 1 ? f[1] : "info";
            string game = "";
            int from = 2;
            if (hasGame && f.Length > 2) { game = f[2]; from = 3; }
            StringBuilder sb = new StringBuilder();
            for (int i = from; i < f.Length; i++) { if (sb.Length > 0) sb.Append('|'); sb.Append(f[i]); }
            return new Row(kind, game, sb.ToString());
        }

        static void Add(Dictionary<string, List<Row>> map, string key, Row r)
        {
            List<Row> l;
            if (!map.TryGetValue(key, out l)) { l = new List<Row>(); map[key] = l; }
            l.Add(r);
        }

        string ackFlagName = "", shopFlagName = "";

        // ---------- 上一轮「为什么」：直接读 state\history.csv 的 detail 列 ----------
        // 这不是第二个判定源。detail 那一列本来就是 lib\ark-daily.ps1 / lib\r1999-daily.ps1 收尾时写的原文，
        // PS 面板和 status.ps1 念的是同一句；外壳只是把它从文件里捞出来给人看。
        // 为什么不顺手让它走 -DumpSummary：LAST 行的协议是 kind|game|一行文本，卡片只有 272px 宽，
        // 判定层动辄两三百字的结论塞进去，整张卡就剩省略号了(2026-09-25 那次 blocked 就是这么被吞掉的)。
        string historyPathOverride;          // --whytest 用它喂一份合成 CSV，不碰真历史
        string HistoryPath { get { return string.IsNullOrEmpty(historyPathOverride) ? Path.Combine(root, @"state\history.csv") : historyPathOverride; } }
        readonly Dictionary<string, string> detailByGame = new Dictionary<string, string>();

        void LoadLastDetails()
        {
            detailByGame.Clear();
            List<string[]> recs;
            try { recs = ParseCsv(ReadAllShared(HistoryPath)); }
            catch { return; }    // 没有历史文件/读不动：详情列全空，卡片照旧只报结论，不影响别的卡
            if (recs.Count < 2) return;
            string[] head = recs[0];
            int gi = CsvCol(head, "game"), di = CsvCol(head, "detail");
            if (gi < 0 || di < 0) return;    // 表头对不上(老格式/别的仓) → 宁可不显示，也不按猜的列位读
            for (int i = 1; i < recs.Count; i++)
            {
                string[] r = recs[i];
                if (r.Length <= gi || r.Length <= di) continue;
                string g = r[gi].Trim();
                if (g.Length == 0) continue;
                // 空 detail 也要写进去(覆盖)：留着更早一轮的原因，比"这一轮没说原因"更容易骗人。
                detailByGame[g] = r[di].Trim();
            }
        }

        // 把 detail 挂到「上一轮」那些红/黄行上。判据只有 kind：kind 是 panel.ps1 算好的，外壳不另判一次。
        void AttachDetails()
        {
            if (detailByGame.Count == 0) return;
            for (int i = 0; i < rowsLast.Count; i++)
            {
                Row r = rowsLast[i];
                if (r.Detail.Length > 0) continue;
                if (r.Kind != "err" && r.Kind != "warn") continue;
                string d;
                if (detailByGame.TryGetValue(r.Game, out d) && d.Length > 0) r.Detail = d;
            }
        }

        static string ReadAllShared(string path)
        {
            // 跑着的那轮结束时会往 history.csv 追加，独占读会直接抛 —— 必须 FileShare.ReadWrite。
            using (FileStream fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
            using (StreamReader sr = new StreamReader(fs, Encoding.UTF8))
                return sr.ReadToEnd();
        }

        // 版本号：唯一来源是仓库根那个 VERSION 文件(PowerShell 侧的 Get-GdVersion 读的是同一个)。
        // 这里不写死任何数字，也不要求它一定在 —— 读不到就退成 "0.0.0"，面板照常开。
        static string ReadVersion(string dir)
        {
            try
            {
                string p = Path.Combine(dir, "VERSION");
                if (!File.Exists(p)) { return "0.0.0"; }
                string first = (ReadAllShared(p) ?? "").Replace("\r\n", "\n").Split('\n')[0].Trim();
                return VersionShape(first);
            }
            catch { return "0.0.0"; }
        }

        static string VersionShape(string raw)
        {
            // 纯函数：形状不对(空、多段、带 v、带空格)一律退成 0.0.0，让"版本号读不出来"在界面上是个看得见的假值。
            if (raw == null) { return "0.0.0"; }
            string s = raw.Trim();
            if (!System.Text.RegularExpressions.Regex.IsMatch(s, @"^\d+\.\d+\.\d+$")) { return "0.0.0"; }
            return s;
        }

        static int CsvCol(string[] head, string name)
        {
            for (int i = 0; i < head.Length; i++) if (head[i].Trim().Trim('"') == name) return i;
            return -1;
        }

        readonly ToolTip rowTip = new ToolTip();

        static string ToolTipHint(Row r)
        {
            string head = r.Detail.Length > 120 ? r.Detail.Substring(0, 120) + "…" : r.Detail;
            return head + "\r\n\r\n点这一行 = 把完整原因打到下面的日志区";
        }

        // 点卡片行 → 完整原因进日志区(日志区是面板里唯一能装下长句的地方，而且本来就在那里等人看)
        void ShowRowDetail(Row r)
        {
            string src = string.IsNullOrEmpty(r.Source) ? @"state\history.csv 的 detail 列，判定层原文" : r.Source;
            AppendLog("──── " + GameName(r.Game) + " 这一行的完整原文(" + src + ") ────");
            // 先按 '\n' 切成段(SV 行的每一拍是一段)，每段再各自折行 ——
            // 直接把整段丢进 WrapUnits 会让"一拍一行"的排版糊成一团，回看时反而数不清几拍。
            foreach (string seg in (r.Detail ?? "").Replace("\r\n", "\n").Split('\n'))
            {
                if (seg.Trim().Length == 0) continue;
                foreach (string l in WrapUnits(seg, 96)) AppendLog("  " + l);
            }
        }

        // 开机第一眼就把"有原因可看"这件事说清楚：红色的行点了有反应，人才会去点。
        void HintDetails()
        {
            int n = 0;
            for (int i = 0; i < rowsLast.Count; i++) if (rowsLast[i].Detail.Length > 0) n++;
            if (n == 0) return;
            AppendLog("「上一轮」里有 " + n + " 行带完整原因 —— 鼠标停在上面能预览，点一下打到下面的日志区。");
            AnnounceDetails("打开面板时");
        }

        // 按"显示宽度"折行：中文/全角算 2 个单位，ASCII 算 1 个。日志框里是中文混排，
        // 只有按字数硬折才不会在折点上再吞一次换行；96 个单位 ≈ 一行放得下 48 个汉字。
        // 到宽度后先回头找最近的"能断的地方"(空格和停顿标点)，找不到才硬切 ——
        // 2026-09-25 第一版就是硬切，把"--autostart 没生效"折成了"-"/"a 实测无效"，读着像坏了。
        static readonly char[] BreakAfter = new char[] {
            ' ', '\t', ',', ';', ':', ')', ']', '，', '。', '；', '：', '）', '】', '》', '、'
        };

        static List<string> WrapUnits(string text, int units)
        {
            List<string> outLines = new List<string>();
            if (string.IsNullOrEmpty(text)) return outLines;
            List<char> cur = new List<char>();
            int w = 0;
            for (int i = 0; i < text.Length; i++)
            {
                char ch = text[i];
                int cw = (ch > 0x1FFF) ? 2 : 1;
                if (w + cw > units && cur.Count > 0)
                {
                    int cut = 0;
                    for (int j = cur.Count - 1; j >= 0 && j >= cur.Count - 16; j--)
                        if (Array.IndexOf(BreakAfter, cur[j]) >= 0 && j + 1 >= 2) { cut = j + 1; break; }
                    if (cut <= 0 || cut > cur.Count) cut = cur.Count;
                    string seg = new string(cur.GetRange(0, cut).ToArray()).TrimEnd(' ', '\t');
                    if (seg.Length > 0) outLines.Add(seg);
                    cur.RemoveRange(0, cut);
                    w = Units(new string(cur.ToArray()));
                }
                cur.Add(ch); w += cw;
            }
            if (cur.Count > 0)
            {
                string tail = new string(cur.ToArray());
                if (tail.Trim().Length > 0) outLines.Add(tail);
            }
            return outLines;
        }

        // 只认 CSV 的这四件事：字段用 " 包、串里的 " 写成 ""、换行分隔记录、行尾 CRLF/LF 都可能。
        // 引号里的逗号和 '|' 都不能当分隔符用 —— history.csv 的 detail 里全是中文标点和路径。
        static List<string[]> ParseCsv(string text)
        {
            List<string[]> recs = new List<string[]>();
            List<string> fields = new List<string>();
            StringBuilder cur = new StringBuilder();
            bool inQuote = false, started = false;   // started = 本字段是否已经开过口(区分"空字段"和"还没开始")
            for (int i = 0; i < text.Length; i++)
            {
                char ch = text[i];
                if (inQuote)
                {
                    if (ch == '"')
                    {
                        if (i + 1 < text.Length && text[i + 1] == '"') { cur.Append('"'); i++; }
                        else inQuote = false;
                        continue;
                    }
                    cur.Append(ch);
                    continue;
                }
                if (ch == '"' && !started) { inQuote = true; started = true; continue; }
                if (ch == ',') { fields.Add(cur.ToString()); cur.Length = 0; started = false; continue; }
                if (ch == '\r') continue;
                if (ch == '\n')
                {
                    fields.Add(cur.ToString()); cur.Length = 0; started = false;
                    if (fields.Count > 1 || fields[0].Length > 0) recs.Add(fields.ToArray());
                    fields.Clear();
                    continue;
                }
                cur.Append(ch); started = true;
            }
            if (started || cur.Length > 0)
            {
                fields.Add(cur.ToString());
                if (fields.Count > 1 || fields[0].Length > 0) recs.Add(fields.ToArray());
            }
            return recs;
        }

        string EntryPath { get { return Path.Combine(root, string.IsNullOrEmpty(entryName) ? "daily-clear.ps1" : entryName); } }

        List<string> DryRunArgs()
        {
            List<string> a = new List<string>();
            a.Add("-DryRun"); a.Add("-Tag"); a.Add("manual");
            if (flagsGames && games.Count > 1) { a.Add("-Games"); a.Add("all"); }
            return a;
        }

        string ElevNote()
        {
            if (elevNames.Count == 0) return "";
            return "，" + string.Join("/", elevNames.ToArray()) + "那一路要管理员";
        }

    }
}