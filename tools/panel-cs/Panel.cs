// tools/panel-cs/Panel.cs —— 每日清体力的图形状态面板(C# / WinForms，.NET Framework 4，系统自带 csc.exe 直接编译)。
//
// 和 panel.ps1 的分工：判定/读数仍然只在 panel.ps1 那一套里 —— 本文件不算任何结论，
// 它做的只有三件事：
//   1. 起一个隐藏子进程跑 `panel.ps1 -DumpSummary`，读 '|' 分隔的协议行(HEADER/GAME/TIME/LAST/GATE/ENV/PROBE/FLAGS/HELP)；
//   2. 把行画成卡片/导航/日志，并把按钮动作转调现成脚本(daily-clear / status / run-via-task)；
//   3. 黑/白双主题切换(唯一的"外观判定"，存在 state\panel-theme.txt)。
// 所以 panel.ps1 里的措辞或 Kind 判据一改，这里一个字不改也跟着变 —— 不会出现两个面板各说各话。
//
// 用法：
//   panel-cs.exe                 打开面板
//   panel-cs.exe --selftest      不显示窗口：读一次协议、渲染、查布局，有红项退出码 1
//   panel-cs.exe --headless      窗口挪到屏幕外，自动起一次预演，验"子进程→日志区→忙碌复位"这条链后自关(零弹窗)
//
// 编译：powershell -File tools\panel-cs\build.ps1   (用 C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe)

using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
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
            t.Window    = C(16, 16, 16);
            t.Rail      = C(8, 8, 8);
            t.Card      = C(28, 28, 28);
            t.CardTitle = C(190, 190, 190);
            t.NavSub    = C(130, 130, 130);
            t.Line      = C(60, 60, 60);
            t.Accent    = C(255, 255, 255);   // 黑底上唯一的"强调"就是白
            t.NavOn     = C(46, 46, 46);
            t.Hover     = C(42, 42, 42);
            t.Down      = C(58, 58, 58);
            t.Text      = C(235, 235, 235);
            t.Ok        = C(96, 214, 120);
            t.Warn      = C(245, 185, 60);
            t.Err       = C(244, 110, 110);
            t.Dim       = C(140, 140, 140);
            t.LogBack   = C(10, 10, 10);
            t.LogOk     = C(126, 231, 135);
            t.LogWarn   = C(227, 179, 65);
            t.LogErr    = C(255, 123, 114);
            t.LogStep   = C(230, 230, 230);
            t.LogInfo   = C(205, 205, 205);
            t.LogDim    = C(120, 120, 120);
            t.LogStamp  = C(125, 125, 125);
            t.ChipBack  = C(42, 42, 42);
            t.ChipFore  = C(140, 140, 140);
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
        public Row(string kind, string game, string text) { Kind = kind; Game = game; Text = text; }
    }

    // ---------- 主窗体 ----------
    sealed class PanelForm : Form
    {
        [DllImport("user32.dll")] static extern bool DestroyIcon(IntPtr h);

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
        readonly Panel cardTimes = new Panel(), cardLast = new Panel(), cardGates = new Panel(), cardEnv = new Panel();
        readonly Panel act = new Panel();
        readonly Panel logCard = new Panel();
        readonly RichTextBox logBox = new RichTextBox();
        Button runBtn, btnDry, btnStatus, btnRefresh, ackBtn, btnShop, btnLog, btnHelp, btnTheme;
        readonly System.Windows.Forms.Timer pump = new System.Windows.Forms.Timer();

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

            pump.Interval = 800;
            pump.Tick += OnPumpTick;
            pump.Start();

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

            hdr.Location = new Point(X0, 12);
            hdr.Size = new Size(CW, 64);
            hdr.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            Controls.Add(hdr);
            themeTag(hdr, "rail");

            pageTitle.Text = "总览";
            pageTitle.Location = new Point(20, 12);
            pageTitle.AutoSize = true;
            pageTitle.Font = new Font("Microsoft YaHei UI", 12f, FontStyle.Bold);
            hdr.Controls.Add(pageTitle);
            themeTag(pageTitle, "title_big");

            Label hSub = MakeLabel(hdr, "结论与命令行跑同一套脚本得到的一样 · 要点鼠标请先看底部日志", 21, 42, 0, 0, null, SmallFont(), true);
            themeTag(hSub, "navsub");

            runChip.Text = "空闲";
            runChip.Location = new Point(792, 12);
            runChip.Size = new Size(106, 26);
            runChip.TextAlign = ContentAlignment.MiddleCenter;
            runChip.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            hdr.Controls.Add(runChip);
            runChip.Tag = "chip";

            stampLabel.Text = "状态刷新于 -";
            stampLabel.Location = new Point(606, 43);
            stampLabel.Size = new Size(292, 17);
            stampLabel.TextAlign = ContentAlignment.TopRight;
            stampLabel.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            hdr.Controls.Add(stampLabel);
            themeTag(stampLabel, "navsub");

            MakeCard(cardTimes, X0, 86, 300, 132, "定时任务", AnchorStyles.Top | AnchorStyles.Left);
            MakeCard(cardLast, X0 + 312, 86, 300, 132, "上一轮", AnchorStyles.Top | AnchorStyles.Left);
            MakeCard(cardEnv, X0 + 624, 86, 292, 132, "环境", AnchorStyles.Top | AnchorStyles.Right);
            MakeCard(cardGates, X0, 226, CW, 132, "两道确认门(新活动要先你手动打过 / 商店领空后不再刷活动)", AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right);

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

            ResumeLayout(true);
        }

        string[] GameIds()
        {
            List<string> ids = new List<string>();
            foreach (string[] g in games) ids.Add(g[0]);
            if (ids.Count == 0) ids.Add("?");
            return ids.ToArray();
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
            foreach (Panel p in new[] { cardTimes, cardLast, cardGates, cardEnv, act, logCard }) p.Invalidate();
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
            if (c == brandTitle) { brandTitle.ForeColor = th.Name == "dark" ? Color.White : Color.Black; brandTitle.BackColor = rail.BackColor; return; }
            if (c == brandSub) { brandSub.ForeColor = th.NavSub; brandSub.BackColor = rail.BackColor; return; }
            if (c == logBox) { logBox.BackColor = th.LogBack; logBox.ForeColor = th.LogInfo; RepaintLog(); return; }
            if (tag == "chip") { SetChip(chipText, busy); return; }
            if (tag.StartsWith("navstripe:")) { /* RefreshCards 统一刷 */ return; }
            if (tag.StartsWith("navdot:")) { return; }
            // Label 的 BackColor 默认是"控件默认浅色"而不是透明 —— 建控件时抓的父底色在
            // ApplyTheme 之前还是系统值，所以每个字色分支都要顺手把底色重新继承一次父容器。
            if (tag.StartsWith("line:")) { c.ForeColor = KindColor(tag.Substring(5)); c.BackColor = th.Card; return; }
            if (tag.StartsWith("cardtitle")) { c.ForeColor = th.CardTitle; c.BackColor = ParentBg(c); return; }
            if (tag == "title_big") { c.ForeColor = th.Name == "dark" ? Color.White : Color.Black; c.BackColor = ParentBg(c); return; }
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

        void OnPumpTick(object sender, EventArgs e)
        {
            lock (lineLock)
            {
                foreach (string l in childOut) AppendLog(l);
                childOut.Clear();
            }
            PumpTail();
            if (busy && child != null && child.HasExited) CompleteTask();
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
            SetBusyUi(false);
            RefreshCards();
        }

        void SetBusyUi(bool b)
        {
            busy = b;
            runBtn.Text = b ? "正在跑…(点这个按钮停跟随)" : idleRunText;
            SetChip(b ? "正在跑" : "空闲", b);
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
            try { ParseSummary(RunDumpSummary()); }
            catch (Exception ex) { AppendLog("[ERROR] 刷新失败：" + ex.Message); return; }
            RefreshCards();
            AppendLog("已刷新上面的状态。");
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
            List<Row> tt;
            if (timesByPage.TryGetValue("all", out tt)) DumpRows("TIME", tt);
            DumpWalk(this, "");
            Close();
        }

        static void DumpRows(string what, List<Row> rows)
        {
            foreach (Row r in rows)
                Console.WriteLine(what + " kind=[" + r.Kind + "] game=[" + r.Game + "] text=[" + r.Text + "]");
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
                foreach (Panel card in new[] { cardTimes, cardLast, cardGates, cardEnv })
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
            Console.WriteLine("布局自检: " + (bad == 0 ? "OK，每一页都不重叠、都在各自容器内、四张卡片都有内容" : "有问题(见上)"));
            Close();
            return bad == 0 ? 0 : 1;
        }

        string cardTitleOf(Panel card)
        {
            foreach (Control c in card.Controls)
                if (c.Tag != null && c.Tag.ToString() == "cardtitle") return c.Text;
            return "?";
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
            bool selftest = false, headless = false, dump = false;
            foreach (string a in argv)
            {
                string l = a.ToLowerInvariant();
                if (l == "--selftest") selftest = true;
                if (l == "--headless") headless = true;
                if (l == "--dump") dump = true;
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            PanelForm f = new PanelForm(headless);
            if (dump) { f.DumpTree(); return; }
            if (selftest) Environment.Exit(f.SelfTest());
            if (headless) Environment.Exit(f.Headless());
            Application.Run(f);
            Console.WriteLine("面板已关闭。定时任务不受影响，到点照样自己跑。");
        }
    }
}
