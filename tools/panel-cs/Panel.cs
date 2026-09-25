// tools/panel-cs/Panel.cs —— 每日清体力的图形状态面板(C# / WinForms，.NET Framework 4，系统自带 csc.exe 直接编译)。
//
// 和 panel.ps1 的分工：判定/读数仍然只在 panel.ps1 那一套里 —— 本文件不算任何结论，
// 它做的只有四件事：
//   1. 起一个隐藏子进程跑 `panel.ps1 -DumpSummary`，读 '|' 分隔的协议行(HEADER/GAME/TIME/LAST/GATE/ENV/PROBE/FLAGS/HELP)；
//   2. 把行画成卡片/导航/日志，并把按钮动作转调现成脚本(daily-clear / status / run-via-task)；
//   3. 黑/白双主题切换(唯一的"外观判定"，存在 state\panel-theme.txt)；
//   4. 「实验功能」卡：面板上唯一直接碰 config\settings.json 的地方，而且只碰一个布尔键
//      (runtime.arkVisionRescue) —— 写之前先备份到 backups\，用整文件行内替换、别的一个字节不动。
//      这不是判定：用不用视觉兜底仍由 lib\vision-rescue.ps1 看那个键决定，面板只是拧开关。
//      这也是"PS 面板渐进换 C# 面板"的起点：新功能先落这一侧，panel.ps1 冻结为兜底、不再双写。
// 所以 panel.ps1 里的措辞或 Kind 判据一改，这里一个字不改也跟着变 —— 不会出现两个面板各说各话。
//
// 用法：
//   panel-cs.exe                 打开面板
//   panel-cs.exe --selftest      不显示窗口：读一次协议、渲染、查布局，有红项退出码 1
//   panel-cs.exe --headless      窗口挪到屏幕外，自动起一次预演，验"子进程→日志区→忙碌复位"这条链后自关(零弹窗)
//   panel-cs.exe --exptest       实验功能读写回环自测：翻一次开关→读回→翻回去→读回，全对才 0(会动 settings.json，只在测试树上跑)
//
// 编译：powershell -File tools\panel-cs\build.ps1   (用 C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe)

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
    // ---------- 主题：只有黑、白两套；语义色(绿/黄/红)各自一档，别的颜色一律中性 ----------
    sealed class Theme
    {
        public string Name;
        public Color Window, Rail, Card, CardTitle, NavSub, Line, Accent, NavOn, Hover, Down;
        public Color Text, Ok, Warn, Err, Dim;
        public Color LogBack, LogOk, LogWarn, LogErr, LogStep, LogInfo, LogDim, LogStamp;
        public Color ChipBack, ChipFore, BusyBack, BusyFore;
        public Color PrimaryFore;

        public static Theme Dark()
        {
            Theme t = new Theme();
            t.Name = "dark";
            // 2026-09-25 用户反馈"黑得太过了"：背景整体抬一档灰、正文白字收到 228，
            // 层次仍靠 窗<栏<卡 三级灰度拉开；语义色(绿/黄/红)不动。
            t.Window    = C(27, 27, 27);
            t.Rail      = C(21, 21, 21);
            t.Card      = C(37, 37, 37);
            t.CardTitle = C(190, 190, 190);
            t.NavSub    = C(130, 130, 130);
            t.Line      = C(64, 64, 64);
            t.Accent    = C(255, 255, 255);   // 黑底上唯一的"强调"就是白
            t.NavOn     = C(52, 52, 52);
            t.Hover     = C(48, 48, 48);
            t.Down      = C(64, 64, 64);
            t.Text      = C(228, 228, 228);
            t.Ok        = C(96, 214, 120);
            t.Warn      = C(245, 185, 60);
            t.Err       = C(244, 110, 110);
            t.Dim       = C(140, 140, 140);
            t.LogBack   = C(22, 22, 22);
            t.LogOk     = C(126, 231, 135);
            t.LogWarn   = C(227, 179, 65);
            t.LogErr    = C(255, 123, 114);
            t.LogStep   = C(230, 230, 230);
            t.LogInfo   = C(205, 205, 205);
            t.LogDim    = C(120, 120, 120);
            t.LogStamp  = C(125, 125, 125);
            t.ChipBack  = C(50, 50, 50);
            t.ChipFore  = C(150, 150, 150);
            t.BusyBack  = C(58, 46, 16);
            t.BusyFore  = C(245, 185, 60);
            t.PrimaryFore = C(0, 0, 0);        // 白底按钮配黑字
            return t;
        }

        public static Theme Light()
        {
            Theme t = new Theme();
            t.Name = "light";
            t.Window    = C(246, 246, 246);
            t.Rail      = C(255, 255, 255);
            t.Card      = C(255, 255, 255);
            t.CardTitle = C(60, 60, 60);
            t.NavSub    = C(120, 120, 120);
            t.Line      = C(210, 210, 210);
            t.Accent    = C(20, 20, 20);      // 白底上唯一的"强调"就是黑
            t.NavOn     = C(228, 228, 228);
            t.Hover     = C(238, 238, 238);
            t.Down      = C(222, 222, 222);
            t.Text      = C(20, 20, 20);
            t.Ok        = C(24, 128, 56);
            t.Warn      = C(158, 106, 0);
            t.Err       = C(178, 34, 34);
            t.Dim       = C(120, 120, 120);
            t.LogBack   = C(255, 255, 255);
            t.LogOk     = C(24, 128, 56);
            t.LogWarn   = C(158, 106, 0);
            t.LogErr    = C(178, 34, 34);
            t.LogStep   = C(20, 20, 20);
            t.LogInfo   = C(45, 45, 45);
            t.LogDim    = C(140, 140, 140);
            t.LogStamp  = C(140, 140, 140);
            t.ChipBack  = C(238, 238, 238);
            t.ChipFore  = C(110, 110, 110);
            t.BusyBack  = C(255, 242, 204);
            t.BusyFore  = C(158, 106, 0);
            t.PrimaryFore = C(255, 255, 255);  // 黑底按钮配白字
            return t;
        }

        static Color C(int r, int g, int b) { return Color.FromArgb(r, g, b); }
    }

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
        public Row(string kind, string game, string text) { Kind = kind; Game = game; Text = text; }
        public Row(string kind, string game, string text, string detail) { Kind = kind; Game = game; Text = text; Detail = detail ?? ""; }
    }

    // ---------- 主窗体 ----------
    sealed class PanelForm : Form
    {
        [DllImport("user32.dll")] static extern bool DestroyIcon(IntPtr h);
        [DllImport("user32.dll")] static extern bool ReleaseCapture();
        [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr h, int msg, IntPtr w, IntPtr l);

        // 无边框窗体：CS_DROPSHADOW 让 Win10/11 给一块淡投影，不然贴屏幕上像张纸
        protected override CreateParams CreateParams
        {
            get { CreateParams cp = base.CreateParams; cp.ClassStyle |= 0x00020000; return cp; }
        }

        const int WM_NCLBUTTONDOWN = 0xA1, HTCAPTION = 2, WM_NCHITTEST = 0x84;

        const int RailW = 200;
        const int X0 = 212;        // 内容区左边界
        const int CW = 916;        // 内容区宽度
        const int RowY0 = 32, RowH = 18, RowPitch = 20;   // 卡片行几何(和 panel.ps1 一致：132 高正好 5 行)

        readonly string root;
        readonly string engine;
        string entryName = "daily-clear.ps1";   // 由 HEADER 协议行赋值(不能 readonly)
        bool flagsGames, flagsViaTask;   // FLAGS 协议行赋值
        readonly List<string[]> games = new List<string[]>();   // [id, 显示名]
        readonly List<string> elevNames = new List<string>();
        readonly List<string[]> helpRows = new List<string[]>(); // [标签, 命令]
        readonly Dictionary<string, List<Row>> timesByPage = new Dictionary<string, List<Row>>();
        List<Row> rowsLast = new List<Row>(), rowsGates = new List<Row>(), rowsEnv = new List<Row>();
        readonly Dictionary<string, List<Row>> probesByGame = new Dictionary<string, List<Row>>();

        Theme th;
        string page = "all";
        bool busy; DateTime confirmAt = DateTime.MinValue, ackAt = DateTime.MinValue;
        string idleRunText = "开始跑一轮(全部)";
        // 外部轮次(别的进程/计划任务起的这一轮)：面板既不能当它不存在(会二次点火撞车)，
        // 也不能把它当自己的子进程(停不掉、也不该由面板的退出码来下结论)。
        bool extBusy; string extNote = "";
        string extLog; long extPos;   // 外部那一轮的日志：面板只跟随，不判定成败

        readonly Panel rail = new Panel();
        readonly Panel hdr = new Panel();
        readonly Label pageTitle = new Label();
        readonly Label runChip = new Label();
        readonly Label stampLabel = new Label();
        readonly PictureBox avatar = new PictureBox();
        Label brandTitle, brandSub, foot1, foot2;
        readonly Dictionary<string, Button> navBtns = new Dictionary<string, Button>();
        readonly Dictionary<string, Panel> navStripes = new Dictionary<string, Panel>();
        readonly Dictionary<string, Label> navDots = new Dictionary<string, Label>();
        readonly Panel cardTimes = new Panel(), cardLast = new Panel(), cardGates = new Panel(), cardEnv = new Panel(), cardExp = new Panel();
        readonly Panel act = new Panel();
        readonly Panel logCard = new Panel();
        readonly RichTextBox logBox = new RichTextBox();
        Button runBtn, btnDry, btnStatus, btnRefresh, ackBtn, btnShop, btnLog, btnHelp, btnTheme, expBtn;
        Button btnOnly;             // 只跑当前页那一路(单游戏仓会自动藏起来)
        DateTime onlyAt = DateTime.MinValue;
        Button btnMin, btnMax, btnClose;   // 无边框后的自绘"标题栏"三键
        readonly System.Windows.Forms.Timer pump = new System.Windows.Forms.Timer();
        // beat：状态自动刷新。没有它的时候"现在到底跑没跑/跑到哪"只能靠手点刷新，
        // 而日常一轮要跑十几到几十分钟，要盯着的人恰恰最不该去点按钮。
        readonly System.Windows.Forms.Timer beat = new System.Windows.Forms.Timer();
        int beatSec = 45;             // 一轮 DumpSummary 要起一个 pwsh 子进程，45 秒是"够快"和"不抢判定层的锁"的折中
        // --no-beat 关掉自动刷新，只留手点「刷新状态」(自检和极慢机器用)。
        // 走 static 是因为要在构造之前传进来：构造函数的签名已经有 headless 一个参数，
        // 不为这个开关去动它的调用点和重载。
        public static bool AutoBeatDefault = true;
        bool autoBeat = AutoBeatDefault;

        Process child;
        string outPath, errPath, tailPath;
        long tailPos;
        DateTime tailSince;
        int lastExit = int.MinValue;   // 子进程走事件队列读，outPos/errPos 那对文件偏移已经不需要了

        // 两种不进屏幕的验证路径共用
        public string HlResult;
        readonly DateTime hlDeadline;

        public PanelForm(bool headless)
        {
            // 定位仓库根：exe 在 <root>\tools\panel-cs\ 下；被复制走(桌面快捷方式等)时退回当前目录
            string exeDir = AppDomain.CurrentDomain.BaseDirectory;
            string guess = Path.GetFullPath(Path.Combine(exeDir, @"..\.."));
            if (File.Exists(Path.Combine(guess, "tools", "panel.ps1"))) root = guess;
            else if (File.Exists(Path.Combine(exeDir, "tools", "panel.ps1"))) root = exeDir;
            else root = Directory.GetCurrentDirectory();

            engine = PickEngine();
            th = LoadTheme();

            Text = "每日清体力 · 状态面板";
            StartPosition = FormStartPosition.CenterScreen;
            // 无边框：系统白标题栏去掉，标题区由 hdr 自绘(拖拽 + — □ × 三个键)。
            // 左边缘被 rail 子窗口盖住，鼠标命中测试到不了本窗体 → 左边不提供拉宽，用最大化按钮兜住。
            FormBorderStyle = FormBorderStyle.None;
            ClientSize = new Size(1140, 730);
            MinimumSize = new Size(1000, 640);
            Font = new Font("Microsoft YaHei UI", 9.5f);
            try
            {
                Icon ic = LoadEmbeddedIcon();
                if (ic != null) Icon = ic;
            }
            catch { }

            BuildUi();
            ApplyTheme();

            string dump;
            try { dump = RunDumpSummary(); }
            catch (Exception ex)
            {
                games.Add(new[] { "all", "总览" });
                dump = null;
                AppendLog("[ERROR] 读状态失败：" + ex.Message);
            }
            if (dump != null) ParseSummary(dump);
            RebuildNav();
            RefreshCards();
            AppendLog("提示：面板只转达，不替你判定 —— 结论和命令行跑同一套脚本得到的一样。(C# 外壳 · " + (th.Name == "dark" ? "黑" : "白") + "底)");
            HintDetails();

            pump.Interval = 800;
            pump.Tick += OnPumpTick;
            pump.Start();

            // 自动刷新：先把"有没有别处在跑"认一次(否则开机时外部那轮要到 45 秒后才看得见)，
            // 再按 beatSec 一拍一拍走。--no-beat 时只留手点「刷新状态」，行为回到加这个功能之前。
            RefreshExternal();
            beat.Interval = beatSec * 1000;
            beat.Tick += OnBeatTick;
            if (autoBeat) beat.Start();

            if (headless)
            {
                StartPosition = FormStartPosition.Manual;
                Location = new Point(-16000, -16000);
                ShowInTaskbar = false;
                hlDeadline = DateTime.Now.AddSeconds(150);
                Shown += delegate
                {
                    AppendLog("[headless] 自动验证：起一次预演(dry-run，不会启动任何游戏/助手)");
                    StartTask(entryName, DryRunArgs(), "headless 预演", false);
                };
            }
            else hlDeadline = DateTime.MinValue;
        }

        // ---------- 引擎 / 数据源 ----------
        static string PickEngine()
        {
            // 和 panel.ps1 同一套优先级：安装版 pwsh 优先，商店版(WindowsApps)不能提权直接 RunAs；没有就用系统 powershell
            string path = Environment.GetEnvironmentVariable("PATH") ?? "";
            foreach (string dir in path.Split(';'))
            {
                if (dir.Trim().Length == 0) continue;
                string cand;
                try { cand = Path.Combine(dir.Trim(), "pwsh.exe"); } catch { continue; }
                if (!File.Exists(cand)) continue;
                if (cand.IndexOf("WindowsApps", StringComparison.OrdinalIgnoreCase) >= 0) continue;
                return cand;
            }
            return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "WindowsPowerShell\\v1.0\\powershell.exe");
        }

        string RunDumpSummary()
        {
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
            // 刷新会再解析一遍：这些列表必须先清空，否则"上一轮/门/环境"每刷新一次翻一倍
            games.Clear(); rowsLast.Clear(); rowsGates.Clear(); rowsEnv.Clear();
            probesByGame.Clear(); helpRows.Clear(); timesByPage.Clear(); elevNames.Clear();
            string[] lines = dump.Replace("\r\n", "\n").Split('\n');
            foreach (string raw in lines)
            {
                string line = raw.Trim();
                if (line.Length == 0 || line.IndexOf('|') < 0) continue;
                string[] f = line.Split('|');
                switch (f[0])
                {
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
            // 结论行到位了再补"为什么"：这一步读的是判定层自己写的 history.csv，不加任何新判断。
            LoadLastDetails();
            AttachDetails();
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
            AppendLog("──── " + GameName(r.Game) + " 上一轮的完整结论(state\\history.csv 的 detail 列，判定层原文) ────");
            foreach (string l in WrapUnits(r.Detail, 96)) AppendLog("  " + l);
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

        // ---------- 版式(和 panel.ps1 同一套坐标：左侧导航带 + 头部 + 四卡 + 动作区 + 日志) ----------
        void BuildUi()
        {
            SuspendLayout();

            rail.Location = new Point(0, 0);
            rail.Size = new Size(RailW, 730);
            rail.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Bottom;
            Controls.Add(rail);

            avatar.Location = new Point(20, 16);
            avatar.Size = new Size(44, 44);
            avatar.SizeMode = PictureBoxSizeMode.Zoom;
            avatar.Anchor = AnchorStyles.Top | AnchorStyles.Left;
            try { Image img = LoadEmbeddedImage(); if (img != null) { avatar.Image = img; avatar.Visible = true; } } catch { }
            rail.Controls.Add(avatar);

            // 图标 loaded 没 loaded：看 avatar.Image，别看 avatar.Visible —— WinForms 的 Visible getter
            // 是"有效可见性"，窗体 Show 之前恒为 false，用它判断布局会让品牌标题永远挤在图标位上。
            bool hasAvatar = avatar.Image != null;
            brandTitle = MakeLabel(rail, "每日清体力", hasAvatar ? 72 : 20, 18, 0, 0, null, new Font("Microsoft YaHei UI", 13.5f, FontStyle.Bold), true);
            brandSub = MakeLabel(rail, "只转达 · 不判定", hasAvatar ? 73 : 21, 50, 0, 0, null, SmallFont(), true);

            Panel sep = new Panel();
            sep.Location = new Point(20, 76);
            sep.Size = new Size(RailW - 40, 1);
            sep.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            rail.Controls.Add(sep);
            themeTag(sep, "line");

            Label navCap = MakeLabel(rail, "导 航", 21, 86, 0, 0, null, SectionFont(), true);
            themeTag(navCap, "navsub");

            hdr.Location = new Point(X0, 0);            // 无边框后顶到窗体最上沿，整条 hdr 就是"标题栏"
            hdr.Size = new Size(CW, 76);
            hdr.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            Controls.Add(hdr);
            themeTag(hdr, "rail");

            btnMin = MakeCaptionBtn("—", CW - 96);
            btnMax = MakeCaptionBtn("□", CW - 64);
            btnClose = MakeCaptionBtn("×", CW - 32);
            btnMin.Click += delegate { WindowState = FormWindowState.Minimized; };
            btnMax.Click += delegate
            {
                if (WindowState == FormWindowState.Maximized) { WindowState = FormWindowState.Normal; return; }
                MaximizedBounds = Screen.FromControl(this).WorkingArea;   // 不盖任务栏
                WindowState = FormWindowState.Maximized;
            };
            btnClose.Click += delegate { Close(); };

            pageTitle.Text = "总览";
            pageTitle.Location = new Point(20, 24);
            pageTitle.AutoSize = true;
            pageTitle.Font = new Font("Microsoft YaHei UI", 12f, FontStyle.Bold);
            hdr.Controls.Add(pageTitle);
            themeTag(pageTitle, "title_big");

            Label hSub = MakeLabel(hdr, "结论与命令行跑同一套脚本得到的一样 · 要点鼠标请先看底部日志", 21, 54, 0, 0, null, SmallFont(), true);
            themeTag(hSub, "navsub");

            runChip.Text = "空闲";
            runChip.Location = new Point(792, 26);
            runChip.Size = new Size(106, 26);
            runChip.TextAlign = ContentAlignment.MiddleCenter;
            runChip.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            hdr.Controls.Add(runChip);
            runChip.Tag = "chip";

            stampLabel.Text = "状态刷新于 -";
            stampLabel.Location = new Point(606, 55);
            stampLabel.Size = new Size(292, 17);
            stampLabel.TextAlign = ContentAlignment.TopRight;
            stampLabel.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            hdr.Controls.Add(stampLabel);
            themeTag(stampLabel, "navsub");

            MakeCard(cardTimes, X0, 86, 300, 132, "定时任务", AnchorStyles.Top | AnchorStyles.Left);
            MakeCard(cardLast, X0 + 312, 86, 300, 132, "上一轮", AnchorStyles.Top | AnchorStyles.Left);
            MakeCard(cardEnv, X0 + 624, 86, 292, 132, "环境", AnchorStyles.Top | AnchorStyles.Right);
            // 第二行原来被"两道门"整行占满；让出右半边给「实验功能」(门最多 5 行、600px 宽装得下最长那条)
            MakeCard(cardGates, X0, 226, 600, 132, "两道确认门(新活动要先你手动打过 / 商店领空后不再刷活动)", AnchorStyles.Top | AnchorStyles.Left);
            MakeCard(cardExp, X0 + 612, 226, CW - 612, 132, "实验功能", AnchorStyles.Top | AnchorStyles.Right);
            // 按钮放卡片右下角：行区到 y=70 为止(RenderRows 最多 2 行)，96 起 26 高，永远不会和行压住
            expBtn = MakeButton(cardExp, "关掉它", 198, 96, 92, 26, "chip");
            expBtn.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            expBtn.Click += delegate
            {
                string to = expBtn.AccessibleName == null ? "" : expBtn.AccessibleName;
                if (to != "on" && to != "off") { AppendLog("实验状态没读到，这个按钮这次不做事。"); return; }
                AppendLog("实验功能：" + WriteExpFlag(to == "on"));
                RefreshCards();
            };

            act.Location = new Point(X0, 366);
            act.Size = new Size(CW, 104);
            act.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            Controls.Add(act);
            themeTag(act, "card");
            Borderless(act);

            runBtn = MakeButton(act, idleRunText, 16, 16, 214, 44, "primary");
            runBtn.Click += OnRunClick;
            btnDry = MakeButton(act, "今天会跑什么", 238, 16, 150, 42, "secondary");
            btnDry.Click += delegate { StartTask(EntryPath, DryRunArgs(), "预演今天会跑什么(不启动任何程序)", false); };
            btnStatus = MakeButton(act, "环境体检", 396, 16, 150, 42, "secondary");
            btnStatus.Click += delegate { StartTask(Path.Combine(root, "status.ps1"), new List<string>(), "环境体检(只读)", false); };
            btnRefresh = MakeButton(act, "刷新状态", 554, 16, 120, 42, "secondary");
            btnRefresh.Click += OnRefreshClick;

            Label note = MakeLabel(act, "开跑前会让你再点一次确认(12 秒内)", 690, 28, 214, 20, ContentAlignment.TopRight, SmallFont(), false);
            themeTag(note, "dim");

            ackBtn = MakeButton(act, "我打过活动了", 16, 68, 170, 30, "chip");
            ackBtn.Click += OnAckClick;
            btnShop = MakeButton(act, "商店已领空", 194, 68, 166, 30, "chip");
            btnShop.Click += delegate
            {
                if (shopFlagName.Length == 0) { AppendLog("这个仓库的入口脚本没有\"商店领空\"参数，这个按钮不做事。"); return; }
                StartTask(EntryPath, new List<string> { "-" + shopFlagName, "-DryRun" }, "标记活动商店已领空(剩余理智转长草)", false);
            };
            btnLog = MakeButton(act, "打开日志目录", 368, 68, 150, 30, "chip");
            btnLog.Click += delegate
            {
                try { Process.Start("explorer.exe", Path.Combine(root, "logs")); }
                catch (Exception ex) { AppendLog("打不开日志目录：" + ex.Message); }
            };
            btnOnly = MakeButton(act, "只跑这一路", 690, 68, 150, 30, "chip");
            btnOnly.Click += OnOnlyClick;
            btnHelp = MakeButton(act, "命令行怎么跑", 526, 68, 150, 30, "chip");
            btnHelp.Click += delegate
            {
                AppendLog("这些按钮对应的命令行(在 " + root + " 下执行)：");
                foreach (string[] r in helpRows) AppendLog("  " + Pad(r[0], 16) + r[1]);
                AppendLog("完整参数说明看 README.md。");
            };

            logCard.Location = new Point(X0, 478);
            logCard.Size = new Size(CW, 240);
            logCard.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Bottom;
            Controls.Add(logCard);
            themeTag(logCard, "card");
            Borderless(logCard);

            Label logTitle = MakeLabel(logCard, "运行日志", 14, 10, 200, 17, null, SectionFont(), true);
            themeTag(logTitle, "cardtitle");
            Label logNote = MakeLabel(logCard, "开跑后会同时跟随 logs\\run-*.log", 586, 10, 316, 17, ContentAlignment.TopRight, SmallFont(), false);
            themeTag(logNote, "dim");

            logBox.Location = new Point(14, 30);
            logBox.Size = new Size(888, 196);
            logBox.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Bottom;
            logBox.BorderStyle = BorderStyle.None;
            logBox.ReadOnly = true;
            logBox.Font = new Font("Consolas", 9.5f);
            logBox.DetectUrls = false;
            logCard.Controls.Add(logBox);

            btnTheme = MakeButton(rail, th.Name == "dark" ? "切到白底" : "切到黑底", 20, 640, 160, 30, "chip");
            btnTheme.Anchor = AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            btnTheme.Click += OnThemeClick;

            // 页脚两行的文案要等协议解析完才知道(这一仓有哪几路)，这里只建控件，文本在 RefreshCards 里刷
            foot1 = MakeLabel(rail, "面板 C# · 子进程 " + Path.GetFileName(engine), 21, 684, RailW - 30, 17, null, SmallFont(), false);
            foot1.Anchor = AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            themeTag(foot1, "navsub");
            foot2 = MakeLabel(rail, "这一仓 …", 21, 705, RailW - 30, 17, null, SmallFont(), false);
            foot2.Anchor = AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            themeTag(foot2, "navsub");

            // 拖拽：无边框后把"空白处按下"交给系统当标题栏拖动(自带双击最大化)。
            // 子控件(标签/按钮)自己吃掉各自的点击，所以只有 hdr/rail 的空底和窗体露出的边条能触发。
            MouseDown += BeginDrag;
            hdr.MouseDown += BeginDrag;
            rail.MouseDown += BeginDrag;

            ResumeLayout(true);
        }

        string[] GameIds()
        {
            List<string> ids = new List<string>();
            foreach (string[] g in games) ids.Add(g[0]);
            if (ids.Count == 0) ids.Add("?");
            return ids.ToArray();
        }

        // ---------- 无边框窗口的"标题栏行为"：拖拽 + 边缘拉宽 ----------
        void BeginDrag(object sender, MouseEventArgs e)
        {
            if (e.Button != MouseButtons.Left || WindowState == FormWindowState.Maximized) return;
            ReleaseCapture();
            SendMessage(Handle, WM_NCLBUTTONDOWN, (IntPtr)HTCAPTION, IntPtr.Zero);
        }

        protected override void WndProc(ref Message m)
        {
            // 露在外面的三条边(上/右/下)给 8px 拉宽把手；左边整条被 rail 盖着，命中测试到不了这里。
            if (m.Msg == WM_NCHITTEST && WindowState != FormWindowState.Maximized)
            {
                int sx = (short)(int)(m.LParam.ToInt64() & 0xFFFF);
                int sy = (short)(int)((m.LParam.ToInt64() >> 16) & 0xFFFF);
                Point p = PointToClient(new Point(sx, sy));
                const int grip = 8;
                bool l = p.X >= 0 && p.X < grip, r = p.X >= Width - grip && p.X < Width;
                bool t = p.Y >= 0 && p.Y < grip, b = p.Y >= Height - grip && p.Y < Height;
                int code = 0;
                if (t && l) code = 13; else if (t && r) code = 14; else if (b && l) code = 16; else if (b && r) code = 17;
                else if (l) code = 10; else if (r) code = 11; else if (t) code = 12; else if (b) code = 15;
                if (code != 0) { m.Result = (IntPtr)code; return; }
            }
            base.WndProc(ref m);
        }

        Button MakeCaptionBtn(string glyph, int x)
        {
            Button b = new Button();
            b.Text = glyph;
            b.Location = new Point(x, 0);
            b.Size = new Size(32, 24);
            b.FlatStyle = FlatStyle.Flat;
            b.UseVisualStyleBackColor = false;
            b.FlatAppearance.BorderSize = 0;
            b.Font = new Font("Microsoft YaHei UI", 10f);
            b.TabStop = false;
            b.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            b.Tag = "capbtn";
            hdr.Controls.Add(b);
            return b;
        }

        void RebuildNav()
        {
            foreach (Control c in navBtns.Values) rail.Controls.Remove(c);
            navBtns.Clear(); navStripes.Clear(); navDots.Clear();
            int y = 112;
            List<string[]> items = new List<string[]>();
            items.Add(new[] { "all", "总览" });
            foreach (string[] g in games) items.Add(g);
            foreach (string[] it in items)
            {
                Button nb = MakeButton(rail, it[1], 12, y, RailW - 24, 34, "nav");
                nb.TextAlign = ContentAlignment.MiddleLeft;
                nb.Padding = new Padding(12, 0, 26, 0);
                string id = it[0];
                nb.Click += delegate { SelectPage(id); };
                navBtns[id] = nb;

                Panel st = new Panel();
                st.Location = new Point(0, 8);
                st.Size = new Size(3, 18);
                st.Tag = "navstripe:" + id;
                nb.Controls.Add(st);
                navStripes[id] = st;

                Label dot = new Label();
                dot.AutoSize = false;
                dot.Location = new Point(RailW - 24 - 24, 9);
                dot.Size = new Size(16, 16);
                dot.TextAlign = ContentAlignment.MiddleCenter;
                dot.Text = "●";
                dot.Font = SmallFont();
                dot.BackColor = th.Rail;    // 跟着导航按钮底色走，RefreshCards 里随选中态改
                dot.Tag = "navdot:" + id;
                nb.Controls.Add(dot);
                navDots[id] = dot;
                y += 40;
            }
            Label hint = MakeLabel(rail, "导航只是换个角度看同一批读数", 21, y + 10, RailW - 30, 17, null, SmallFont(), false);
            themeTag(hint, "navsub");
        }

        // ---------- 卡片渲染 ----------
        void SelectPage(string id)
        {
            page = id;
            RefreshCards();
        }

        void RefreshCards()
        {
            string title = "总览";
            foreach (string[] g in games) if (g[0] == page) title = g[1];
            if (page == "all") title = "总览";
            pageTitle.Text = title;
            UpdateOnlyBtn();

            List<Row> last = PageRows(rowsLast);
            if (last.Count == 0 && page != "all") last.Add(new Row("dim", "", PageTitle() + " 这一路这台机器还没跑过"));
            List<Row> gates = PageRows(rowsGates);
            if (gates.Count == 0) gates.Add(new Row("warn", "", "没读到门状态(判定库不在？)"));
            // 环境卡：总览 = 原样；某一路 = 结论行 + 这一路的探针 + 说明行(和 panel.ps1 Get-PanelPageRows 同序)
            List<Row> env = new List<Row>();
            if (page == "all") env.AddRange(rowsEnv);
            else
            {
                foreach (Row r in rowsEnv) if (r.Kind != "dim") env.Add(r);
                List<Row> mod;
                if (probesByGame.TryGetValue(page, out mod)) env.AddRange(mod);
                foreach (Row r in rowsEnv) if (r.Kind == "dim" && r.Text.IndexOf("这一仓") < 0) env.Add(r);
            }
            List<Row> t;
            RenderRows(cardTimes, timesByPage.TryGetValue("all", out t) ? t : new List<Row>(), 4);
            RenderRows(cardLast, last, 4);
            RenderRows(cardGates, gates, 5);
            RenderRows(cardEnv, env, 6);

            // ---- 实验功能卡：唯一不经 -DumpSummary 的读数(只读那一个布尔键)，分页时不隐藏 ----
            List<Row> exp = new List<Row>();
            if (ExpImplemented())
            {
                bool? v = ReadExpFlag();
                bool on = !v.HasValue || v.Value;      // 和 vision-rescue 同语义：没显式 false 就是开
                exp.Add(new Row(on ? "ok" : "dim", "",
                    "AI 辅助(视觉兜底)：" + (v.HasValue ? (on ? "开着" : "关了") : "开着(默认)")));
                exp.Add(new Row("dim", "", "卡住时才用 · 正常一轮零请求"));
                expBtn.Text = on ? "关掉它" : "打开它";
                expBtn.AccessibleName = on ? "off" : "on";   // 点击要写去的目标值，这里一次算好
                expBtn.Visible = true;
            }
            else
            {
                exp.Add(new Row("dim", "", "这一仓没有可配置的实验功能"));
                expBtn.Visible = false;
            }
            RenderRows(cardExp, exp, 2);

            foreach (KeyValuePair<string, Button> kv in navBtns)
            {
                bool on = kv.Key == page;
                string kind = WorstKind(kv.Key);
                kv.Value.BackColor = on ? th.NavOn : th.Rail;
                kv.Value.FlatAppearance.MouseOverBackColor = on ? th.NavOn : th.Hover;
                Panel st = navStripes[kv.Key];
                st.BackColor = on ? th.Accent : th.Rail;
                Label dot = navDots[kv.Key];
                dot.BackColor = kv.Value.BackColor;      // 圆点嵌在按钮里：底色跟选中态走，不然选中时带一块色斑
                dot.ForeColor = KindColor(kind == "dim" ? (on ? "info" : "dim") : kind);
                dot.Visible = kind != "dim" || on;
            }
            stampLabel.Text = "状态刷新于 " + DateTime.Now.ToString("HH:mm:ss");
            foot2.Text = "这一仓 " + string.Join(" + ", GameIds());
        }

        string PageTitle()
        {
            foreach (string[] g in games) if (g[0] == page) return g[1];
            return "总览";
        }

        string WorstKind(string forPage)
        {
            List<Row> all = new List<Row>();
            all.AddRange(PageRows(rowsLast));
            all.AddRange(PageRows(rowsGates));
            if (forPage != "all")
            {
                List<Row> mod;
                if (probesByGame.TryGetValue(forPage, out mod)) all.AddRange(mod);
            }
            else foreach (KeyValuePair<string, List<Row>> kv in probesByGame) all.AddRange(kv.Value);
            foreach (Row r in all) if (r.Kind == "err") return "err";
            foreach (Row r in all) if (r.Kind == "warn") return "warn";
            foreach (Row r in all) if (r.Kind == "ok") return "ok";
            return "dim";
        }

        List<Row> PageRows(List<Row> rows)
        {
            List<Row> outRows = new List<Row>();
            foreach (Row r in rows)
                if (page == "all" || r.Game.Length == 0 || r.Game == page) outRows.Add(r);
            return outRows;
        }

        // ---------- 实验功能：面板唯一直接读写 settings.json 的地方(只碰一个布尔键) ----------
        // 为什么敢让 C# 直接碰配置：这不是判定 —— 用不用视觉兜底仍由 lib\vision-rescue.ps1 读同一个键决定，
        // 面板只是替玩家拧开关。写法用"整文件行内替换"：只换 true/false 那几个字节，
        // 别的内容(含中文说明、注释性键)一个字节不动，改前照 PS 那边的规矩备份 + 每文件只留 20 份。
        static readonly Regex ExpKeyRe = new Regex("(\"arkVisionRescue\"\\s*:\\s*)(true|false)");
        string SettingsPath { get { return Path.Combine(root, @"config\settings.json"); } }
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

        void RenderRows(Panel card, List<Row> rows, int maxLines)
        {
            // 清掉上一批行(Tag 前缀 line:)，标题/强调块不动
            List<Control> dead = new List<Control>();
            foreach (Control c in card.Controls)
                if (c.Tag != null && c.Tag.ToString().StartsWith("line:")) dead.Add(c);
            foreach (Control c in dead) { card.Controls.Remove(c); c.Dispose(); }

            int w = card.ClientSize.Width - 28;
            int cap = (int)Math.Floor((card.ClientSize.Height - RowY0 - RowH) / (double)RowPitch) + 1;
            if (cap < 1) cap = 1;
            int n = Math.Min(rows.Count, maxLines);
            if (n > cap) n = cap;
            if (rows.Count > n && n >= cap) n--;      // 末尾留一行给"…另有 N 条"
            if (n < 0) n = 0;
            int y = RowY0;
            for (int i = 0; i < n; i++)
            {
                Row r = rows[i];
                Label l = new Label();
                l.AutoSize = false;
                l.AutoEllipsis = true;
                l.Location = new Point(14, y);
                l.Size = new Size(w, RowH);
                l.Text = r.Text;
                l.BackColor = th.Card;      // 同 MakeLabel：不显式给底色，主题换完就是"隐形行"
                l.ForeColor = KindColor(r.Kind);
                l.Tag = "line:" + r.Kind;
                l.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
                // 带完整原因的行 = 可点。光标变手 + 悬停先给一句预告，点开才把整段打进日志区。
                // 不做"就地展开成两行"：卡片容量是按行算死的，展开会把同卡别的行挤掉，
                // 而"为什么"恰恰经常在两条以上(方舟和 1999 一起红) —— 那时日志区才是够宽的地方。
                if (r.Detail.Length > 0)
                {
                    Row captured = r;
                    l.Cursor = Cursors.Hand;
                    rowTip.SetToolTip(l, ToolTipHint(captured));
                    l.Click += delegate { ShowRowDetail(captured); };
                }
                card.Controls.Add(l);
                y += RowPitch;
            }
            if (rows.Count > n)
            {
                Label l = new Label();
                l.AutoSize = false;
                l.AutoEllipsis = true;
                l.Location = new Point(14, y);
                l.Size = new Size(w, RowH);
                l.Text = "…另有 " + (rows.Count - n) + " 条，命令行跑 status.ps1 看全部";
                l.BackColor = th.Card;
                l.ForeColor = th.Dim;
                l.Tag = "line:dim";
                l.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
                card.Controls.Add(l);
            }
        }

        Color KindColor(string kind)
        {
            switch (kind)
            {
                case "ok": return th.Ok;
                case "warn": return th.Warn;
                case "err": return th.Err;
                case "dim": return th.Dim;
                default: return th.Text;
            }
        }

        // ---------- 主题 ----------
        void OnThemeClick(object sender, EventArgs e)
        {
            th = th.Name == "dark" ? Theme.Light() : Theme.Dark();
            ApplyTheme();
            btnTheme.Text = th.Name == "dark" ? "切到白底" : "切到黑底";
            RefreshCards();
            SetChip(busy ? "正在跑" : "空闲", busy);
            SaveTheme();
            AppendLog("已切到" + (th.Name == "dark" ? "黑" : "白") + "底主题(记在 state\\panel-theme.txt，下次打开还在)。");
        }

        void ApplyTheme()
        {
            BackColor = th.Window;
            foreach (Control c in AllControls()) ApplyOne(c);
            foreach (Panel p in new[] { cardTimes, cardLast, cardGates, cardEnv, cardExp, act, logCard }) p.Invalidate();
            hdr.Invalidate();
            logBox.Invalidate();
        }

        void ApplyOne(Control c)
        {
            object tagObj = c.Tag;
            string tag = tagObj == null ? "" : tagObj.ToString();
            if (c == runBtn)
            {
                runBtn.BackColor = th.Accent;
                runBtn.ForeColor = th.PrimaryFore;
                runBtn.FlatAppearance.MouseOverBackColor = th.Name == "dark" ? ControlPaint.Light(th.Accent, 0.6f) : ControlPaint.Dark(th.Accent, 0.15f);
                runBtn.FlatAppearance.MouseDownBackColor = th.Name == "dark" ? ControlPaint.Dark(th.Accent, 0.15f) : ControlPaint.Light(th.Accent, 0.6f);
                return;
            }
            if (c == rail) { rail.BackColor = th.Rail; return; }
            if (c == avatar) { avatar.BackColor = th.Rail; return; }
            if (c == brandTitle) { brandTitle.ForeColor = th.Text; brandTitle.BackColor = rail.BackColor; return; }
            if (c == brandSub) { brandSub.ForeColor = th.NavSub; brandSub.BackColor = rail.BackColor; return; }
            if (c == logBox) { logBox.BackColor = th.LogBack; logBox.ForeColor = th.LogInfo; RepaintLog(); return; }
            // "chip" 这个 Tag 被两类控件共用：runChip 那颗状态灯(Label)和一排小按钮(Button)。
            // 早先不区分，chip 按钮全被 SetChip 分支提前 return，切主题时字色不跟着换 —— 这里只留 Label 走 SetChip。
            if (tag == "chip" && !(c is Button)) { SetChip(chipText, busy); return; }
            if (tag.StartsWith("navstripe:")) { /* RefreshCards 统一刷 */ return; }
            if (tag.StartsWith("navdot:")) { return; }
            // Label 的 BackColor 默认是"控件默认浅色"而不是透明 —— 建控件时抓的父底色在
            // ApplyTheme 之前还是系统值，所以每个字色分支都要顺手把底色重新继承一次父容器。
            if (tag.StartsWith("line:")) { c.ForeColor = KindColor(tag.Substring(5)); c.BackColor = th.Card; return; }
            if (tag.StartsWith("cardtitle")) { c.ForeColor = th.CardTitle; c.BackColor = ParentBg(c); return; }
            if (tag == "title_big") { c.ForeColor = th.Text; c.BackColor = ParentBg(c); return; }
            if (tag == "navsub") { c.ForeColor = th.NavSub; c.BackColor = ParentBg(c); return; }
            if (tag == "dim") { c.ForeColor = th.Dim; c.BackColor = ParentBg(c); return; }
            if (tag == "line") { c.BackColor = th.Line; return; }
            if (tag == "accent_sq") { c.BackColor = th.Accent; return; }   // 卡片标题前那 3px 小块
            if (tag == "rail") { c.BackColor = th.Rail; return; }
            if (tag == "card") { c.BackColor = th.Card; return; }
            if (c is Button)
            {
                Button b = (Button)c;
                b.ForeColor = th.Text;
                b.FlatAppearance.MouseOverBackColor = th.Hover;
                b.FlatAppearance.MouseDownBackColor = th.Down;
                if (tag == "capbtn") { b.BackColor = th.Rail; return; }   // 贴在 hdr(同为 rail 色)上，看不出是控件
                if (tag == "nav") b.BackColor = th.Rail;          // 选中态由 RefreshCards 覆盖
                else b.BackColor = tag == "chip" ? th.Card : th.Card;
                return;
            }
        }

        static Color ParentBg(Control c)
        {
            return c.Parent != null ? c.Parent.BackColor : SystemColors.Window;
        }

        IEnumerable<Control> AllControls()
        {
            return Walk(this);
        }

        static IEnumerable<Control> Walk(Control parent)
        {
            foreach (Control c in parent.Controls)
            {
                yield return c;
                foreach (Control d in Walk(c)) yield return d;
            }
        }

        static Theme LoadTheme()
        {
            try
            {
                string exeDir = AppDomain.CurrentDomain.BaseDirectory;
                foreach (string cand in new[] { Path.Combine(exeDir, "panel-theme.txt"), Path.Combine(Directory.GetCurrentDirectory(), "state", "panel-theme.txt") })
                    if (File.Exists(cand) && File.ReadAllText(cand).Trim() == "light") return Theme.Light();
            }
            catch { }
            return Theme.Dark();
        }

        void SaveTheme()
        {
            try
            {
                string stateDir = Path.Combine(root, "state");
                if (!Directory.Exists(stateDir)) return;   // 没配置就跑不出 state，静默跳过：主题只是外观
                File.WriteAllText(Path.Combine(stateDir, "panel-theme.txt"), th.Name);
            }
            catch { }
        }

        // ---------- 外部轮次：跑的那一轮往 state\running.json 留活体标记，面板只读不写 ----------
        // 为什么按 PID 判活，而不是"日志最近有没有新行"：实机量过，MAA 的 Infrast 那一段能连着
        // 10 分 41 秒一行都不写(2026-09-25 18:50:44 Recruit → 19:01:25 Infrast)。
        // 按日志新鲜度判会把"正在跑基建"看成"空闲"，照样放行走二次点火 —— 判据必须挂在进程活没活上。
        static readonly Regex RunPidRe = new Regex(@"""pid""\s*:\s*(\d+)");
        static readonly Regex RunLogRe = new Regex(@"""log""\s*:\s*""((?:[^""\\]|\\.)*)""");
        static readonly Regex RunStartedRe = new Regex(@"""started""\s*:\s*""([^""]*)""");
        static readonly Regex RunGamesRe = new Regex(@"""games""\s*:\s*""([^""]*)""");

        string ExtLockPath { get { return Path.Combine(root, @"state\running.json"); } }

        // 返回 true = 有别的进程正在跑这一轮。pid 已不在的残留标记按"没在跑"处理，
        // 免得一轮崩溃把面板永久锁死；那种残留由写方下一轮起跑时自己覆盖。
        bool ReadExtRun(out int pid, out string log, out string started, out string games)
        {
            pid = 0; log = ""; started = ""; games = "";
            try
            {
                string path = ExtLockPath;
                if (!File.Exists(path)) return false;
                string text = File.ReadAllText(path);
                Match mp = RunPidRe.Match(text);
                if (!mp.Success) return false;
                pid = int.Parse(mp.Groups[1].Value);
                Match ml = RunLogRe.Match(text); if (ml.Success) log = ml.Groups[1].Value.Replace("\\\\", "\\");
                Match mt = RunStartedRe.Match(text); if (mt.Success) started = mt.Groups[1].Value;
                Match mg = RunGamesRe.Match(text); if (mg.Success) games = mg.Groups[1].Value;
                // 自己起的那一轮走 busy 那条线：它已经有退出码和跟随逻辑，别当成"外部"重复报一次
                if (child != null && !child.HasExited && child.Id == pid) return false;
                try
                {
                    using (Process p = Process.GetProcessById(pid)) { return !p.HasExited; }
                }
                catch (ArgumentException) { return false; }
                // 提权那一轮的进程面板读不到细节(完整性级别不同)→ 宁可当成还在跑，也别放行二次点火
                catch { return true; }
            }
            catch { return false; }
        }

        // ---------- 子进程(和 panel.ps1 同一套：重定向到 TEMP 文件 + 定时器增量读) ----------
        void StartTask(string script, List<string> args, string title, bool tailLog)
        {
            if (busy && child != null && !child.HasExited)
            {
                AppendLog("上一个动作还在跑：等它结束，或点\"开始跑一轮\"那个按钮停掉跟随。");
                return;
            }
            if (busy) CompleteTask();
            if (!File.Exists(script)) { AppendLog("找不到要跑的脚本：" + script); return; }

            StringBuilder sb = new StringBuilder();
            sb.Append("-NoProfile -ExecutionPolicy Bypass -File \"").Append(script).Append('"');
            foreach (string a in args) sb.Append(' ').Append(a);
            AppendLog("──── " + DateTime.Now.ToString("HH:mm:ss") + "  " + title + " ────");
            AppendLog("  " + Path.GetFileName(engine) + " " + sb);

            outPath = Path.Combine(Path.GetTempPath(), "game-daily-cs-panel-stdout-" + Process.GetCurrentProcess().Id + ".txt");
            errPath = Path.Combine(Path.GetTempPath(), "game-daily-cs-panel-error-" + Process.GetCurrentProcess().Id + ".txt");
            tailPos = 0; tailPath = null;
            ProcessStartInfo psi = new ProcessStartInfo();
            psi.FileName = engine;
            psi.Arguments = sb.ToString();
            psi.WorkingDirectory = root;
            psi.UseShellExecute = false;
            psi.CreateNoWindow = true;
            psi.RedirectStandardOutput = true;
            psi.RedirectStandardError = true;
            psi.StandardOutputEncoding = Encoding.UTF8;
            psi.StandardErrorEncoding = Encoding.UTF8;
            try
            {
                child = Process.Start(psi);
                child.OutputDataReceived += delegate (object s, DataReceivedEventArgs e2)
                { if (e2.Data != null) lock (lineLock) childOut.Add(e2.Data); };
                child.ErrorDataReceived += delegate (object s, DataReceivedEventArgs e2)
                { if (e2.Data != null) lock (lineLock) childOut.Add("[stderr] " + e2.Data); };
                child.BeginOutputReadLine();
                child.BeginErrorReadLine();
                busy = true;
                if (tailLog) tailSince = DateTime.Now; else tailSince = DateTime.MinValue;
                SetBusyUi(true);
            }
            catch (Exception ex)
            {
                AppendLog("启动失败：" + ex.Message);
                child = null;
                SetBusyUi(false);
            }
        }

        readonly object lineLock = new object();
        readonly List<string> childOut = new List<string>();

        DateTime lastExtCheck = DateTime.MinValue;

        void OnPumpTick(object sender, EventArgs e)
        {
            lock (lineLock)
            {
                foreach (string l in childOut) AppendLog(l);
                childOut.Clear();
            }
            PumpTail();
            if (busy && child != null && child.HasExited) CompleteTask();
            // 外部轮次 3 秒一查：挡点火这件事要够用，但没到每拍(800ms)都去读盘和查 PID 的程度。
            // 自己那轮在跑时不查 —— 锁文件本来就是我写的，读到的 PID 就是自家子进程。
            if (!busy && (DateTime.Now - lastExtCheck).TotalSeconds >= 3)
            {
                lastExtCheck = DateTime.Now;
                RefreshExternal();
            }
            if (!busy && extBusy) PumpExtTail();
        }

        void PumpTail()
        {
            if (tailSince == DateTime.MinValue) return;
            if (tailPath == null)
            {
                try
                {
                    DirectoryInfo di = new DirectoryInfo(Path.Combine(root, "logs"));
                    if (di.Exists)
                    {
                        FileInfo best = null;
                        foreach (FileInfo f in di.GetFiles("run-*.log"))
                        {
                            if (f.LastWriteTime < tailSince) continue;
                            if (best == null || f.LastWriteTime > best.LastWriteTime) best = f;
                        }
                        if (best != null) { tailPath = best.FullName; tailPos = 0; AppendLog("  跟随日志：" + tailPath); }
                    }
                }
                catch { }
            }
            if (tailPath == null) return;
            foreach (string l in ReadNew(tailPath, ref tailPos)) AppendLog("  | " + l);
        }

        static IEnumerable<string> ReadNew(string path, ref long pos)
        {
            List<string> lines = new List<string>();
            try
            {
                using (FileStream fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
                {
                    if (fs.Length < pos) pos = 0;
                    fs.Seek(pos, SeekOrigin.Begin);
                    byte[] buf = new byte[(int)(fs.Length - pos)];
                    int got = fs.Read(buf, 0, buf.Length);
                    pos = fs.Length;
                    string txt = Encoding.UTF8.GetString(buf, 0, got);
                    foreach (string l in txt.Replace("\r\n", "\n").Split('\n'))
                        if (l.Length > 0) lines.Add(l.TrimEnd('\r'));
                }
            }
            catch { }
            return lines;
        }

        void CompleteTask()
        {
            if (child != null)
            {
                try
                {
                    lastExit = child.ExitCode;
                    AppendLog("退出码 " + lastExit + "(0=正常；「无可刷活动关卡 / 无进行中活动」也是正常结论)");
                }
                catch { AppendLog("子进程已结束(没取到退出码)。"); }
                try { child.Dispose(); } catch { }
                child = null;
            }
            tailSince = DateTime.MinValue; tailPath = null;
            // 自家这轮刚结束：先把外部标记重读一遍再定状态灯，否则"自己那轮"和"外部那轮"
            // 在锁文件被写方删掉的这一瞬间会接错位。
            RefreshExternal();
            SetBusyUi(false);
            // 卡片先重读一次再念原因：这一轮的结论是子进程退出前才写进 history.csv 的，
            // 不重读就会拿"开跑前那批行"去公告，念出来的还是上一轮为什么红。
            RefreshNow("跑完后的自动刷新");
            AnnounceDetails("这一轮跑完了");
        }

        // 状态卡的一次重读(-DumpSummary 那趟子进程 + 分页渲染)。三处收尾都要用，
        // 集中一处写错误处理：读不到时卡片保留上一次的内容，只把原因说清楚。
        bool RefreshNow(string when)
        {
            try { ParseSummary(RunDumpSummary()); }
            catch (Exception ex)
            {
                AppendLog("[WARN] " + when + "没读到新状态(" + ex.Message + ")，卡片还是上一次的内容。");
                return false;
            }
            RefreshCards();
            return true;
        }

        // 一轮收尾时主动把红/黄行的完整原因念一遍：人最不看日志的时刻恰恰是"跑完了、红了、为什么"那一刻，
        // 而卡片那一行只放得下一个 not-started。同一个状态只念一次，靠 lastAnnounced 去重。
        string lastAnnounced = null;
        void AnnounceDetails(string when)
        {
            StringBuilder key = new StringBuilder();
            for (int i = 0; i < rowsLast.Count; i++)
            {
                Row r = rowsLast[i];
                if (r.Detail.Length == 0) continue;
                key.Append(r.Game).Append(':').Append(r.Kind).Append(':').Append(r.Detail.Length).Append(';');
            }
            if (key.Length == 0) return;
            if (key.ToString() == lastAnnounced) return;
            lastAnnounced = key.ToString();
            AppendLog(when + "，有几路没给好脸色 —— 完整原因(点「上一轮」里那几行随时能再看一次)：");
            for (int i = 0; i < rowsLast.Count; i++)
            {
                Row r = rowsLast[i];
                if (r.Detail.Length == 0) continue;
                AppendLog("  " + GameName(r.Game) + "(" + r.Text + ")：");
                foreach (string l in WrapUnits(r.Detail, 96)) AppendLog("      " + l);
            }
        }

        void SetBusyUi(bool b)
        {
            busy = b;
            UpdateRunSurface();
        }

        // 状态灯三态：自己的子进程 > 外部那一轮 > 空闲。顺序不能反 ——
        // 自己那轮的退出码还没读到手时，宁可显示"正在跑"，也别被外部标记抢过去把退出码吞了。
        void UpdateRunSurface()
        {
            if (busy) { runBtn.Text = "正在跑…(点这个按钮停跟随)"; SetChip("正在跑", true); return; }
            if (extBusy) { runBtn.Text = "外部有一轮在跑(再点=停掉它)"; SetChip("外部在跑", true); return; }
            // 空闲态也要看有没有在等二次确认：外部标记每 3 秒查一次，一刷就把
            // "再点一次就真的开跑"冲回默认文案的话，人根本看不见确认提示、只会以为按钮坏了。
            if (confirmAt == DateTime.MinValue || (DateTime.Now - confirmAt).TotalSeconds >= 12) runBtn.Text = idleRunText;
            SetChip("空闲", false);
        }

        // 单游戏仓直接藏掉它(只有一路时它和"开始跑一轮"是同一件事，多一个按钮只会让人猜差别在哪)。
        void UpdateOnlyBtn()
        {
            if (btnOnly == null) return;
            bool many = flagsGames && games.Count > 1;
            btnOnly.Visible = many;
            if (!many || busy) return;
            if (page == "all" || page.Length == 0) { btnOnly.Text = "只跑这一路(先选一路)"; return; }
            if (onlyAt != DateTime.MinValue && (DateTime.Now - onlyAt).TotalSeconds < 12) return;   // 等确认时别冲掉"再点一次="
            btnOnly.Text = "只跑" + GameName(page);
        }

        // 每拍打一次"有没有别处在跑"。它刻意不走 DumpSummary 那条线：读一个 json + 查一个 PID，
        // 不起 pwsh 子进程，所以能按 pump 的节奏跑，撞车提示才不会慢半拍。
        bool RefreshExternal()
        {
            int pid; string log, started, g;
            bool now = ReadExtRun(out pid, out log, out started, out g);
            List<string> bits = new List<string>();
            if (now)
            {
                if (pid > 0) bits.Add("PID " + pid);
                if (g.Length > 0) bits.Add(g);
                if (started.Length > 0) bits.Add("起于 " + started);
            }
            string note = now ? string.Join(" · ", bits.ToArray()) : "";
            bool changed = (now != extBusy) || (note != extNote);
            extBusy = now;
            extNote = note;
            string newLog = (now && log.Length > 0 && File.Exists(log)) ? log : null;
            if (newLog != extLog) { extLog = newLog; extPos = 0; }
            if (changed)
            {
                UpdateRunSurface();
                if (now) AppendLog("检测到外部有一轮在跑(" + note + ") —— 面板不再接受点火，避免两轮抢同一个助手/端游。");
                else AppendLog("外部那一轮已经不在跑了。");
            }
            return changed;
        }

        void PumpExtTail()
        {
            if (!extBusy || extLog == null) return;
            foreach (string l in ReadNew(extLog, ref extPos)) AppendLog("  | " + l);
        }

        void OnBeatTick(object sender, EventArgs e)
        {
            RefreshExternal();
            // 自己那轮还在跑时不去抢：DumpSummary 会读同一批状态文件，而且此刻刷出来的卡片必然是过时的
            if (busy) return;
            RefreshNow("自动刷新");
        }

        // 停掉外部那一轮：只有走计划任务那条路才停得干净 —— 面板是普通完整性，
        // 直接 Kill 提权进程会被拒；任务自己的 Stop-ScheduledTask 才会连它派生的进程树一起收。
        void StopExternal()
        {
            string script = Path.Combine(root, "run-via-task.ps1");
            if (!File.Exists(script)) { AppendLog("没有 run-via-task.ps1，面板停不了外部那一轮 —— 去任务计划程序里结束它。"); return; }
            StartTask(script, new List<string> { "-Stop" }, "停掉正在跑的这一轮(走计划任务)", false);
        }

        string chipText = "空闲";
        void SetChip(string text, bool isBusy)
        {
            chipText = text;
            runChip.Text = text;
            runChip.BackColor = isBusy ? th.BusyBack : th.ChipBack;
            runChip.ForeColor = isBusy ? th.BusyFore : th.ChipFore;
        }

        void StopChild()
        {
            if (child != null)
            {
                try { if (!child.HasExited) child.Kill(); } catch { }
                try { child.Dispose(); } catch { }
                child = null;
            }
            tailSince = DateTime.MinValue; tailPath = null;
            SetBusyUi(false);
        }

        // ---------- 按钮动作(全部转调现成脚本，和 panel.ps1 逐条对齐) ----------
        void OnRunClick(object sender, EventArgs e)
        {
            if (busy && child != null && !child.HasExited)
            {
                StopChild();
                AppendLog("面板不再跟随这一轮。若这一轮是走计划任务起的，本体不受影响，到点还会自己跑完并收尾。");
                return;
            }
            if (busy) CompleteTask();
            DateTime now = DateTime.Now;
            // 真要点火前再实时查一次：3 秒一查的缓存挡不住"别人刚点下去"这种毫秒级撞车。
            // 这里不新增"能不能跑"的判定 —— 有没有在跑只看那一轮自己留的活体标记。
            RefreshExternal();
            if (extBusy)
            {
                if ((now - confirmAt).TotalSeconds < 12)
                {
                    confirmAt = DateTime.MinValue;
                    StopExternal();
                    return;
                }
                confirmAt = now;
                runBtn.Text = "再点一次=停掉正在跑的那一轮";
                AppendLog("外部已有一轮在跑(" + extNote + ")。再点一次是去停它；想换别的跑法，先等这一轮自己结束。");
                return;
            }
            if ((now - confirmAt).TotalSeconds < 12)
            {
                confirmAt = DateTime.MinValue;
                runBtn.Text = idleRunText;
                if (flagsViaTask)
                {
                    List<string> a = new List<string>();
                    if (flagsGames && games.Count > 1) { a.Add("-Games"); a.Add("all"); }
                    StartTask(Path.Combine(root, "run-via-task.ps1"), a, "跑一轮(走计划任务，免 UAC)", true);
                }
                else
                {
                    List<string> a = new List<string> { "-Tag", "manual" };
                    if (flagsGames && games.Count > 1) { a.Insert(0, "-Games"); a.Insert(1, "all"); }
                    StartTask(EntryPath, a, "跑一轮(本机直跑" + ElevNote() + ")", true);
                }
                return;
            }
            confirmAt = now;
            runBtn.Text = "再点一次就真的开跑(12 秒内)";
            AppendLog("再点一次这个按钮就开跑。跑完会自动关掉助手、端游和模拟器；中途想停就再点它。");
        }

        string GameName(string id)
        {
            foreach (string[] g in games) if (g[0] == id) return g[1];
            return id;
        }

        // 只跑左边当前页那一路。刻意不做下拉框：跑哪一路已经由页面表达了，
        // 面板上再多一个和页面状态重复的选择器，就多一个"看着选的是 A、实际跑的是 B"的机会。
        void OnOnlyClick(object sender, EventArgs e)
        {
            if (!flagsGames || games.Count < 2) { AppendLog("这一仓只有一路，「只跑这一路」和「开始跑一轮」是同一件事。"); return; }
            if (page == "all" || page.Length == 0) { AppendLog("先在左边点进某一款游戏，再回来点「只跑这一路」。"); return; }
            if (busy && child != null && !child.HasExited) { AppendLog("面板里已经有一轮在跑了，先等它结束(或点大按钮停跟随)。"); return; }
            if (busy) CompleteTask();
            RefreshExternal();
            if (extBusy) { AppendLog("外部已有一轮在跑(" + extNote + ") —— 只跑一路也会和它抢同一个助手，先停掉那一轮。"); return; }
            string id = page;
            string shown = GameName(id);
            DateTime now = DateTime.Now;
            if ((now - onlyAt).TotalSeconds < 12)
            {
                onlyAt = DateTime.MinValue;
                btnOnly.Text = "只跑这一路";
                // 走任务时 -Games 靠 run-via-task.ps1 覆盖任务参数实现免 UAC；
                // 它要是拼不出来会自己回落直跑并在日志里说清楚(那时可能弹一次 UAC)。
                if (flagsViaTask)
                    StartTask(Path.Combine(root, "run-via-task.ps1"), new List<string> { "-Games", id }, "只跑" + shown + "(走计划任务，免 UAC)", true);
                else
                    StartTask(EntryPath, new List<string> { "-Games", id, "-Tag", "manual" }, "只跑" + shown + "(本机直跑" + ElevNote() + ")", true);
                return;
            }
            onlyAt = now;
            btnOnly.Text = "再点一次=只跑" + shown;
            AppendLog("只跑" + shown + "。再点一次这个按钮开跑(12 秒内)；要跑全部就切回「总览」用左边那个大按钮。");
        }

        void OnAckClick(object sender, EventArgs e)
        {
            if (ackFlagName.Length == 0) { AppendLog("这个仓库的入口脚本没有\"活动确认\"参数，这个按钮不做事。"); return; }
            DateTime now = DateTime.Now;
            if ((now - ackAt).TotalSeconds < 12)
            {
                ackAt = DateTime.MinValue;
                ackBtn.Text = "我打过活动了";
                StartTask(EntryPath, new List<string> { "-" + ackFlagName, "-DryRun" }, "标记本轮活动已打过(不再刷活动图)", false);
                return;
            }
            ackAt = now;
            ackBtn.Text = "再点一次=确认我已打过活动";
            AppendLog("作用：版本更新后你先手动打活动，然后点它，之后到点就只刷日常、不碰活动图。再点一次确认。");
        }

        void OnRefreshClick(object sender, EventArgs e)
        {
            // 手点刷新顺带把外部轮次也认一次：自动刷新被 --no-beat 关掉时，这条路就是唯一的感知时机
            RefreshExternal();
            if (RefreshNow("手动刷新")) AppendLog("已刷新上面的状态。");
        }

        // ---------- 日志区(逐行按 Write-GdLog 的级别上色；判据和 panel.ps1 的同一条正则) ----------
        readonly List<string[]> logLines = new List<string[]>();   // [kind, text]，换主题时整框重画

        void AppendLog(string text)
        {
            if (string.IsNullOrEmpty(text)) return;
            logLines.Add(new[] { LogKind(text), text });
            if (logLines.Count > 2000) logLines.RemoveRange(0, logLines.Count - 1500);
            AppendColored(text, LogKind(text));
        }

        void AppendColored(string text, string kind)
        {
            Match m = Regex.Match(text, @"^(?:  \| )?(?:(?:\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]|\[\d{2}:\d{2}:\d{2}\])+)(?:\[[A-Z]+\])? (?<rest>.*)$");
            Color stamp = th.LogStamp;
            Color body = LogColor(kind);
            SuspendLayoutLog();
            if (m.Success)
            {
                AddText(text.Substring(0, text.Length - m.Groups["rest"].Length), stamp);
                AddText(m.Groups["rest"].Value, body);
            }
            else AddText(text, body);
            AddText("\n", body);
            ResumeLayoutLog();
        }

        void RepaintLog()
        {
            logBox.Clear();
            foreach (string[] e in logLines) AddRaw(e[1], e[0]);
            ScrollEnd();
        }

        void AddRaw(string text, string kind)
        {
            Match m = Regex.Match(text, @"^(?:  \| )?(?:(?:\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]|\[\d{2}:\d{2}:\d{2}\])+)(?:\[[A-Z]+\])? (?<rest>.*)$");
            SuspendLayoutLog();
            if (m.Success)
            {
                AddText(text.Substring(0, text.Length - m.Groups["rest"].Length), th.LogStamp);
                AddText(m.Groups["rest"].Value, LogColor(kind));
            }
            else AddText(text, LogColor(kind));
            AddText("\n", LogColor(kind));
            ResumeLayoutLog();
        }

        void SuspendLayoutLog() { logBox.SuspendLayout(); }
        void ResumeLayoutLog() { logBox.ResumeLayout(); ScrollEnd(); }
        void ScrollEnd() { try { logBox.SelectionStart = logBox.TextLength; logBox.ScrollToCaret(); } catch { } }

        void AddText(string text, Color c)
        {
            if (text.Length == 0) return;
            logBox.Select(logBox.TextLength, 0);
            logBox.SelectionColor = c;
            logBox.AppendText(text);
        }

        Color LogColor(string kind)
        {
            switch (kind)
            {
                case "ok": return th.LogOk;
                case "warn": return th.LogWarn;
                case "err": return th.LogErr;
                case "step": return th.LogStep;
                case "dim": return th.LogDim;
                default: return th.LogInfo;
            }
        }

        static string LogKind(string text)
        {
            if (text.StartsWith("[stderr")) return "err";
            if (Regex.IsMatch(text, @"^\s*[=─]{4,}")) return "dim";
            if (text.StartsWith("────")) return "dim";
            if (text.Contains("[ERROR]")) return "err";
            if (text.Contains("[WARN]")) return "warn";
            if (text.Contains("[OK]")) return "ok";
            if (text.Contains("[STEP]")) return "step";
            if (Regex.IsMatch(text, @"^退出码 0(\(|$)")) return "ok";
            if (Regex.IsMatch(text, @"^退出码 [1-9]")) return "err";
            return "info";
        }

        // ---------- 小工具 ----------
        Label MakeLabel(Control parent, string text, int x, int y, int w, int h, ContentAlignment? align, Font font, bool autoSize)
        {
            Label l = new Label();
            l.Text = text;
            l.Location = new Point(x, y);
            if (autoSize || w <= 0 || h <= 0) l.AutoSize = true;
            else
            {
                l.AutoSize = false;
                l.AutoEllipsis = true;
                l.Size = new Size(w, h);
                if (align.HasValue) l.TextAlign = align.Value;
            }
            if (font != null) l.Font = font;
            // Label 默认 BackColor=Transparent，但 WinForms 是"画父容器背景"——父底色之后被主题改掉，
            // 而 Label 的 ForeColor 还停在控件默认值上(没显式设过的就是系统深字)：黑底上直接隐形。
            // 所以建控件就显式给一个和"这个父容器在主题里的角色"相称的字色，ApplyOne 再按 Tag 精修。
            l.BackColor = parent.BackColor;
            l.ForeColor = th == null ? Color.FromKnownColor(KnownColor.ControlText) : th.Text;
            parent.Controls.Add(l);
            return l;
        }

        Button MakeButton(Control parent, string text, int x, int y, int w, int h, string kind)
        {
            Button b = new Button();
            b.Text = text;
            b.Location = new Point(x, y);
            b.Size = new Size(w, h);
            b.FlatStyle = FlatStyle.Flat;
            b.UseVisualStyleBackColor = false;
            b.Cursor = Cursors.Hand;
            b.Tag = kind;                                  // ApplyOne 靠这个认"这是什么按钮"
            b.FlatAppearance.BorderSize = 0;
            b.ForeColor = kind == "primary" ? th.PrimaryFore : th.Text;   // 建控件时就给主题字色(ApplyOne 之后还会精修)
            if (kind == "primary") b.Font = new Font("Microsoft YaHei UI", 10.5f, FontStyle.Bold);
            else if (kind == "chip") b.Font = SmallFont();
            parent.Controls.Add(b);
            // 圆角只给不横向拉伸的按钮(主动作/导航)：secondary 跟着动作区变宽时 Region 会过期，右侧被裁掉
            if (kind == "primary" || kind == "nav") Round(b, 8);
            return b;
        }

        static void themeTag(Control c, string tag) { c.Tag = tag; }

        void MakeCard(Panel card, int x, int y, int w, int h, string title, AnchorStyles anchor)
        {
            card.Location = new Point(x, y);
            card.Size = new Size(w, h);
            card.Anchor = anchor;
            Controls.Add(card);
            themeTag(card, "card");
            Borderless(card);

            Panel sq = new Panel();
            sq.Location = new Point(14, 13);
            sq.Size = new Size(3, 13);
            sq.Tag = "accent_sq";
            card.Controls.Add(sq);

            Label t = MakeLabel(card, title.ToUpper(), 24, 10, w - 38, 17, null, SectionFont(), false);
            t.Tag = "cardtitle";
            t.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
        }

        void Borderless(Control c)
        {
            // 深/浅底上卡片只靠 1px 描边分层；Paint 里读当前主题，切主题 Invalidate 即可
            c.Paint += delegate (object s, PaintEventArgs pe)
            {
                using (Pen pen = new Pen(th.Line))
                    pe.Graphics.DrawRectangle(pen, 0, 0, ((Control)s).Width - 1, ((Control)s).Height - 1);
            };
        }

        static void Round(Control c, int radius)
        {
            int w = c.Width, h = c.Height, d = radius * 2;
            if (w <= d || h <= d) { c.Region = null; return; }
            using (System.Drawing.Drawing2D.GraphicsPath gp = new System.Drawing.Drawing2D.GraphicsPath())
            {
                gp.AddArc(0, 0, d, d, 180, 90);
                gp.AddArc(w - d, 0, d, d, 270, 90);
                gp.AddArc(w - d, h - d, d, d, 0, 90);
                gp.AddArc(0, h - d, d, d, 90, 90);
                gp.CloseFigure();
                Region old = c.Region;
                c.Region = new Region(gp);
                if (old != null) { try { old.Dispose(); } catch { } }
            }
        }

        Font SmallFont() { return new Font("Microsoft YaHei UI", 8.5f); }
        Font SectionFont() { return new Font("Microsoft YaHei UI", 8.5f, FontStyle.Bold); }

        static string Pad(string text, int width)
        {
            int cols = 0;
            foreach (char ch in text) cols += ch >= 0x2E80 ? 2 : 1;
            if (cols >= width) return text + " ";
            return text + new string(' ', width - cols);
        }

        Image embeddedImg;
        Image LoadEmbeddedImage()
        {
            if (embeddedImg != null) return embeddedImg;
            // 优先用旁边那张 assets\icon.png(build.ps1 的产物目录会带上)；没有就用内嵌资源
            try
            {
                string exeDir = AppDomain.CurrentDomain.BaseDirectory;
                foreach (string cand in new[] {
                    Path.Combine(exeDir, @"tools\assets\icon.png"),
                    Path.Combine(root, @"tools\assets\icon.png") })
                    if (File.Exists(cand)) { embeddedImg = Image.FromFile(cand); return embeddedImg; }
                string beside = Path.Combine(exeDir, "icon.png");
                if (File.Exists(beside)) { embeddedImg = Image.FromFile(beside); return embeddedImg; }
                foreach (string n in GetType().Assembly.GetManifestResourceNames())
                    if (n.EndsWith("icon.png"))
                        using (Stream st = GetType().Assembly.GetManifestResourceStream(n))
                        { if (st != null) { embeddedImg = Image.FromStream(st); return embeddedImg; } }
            }
            catch { }
            return null;
        }

        Icon LoadEmbeddedIcon()
        {
            Image img = LoadEmbeddedImage();
            if (img == null) return null;
            Bitmap bmp = new Bitmap(img, new Size(32, 32));
            IntPtr h = bmp.GetHicon();
            Icon ic = Icon.FromHandle(h);
            // FromHandle 不复制句柄，但 Form.Icon 内部会再拷一份用于窗口消息；这里在设置完成后销毁原始句柄。
            // 保险起见留到 OnFormClosed 再 DestroyIcon(见下)，避免绘制时机不同导致图标变白块。
            iconHandleToFree = h;
            return ic;
        }

        IntPtr iconHandleToFree = IntPtr.Zero;
        protected override void OnFormClosed(FormClosedEventArgs e)
        {
            pump.Stop();
            StopChild();
            try { foreach (string f in new[] { outPath, errPath }) if (f != null && File.Exists(f)) File.Delete(f); } catch { }
            if (iconHandleToFree != IntPtr.Zero) { try { DestroyIcon(iconHandleToFree); } catch { } }
            base.OnFormClosed(e);
        }

        // ---------- 自检(--selftest / --headless / --dump) ----------
        public void DumpTree()
        {
            Console.WriteLine("---- 控件树(调试用) ----");
            Console.WriteLine("---- 解析出的行 ----");
            DumpRows("LAST", rowsLast);
            DumpRows("GATE", rowsGates);
            bool? expFlag = ReadExpFlag();
            Console.WriteLine("EXP implemented=" + ExpImplemented() + " flag=" + (expFlag.HasValue ? expFlag.Value.ToString() : "-") +
                              " 按钮=" + expBtn.Text + "/" + expBtn.AccessibleName);
            List<Row> tt;
            if (timesByPage.TryGetValue("all", out tt)) DumpRows("TIME", tt);
            Console.WriteLine("---- 开机日志区最后几行(真数据，验提示/播报有没有真说出来) ----");
            for (int i = Math.Max(0, logLines.Count - 6); i < logLines.Count; i++)
                Console.WriteLine("LOG[" + logLines[i][0] + "] " + logLines[i][1]);
            DumpWalk(this, "");
            Close();
        }

        static void DumpRows(string what, List<Row> rows)
        {
            foreach (Row r in rows)
                Console.WriteLine(what + " kind=[" + r.Kind + "] game=[" + r.Game + "] text=[" + r.Text + "]" +
                                  (r.Detail.Length > 0 ? " detail=[" + (r.Detail.Length > 60 ? r.Detail.Substring(0, 60) + "…" : r.Detail) + "](" + r.Detail.Length + "字)" : ""));
        }

        void DumpWalk(Control parent, string indent)
        {
            foreach (Control c in parent.Controls)
            {
                string t = c.Text == null ? "" : c.Text;
                if (t.Length > 24) t = t.Substring(0, 24);
                Console.WriteLine(indent + c.GetType().Name + " \"" + t + "\" bounds=" + c.Bounds +
                    " tag=" + (c.Tag == null ? "-" : c.Tag.ToString()) +
                    " fore=" + (c is Label || c is Button ? ((Control)c).ForeColor.ToString() : "-") +
                    " back=" + c.BackColor.ToString() + " vis=" + c.Visible);
                DumpWalk(c, indent + "  ");
            }
        }

        public int SelfTest()
        {
            int bad = 0;
            Console.WriteLine("---- panel-cs 自检(不显示窗口) ----");
            Console.WriteLine("root=" + root + "  引擎=" + engine);
            Console.WriteLine("这一仓=" + string.Join(" + ", GameIds()) + "  入口=" + entryName + "  FLAGS: Ack=" + ackFlagName + " Shop=" + shopFlagName + " Games=" + flagsGames + " ViaTask=" + flagsViaTask);
            Console.WriteLine("导航项: " + string.Join(" / ", new List<string>(navBtns.Keys).ToArray()));
            Console.WriteLine("帮助行 " + helpRows.Count + " 条");
            Location = new Point(-16000, -16000);
            ShowInTaskbar = false;
            Show();
            Hide();
            foreach (string id in new List<string>(navBtns.Keys))
            {
                SelectPage(id);
                foreach (Panel card in new[] { cardTimes, cardLast, cardGates, cardEnv, cardExp })
                {
                    int lines = 0;
                    foreach (Control c in card.Controls)
                    {
                        if (c.Tag == null || !c.Tag.ToString().StartsWith("line:")) continue;
                        lines++;
                        Rectangle a = c.Bounds;
                        Rectangle cl = new Rectangle(Point.Empty, card.ClientSize);
                        if (a.X < 0 || a.Y < 0 || a.Right > cl.Right || a.Bottom > cl.Bottom)
                        { Console.WriteLine("  布局问题 " + PageTitle() + " 越出容器 " + cardTitleOf(card) + ": " + a); bad++; }
                    }
                    if (lines == 0) { Console.WriteLine("  布局问题 " + PageTitle() + " 卡片一行都没渲染: " + cardTitleOf(card)); bad++; }
                    List<Control> kids = new List<Control>();
                    foreach (Control c in card.Controls) kids.Add(c);
                    for (int i = 0; i < kids.Count; i++)
                        for (int j = i + 1; j < kids.Count; j++)
                        {
                            Rectangle a = kids[i].Bounds, b = kids[j].Bounds;
                            if (a.X < b.Right && b.X < a.Right && a.Y < b.Bottom && b.Y < a.Bottom)
                            { Console.WriteLine("  布局问题 " + PageTitle() + " 互相压住(" + cardTitleOf(card) + "): " + a + " × " + b); bad++; }
                        }
                }
                if (logLines.Count == 0) { Console.WriteLine("  布局问题 日志区是空的"); bad++; }
            }
            SelectPage("all");
            // 主题切换链：切一次、再切回来，断言两套色都真的落到了控件上
            Color before = BackColor;
            OnThemeClick(null, EventArgs.Empty);
            if (BackColor == before) { Console.WriteLine("  主题没生效"); bad++; }
            OnThemeClick(null, EventArgs.Empty);
            Console.WriteLine("布局自检: " + (bad == 0 ? "OK，每一页都不重叠、都在各自容器内、各张卡片都有内容" : "有问题(见上)"));
            Close();
            return bad == 0 ? 0 : 1;
        }

        string cardTitleOf(Panel card)
        {
            foreach (Control c in card.Controls)
                if (c.Tag != null && c.Tag.ToString() == "cardtitle") return c.Text;
            return "?";
        }

        // --exptest：实验功能读写回环自测(会真动 settings.json，只在测试树上跑，测完自己翻回去)。
        public int ExpTest()
        {
            Console.WriteLine("---- panel-cs 实验功能读写自测 ----");
            if (!ExpImplemented()) { Console.WriteLine("这一仓没有实现库(lib\\vision-rescue.ps1 不在)，卡片就该是空的 —— 没得测。"); return 0; }
            string path = SettingsPath;
            if (!File.Exists(path)) { Console.WriteLine("没有 " + path + " —— 把 config\\settings.example.json 复制成 settings.json 再跑。"); return 1; }
            bool? before = ReadExpFlag();
            if (!before.HasValue) { Console.WriteLine("settings.json 里没有 arkVisionRescue 键，写路径测不了。"); return 1; }
            bool target = !before.Value;
            string m1 = WriteExpFlag(target);
            bool? mid = ReadExpFlag();
            string m2 = WriteExpFlag(before.Value);
            bool? after = ReadExpFlag();
            int baks = 0;
            try { baks = Directory.GetFiles(Path.Combine(root, "backups"), "settings.json.*.bak").Length; } catch { }
            Console.WriteLine("初始 " + before.Value + " → 写 " + target + " 读到 " + mid + "；写回 " + before.Value + " 读到 " + after + "；备份份数 " + baks);
            Console.WriteLine("  " + m1);
            Console.WriteLine("  " + m2);
            bool ok = mid.HasValue && mid.Value == target && after.HasValue && after.Value == before.Value && baks > 0;
            Console.WriteLine("结论: " + (ok ? "PASS(读写一致，settings.json 已还原)" : "FAIL —— 检查 config\\settings.json 的 arkVisionRescue 是否停在 " + after));
            return ok ? 0 : 1;
        }

        // --whytest：「上一轮为什么」这条读数链路的离线自测。
        // 喂的是临时目录里的合成 CSV 和合成 -DumpSummary 文本：不碰 state\history.csv、不起 pwsh 子进程、不弹窗。
        // 要验的四件事：CSV 解析(逗号/双引号/'|' 都不能当分隔符)、按表头取列而不是按位置、
        // detail 只挂到红/黄行上、以及同一路后来的空 detail 会把旧原因覆盖掉(留着更早期原因比没有更误导)。
        public int WhyTest()
        {
            Console.WriteLine("---- panel-cs「上一轮原因」自测(合成 CSV，不碰真历史) ----");
            int fail = 0;
            string tmp = Path.Combine(Path.GetTempPath(), "gd-whytest-" + Process.GetCurrentProcess().Id + ".csv");
            string keepOverride = historyPathOverride;
            // 判定层真写出来的那种句子：中英文混排 + 逗号 + 方括号 + 箭头 + 竖线
            string nasty = "M9A 有 7 个任务报『任务失败』(第一个任务[启动游戏]就失败)；资源/程序落后：当前 v4.7.1 → 最新 v4.10.0 | 整包更新，命令行没有更新入口";
            try
            {
                StringBuilder csv = new StringBuilder();
                // 表头故意打乱列序(game 放到最后一列)：按位置读的实现会在这里就崩
                csv.Append("\"time\",\"status\",\"detail\",\"game\"").Append("\r\n");
                csv.Append("\"2026-09-25 19:30:00\",\"ok\",\"完成链路：Award, Fight, Infrast\",\"ark\"").Append("\r\n");
                csv.Append("\"2026-09-25 19:36:15\",\"blocked\",\"").Append(nasty.Replace("\"", "\"\"")).Append("\",\"r1999\"").Append("\r\n");
                csv.Append("\"2026-09-25 20:05:00\",\"blocked\",\"\",\"ark\"").Append("\r\n");
                File.WriteAllText(tmp, csv.ToString(), new UTF8Encoding(false));
                historyPathOverride = tmp;

                List<string[]> recs = ParseCsv(File.ReadAllText(tmp));
                Write("表头 + 3 条记录都解析出来(末尾无换行也不丢一行)", recs.Count == 4);
                Write("打乱列序后按表头取到 game 列", recs[2][3] == "r1999");
                Write("CSV 里的 \"\" 还原成一个引号，逗号/'|' 没被当分隔符", ParseCsv("a,b\r\n\"x,\"\"y\",z|w\r\n")[1][0] == "x,\"y");
                if (recs.Count != 4 || recs[2][3] != "r1999") fail++;

                ParseSummary(SyntheticDump());
                Row ark = FindLast("ark"), m9 = FindLast("r1999");
                Write("红行(r1999)挂上了完整原因", m9 != null && m9.Detail == nasty);
                Write("同一路后来的空 detail 覆盖掉旧原因，不挂(ark)", ark != null && ark.Detail.Length == 0);
                Write("没 detail 的结论行本身不受影响", ark != null && ark.Text.StartsWith("20:05"));
                if (m9 == null || m9.Detail != nasty) fail++;
                if (ark == null || ark.Detail.Length != 0) fail++;

                int before = logLines.Count;
                ShowRowDetail(m9);
                StringBuilder joined = new StringBuilder();
                for (int i = before + 1; i < logLines.Count; i++) joined.Append(logLines[i][1]);
                Write("点这一行 → 完整原因进了日志区，折行后一个字没丢",
                      logLines.Count > before + 1 && NoSpace(joined.ToString()) == NoSpace(nasty));
                if (logLines.Count <= before + 1 || NoSpace(joined.ToString()) != NoSpace(nasty)) fail++;

                List<string> wr = WrapUnits(nasty, 40);
                // 断点处的空格是"本来就该被吃掉"的(在词边界换行)，所以比的是"除空格以外一个字不丢"
                bool joined2 = NoSpace(string.Join("", wr.ToArray())) == NoSpace(nasty);
                bool fits = true;
                foreach (string l in wr) if (Units(l) > 40) fits = false;
                Write("折行不丢字、每行不超宽", joined2 && fits && wr.Count > 1);
                if (!joined2 || !fits || wr.Count <= 1) fail++;

                // 真出过事的那句：断点不能落在 "-" 后面(把 -a 拆成 "-" + "a 实测无效")，也不能留个空格尾巴。
                // 反过来，断在"短参数␣-a"的空格处是可以的，所以这里只禁真正的坏断点。
                string tricky = "命令行 --autostart 没生效(短参数 -a 实测无效)，或资源/程序有新版本要更新；日志：log-20260925.log(游戏进程出现过)";
                List<string> wr2 = WrapUnits(tricky, 40);
                bool cleanBreaks = true;
                foreach (string l in wr2)
                {
                    if (Units(l) > 40) cleanBreaks = false;
                    if (l.EndsWith("-") || l.EndsWith(" ") || l.EndsWith("　")) cleanBreaks = false;
                }
                Write("断点不会把 -a 这种词拆两半(回看到最近的标点/空格再断)",
                      cleanBreaks && NoSpace(string.Join("", wr2.ToArray())) == NoSpace(tricky) && wr2.Count > 1);
                if (!cleanBreaks) fail++;

                // 上面那步是直接调函数，测不到"渲染那一行时到底有没有把点击挂上去"。
                // Label 没有 PerformClick，只能反射拿 Control 的 Click 挂接表再 DynamicInvoke ——
                // 丑，但这条链路一旦退化成"能看见、点不动"，只有这种测法能发现。
                pump.Stop(); beat.Stop();
                Location = new Point(-16000, -16000);
                ShowInTaskbar = false;
                Show();
                SelectPage("all");
                Label rowLbl = FindLabel("19:36");
                int beforeClick = logLines.Count;
                bool invoked = false;
                if (rowLbl != null) invoked = RaiseClick(rowLbl);
                bool grew = invoked && logLines.Count > beforeClick &&
                            NoSpace(JoinLog(beforeClick)).Contains(NoSpace(nasty));
                Write("卡片上那一行确实挂了点击(反射点火后日志区打出全文)", grew);
                if (!grew) fail++;
                Label arkLbl = FindLabel("20:05");
                bool arkPlain = arkLbl != null && !HasClick(arkLbl);
                Write("没原因的结论行不挂点击(ark 那行点不动，也不是点了没反应)", arkPlain);
                if (!arkPlain) fail++;
                Hide();

                historyPathOverride = Path.Combine(Path.GetTempPath(), "gd-whytest-不存在-" + Process.GetCurrentProcess().Id + ".csv");
                LoadLastDetails();
                Write("历史文件不存在时静默(不抛，详情为空)", detailByGame.Count == 0);
                if (detailByGame.Count != 0) fail++;

                string old = Path.Combine(Path.GetTempPath(), "gd-whytest-old-" + Process.GetCurrentProcess().Id + ".csv");
                File.WriteAllText(old, "\"time\",\"game\",\"status\"\r\n\"2026-09-25 19:30:00\",\"ark\",\"ok\"\r\n", new UTF8Encoding(false));
                historyPathOverride = old;
                LoadLastDetails();
                Write("老格式(没有 detail 列)按表头判缺列，不按位置硬读", detailByGame.Count == 0);
                if (detailByGame.Count != 0) fail++;
                try { File.Delete(old); } catch { }
            }
            catch (Exception ex) { Console.WriteLine("  FAIL 自测中途抛了：" + ex.Message); fail++; }
            finally
            {
                historyPathOverride = keepOverride;
                try { File.Delete(tmp); } catch { }
            }

            // 真历史只读一眼，作信息不作断言：新克隆的仓里 history.csv 可能压根没有内容
            try
            {
                LoadLastDetails();
                int with = 0;
                foreach (KeyValuePair<string, string> kv in detailByGame) if (kv.Value.Length > 0) with++;
                Console.WriteLine("  INFO 真历史 " + HistoryPath + " 里有 " + detailByGame.Count + " 路、" + with + " 路带原因");
            }
            catch (Exception ex) { Console.WriteLine("  INFO 真历史读不了：" + ex.Message); }

            Console.WriteLine("结论: " + (fail == 0 ? "PASS(原因链路只转达判定层原文，列序/空覆盖/缺列都按表头判)" : "FAIL(" + fail + " 条不对)"));
            return fail == 0 ? 0 : 1;
        }

        static string SyntheticDump()
        {
            StringBuilder sb = new StringBuilder();
            sb.Append("HEADER|ark r1999|daily-clear.ps1|pwsh.exe|pwsh|" + "root").Append('\n');
            sb.Append("GAME|ark|方舟").Append('\n');
            sb.Append("GAME|r1999|1999").Append('\n');
            sb.Append("LAST|err|ark|20:05 ark → blocked -").Append('\n');
            sb.Append("LAST|err|r1999|19:36 r1999 blocked→blocked 662s").Append('\n');
            return sb.ToString();
        }

        Row FindLast(string game)
        {
            foreach (Row r in rowsLast) if (r.Game == game) return r;
            return null;
        }

        static int Units(string s)
        {
            int n = 0;
            foreach (char c in s) n += (c > 0x1FFF) ? 2 : 1;
            return n;
        }

        // 折行本身可能正好落在空格上(行首空格会被 Trim 掉)，所以比对"丢没丢字"时要先把空格摘干净
        static string NoSpace(string s)
        {
            StringBuilder sb = new StringBuilder(s.Length);
            foreach (char c in s) if (c != ' ' && c != '\t' && c != '\r' && c != '\n') sb.Append(c);
            return sb.ToString();
        }

        // ---- 只在 --whytest 里用到的反射点火：Label 没有 PerformClick(那是 ButtonBase 才有的)，
        // 而控件树里"这一行到底挂没挂 Click"是这次功能真正交付的东西，只能从 Control 内部的
        // 事件表(EventClick 键)把挂接取出来自己点一次。取不到就返回 false，让自测红。 ----
        static Delegate ClickHandlerOf(Control c)
        {
            try
            {
                FieldInfo key = typeof(Control).GetField("EventClick", BindingFlags.Static | BindingFlags.NonPublic);
                PropertyInfo events = typeof(Control).GetProperty("Events", BindingFlags.Instance | BindingFlags.NonPublic);
                if (key == null || events == null) return null;
                object list = events.GetValue(c, null);
                if (list == null) return null;
                PropertyInfo idx = list.GetType().GetProperty("Item");
                if (idx == null) return null;
                return idx.GetValue(list, new object[] { key.GetValue(null) }) as Delegate;
            }
            catch { return null; }
        }

        static bool HasClick(Control c) { return ClickHandlerOf(c) != null; }

        static bool RaiseClick(Control c)
        {
            Delegate d = ClickHandlerOf(c);
            if (d == null) return false;
            try { d.DynamicInvoke(c, EventArgs.Empty); return true; } catch { return false; }
        }

        Label FindLabel(string textStart)
        {
            foreach (Control c in cardLast.Controls)
            {
                Label l = c as Label;
                if (l != null && l.Text.StartsWith(textStart)) return l;
            }
            return null;
        }

        string JoinLog(int from)
        {
            StringBuilder sb = new StringBuilder();
            for (int i = from; i < logLines.Count; i++) sb.Append(logLines[i][1]);
            return sb.ToString();
        }

        // --exttest：外部轮次标记的读写回环自测。不碰游戏、不起助手，只验四件事：
        // 没标记判成空闲、活 PID 判成在跑、已退出的 PID 判成残留(否则一轮崩溃会把面板永久锁死)、
        // 以及日志路径里 JSON 的双重转义能还原成 File.Exists 认得的路径。
        public int ExtTest()
        {
            Console.WriteLine("---- panel-cs 外部轮次标记自测 ----");
            string path = ExtLockPath;
            // 真有一轮在跑时绝不碰这个文件：覆盖了就把跑着那轮的活体标记抹了，撞车保护当场失效。
            int livePid; string x1, x2, x3;
            if (ReadExtRun(out livePid, out x1, out x2, out x3))
            {
                Console.WriteLine("现在有一轮正在跑(PID " + livePid + ")，自测会覆盖它的活体标记 —— 等它结束再跑 --exttest。");
                return 2;
            }
            string backup = null;
            try { if (File.Exists(path)) backup = File.ReadAllText(path); } catch { }

            int mine = Process.GetCurrentProcess().Id;
            // 转义这一条要拿真实存在的文件来验：还原对了 File.Exists 才为 true，
            // 用假路径测不出"还原成了另一个能碰巧存在的怪路径"这种错。
            string realFile = Path.Combine(root, @"state\panel-theme.txt");
            int fail = 0;

            Write("基线：没有活标记", !ReadExtRun(out livePid, out x1, out x2, out x3));
            try
            {
                WriteLock(path, mine, "2026-09-25 19:11:35", "ark + r1999", realFile);
                bool alive = ReadExtRun(out livePid, out x1, out x2, out x3);
                Write("活 PID(" + mine + ")判成在跑", alive && livePid == mine);
                Write("games 读回 ark + r1999", x3 == "ark + r1999");
                Write("started 读回 2026-09-25 19:11:35", x2 == "2026-09-25 19:11:35");
                Write("日志路径反斜杠转义已还原(读回的就是那个真文件)", x1 == realFile && File.Exists(x1));
                if (!alive || x1 != realFile) fail++;

                int dead = DeadPid();
                if (dead > 0)
                {
                    WriteLock(path, dead, "2026-09-25 19:11:35", "ark", realFile);
                    Write("已退出的 PID(" + dead + ")判成残留=没在跑", !ReadExtRun(out livePid, out x1, out x2, out x3));
                }
                else Console.WriteLine("  SKIP 拿不到一个已退出的 PID，残留这条没测到");
            }
            catch (Exception ex) { Console.WriteLine("  FAIL 自测中途抛了：" + ex.Message); fail++; }

            try
            {
                if (backup != null) File.WriteAllText(path, backup, new UTF8Encoding(false));
                else File.Delete(path);
            }
            catch (Exception ex) { Console.WriteLine("  WARN 没能还原 " + path + "：" + ex.Message); }
            Write("跑完已还原(不留标记)", !File.Exists(path) || backup != null);
            if (fail > 0) { Console.WriteLine("结论: FAIL(" + fail + " 条不对)"); return 1; }
            Console.WriteLine("结论: PASS(判活/判残留/转义都对，面板不会把崩溃残留当成在跑)");
            return 0;
        }

        static void Write(string what, bool ok) { Console.WriteLine((ok ? "  PASS " : "  FAIL ") + what); }

        static void WriteLock(string path, int pid, string started, string games, string log)
        {
            string dir = Path.GetDirectoryName(path);
            if (dir != null && !Directory.Exists(dir)) Directory.CreateDirectory(dir);
            string esc = log.Replace("\\", "\\\\");
            string json = "{\"pid\":" + pid + ",\"started\":\"" + started +
                          "\",\"games\":\"" + games + "\",\"log\":\"" + esc + "\"}";
            File.WriteAllText(path, json, new UTF8Encoding(false));
        }

        // 起一个立刻退出的 cmd，拿它的 PID 用 —— 比瞎猜一个"应该不存在"的数字可靠，
        // 那个数字可能刚好被系统复用，测出来的结论就是假的。
        static int DeadPid()
        {
            try
            {
                ProcessStartInfo psi = new ProcessStartInfo();
                psi.FileName = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "cmd.exe");
                psi.Arguments = "/d /c exit";
                psi.UseShellExecute = false;
                psi.CreateNoWindow = true;
                using (Process p = Process.Start(psi)) { p.WaitForExit(); return p.Id; }
            }
            catch { return 0; }
        }

        public int Headless()
        {
            Show();
            Application.Run(this);
            Console.WriteLine("---- panel-cs headless(子进程输出 -> 事件队列 -> 日志区) ----");
            Console.WriteLine("结论: " + (HlResult ?? "TIMEOUT"));
            return HlResult == "PASS" ? 0 : 1;
        }

        protected override void OnLoad(EventArgs e)
        {
            base.OnLoad(e);
            if (hlDeadline == DateTime.MinValue) return;
            System.Windows.Forms.Timer check = new System.Windows.Forms.Timer();
            check.Interval = 1000;
            check.Tick += delegate
            {
                string txt = logBox.Text;
                bool ended = txt.Contains("退出码 ") || txt.Contains("没取到退出码");
                if (!busy && txt.Contains("[dry-run] 完成") && ended)
                { HlResult = "PASS"; check.Stop(); Close(); return; }
                if (DateTime.Now > hlDeadline)
                {
                    AppendLog("[headless] 超时，停止跟随。");
                    HlResult = "TIMEOUT";
                    check.Stop();
                    StopChild();
                    Close();
                }
            };
            check.Start();
        }
    }

    static class Program
    {
        [STAThread]
        static void Main(string[] argv)
        {
            bool selftest = false, headless = false, dump = false, exptest = false, exttest = false, whytest = false;
            foreach (string a in argv)
            {
                string l = a.ToLowerInvariant();
                if (l == "--selftest") selftest = true;
                if (l == "--headless") headless = true;
                if (l == "--dump") dump = true;
                if (l == "--exptest") exptest = true;
                if (l == "--exttest") exttest = true;
                if (l == "--whytest") whytest = true;
                if (l == "--no-beat") PanelForm.AutoBeatDefault = false;
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            PanelForm f = new PanelForm(headless);
            if (exptest) Environment.Exit(f.ExpTest());
            if (exttest) Environment.Exit(f.ExtTest());
            if (whytest) Environment.Exit(f.WhyTest());
            if (dump) { f.DumpTree(); return; }
            if (selftest) Environment.Exit(f.SelfTest());
            if (headless) Environment.Exit(f.Headless());
            Application.Run(f);
            Console.WriteLine("面板已关闭。定时任务不受影响，到点照样自己跑。");
        }
    }
}
