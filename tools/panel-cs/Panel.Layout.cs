// 版式：导航带 + 头部 + 四卡 + 动作区的坐标、无边框窗口的"标题栏行为"、卡片渲染。
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

    }
}