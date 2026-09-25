// tools/panel-cs/Panel.cs —— 每日清体力的图形状态面板(C# / WinForms，.NET Framework 4，系统自带 csc.exe 直接编译)。
//
// 和 panel.ps1 的分工：判定/读数仍然只在 panel.ps1 那一套里 —— 本文件不算任何结论，
// 它做的只有这几件事：
//   1. 起一个隐藏子进程跑 `panel.ps1 -DumpSummary`，读 '|' 分隔的协议行(PROTO/HEADER/GAME/TIME/LAST/GATE/ENV/PROBE/FLAGS/HELP)；
//      第一行 PROTO|<版本> 是这份读数的版本号：不认识时面板要在日志区明说"判定层协议 v几 / 我只认 v几"，
//      而不是让某个格子静默变空 —— 面板只会转达，读不懂必须说出来。
//   2. 把行画成卡片/导航/日志，并把按钮动作转调现成脚本(daily-clear / status / run-via-task)；
//   3. 黑/白双主题切换(唯一的"外观判定"，存在 state\panel-theme.txt)；
//   4. 「实验功能」卡：只碰一个布尔键(runtime.arkVisionRescue) —— 写之前先备份到 backups\，
//      用整文件行内替换、别的一个字节不动；用不用视觉兜底仍由 lib\vision-rescue.ps1 看那个键决定。
//   5. 主控台四页(配置/定时/历史/诊断)：把原来只能敲命令行的日常操作搬进界面。
//      读 = 行内读 settings.json 的那几个键 + 转达 -DumpSummary 已有的 PROBE/TIME 行 + 直读 history.csv；
//      写 = 路径改动代跑 setup.ps1、装卸任务代跑 install-task.ps1、换时间点代跑 reschedule-daily.ps1，
//            只有视觉模型/地址/Key/Webhook 这类标量走行内替换(和 arkVisionRescue 同一套规矩)；
//      一条硬的：**视觉 Key 不进命令行参数**(命令行在别的用户/工具面前是明文)，也不写进日志。
//      面板仍然不加判定："这一路就绪没有""下一个时间点几点"都是脚本说的原话。
// 所以 panel.ps1 里的措辞或 Kind 判据一改，这里一个字不改也跟着变 —— 不会出现两个面板各说各话。
//
// 用法：
//   panel-cs.exe                 打开面板(左侧导航：总览 / 每一路 / 配置 / 定时 / 历史 / 诊断)
//   panel-cs.exe --selftest      不显示窗口：读一次协议、渲染、查布局，有红项退出码 1
//   panel-cs.exe --consoletest   不显示窗口：主控台四页的读写回环(打在临时副本上)、版式、抹 key，全对才 0
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
        // -DumpSummary 的协议版本：判定层第一行 PROTO|<n> 报上来的。面板只认 ProtoKnown，
        // 对不上就在日志区说一次(-1 = 判定层压根没给版本行，那是它比这个面板旧)。
        const int ProtoKnown = 1;
        int protoVer = -1;
        int protoReported = -2;    // 只在不认识/版本变了的那一次说话，别每 45 秒刷一遍横幅
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

            // 主控台四页的操作台：整块盖在"四卡 + 动作区"那块地上(86..470)，默认藏着，导航点到才换上来。
            // 建在这里而不是每张页各建一块，是为了让四页共用同一套坐标 —— 版式自检只需认这一只容器。
            consoleHost = new Panel();
            consoleHost.Location = new Point(X0, 86);
            consoleHost.Size = new Size(CW, 384);
            consoleHost.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Bottom;
            consoleHost.Visible = false;
            Controls.Add(consoleHost);
            themeTag(consoleHost, "card");
            Borderless(consoleHost);

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
            foot1 = MakeLabel(rail, "面板 C# · 引擎 " + Path.GetFileName(engine).Replace(".exe", ""), 21, 684, RailW - 30, 17, null, SmallFont(), false);
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
            // 主控台四页永远在：这一仓有几路只影响"配置页要填几个路径"，不影响能不能进这些页
            items.Add(new[] { "cfg", "配 置" });
            items.Add(new[] { "sched", "定 时" });
            items.Add(new[] { "hist", "历 史" });
            items.Add(new[] { "diag", "诊 断" });
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
            ShowPageFor(id);
            if (IsConsolePage(id))
            {
                // 同一页不重建：人家正在框里打字，45 秒一拍不能把字抹了
                if (consoleBuilt != id) { RebuildConsole(id); consoleBuilt = id; }
                else RefreshConsoleInPlace();
            }
            RefreshCards();
        }

        void RefreshCards()
        {
            string title = "总览";
            foreach (string[] g in games) if (g[0] == page) title = g[1];
            if (page == "all") title = "总览";
            string ct = ConsoleTitle(page);
            pageTitle.Text = ct != null ? ct : title;
            UpdateOnlyBtn();

            // 主控台四页把这块地整个让给操作台，卡片这次不画(它们也看不见)。
            // 读数照常留在 rows* / timesByPage / probesByGame 里 —— 回到总览或某一路就照原样铺出来。
            if (!IsConsolePage(page))
            {
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
            }
            else RefreshConsoleInPlace();   // 自动刷新那一拍到了主控台页：只把"转达"的那几行换新的

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

        // ---------- 页面切换：主控台四页把"四卡 + 动作区"整块换成一张操作台 ----------
        void RebuildConsole(string id)
        {
            consoleHost.Controls.Clear();
            boxByKey.Clear();
            consoleHost.Visible = true;
            consoleNote = MakeLabel(consoleHost, "", 300, 10, CW - 320, 17, ContentAlignment.TopRight, SmallFont(), false);
            themeTag(consoleNote, "dim");
            if (id == "cfg") BuildCfgPage();
            else if (id == "sched") BuildSchedPage();
            else if (id == "hist") BuildHistPage();
            else BuildDiagPage();
            ApplyTheme();
        }

        Label ConsoleCaption(string text, int y)
        {
            Label sq = new Label();
            sq.Text = "";
            sq.Location = new Point(16, y + 2);
            sq.Size = new Size(3, 13);
            sq.Tag = "accent_sq";
            consoleHost.Controls.Add(sq);
            // 第一行(y=10)右边那块位置被 consoleNote(右上角小字)占了：标题只留到 x=294，别和它压住
            int capW = (y <= 10 ? 268 : CW - 42);
            Label l = MakeLabel(consoleHost, text.ToUpper(), 26, y, capW, 17, null, SectionFont(), false);
            l.Tag = "cardtitle";
            return l;
        }

        TextBox MakeBox(Control parent, string key, int x, int y, int w, bool secret)
        {
            TextBox t = new TextBox();
            t.Location = new Point(x, y);
            t.Size = new Size(w, 24);
            t.Font = new Font("Microsoft YaHei UI", 9.5f);
            t.BorderStyle = BorderStyle.FixedSingle;
            if (secret) t.UseSystemPasswordChar = true;
            t.Tag = "input";
            parent.Controls.Add(t);
            if (key != null) boxByKey[key] = t;
            return t;
        }

        // ---------- 页 1：配置 ----------
        void BuildCfgPage()
        {
            consoleNote.Text = "改路径 → 代跑 setup.ps1 · 视觉与通知 → 面板行内写(写前备份) · 不点保存不落盘";
            ConsoleCaption("助手路径", 10);
            int y = 32;
            List<CfgField> fields = CfgFields();
            foreach (CfgField f in fields)
            {
                if (f.SetupArg.Length == 0) break;     // 剩下的归下面那组
                AddCfgRow(f, ref y);
            }
            ConsoleCaption("视觉兜底与通知(可选)", y + 6);
            y += 30;   // 标题占 y+6..y+23，下一行输入框从 y+30 起，留出不叠压的空档
            foreach (CfgField f in fields)
            {
                if (f.SetupArg.Length != 0) continue;
                AddCfgRow(f, ref y);
            }

            cfgProbeArk = MakeLabel(consoleHost, "", 16, y + 6, CW - 32, 17, null, SmallFont(), false);
            themeTag(cfgProbeArk, "dim");
            cfgProbeR1999 = MakeLabel(consoleHost, "", 16, y + 25, CW - 32, 17, null, SmallFont(), false);
            themeTag(cfgProbeR1999, "dim");
            RefreshProbeLines();

            Button bScan = MakeButton(consoleHost, "自动扫描并保存", 16, 344, 168, 30, "primary");
            bScan.Click += delegate { StartTask(Path.Combine(root, "setup.ps1"), new List<string>(), "自动扫描助手位置并写配置(setup.ps1)", false); };
            Button bPaths = MakeButton(consoleHost, "保存路径改动", 192, 344, 148, 30, "secondary");
            bPaths.Click += delegate { SavePathEdits(); };
            Button bScalar = MakeButton(consoleHost, "保存视觉与通知", 348, 344, 148, 30, "secondary");
            bScalar.Click += delegate { SaveScalarEdits(); };
            Button bReload = MakeButton(consoleHost, "重读配置", 504, 344, 116, 30, "chip");
            bReload.Click += delegate { LoadCfgValues(); RefreshNow("重读配置"); RefreshProbeLines(); AppendLog("配置页已按磁盘上的 settings.json 重读。"); };
            Button bClear = MakeButton(consoleHost, "清空视觉 Key", 628, 344, 116, 30, "chip");
            bClear.Visible = HasGame("ark");           // 这一仓没有 ark 那一路就没有视觉 Key 可清
            bClear.Click += delegate
            {
                string r = WriteSettingString("visionApiKey", "");
                TextBox t; if (boxByKey.TryGetValue("visionApiKey", out t)) { t.Text = ""; loadedVal["visionApiKey"] = ""; }
                AppendLog(r.Length == 0 ? "视觉 Key 已从 settings.json 清掉(旧配置备份在 backups\\)。" : "清不掉：" + r);
            };
            Label warn = MakeLabel(consoleHost, "Key 不进命令行、不入仓", 752, 348, 156, 26, null, SmallFont(), false);
            warn.Visible = HasGame("ark");
            themeTag(warn, "dim");
            LoadCfgValues();
        }

        void AddCfgRow(CfgField f, ref int y)
        {
            Label cap = MakeLabel(consoleHost, f.Title, 16, y + 3, 146, 18, null, null, false);
            cap.Tag = "dim";
            if (f.Help.Length > 0) rowTip.SetToolTip(cap, f.Help);   // 悬停看"这一项到底指哪儿"，不用把说明挤进标签
            TextBox t = MakeBox(consoleHost, f.Key, 168, y, 620, f.Secret);
            loadedVal[f.Key] = "";
            if (f.SetupArg.Length > 0 && !f.NoPick)
            {
                Button b = MakeButton(consoleHost, f.Dir ? "浏览…" : "选文件…", 796, y, 74, 24, "chip");
                bool dir = f.Dir;
                b.Click += delegate { PickPath(t, dir, f.Title); };
            }
            else if (f.SetupArg.Length > 0 && f.NoPick)
            {
                Label s = MakeLabel(consoleHost, "填数字", 796, y + 3, 100, 18, null, SmallFont(), false);
                themeTag(s, "dim");
            }
            else if (f.Secret)
            {
                Label s = MakeLabel(consoleHost, "留空=不改", 796, y + 3, 100, 18, null, SmallFont(), false);
                themeTag(s, "dim");
            }
            y += 26;
        }

        void PickPath(TextBox box, bool dir, string title)
        {
            try
            {
                if (dir)
                {
                    using (FolderBrowserDialog d = new FolderBrowserDialog())
                    {
                        d.Description = title;
                        try { if (box.Text.Length > 0 && Directory.Exists(box.Text)) d.SelectedPath = box.Text; } catch { }
                        if (d.ShowDialog(this) != DialogResult.OK) return;
                        box.Text = d.SelectedPath;
                    }
                }
                else
                {
                    using (OpenFileDialog d = new OpenFileDialog())
                    {
                        d.Title = title;
                        d.CheckFileExists = false;
                        try { if (box.Text.Length > 0 && File.Exists(box.Text)) d.FileName = box.Text; } catch { }
                        if (d.ShowDialog(this) != DialogResult.OK) return;
                        box.Text = d.FileName;
                    }
                }
                AppendLog("选好了一条：" + box.Text + " —— 还没写盘，点「保存路径改动」才交给 setup.ps1。");
            }
            catch (Exception ex) { AppendLog("浏览框没打开：" + ex.Message); }
        }

        void LoadCfgValues()
        {
            foreach (CfgField f in CfgFields())
            {
                TextBox t; if (!boxByKey.TryGetValue(f.Key, out t)) continue;
                string v = ReadSettingString(f.Key);
                string s = v == null ? "" : v;
                t.Text = s;
                loadedVal[f.Key] = s;
            }
        }

        void RefreshProbeLines()
        {
            if (cfgProbeArk == null) return;
            cfgProbeArk.Text = "判定层怎么说(转达 PROBE 原文)：" + ProbeLine("ark");
            cfgProbeR1999.Text = HasGame("r1999") ? "　└ 1999：" + ProbeLine("r1999") : "　└ 这一仓没有 1999 那一路，上面填了也用不上";
        }

        string ProbeLine(string game)
        {
            List<string> parts = new List<string>();
            List<Row> l;
            if (probesByGame.TryGetValue(game, out l))
                foreach (Row r in l) parts.Add(r.Text);
            return parts.Count == 0 ? "没读到探针(判定库不在？)" : string.Join("　", parts.ToArray());
        }

        // 把"哪些框被改过、该带哪几个参数给 setup.ps1"这段算法单拎出来：
        // 它决定"只下发改过的那几项"(其余沿用现有 settings.json)，也决定带引号的路径直接不交给命令行 —— 自检要钉的就是这两条。
        List<string> ChangedPathArgs(List<string> rejected)
        {
            List<string> args = new List<string>();
            foreach (CfgField f in CfgFields())
            {
                if (f.SetupArg.Length == 0 || f.Secret) continue;
                TextBox t; if (!boxByKey.TryGetValue(f.Key, out t)) continue;
                string now = t.Text.Trim();
                string was = loadedVal.ContainsKey(f.Key) ? loadedVal[f.Key] : "";
                if (now.Length == 0 || now == was) continue;
                if (now.IndexOf('"') >= 0)
                {
                    if (rejected != null) rejected.Add(f.Title + "：路径里有 \"，面板不把它交给命令行");
                    continue;
                }
                args.Add(f.SetupArg);
                args.Add("\"" + now + "\"");
            }
            return args;
        }

        void SavePathEdits()
        {
            List<string> rejected = new List<string>();
            List<string> args = ChangedPathArgs(rejected);
            foreach (string r in rejected) AppendLog("这一项没交给 setup.ps1 ——" + r);
            if (args.Count == 0) { AppendLog("路径没改动(或者上面那几项被挡下了)，这次不跑 setup.ps1。"); return; }
            foreach (string a in args) if (a.StartsWith("-")) AppendLog("要下发：" + a + " …");
            List<string> all = new List<string>();
            all.Add("-SkipScan");                 // 没点的那几项沿用现有 settings.json，不重新扫盘
            all.AddRange(args.ToArray());
            StartTask(Path.Combine(root, "setup.ps1"), all, "把改过的路径交给 setup.ps1(-SkipScan：其余沿用现有配置)", false);
        }

        void SaveScalarEdits()
        {
            int changed = 0;
            foreach (CfgField f in CfgFields())
            {
                if (f.SetupArg.Length != 0) continue;
                TextBox t; if (!boxByKey.TryGetValue(f.Key, out t)) continue;
                string now = t.Text.Trim();
                string was = loadedVal.ContainsKey(f.Key) ? loadedVal[f.Key] : "";
                if (f.Secret && now.Length == 0) continue;          // 留空 = 这一项不动(要清请用「清空视觉 Key」)
                if (now == was) continue;
                string r = WriteSettingString(f.Key, now);
                if (r.Length != 0 && r.IndexOf("写好了") < 0) { AppendLog(f.Title + " 没写成：" + r); continue; }
                loadedVal[f.Key] = now;
                changed++;
                AppendLog(f.Title + " 已写进 settings.json" + (f.Secret ? "(值本身不进日志)" : "：" + now) +
                          (r.Length == 0 ? "，旧配置备份在 backups\\" : "，" + r));
            }
            if (changed == 0) AppendLog("视觉/通知这几项没改动，什么都没写。");
            else RefreshNow("保存配置后");
        }

        // ---------- 页 2：定时 ----------
        void BuildSchedPage()
        {
            consoleNote.Text = "时间点存 settings.json 的 schedule.times，注册动作仍归 install-task.ps1";
            ConsoleCaption("机器上现在的定时任务(转达 TIME 原文)", 10);
            schedReadout = MakeLabel(consoleHost, "", 16, 34, CW - 32, 84, null, null, false);
            themeTag(schedReadout, "dim");
            schedReadout.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;

            int y = 128;
            MakeLabel(consoleHost, "时间点", 16, y + 3, 90, 18, null, null, false).Tag = "dim";
            TextBox t = MakeBox(consoleHost, "times", 108, y, 250, false);
            string[] cur = ReadTimes();
            t.Text = cur.Length == 0 ? "" : string.Join(",", cur);
            loadedVal["times"] = t.Text;
            Label hint = MakeLabel(consoleHost, "逗号分隔，HH:mm；一个时间点一个任务", 366, y + 3, 300, 18, null, SmallFont(), false);
            themeTag(hint, "dim");
            Label note2 = MakeLabel(consoleHost, "跑哪几路由脚本按这一仓有几路决定，面板不动它；换时间点会等正在跑的那一轮结束再动手。", 108, y + 32, 700, 18, null, SmallFont(), false);
            themeTag(note2, "dim");
            RefreshSchedReadout();

            Button bDry = MakeButton(consoleHost, "先看要改什么", 16, 344, 148, 30, "secondary");
            bDry.Click += delegate { RunReschedule(true); };
            Button bApply = MakeButton(consoleHost, "应用时间点(弹一次 UAC)", 172, 344, 190, 30, "primary");
            bApply.Click += delegate { RunReschedule(false); };
            Button bInstall = MakeButton(consoleHost, "按现在的时间点注册", 370, 344, 168, 30, "secondary");
            bInstall.Click += delegate { StartTask(Path.Combine(root, "install-task.ps1"), new List<string>(), "注册每日计划任务(install-task.ps1，要管理员)", false); };
            Button bUninstall = MakeButton(consoleHost, "卸掉这些任务", 546, 344, 130, 30, "chip");
            bUninstall.Click += delegate { StartTask(Path.Combine(root, "install-task.ps1"), new List<string> { "-Uninstall" }, "移除计划任务(install-task.ps1 -Uninstall)", false); };
            Button bReload2 = MakeButton(consoleHost, "重读", 684, 344, 90, 30, "chip");
            bReload2.Click += delegate { RebuildConsole("sched"); RefreshNow("重读定时"); };
        }

        void RunReschedule(bool dryRun)
        {
            TextBox t; if (!boxByKey.TryGetValue("times", out t)) return;
            string raw = t.Text.Trim();
            if (raw.Length == 0) { AppendLog("时间点框是空的 —— 不敢把任务改成「一个都没有」，先填上 HH:mm。"); return; }
            List<string> bad = new List<string>();
            foreach (string one in raw.Split(','))
            {
                string s = one.Trim();
                if (s.Length == 0) continue;
                if (!Regex.IsMatch(s, @"^\d{1,2}:\d{2}$")) bad.Add(s);
            }
            if (bad.Count > 0) { AppendLog("这几个不像时间点：" + string.Join(" ", bad.ToArray()) + "(要 06:00 这种)。"); return; }
            List<string> args = new List<string>();
            args.Add("-Times");
            args.Add("\"" + raw + "\"");
            if (dryRun) args.Add("-DryRun");
            StartTask(Path.Combine(root, @"tools\reschedule-daily.ps1"), args,
                      dryRun ? "预览要改的时间点(reschedule-daily.ps1 -DryRun，不写不注册)" : "改时间点(等当前轮跑完 → 写配置 → 重注册 → 删旧任务)", false);
        }

        void RefreshSchedReadout()
        {
            if (schedReadout == null) return;
            List<string> lines = new List<string>();
            List<Row> t;
            if (timesByPage.TryGetValue("all", out t))
                foreach (Row r in t) lines.Add(r.Kind + "  " + r.Text);
            schedReadout.Text = lines.Count == 0 ? "没读到定时行(判定库不在？)" : string.Join("   ｜   ", lines.ToArray());
        }

        // ---------- 页 3：历史 ----------
        void BuildHistPage()
        {
            consoleNote.Text = "state\\history.csv 是判定层每轮逐字写的，面板只铺表、不改一个字";
            ConsoleCaption("历  史", 10);
            histHost = new Panel();
            histHost.Location = new Point(16, 34);
            histHost.Size = new Size(CW - 32, 300);
            histHost.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Bottom;
            histHost.Tag = "card";
            consoleHost.Controls.Add(histHost);

            histHint = MakeLabel(consoleHost, "", 16, 340, 560, 18, null, SmallFont(), false);
            themeTag(histHint, "dim");
            Button bRef = MakeButton(consoleHost, "刷新历史", 592, 336, 116, 30, "secondary");
            bRef.Click += delegate { RebuildConsole("hist"); };
            Button bLog = MakeButton(consoleHost, "打开日志目录", 716, 336, 116, 30, "chip");
            bLog.Click += delegate
            {
                try { Process.Start("explorer.exe", Path.Combine(root, "logs")); }
                catch (Exception ex) { AppendLog("打不开日志目录：" + ex.Message); }
            };
            RenderHistory();
        }

        void RenderHistory()
        {
            if (histHost == null) return;
            histRows = 0;
            histHost.Controls.Clear();
            List<string[]> recs;
            try { recs = ParseCsv(ReadAllShared(HistoryPath)); }
            catch (Exception ex) { histHint.Text = "读 history.csv 失败：" + ex.Message; return; }
            if (recs.Count < 2) { histHint.Text = "还没有历史(跑过一轮才会有)。"; return; }
            string[] head = recs[0];
            int ti = CsvCol(head, "time"), gi = CsvCol(head, "game"), si = CsvCol(head, "status"),
                mi = CsvCol(head, "mode"), di = CsvCol(head, "duration_sec"), vi = CsvCol(head, "detail");
            if (ti < 0 || gi < 0 || si < 0) { histHint.Text = "history.csv 表头对不上(要 time/game/status)，面板不猜列位。"; return; }

            int total = recs.Count - 1;
            int cap = 12;
            int from = total > cap ? total - cap + 1 : 1;   // 记录从下标 1 开始：取最后 cap 条就是 [total-cap+1 .. total]
            int y = 4;
            int shown = 0;
            for (int i = recs.Count - 1; i >= from; i--)
            {
                string[] r = recs[i];
                if (r.Length <= ti || r.Length <= gi || r.Length <= si) continue;
                string game = r[gi].Trim();
                string status = r[si].Trim();
                string detail = vi >= 0 && r.Length > vi ? r[vi].Trim() : "";
                string kind = status == "ok" ? "ok" : (status == "blocked" || status == "error" ? "err" : "warn");
                string dur = di >= 0 && r.Length > di && r[di].Trim().Length > 0 ? r[di].Trim() + "s" : "-";
                string line = Pad(r[ti].Trim(), 20) + "  " + Pad(GameName(game), 8) + "  " + Pad(dur, 8) + "  " +
                              (detail.Length > 0 ? Cut(detail, 96) : "(这轮没写原因)");
                // 等宽字体：这几列是靠 Pad() 数格子对齐的，用雅黑那种比例字体"方舟"和"1999"宽度就不齐
                Label l = MakeLabel(histHost, line, 6, y, histHost.Width - 12, 18, null, MonoFont(), false);
                themeTag(l, "line:" + kind);
                if (detail.Length > 0)
                {
                    l.Cursor = Cursors.Hand;
                    Row captured = new Row(kind, game, status + " · " + r[ti].Trim(), detail);
                    ToolTipHintRow(l, captured);
                    l.Click += delegate { ShowRowDetail(captured); };
                }
                y += 22; shown++;
            }
            histRows = shown;
            histHint.Text = "共 " + total + " 条，这里显示最近 " + shown + " 条(新的在上) —— 带原因的行可以点，完整那句会打进日志区。";
        }

        void ToolTipHintRow(Label l, Row r)
        {
            rowTip.SetToolTip(l, ToolTipHint(r));
        }

        static string Cut(string s, int n)
        {
            return s.Length <= n ? s : s.Substring(0, n) + "…";
        }

        // ---------- 页 4：诊断打包 ----------
        void BuildDiagPage()
        {
            consoleNote.Text = "只放这台机器上给人看，默认落桌面；包里的 settings 摘要是去掉 key 之后的";
            ConsoleCaption("打  包", 10);
            string[] inc = new string[] {
                "读数.txt —— 最近一次 -DumpSummary 的协议原文(判定层说的那些行，一个字不改)",
                "体检.txt —— status.ps1 -Offline 的输出(只读，不会启动任何程序)",
                "配置摘要.txt —— settings.json 里 paths / schedule / runtime 这几段的现值，visionApiKey 与 webhook 已抹掉",
                "history.csv —— 每轮的状态与结论(判定层逐字写的)",
                "logs\\run-*.log 最近 5 个 —— 我们自己的那轮日志(单个超 4MB 的跳过)"
            };
            int y = 34;
            foreach (string s in inc)
            {
                Label b = MakeLabel(consoleHost, "· " + s, 16, y, CW - 40, 18, null, SmallFont(), false);
                themeTag(b, "dim");
                y += 20;
            }
            Label warn = MakeLabel(consoleHost, "里面有本机绝对路径和某一天的时间点：这类值只在这台机器上成立，别当成仓库里的东西发出去，也别忘了包里有日志里的那些路径。",
                                    16, y + 8, CW - 40, 34, null, SmallFont(), false);
            themeTag(warn, "warn");
            Button bPack = MakeButton(consoleHost, "打包到桌面", 16, 344, 168, 30, "primary");
            bPack.Click += delegate { AppendLog("开始打包：体检那一段要现跑 status.ps1 -Offline，十几秒到几十秒，界面会短暂无响应。"); PackDiag(Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory), true, true); };
            Button bTmp = MakeButton(consoleHost, "先打一份到临时目录看看", 192, 344, 200, 30, "secondary");
            bTmp.Click += delegate { PackDiag(Path.Combine(Path.GetTempPath(), "gd-diag-preview"), true, false); };
            Button bOpen = MakeButton(consoleHost, "打开桌面", 400, 344, 116, 30, "chip");
            bOpen.Click += delegate
            {
                try { Process.Start("explorer.exe", Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory)); }
                catch (Exception ex) { AppendLog("打不开桌面：" + ex.Message); }
            };
        }

        // 打包本身不判定任何东西：只是把已经在盘上的读数/日志按清单抄一份压成 zip。
        // key 一律不进包(写摘要时按行抹掉)，zip 靠现成的 Compress-Archive 生成，不在 C# 里引新程序集。
        string PackDiag(string destDir, bool tell, bool withStatus)
        {
            if (diagBusy) { if (tell) AppendLog("上一次打包还没完，等它。"); return ""; }
            diagBusy = true;
            string stage = "";
            try
            {
                stage = Path.Combine(Path.GetTempPath(), "gd-diag-" + DateTime.Now.ToString("yyyyMMdd-HHmmss"));
                Directory.CreateDirectory(Path.Combine(stage, "logs"));
                if (lastDump != null && lastDump.Length > 0) File.WriteAllText(Path.Combine(stage, "读数.txt"), lastDump, new UTF8Encoding(true));
                else File.WriteAllText(Path.Combine(stage, "读数.txt"), "(这次没读到 -DumpSummary 原文)", new UTF8Encoding(true));
                WriteSettingsDigest(Path.Combine(stage, "配置摘要.txt"));
                if (withStatus) File.WriteAllText(Path.Combine(stage, "体检.txt"), RunStatusOffline(), new UTF8Encoding(true));
                try { if (File.Exists(HistoryPath)) File.Copy(HistoryPath, Path.Combine(stage, "history.csv"), true); } catch { }
                int copied = CopyRecentLogs(Path.Combine(stage, "logs"));

                // 压缩交给现成的 Compress-Archive。这里刻意走"写一个临时 .ps1 再 -File 调它"：
                // powershell.exe 5.1 的 -Command 会把整串当成一个单引号字符串字面量，不执行(实测静默什么都不压)。
                string dest = Path.Combine(destDir, "GD诊断-" + DateTime.Now.ToString("yyyyMMdd-HHmmss") + ".zip");
                string pack = Path.Combine(Path.GetTempPath(), "gd-diag-pack-" + Process.GetCurrentProcess().Id + ".ps1");
                StringBuilder ps = new StringBuilder();
                ps.AppendLine("$ErrorActionPreference = 'Stop'");
                ps.AppendLine("New-Item -ItemType Directory -Force -Path '" + QuoteSingle(destDir) + "' | Out-Null");
                ps.AppendLine("Compress-Archive -LiteralPath '" + QuoteSingle(stage) + "' -DestinationPath '" +
                              QuoteSingle(dest) + "' -Force");
                // 带 BOM：这台机器的 5.1 读无 BOM 的 UTF-8 中文 .ps1 会翻车(路径本身是 ASCII，但规矩按规矩来)
                File.WriteAllText(pack, ps.ToString(), new UTF8Encoding(true));

                ProcessStartInfo psi = new ProcessStartInfo();
                psi.FileName = engine;
                psi.Arguments = "-NoProfile -ExecutionPolicy Bypass -File \"" + pack + "\"";
                psi.WorkingDirectory = root;
                psi.UseShellExecute = false;
                psi.CreateNoWindow = true;
                psi.RedirectStandardOutput = true;
                psi.RedirectStandardError = true;
                psi.StandardOutputEncoding = Encoding.UTF8;
                psi.StandardErrorEncoding = Encoding.UTF8;
                Process p = Process.Start(psi);
                string so = p.StandardOutput.ReadToEnd();
                string se = p.StandardError.ReadToEnd();
                if (!p.WaitForExit(60000))
                {
                    try { p.Kill(); } catch { }
                    lastPackError = "打包超时(60 秒)";
                    if (tell) AppendLog("打包超时(60 秒)，这次没成。");
                    CleanupPack(pack, stage); return "";
                }
                if (tell)
                {
                    foreach (string l in (so + se).Replace("\r", "").Split('\n'))
                        if (l.Trim().Length > 0) AppendLog("  " + l);
                }
                bool ok = p.ExitCode == 0 && File.Exists(dest);
                lastPackError = ok ? "" : "退出码 " + p.ExitCode + (se.Trim().Length > 0 ? "；" + OneLine(se.Trim()) : (File.Exists(dest) ? "" : "；zip 没生成"));
                if (tell)
                {
                    AppendLog(ok ? "包好了：" + dest + "(日志 " + copied + " 个；settings 摘要里没有 key)"
                                 : "打包没成 —— " + lastPackError);
                }
                CleanupPack(pack, stage);
                return ok ? dest : "";
            }
            catch (Exception ex) { lastPackError = ex.Message; if (tell) AppendLog("打包失败：" + ex.Message); return ""; }
            finally { diagBusy = false; }
        }

        static string QuoteSingle(string s)
        {
            return s == null ? "" : s.Replace("'", "''");
        }

        // 打包用的临时脚本和临时目录都得收掉：脚本留在 %TEMP% 里没人清，目录里是抄出来的日志
        void CleanupPack(string script, string stage)
        {
            try { if (script != null && File.Exists(script)) File.Delete(script); } catch { }
            try { if (stage != null && Directory.Exists(stage)) Directory.Delete(stage, true); } catch { }
        }

        static string OneLine(string s)
        {
            if (s == null) return "";
            string r = s.Replace("\r", " ").Replace("\n", " ").Trim();
            return r.Length > 180 ? r.Substring(0, 180) + "…" : r;
        }

        void WriteSettingsDigest(string path)
        {
            StringBuilder sb = new StringBuilder();
            sb.AppendLine("settings.json 摘要(生成于 " + DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + ")");
            sb.AppendLine("规则：_readme 那种长说明不抄；visionApiKey / webhook 两行按值抹成 <已抹掉>，其余原样。");
            try
            {
                if (!File.Exists(SettingsPath)) { sb.AppendLine("(这台机器还没有 config\\settings.json)"); }
                else
                {
                    foreach (string raw in File.ReadAllLines(SettingsPath))
                    {
                        string line = raw.TrimEnd();
                        if (line.Trim() == "{") continue;
                        if (line.IndexOf("\"_readme\"") >= 0) { sb.AppendLine("  \"_readme\": <模板说明，省略>"); continue; }
                        if (Regex.IsMatch(line, "\"(visionApiKey|webhook)\"\\s*:\\s*\"[^\"]+\""))
                            line = Regex.Replace(line, "(:\\s*)\"[^\"]+\"", "$1\"<已抹掉>\"");
                        sb.AppendLine(line);
                    }
                }
            }
            catch (Exception ex) { sb.AppendLine("(读 settings.json 失败：" + ex.Message + ")"); }
            File.WriteAllText(path, sb.ToString(), new UTF8Encoding(true));
        }

        // 体检那一段是"现跑一次只读脚本再把输出抄进包"，不自己算任何东西。
        // 同步等最多 60 秒：面板会短暂无响应，所以点按钮时先在日志里预告了。
        string RunStatusOffline()
        {
            try
            {
                ProcessStartInfo psi = new ProcessStartInfo();
                psi.FileName = engine;
                psi.Arguments = "-NoProfile -ExecutionPolicy Bypass -File \"" + Path.Combine(root, "status.ps1") + "\" -Offline";
                psi.WorkingDirectory = root;
                psi.UseShellExecute = false;
                psi.CreateNoWindow = true;
                psi.RedirectStandardOutput = true;
                psi.RedirectStandardError = true;
                psi.StandardOutputEncoding = Encoding.UTF8;
                psi.StandardErrorEncoding = Encoding.UTF8;
                Process p = Process.Start(psi);
                string so = p.StandardOutput.ReadToEnd();
                string se = p.StandardError.ReadToEnd();
                if (!p.WaitForExit(60000)) { try { p.Kill(); } catch { } return "(status.ps1 -Offline 超 60 秒没跑完，这是已拿到的部分)\r\n" + so + se; }
                return "status.ps1 -Offline 退出码 " + p.ExitCode + "\r\n\r\n" + so + (se.Length > 0 ? "\r\n[stderr]\r\n" + se : "");
            }
            catch (Exception ex) { return "(体检没跑成：" + ex.Message + ")"; }
        }

        int CopyRecentLogs(string logStage)
        {
            int n = 0;
            try
            {
                string dir = Path.Combine(root, "logs");
                if (!Directory.Exists(dir)) return 0;
                List<FileInfo> files = new List<FileInfo>();
                foreach (string f in Directory.GetFiles(dir, "run-*.log")) files.Add(new FileInfo(f));
                files.Sort(delegate(FileInfo a, FileInfo b) { return b.LastWriteTime.CompareTo(a.LastWriteTime); });
                for (int i = 0; i < files.Count && n < 5; i++)
                {
                    if (files[i].Length > 4L * 1024 * 1024) continue;
                    try { File.Copy(files[i].FullName, Path.Combine(logStage, files[i].Name), true); n++; } catch { }
                }
            }
            catch { }
            return n;
        }

        // ---------- 四页共用的显示/刷新 ----------
        void RefreshConsoleInPlace()
        {
            if (page == "cfg") { RefreshProbeLines(); }
            else if (page == "sched") { RefreshSchedReadout(); }
            else if (page == "hist") { RenderHistory(); }
        }

        bool consoleUp;                    // 我们自己"要求显示"的状态：窗体没 Show 时控件的 Visible getter 是有效可见性，一律 false，自检不能拿它当依据
        bool cardsUp = true;               // 同上，卡片/动作区那一侧
        void ShowPageFor(string id)
        {
            bool console = IsConsolePage(id);
            consoleUp = console;
            cardsUp = !console;
            consoleHost.Visible = console;
            cardTimes.Visible = !console;
            cardLast.Visible = !console;
            cardGates.Visible = !console;
            cardEnv.Visible = !console;
            cardExp.Visible = !console;
            act.Visible = !console;
            if (!console) consoleBuilt = null;
        }

        string ConsoleTitle(string id)
        {
            if (id == "cfg") return "配置";
            if (id == "sched") return "定时";
            if (id == "hist") return "历史";
            if (id == "diag") return "诊断";
            return null;
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
            foreach (Panel p in new[] { cardTimes, cardLast, cardGates, cardEnv, cardExp, act, logCard, consoleHost }) p.Invalidate();
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
            // 输入框：黑白双主题里"能敲的地方"只靠一档底色差体现，不加第三种颜色
            if (tag == "input" && c is TextBox)
            {
                c.BackColor = th.Name == "dark" ? th.NavOn : th.Window;
                c.ForeColor = th.Text;
                return;
            }
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

        // 历史表那几列靠 Pad() 数"格子"对齐(中文算两格)，只有等宽字体才数得准。
        // 挑字体的办法不靠"量一个字母一个空格" —— MeasureText 对短串有个跟长度有关的底补
        // (这台机器上量单个空格 14px、量 8 个才 63px，按单字算出来的比例全是假的)。
        // 直接量**真会画出来的那两行**：哪套候选字体下"方舟行"和"1999 行"一样宽，就用哪套；
        // 一套都找不到就退回雅黑(宁可没对齐也别缺字)，那时 --consoletest 的列对齐断言会当场红给你看。
        Font monoFont;
        static string MonoProbeA { get { return Pad("2026-09-25 19:30:00", 20) + "  " + Pad("方舟", 8) + "  " + Pad("662s", 8); } }
        static string MonoProbeB { get { return Pad("2026-09-25 19:36:15", 20) + "  " + Pad("1999", 8) + "  " + Pad("-", 8); } }

        Font MonoFont()
        {
            if (monoFont == null)
            {
                Font best = null;
                foreach (string cand in new string[] { "NSimSun", "SimSun", "Microsoft YaHei UI" })
                {
                    Font f = null;
                    try
                    {
                        f = new Font(cand, 9.5f);
                        if (Math.Abs(TextRenderer.MeasureText(MonoProbeA, f).Width - TextRenderer.MeasureText(MonoProbeB, f).Width) <= 1)
                        { best = f; break; }
                        f.Dispose();
                    }
                    catch { if (f != null) f.Dispose(); }
                }
                monoFont = best != null ? best : SmallFont();
            }
            return monoFont;
        }

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
                if (IsConsolePage(id))
                {
                    bad += AuditConsole(id);
                    continue;
                }
                if (consoleUp || !cardsUp)
                { Console.WriteLine("  布局问题 " + PageTitle() + " 不是主控台页，操作台却还挡在卡片前面"); bad++; }
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

        // 主控台页的版式自检：操作台得露着、卡片/动作区得让位，页内控件不许越出容器、不许互相压住。
        // 卡片那套断言在主控台页上没有意义(整块被盖住了)，所以这里换成认 consoleHost。
        int AuditConsole(string id)
        {
            int bad = 0;
            string what = ConsoleTitle(id);
            if (!consoleUp) { Console.WriteLine("  布局问题 " + what + " 页操作台没要求显示"); return 1; }
            if (cardsUp) { Console.WriteLine("  布局问题 " + what + " 页卡片或动作区没让位"); bad++; }
            List<Control> kids = new List<Control>();
            foreach (Control c in consoleHost.Controls) kids.Add(c);
            if (kids.Count < 3) { Console.WriteLine("  布局问题 " + what + " 页只摆了 " + kids.Count + " 个控件，像是没建起来"); bad++; }
            Rectangle cl = new Rectangle(Point.Empty, consoleHost.ClientSize);
            foreach (Control c in kids)
            {
                Rectangle a = c.Bounds;
                if (a.X < 0 || a.Y < 0 || a.Right > cl.Right || a.Bottom > cl.Bottom)
                { Console.WriteLine("  布局问题 " + what + " 页控件越出操作台(" + Describe(c) + "): " + a); bad++; }
            }
            for (int i = 0; i < kids.Count; i++)
                for (int j = i + 1; j < kids.Count; j++)
                {
                    Rectangle a = kids[i].Bounds, b = kids[j].Bounds;
                    if (a.X < b.Right && b.X < a.Right && a.Y < b.Bottom && b.Y < a.Bottom)
                    { Console.WriteLine("  布局问题 " + what + " 页两个控件互相压住(" + Describe(kids[i]) + " × " + Describe(kids[j]) + ")"); bad++; }
                }
            if (id == "hist")
            {
                int rows = 0;
                if (histHost != null) foreach (Control c in histHost.Controls) if (c is Label) rows++;
                if (File.Exists(HistoryPath) && histRows > 0 && rows == 0)
                { Console.WriteLine("  布局问题 历史页有 " + histRows + " 条历史却一行没铺"); bad++; }
            }
            Console.WriteLine("  " + what + " 页：" + kids.Count + " 个控件，边界/叠压检查" + (bad == 0 ? "通过" : "有红项"));
            return bad;
        }

        static string Describe(Control c)
        {
            string t = c.Text == null ? "" : c.Text;
            if (t.Length > 14) t = t.Substring(0, 14) + "…";
            return c.GetType().Name + (t.Length > 0 ? "「" + t + "」" : "");
        }

        // --consoletest：主控台四页的离线自检。零弹窗、不碰真 settings.json(读写全部打在 %TEMP% 的一份副本上)、
        // 不起 daily-clear/status/setup 那一类脚本(只有打包那一步按需起一次 Compress-Archive，用完即删)。
        // 验的是"把手接得对"：转达的行、转发给 setup.ps1 的参数、写回的键、包里抹没抹掉 key。
        public int ConsoleTest()
        {
            int fail = 0;
            chkFail = 0;    // Chk 的账记在字段上，收尾时并进来(见 Chk 上方注释)
            Console.WriteLine("---- panel-cs 主控台四页自测(不显示窗口；只起一次压缩，不跑日常脚本) ----");
            Console.WriteLine("root=" + root + "  引擎=" + Path.GetFileName(engine) + "  这一仓=" + string.Join(" + ", GameIds()));
            string keepSettings = settingsPathOverride, keepHistory = historyPathOverride;
            string tmpSettings = Path.Combine(Path.GetTempPath(), "gd-consoletest-settings-" + Process.GetCurrentProcess().Id + ".json");
            pump.Stop(); beat.Stop();
            Location = new Point(-16000, -16000);
            ShowInTaskbar = false;
            try
            {
                // ---- 1) JSON 转义往返：路径里的反斜杠/引号/中文/制表都必须原样回来 ----
                // 这几条是**编出来的形状**(有空格、有版本号、有反斜杠)，不许抄本机真路径：
                // lib\common.ps1 的 Test-GdExportLeaks 会拿 settings.json 里的每个本机值来扫源码
                // (tools\test-modules.ps1 [m8] 现在就扫，不用等导出)，
                // 上一轮就是因为把真实的 MAA 目录端游路径粘进测试样本，导出时被这道闸判红。
                string[] probes = new string[] {
                    @"C:\Tools\MAA-v0.0.0-win-x64",
                    @"D:\Games\Arknights Game\Arknights.exe",
                    "带\"引号\"的路径",
                    @"尾巴一个反斜杠\",
                    "tab\t制表 与 中文"
                };
                foreach (string s in probes)
                {
                    string rt = JsonUnescape(JsonEscape(s));
                    Chk("转义往返：" + s.Replace("\t", "\\t"), rt == s, rt == s ? "" : "回来的是 " + rt.Replace("\t", "\\t"));
                }
                // 转义过的串里，每个 " 前头都必须是奇数个反斜杠；偶数(含 0)就是裸引号，会把 settings.json 那一行弄断
                string escq = JsonEscape("他说\"停\"，然后\\继续\"");
                bool naked = false;
                for (int i = 0; i < escq.Length; i++)
                {
                    if (escq[i] != '"') continue;
                    int bs = 0;
                    for (int j = i - 1; j >= 0 && escq[j] == '\\'; j--) bs++;
                    if (bs % 2 == 0) naked = true;
                }
                Chk("转义后没有裸引号(每个 \\\" 前都是奇数个反斜杠)", !naked, escq);

                // ---- 2) 标量读写：全部打在临时副本上，真配置一个字节都不动 ----
                bool haveReal = File.Exists(Path.Combine(root, @"config\settings.json"));
                byte[] realBytes = haveReal ? File.ReadAllBytes(Path.Combine(root, @"config\settings.json")) : null;
                if (haveReal) File.Copy(Path.Combine(root, @"config\settings.json"), tmpSettings, true);
                else File.WriteAllText(tmpSettings, "{\n  \"paths\": {\n    \"maaDir\": \"C:\\\\Fake\\\\MAA\"\n  },\n  \"notify\": {\n    \"webhook\": \"\"\n  }\n}\n", new UTF8Encoding(false));
                settingsPathOverride = tmpSettings;
                string beforeMaa = ReadSettingString("maaDir");
                Chk("临时副本读得到 maaDir", beforeMaa != null, "值=" + (beforeMaa == null ? "null" : beforeMaa));
                Chk("键不存在时说的是『老模板、改不动』而不是硬塞",
                    WriteSettingString("no_such_key_for_test", "x").IndexOf("老模板") >= 0, "");

                string hook = "https://example.invalid/hook?a=1&b=2";
                string origHook = ReadSettingString("webhook");
                if (origHook == null) origHook = "";
                string w = WriteSettingString("webhook", hook);
                Chk("写 webhook 有成(" + ShortMsg(w) + ")", w.Length == 0 || w.IndexOf("写好了") >= 0, w);
                Chk("读回来和写进去一模一样", ReadSettingString("webhook") == hook, "读到 " + ReadSettingString("webhook"));
                Chk("改动没牵连别处：maaDir 还是原来那条", ReadSettingString("maaDir") == beforeMaa, "");
                string w2 = WriteSettingString("webhook", origHook);
                Chk("能原样写回(" + ShortMsg(w2) + ")", w2.Length == 0 || w2.IndexOf("写好了") >= 0, w2);
                Chk("写回后和动手前一字不差", ReadSettingString("webhook") == origHook, "读到[" + ReadSettingString("webhook") + "]");
                byte[] head = new byte[3];
                using (FileStream fs = new FileStream(tmpSettings, FileMode.Open, FileAccess.Read)) { if (fs.Read(head, 0, 3) < 3) { head = new byte[] { 0, 0, 0 }; } }
                Chk("写出去的还是 UTF-8 无 BOM(5.1 读带 BOM 的中文行会翻车)", !(head[0] == 0xEF && head[1] == 0xBB && head[2] == 0xBF),
                    head[0].ToString("X2") + head[1].ToString("X2") + head[2].ToString("X2"));
                if (haveReal)
                {
                    byte[] tmpBytes = File.ReadAllBytes(tmpSettings);
                    Chk("绕完这一圈，副本和真 settings.json 逐字节一样(那几笔写全回退了)",
                        SameBytes(tmpBytes, realBytes), tmpBytes.Length + " vs " + realBytes.Length + " 字节");
                }

                // ---- 3) 四页都能建起来、都让卡片让位、控件不越界不叠压 ----
                Show(); Hide();
                string[] ids = new string[] { "cfg", "sched", "hist", "diag" };
                foreach (string id in ids)
                {
                    SelectPage(id);
                    int btns = 0;
                    foreach (Control c in consoleHost.Controls) if (c is Button) btns++;
                    int a = AuditConsole(id);
                    Chk(id + " 页版式自检", a == 0, "红项 " + a);
                    Chk(id + " 页有能点的把手(按钮 " + btns + " 个)", btns >= 2, "");
                    fail += a;
                    if (btns < 2) fail++;
                }
                SelectPage("all");
                Chk("回总览后卡片回来了、操作台退下去了", consoleUp == false && cardsUp, "");

                // ---- 4) 配置页：现值进框、只把改过的交给 setup.ps1、带引号的路径挡在命令行外 ----
                SelectPage("cfg");
                TextBox maa;
                if (boxByKey.TryGetValue("maaDir", out maa))
                {
                    Chk("配置页的 maaDir 框里就是 settings.json 里那条", maa.Text == beforeMaa, "框[" + maa.Text + "] 文件[" + beforeMaa + "]");
                    string was = maa.Text;
                    maa.Text = @"D:\Tools\MAA 新位置";
                    List<string> rejected = new List<string>();
                    List<string> args = ChangedPathArgs(rejected);
                    Chk("改过的那项变成 -MaaDir + 带引号的值", args.IndexOf("-MaaDir") >= 0 &&
                        (args.IndexOf("\"D:\\Tools\\MAA 新位置\"") >= 0 || args.Contains("\"D:\\Tools\\MAA 新位置\"")), string.Join(" ", args.ToArray()));
                    Chk("没改动的项不跟着下发(其余沿用现有配置)", CountSwitches(args, "-ArkExe") == 0 && CountSwitches(args, "-M9aDir") == 0, string.Join(" ", args.ToArray()));
                    maa.Text = "有\"引号\"的路径";
                    rejected.Clear();
                    List<string> args2 = ChangedPathArgs(rejected);
                    Chk("带引号的路径不交给命令行，并且当面说清为什么", args2.Count == 0 && rejected.Count == 0 == false,
                        "args=" + args2.Count + " 挡下=" + rejected.Count);
                    maa.Text = was;
                    Chk("全部回到原值后不再下发任何参数", ChangedPathArgs(null).Count == 0, "");
                }
                else Chk("这棵树上没有 maaDir 这一项(单游戏仓正常)，跳过第 4 段", true, "");

                // ---- 5) 定时页：面板读到的时间点 == 判定层转达的行数；不像时间点的字符串不落地 ----
                string[] times = ReadTimes();
                List<Row> trows; timesByPage.TryGetValue("all", out trows);
                int tn = trows == null ? 0 : trows.Count;
                Chk("时间点读到了(" + times.Length + " 个)并且和 TIME 行对得上(" + tn + " 行)",
                    times.Length == 0 || tn == 0 || times.Length == tn, times.Length + " vs " + tn);
                TextBox tb;
                if (boxByKey.TryGetValue("times", out tb)) Chk("定时页框里是现值(逗号分隔)", tb.Text.Replace(" ", "") == string.Join(",", times), "[" + tb.Text + "]");

                // ---- 6) 历史页：铺的行数 = min(12, 历史条数)，带原因的行点得动 ----
                SelectPage("hist");
                int recs = 0;
                try { List<string[]> rr = ParseCsv(ReadAllShared(HistoryPath)); recs = rr.Count > 1 ? rr.Count - 1 : 0; } catch { }
                int expect = recs > 12 ? 12 : recs;
                Chk("历史页铺了 " + histRows + " 行，该是 " + expect, histRows == expect, "history.csv 共 " + recs + " 条");
                Label clickable = null, plain = null;
                foreach (Control c in histHost.Controls)
                {
                    Label l = c as Label; if (l == null) continue;
                    if (HasClick(l) && clickable == null) clickable = l;
                    if (!HasClick(l) && plain == null) plain = l;
                }
                if (clickable != null)
                {
                    int n0 = logLines.Count;
                    Chk("点一行带原因的，完整那句进了日志区", RaiseClick(clickable) && logLines.Count > n0, "日志行数 " + n0 + "→" + logLines.Count);
                }
                else Console.WriteLine("  (这份历史里没有带原因的行，跳过分身检查)");
                if (plain != null) Chk("没原因的行不装得能点", !HasClick(plain), "");
                // 行是"数格子"拼出来的(Pad 里中文算两格)，只有等宽字体才对得齐；
                // 这里量两行前三列的像素宽，差太多就说明换字体后列又歪了。
                if (clickable != null)
                {
                    int wArk = TextRenderer.MeasureText(MonoProbeA, MonoFont()).Width;
                    int wR = TextRenderer.MeasureText(MonoProbeB, MonoFont()).Width;
                    Chk("历史表前三列在不同行之间对得齐", Math.Abs(wArk - wR) <= 1, "字体[" + MonoFont().FontFamily.Name + "] 方舟行 " + wArk + "px · 1999 行 " + wR + "px");
                }

                // ---- 7) 诊断包：摘要里没有 key，包本身能生成 ----
                settingsPathOverride = null;                 // 抹除检查要拿真配置说事
                string key = ReadSettingString("visionApiKey");
                string digest = Path.Combine(Path.GetTempPath(), "gd-consoletest-digest-" + Process.GetCurrentProcess().Id + ".txt");
                WriteSettingsDigest(digest);
                string dtxt = File.ReadAllText(digest);
                if (key != null && key.Length > 0)
                {
                    Chk("配置摘要里没有那把视觉 Key", dtxt.IndexOf(key) < 0, "");
                    Chk("抹掉的是值，键名还在(不然看不出配过没配)", dtxt.IndexOf("visionApiKey") >= 0 && dtxt.IndexOf("<已抹掉>") >= 0, "");
                    int leaks = 0;
                    foreach (string[] e in logLines) if (e[1].IndexOf(key) >= 0) leaks++;
                    Chk("逛完四页，日志区里也没露出过 key", leaks == 0, "命中 " + leaks + " 行");
                }
                else Console.WriteLine("  (这台机器没配视觉 key，抹除检查只能看键名)");
                Chk("摘要里带着路径类读数(不然递给人也看不出什么)", dtxt.IndexOf("maaDir") >= 0 || dtxt.IndexOf("m9aDir") >= 0, "");
                string packDir = Path.Combine(Path.GetTempPath(), "gd-consoletest-pack-" + Process.GetCurrentProcess().Id);
                string zip = PackDiag(packDir, false, false);
                Chk("包打出来了", zip.Length > 0 && File.Exists(zip), "回报：" + (zip.Length > 0 ? zip : lastPackError));
                if (zip.Length > 0 && File.Exists(zip))
                {
                    Chk("包不是空的(至少几 KB)", new FileInfo(zip).Length > 2048, new FileInfo(zip).Length + " 字节");
                    if (key != null && key.Length > 0)
                    {
                        byte[] raw = File.ReadAllBytes(zip);
                        Chk("zip 里也搜不到那把 key(明文和转义两种都不在)",
                            IndexOfBytes(raw, Encoding.UTF8.GetBytes(key)) < 0 && IndexOfBytes(raw, Encoding.UTF8.GetBytes(JsonEscape(key))) < 0, "");
                    }
                    try { File.Delete(zip); } catch { }
                }
                try { Directory.Delete(packDir, true); } catch { }

                // ---- 8) 协议版本：读不懂的时候必须说出来，不能让格子静默变空 ----
                // 前四条问的是"这份真读数"，后面几条是把 protoVer 拨到对不上的位置，单问 ProtoMismatch()
                // 那句话长什么样(纯函数，不解析、不起子进程，所以对得上/对不上都不会污染上面的四页检查)。
                Chk("判定层这份读数带版本号，而且正好是这个面板认的那一版", protoVer == ProtoKnown,
                    "读到 v" + protoVer + "，面板认 v" + ProtoKnown);
                int keepProto = protoVer;
                protoVer = -1;
                Chk("没有版本行时说的是「它比这个面板旧」，不是沉默", ProtoMismatch().IndexOf("没有协议版本号") >= 0, ProtoMismatch());
                int newer = ProtoKnown + 7;
                protoVer = newer;
                Chk("判定层比面板新：两个版本号都报出来，并给出重编命令",
                    ProtoMismatch().IndexOf("v" + newer) >= 0 && ProtoMismatch().IndexOf("build.ps1") >= 0, ProtoMismatch());
                protoVer = 0;
                Chk("判定层比面板旧(但报了 v0)：说的是「比这个面板认的还旧」", ProtoMismatch().IndexOf("还旧") >= 0, ProtoMismatch());
                protoVer = keepProto;
                Chk("对得上时一个字都不说(别每 45 秒刷一条横幅盖住真读数)", ProtoMismatch().Length == 0, ProtoMismatch());
            }
            catch (Exception ex)
            {
                Console.WriteLine("  FAIL 自测自己炸了：" + ex.Message);
                if (ex.StackTrace != null)
                {
                    // 只点到第一处自家帧，别把整条 WinForms 栈刷下来
                    string[] fr = ex.StackTrace.Split('\n');
                    for (int i = 0; i < fr.Length && i < 6; i++) Console.WriteLine("      " + fr[i].Trim());
                }
                fail++;
            }
            finally
            {
                settingsPathOverride = keepSettings;
                historyPathOverride = keepHistory;
                try { if (File.Exists(tmpSettings)) File.Delete(tmpSettings); } catch { }
                try { if (File.Exists(Path.Combine(Path.GetTempPath(), "gd-consoletest-digest-" + Process.GetCurrentProcess().Id + ".txt")))
                        File.Delete(Path.Combine(Path.GetTempPath(), "gd-consoletest-digest-" + Process.GetCurrentProcess().Id + ".txt")); } catch { }
                if (autoBeat) beat.Start();
            }
            fail += chkFail;   // 把 Chk 的账并进来，红项数才和上面打印的一条条对得上
            Console.WriteLine("结论: " + (fail == 0 ? "PASS(四页都接对了)" : "FAIL —— 红 " + fail + " 项"));
            return fail == 0 ? 0 : 1;
        }

        // Chk = 断言 + 计数。以前只打印不计数，"结论 PASS"和退出码都盖不住这里的红项(2026-09-25 发现，已修)：
        // 任何一条 Chk(false) 都必须让 --consoletest 整体变红、退出码非 0。
        // 计数走自家字段(各测试方法里的 fail 是局部变量，方法够不着)，ConsoleTest 在收尾时把它并进 fail。
        int chkFail;
        void Chk(string name, bool ok, string extra)
        {
            if (!ok) chkFail++;
            Console.WriteLine((ok ? "  PASS  " : "  FAIL  ") + name + (extra.Length > 0 && !ok ? "　[" + extra + "]" : (extra.Length > 0 ? "　(" + extra + ")" : "")));
        }

        static string ShortMsg(string m)
        {
            if (m.Length == 0) return "无告警";
            return m.Length > 24 ? m.Substring(0, 24) + "…" : m;
        }

        static int CountSwitches(List<string> args, string sw)
        {
            int n = 0;
            foreach (string a in args) if (a == sw) n++;
            return n;
        }

        static bool SameBytes(byte[] a, byte[] b)
        {
            if (a == null || b == null || a.Length != b.Length) return false;
            for (int i = 0; i < a.Length; i++) if (a[i] != b[i]) return false;
            return true;
        }

        static int IndexOfBytes(byte[] hay, byte[] needle)
        {
            if (hay == null || needle == null || needle.Length == 0 || hay.Length < needle.Length) return -1;
            for (int i = 0; i + needle.Length <= hay.Length; i++)
            {
                bool hit = true;
                for (int j = 0; j < needle.Length; j++) if (hay[i + j] != needle[j]) { hit = false; break; }
                if (hit) return i;
            }
            return -1;
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
            sb.Append("PROTO|1").Append('\n');
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
            bool selftest = false, headless = false, dump = false, exptest = false, exttest = false, whytest = false, consoletest = false;
            foreach (string a in argv)
            {
                string l = a.ToLowerInvariant();
                if (l == "--selftest") selftest = true;
                if (l == "--headless") headless = true;
                if (l == "--dump") dump = true;
                if (l == "--exptest") exptest = true;
                if (l == "--exttest") exttest = true;
                if (l == "--whytest") whytest = true;
                if (l == "--consoletest") consoletest = true;
                if (l == "--no-beat") PanelForm.AutoBeatDefault = false;
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            PanelForm f = new PanelForm(headless);
            if (exptest) Environment.Exit(f.ExpTest());
            if (exttest) Environment.Exit(f.ExtTest());
            if (whytest) Environment.Exit(f.WhyTest());
            if (consoletest) Environment.Exit(f.ConsoleTest());
            if (dump) { f.DumpTree(); return; }
            if (selftest) Environment.Exit(f.SelfTest());
            if (headless) Environment.Exit(f.Headless());
            Application.Run(f);
            Console.WriteLine("面板已关闭。定时任务不受影响，到点照样自己跑。");
        }
    }
}
