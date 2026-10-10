// tools/panel-cs/Live.cs —— B-2b(2026-10-08)：视觉兜底的**现场件**七枚 verb(切分与理由见 docs\rust-core-plan.md §11.34)。
//
// 这一份管"必须碰这台机器"的那一半：user32 拍窗口/点窗口/抢前台、枚举进程找端游、System.Drawing 算亮度签名、
// 起 Python 子进程问模型。Rust 核(B-2b③)只做编排与落盘，Windows 交互与 key 一律待在这一侧 —— 宿主 A 形态的落点，
// 也是 §11.34 ① 那条"P/Invoke 真身仍一份"的兑现处：源文件仍是 tools\gdinterop\GdWin.cs，由 build.ps1 多带一条
// 路径编进同一份装配，这里不抄第二份签名(改那份 .cs 从此要跑两套装配：PS 现编门 t14 与 panel-cs 编译)。
//
// 单一事实源：话术逐字抄自 lib\vision-ark.ps1 与 lib\vision-rescue.ps1 的真身(那两个文件在生产接线 B-2c 之前
// 仍是老编排的事实源)。改一句文案 = 老路新路各说各话，先跑 tools\probe-arkrun-rust-golden.ps1 与 tools\test-ark.ps1。
// "截图失败：" 的**双前缀**是原样怪癖(Save-GdWindowShot 的 catch 已经带过一次，Invoke-GdVisionRescue 又包一次)：
// 本文件把包装做在 Skip 里，正是要让那两次叠加照旧发生 —— 别"顺手修好"。
//
// 安全边界(照 lib\vision-rescue.ps1 文件头执行，一条不松)：
//   * API key 只在本文件内部取值(env DASHSCOPE_API_KEY → settings.runtime.visionApiKey，都要 Trim)，
//     **只写进子进程的 EnvironmentVariables**：不进命令行、不进请求文件、不进日志、不进信封、不进核；
//   * hasKey / pythonExe / noSpawn / fakeOut 这几枚是**自检与对拍夹具的注入旋钮**(身份同 Cli.cs 的 --garbage)：
//     真跑一侧不许送 hasKey=true —— 那会让子进程拿到一个假 key，请求必然 401。缺键才现读，所以默认路径不碰夹具。
//
// 与旧实现的两处已知形态差(都不是判据，如实记在这)：
//   * Get-GdPythonExe 把找到的路径 .ToLowerInvariant() 之后再返回，这里保留原样大小写(Windows 不分大小写)；
//   * 请求文件从 Write-GdJson 的缩进变成 GdJson.Write 的单行 —— 这文件只有 tools\vision\ask.py 读(它按 utf-8-sig
//     解析 JSON)，人眼回看的那本账是 state\vision\calls.jsonl，没受影响。
//
// 批次纪律：--livetest 只钉纯规则段(尺寸闸分支/闸门话术/请求装配/内存位图跑 stats/stdout 回扫)；
// 真窗口与真 HTTP 不进门(只读自检不许要求机器现场状态)，那一轮实跑另留。

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Globalization;
using System.IO;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading.Tasks;

namespace GdPanel
{
    static class Live
    {
        // ==================== 入参取数：缺键=自己现读(现场)，有键=用夹具 ====================

        static Dictionary<string, object> View(Dictionary<string, object> raw)
        {
            Dictionary<string, object> d = Gd.Ci(raw);
            if (d == null) d = new Dictionary<string, object>(StringComparer.OrdinalIgnoreCase);
            return d;
        }

        static bool Given(Dictionary<string, object> c, string k)
        {
            object v;
            return c.TryGetValue(k, out v) && v != null;
        }

        // 默认值用 Func 给：现场读(注册表之外的盘/settings/探测子进程)只在该读时才读，
        // 否则自检喂了夹具也照样把 settings.json 与 python 探测走一遍 —— 那是"要求机器现场状态"。
        static string OptStr(Dictionary<string, object> c, string k, Func<string> def)
        {
            object v;
            if (c.TryGetValue(k, out v) && v != null) return Gd.Str(v);
            return def == null ? "" : def();
        }

        static bool OptFlag(Dictionary<string, object> c, string k, Func<bool> def)
        {
            object v;
            if (c.TryGetValue(k, out v)) return Gd.Truthy(v);
            return def == null ? false : def();
        }

        static long OptLng(Dictionary<string, object> c, string k, long def)
        {
            object v;
            if (!c.TryGetValue(k, out v) || v == null) return def;
            try { return Convert.ToInt64(v, CultureInfo.InvariantCulture); } catch { return def; }
        }

        static Dictionary<string, object> Sub(params object[] kv)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            for (int i = 0; i + 1 < kv.Length; i += 2) d[(string)kv[i]] = kv[i + 1];
            return d;
        }

        static long Num(object v)
        {
            if (v == null) return 0;
            try { return Convert.ToInt64(v, CultureInfo.InvariantCulture); } catch { return 0; }
        }

        static string StrOf(object v, string def) { return v == null ? def : Gd.Str(v); }

        // ==================== 共同的小现场件：settings / key / python / 管理员 ====================

        static Dictionary<string, object> settingsCache;
        static string settingsTextCache;
        static string pythonCache;
        static bool pythonProbed;

        static Dictionary<string, object> Settings()
        {
            if (settingsCache != null) return settingsCache;
            List<string> missing;
            // requirePaths 给空表：现场 verb 不催助手目录(那是 Stop-GdBeforeRun 的事)，只要读得出 runtime 那几格。
            settingsCache = GdSettings.Load(out missing, true, new List<string>());
            return settingsCache;
        }

        static object Runtime(string key) { return Gd.Get(Gd.Get(Settings(), "runtime"), key); }

        static string SettingsText()
        {
            if (settingsTextCache != null) return settingsTextCache;
            try { settingsTextCache = File.ReadAllText(Path.Combine(GdEnv.Root, @"config\settings.json")); }
            catch { settingsTextCache = ""; }
            return settingsTextCache;
        }

        // 原话逐字：lib\vision-rescue.ps1 的 Get-GdVisionApiKey(环境变量优先，其次 settings，都要 Trim)。
        // 返回值只在进程内递给子进程环境变量，永不进信封、永不进日志、永不回给核。
        static string VisionKey()
        {
            string k = Environment.GetEnvironmentVariable("DASHSCOPE_API_KEY");
            if (k == null || k.Length == 0) k = StrOf(Runtime("visionApiKey"), "");
            if (k.Length == 0) return "";
            return k.Trim();
        }

        static bool IsAdminNow()
        {
            // Test-GdAdmin 的形状：读不到就当不是(宁可这一轮不救，也不硬点)。
            try
            {
                WindowsIdentity id = WindowsIdentity.GetCurrent();
                return new WindowsPrincipal(id).IsInRole(WindowsBuiltInRole.Administrator);
            }
            catch { return false; }
        }

        // Get-GdPythonExe 的同一条顺序(见 Agent.cs) + 同一份"只起一次"的缓存(没找到也要缓存，
        // 否则每次问都白起一遍探测进程)。
        static string PythonExe()
        {
            if (pythonProbed) return pythonCache;
            pythonCache = GdAgentLink.FindPython(SettingsText(),
                Environment.GetEnvironmentVariable("GD_PYTHON"), GdEnv.Root,
                Environment.GetEnvironmentVariable("PATH"), GdAgentLink.PythonUsable);
            pythonProbed = true;
            return pythonCache;
        }

        // ==================== 端游窗口(原 Get-GdArkWindow) ====================

        sealed class ArkWin
        {
            public bool Found;
            public IntPtr Handle;
            public long ProcId;
            public string Title = "";
        }

        // 用进程名匹配，不用窗口标题(标题会随版本变，进程名跟着 settings 的 paths.arknightsExe 走) —— 逐字照旧实现。
        static string ArkProcName()
        {
            string s = StrOf(Gd.Get(Gd.Get(Settings(), "paths"), "arknightsExe"), "");
            if (s.Length == 0) return "";
            try { return Path.GetFileNameWithoutExtension(s); } catch { return ""; }
        }

        static ArkWin FindArk(string procName)
        {
            ArkWin none = new ArkWin();
            if (string.IsNullOrEmpty(procName)) return none;
            foreach (Process p in Process.GetProcesses())
            {
                try
                {
                    if (p.MainWindowHandle == IntPtr.Zero) continue;
                    // PS 的 -eq 比字符串不分大小写，这里同形
                    if (!string.Equals(p.ProcessName, procName, StringComparison.OrdinalIgnoreCase)) continue;
                    ArkWin w = new ArkWin();
                    w.Found = true;
                    w.Handle = p.MainWindowHandle;
                    w.ProcId = p.Id;
                    w.Title = p.MainWindowTitle == null ? "" : p.MainWindowTitle;
                    return w;
                }
                catch { }     // 枚举期间进程退了：跳过这一个(旧实现 Get-Process -ErrorAction SilentlyContinue 同形)
                finally { try { p.Dispose(); } catch { } }
            }
            return none;
        }

        static string ProcNameOf(Dictionary<string, object> c)
        {
            return OptStr(c, "procName", delegate { return ArkProcName(); });
        }

        static ArkWin WindowFrom(Dictionary<string, object> c)
        {
            long hv = OptLng(c, "handle", 0);
            if (hv == 0) return FindArk(ProcNameOf(c));
            ArkWin w = new ArkWin();
            w.Found = true;
            w.Handle = new IntPtr(hv);
            return w;
        }

        // verb: ark-find-window —— 只回答"端游窗口在不在、句柄是多少"，不动它也不点它。
        internal static object ArkFindWindow(Dictionary<string, object> raw)
        {
            Dictionary<string, object> c = View(raw);
            string name = ProcNameOf(c);
            ArkWin w = FindArk(name);
            Dictionary<string, object> data = new Dictionary<string, object>();
            data["Found"] = w.Found;
            data["Name"] = name;
            data["Handle"] = w.Found ? w.Handle.ToInt64() : 0L;
            data["ProcId"] = w.Found ? w.ProcId : 0L;
            data["Title"] = w.Title;
            return data;
        }

        // ==================== verb: ark-desktop-state(原 Get-GdArkDesktopState 的"现场量"那半) ====================
        // 拼话术的规矩仍在 Vision.cs(compose-ark-desktop-state)，这一枚只补"那几个量从哪来"：
        // 输入桌面名 / 前台归属 / 最小化 / 可见 / 窗口矩形 / 光标。没句柄就别碰窗口调用 —— 旧实现也只在有窗口时
        // 才读那三项，"没读"与"读到 false"在话术里不是一回事。

        internal static object ArkDesktopState(Dictionary<string, object> raw)
        {
            Dictionary<string, object> c = View(raw);
            long hv = OptLng(c, "handle", 0);
            bool hwndFound = hv != 0;
            IntPtr hwnd = new IntPtr(hv);

            string desk = OptStr(c, "desk", delegate { return GdWin.InputDesktopName(); });
            bool onFg;
            string fgProcess;
            if (Given(c, "onFg"))
            {
                onFg = OptFlag(c, "onFg", null);
                fgProcess = OptStr(c, "fgProcess", null);
            }
            else
            {
                IntPtr fg = GdWin.ForegroundWindow();
                onFg = hwndFound && fg != IntPtr.Zero && fg.ToInt64() == hv;
                fgProcess = "";
                try
                {
                    uint pid = GdWin.WindowPid(fg);
                    if (pid > 0)
                    {
                        Process pf = Process.GetProcessById((int)pid);
                        try { fgProcess = pf.ProcessName == null ? "" : pf.ProcessName; }
                        finally { try { pf.Dispose(); } catch { } }
                    }
                }
                catch { fgProcess = ""; }     // 前台进程已退/查不到：PS 那边 SilentlyContinue 出来就是空串
            }
            string cursor = OptStr(c, "cursor", delegate { return GdWin.CursorText(); });
            bool iconic = OptFlag(c, "iconic", delegate { return hwndFound && GdWin.IsIconic(hwnd); });
            bool visible = OptFlag(c, "visible", delegate { return hwndFound && GdWin.Visible(hwnd); });
            string rectText = OptStr(c, "rectText", delegate { return hwndFound ? GdWin.WindowRectText(hwnd) : ""; });

            Dictionary<string, object> v = Vision.DesktopState(Sub(
                "desk", desk, "hwndFound", hwndFound, "onFg", onFg, "fgProcess", fgProcess,
                "iconic", iconic, "visible", visible, "rectText", rectText, "cursor", cursor)) as Dictionary<string, object>;

            Dictionary<string, object> data = new Dictionary<string, object>();
            data["Desktop"] = desk;
            data["Locked"] = v == null ? null : v["Locked"];      // null=读不到桌面名("不知道")，不是"锁屏了"
            data["LockedKnown"] = v != null && Gd.Truthy(v["LockedKnown"]);
            data["GameOnFg"] = onFg;
            data["FgProcess"] = fgProcess;
            data["Cursor"] = cursor;
            data["Detail"] = v == null ? "" : v["Detail"];
            return data;
        }

        // ==================== verb: ark-grab-foreground(原 Invoke-GdArkForegroundGrab) ====================
        // ALT hack 抢前台：只动窗口状态、不点客户区，所以不会误点游戏里的按钮。抢不到照实回 false。
        // Detail 那两句("已把端游窗口抢到前台(标题) —— …"/"抢前台没成功 —— …")是 test-ark 的锚，逐字不许改。

        internal static object ArkGrab(Dictionary<string, object> raw)
        {
            Dictionary<string, object> c = View(raw);
            ArkWin w = FindArk(ProcNameOf(c));
            Dictionary<string, object> data = new Dictionary<string, object>();
            if (!w.Found)
            {
                data["Ok"] = false;
                data["Handle"] = 0L;
                data["Detail"] = "端游窗口没找到，抢前台无从谈起";
                return data;
            }
            bool ok = GdWin.TryForeground(w.Handle);
            Dictionary<string, object> st = ArkDesktopState(Sub("handle", w.Handle.ToInt64())) as Dictionary<string, object>;
            string detail = st == null ? "" : Gd.Str(st["Detail"]);
            data["Ok"] = ok;
            data["Handle"] = w.Handle.ToInt64();
            data["Detail"] = ok
                ? "已把端游窗口抢到前台(" + w.Title + ") —— " + detail
                : "抢前台没成功 —— " + detail;
            return data;
        }

        // ==================== verb: ark-window-shot(原 Save-GdWindowShot + ark 靶子的 $shot 闭包) ====================

        // 16x9=144 格粗亮度签名 + 平均亮度：逐字照 lib\vision-rescue.ps1 的 Get-GdShotStats
        // (Step 7、R*30+G*59+B*11 除 100 后取整、gy 封顶 15 / gx 封顶 8、格均值再 /8 量化)。
        // 两处 [int] 与两处 Round 在 PS 里都是**ToEven(银行家舍入)**，不是截断 —— §11.34 ⑦-1 那条地雷，
        // 夹具因此专门喂 AvgLum 7.5/7.4/8.0 三档。AvgLum 落成 long：PS 的 [math]::Round 给的是整值 double，
        // 插进人话就是"8"这种写法，两侧读出来同一个词。
        internal static Dictionary<string, object> Stats(Bitmap bmp, int w, int h, int step)
        {
            long sum = 0L;
            int n = 0;
            int[] acc = new int[144];
            int[] cnt = new int[144];
            for (int y = 0; y < h; y += step)
            {
                for (int x = 0; x < w; x += step)
                {
                    Color px = bmp.GetPixel(x, y);
                    int l = Convert.ToInt32((px.R * 30 + px.G * 59 + px.B * 11) / 100.0);
                    sum += l;
                    n++;
                    int gy = Convert.ToInt32(y / (double)h * 16); if (gy > 15) gy = 15;
                    int gx = Convert.ToInt32(x / (double)w * 9); if (gx > 8) gx = 8;
                    int k = gy * 9 + gx;
                    acc[k] += l;
                    cnt[k]++;
                }
            }
            StringBuilder sig = new StringBuilder();
            for (int k = 0; k < 144; k++)
            {
                if (k > 0) sig.Append(',');
                double cell = cnt[k] > 0 ? (double)acc[k] / cnt[k] / 8.0 : 0.0;
                sig.Append(Convert.ToInt32(Math.Round(cell, MidpointRounding.ToEven), CultureInfo.InvariantCulture));
            }
            return Sub(
                "AvgLum", Convert.ToInt64(Math.Round(sum / (double)Math.Max(1, n), MidpointRounding.ToEven), CultureInfo.InvariantCulture),
                "Sig", sig.ToString());
        }

        // 尺寸闸的两种话术(旧实现按"恢复之后还在最小化吗"分开报，别合成一句)。
        internal static string ShotSizeWhy(int w, int h, bool iconic)
        {
            if (w >= 200 && h >= 150) return "";
            return iconic ? "端游窗口仍最小化(拍不到东西) " + w + "x" + h
                          : "窗口尺寸异常 " + w + "x" + h;
        }

        static Dictionary<string, object> ShotFail(string why)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            d["Ok"] = false;
            d["Why"] = why;
            d["CanContinue"] = false;
            // 旧实现这一层的原话是 Invoke-GdVisionRescue 里的 "截图失败：$($before.Why)"；而 catch 那条的 Why
            // 自己就叫 "截图失败：…" —— 于是异常路径上印两次前缀。原样怪癖，别修。
            d["Skip"] = "截图失败：" + why;
            d["LogLine"] = "";
            return d;
        }

        // 黑屏闸的话术是判据(阈值为什么是 8 见 vision-rescue 头注)，留在这里；包装见 ShotFail 那条注释。
        // 单独抽成一枚 internal：--livetest 不碰真窗口，够不着 WindowShot，这句就得靠它自己钉住。
        internal static string BlackScreenSkip(long lum)
        {
            return "截图几乎是全黑(平均亮度 " + lum + ")：桌面可能锁了或窗口没在渲染，问模型只会白花钱";
        }

        internal static object WindowShot(Dictionary<string, object> raw)
        {
            Dictionary<string, object> c = View(raw);
            string path = OptStr(c, "path", null);
            if (path.Length == 0) throw new GdRuleException("ark-window-shot：没给 path(截图要落盘的路径属现场量，由编排侧拼 ark-<Tag>-时间-before.png 送进来)");
            string name = OptStr(c, "name", delegate { return "端游窗口"; });
            ArkWin w = WindowFrom(c);
            if (!w.Found) return ShotFail("端游窗口没找到(可能已经关了)");

            IntPtr hwnd = w.Handle;
            Bitmap bmp = null;
            Graphics g = null;
            try
            {
                if (GdWin.IsIconic(hwnd))
                {
                    GdLog.Line("端游窗口最小化中 → 先 SW_RESTORE 叫回来再拍", "WARN");
                    GdWin.Restore(hwnd);
                    System.Threading.Thread.Sleep(1500);       // 给它一帧时间重绘，否则拍到的还是空位图
                }
                GdRect wr = GdWin.WindowRect(hwnd);
                int cw = wr.Right - wr.Left, ch = wr.Bottom - wr.Top;
                string sizeWhy = ShotSizeWhy(cw, ch, GdWin.IsIconic(hwnd));
                if (sizeWhy.Length > 0) return ShotFail(sizeWhy);

                bmp = new Bitmap(cw, ch, PixelFormat.Format32bppArgb);
                g = Graphics.FromImage(bmp);
                IntPtr hdc = g.GetHdc();
                // 2 = PW_RENDERFULLCONTENT：Win10 之后带 DirectComposition/硬件加速的窗口不加这个会拍成黑屏
                bool ok = GdWin.PrintWindow(hwnd, hdc, 2);
                if (!ok) ok = GdWin.PrintWindow(hwnd, hdc, 0);   // 老一点的窗口过程只认 0，再试一次
                g.ReleaseHdc(hdc);
                if (!ok) return ShotFail("PrintWindow 返回 false(权限不够或窗口已关闭)");

                Dictionary<string, object> stats = Stats(bmp, cw, ch, 7);
                string dir = Path.GetDirectoryName(path);
                if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir)) Directory.CreateDirectory(dir);
                bmp.Save(path, ImageFormat.Png);
                GdPoint co = GdWin.ClientOrigin(hwnd);
                GdRect cr = GdWin.ClientRect(hwnd);
                long lum = Convert.ToInt64(stats["AvgLum"], CultureInfo.InvariantCulture);

                Dictionary<string, object> data = new Dictionary<string, object>();
                data["Ok"] = true;
                data["Path"] = path;
                data["ImgW"] = cw;
                data["ImgH"] = ch;
                data["ClientLeft"] = co.X - wr.Left;
                data["ClientTop"] = co.Y - wr.Top;
                data["ClientW"] = cr.Right;
                data["ClientH"] = cr.Bottom;
                data["AvgLum"] = lum;
                data["Sig"] = stats["Sig"];
                data["Hash"] = Md5File(path);
                data["Why"] = "";
                data["CanContinue"] = lum >= 8;
                data["Skip"] = lum >= 8 ? "" : BlackScreenSkip(lum);
                data["LogLine"] = "视觉兜底：已拍" + name + "(" + cw + "x" + ch + "，亮度 " + lum + ")，交给模型判断";
                return data;
            }
            catch (Exception ex) { return ShotFail("截图失败：" + ex.Message); }
            finally
            {
                if (g != null) { try { g.Dispose(); } catch { } }
                if (bmp != null) { try { bmp.Dispose(); } catch { } }
            }
        }

        // Get-FileHash -Algorithm MD5 给大写十六进制。
        static string Md5File(string path)
        {
            using (MD5 m = MD5.Create())
            using (FileStream fs = File.OpenRead(path))
            {
                byte[] hash = m.ComputeHash(fs);
                StringBuilder sb = new StringBuilder();
                foreach (byte b in hash) sb.Append(b.ToString("X2", CultureInfo.InvariantCulture));
                return sb.ToString();
            }
        }

        // ==================== verb: ark-window-click(原 Send-GdArkClick + ark 靶子的 $click 闭包) ====================
        // 默认"抢前台 + 真实鼠标"，抢不到才退回 PostMessage —— 宁可点不动，也不能把真实点击漏进用户正在用的窗口。
        // 为什么不只靠 PostMessage、也别把它说成"MAA 也这么点"：两次实测都写在 lib\vision-ark.ps1 那段注释里。

        internal static object WindowClick(Dictionary<string, object> raw)
        {
            Dictionary<string, object> c = View(raw);
            int x = Convert.ToInt32(OptLng(c, "clientX", 0), CultureInfo.InvariantCulture);
            int y = Convert.ToInt32(OptLng(c, "clientY", 0), CultureInfo.InvariantCulture);
            string name = OptStr(c, "name", delegate { return "端游窗口"; });
            string what = OptStr(c, "what", null);
            string why = OptStr(c, "why", null);

            Dictionary<string, object> data = new Dictionary<string, object>();
            data["ClientX"] = x;
            data["ClientY"] = y;
            ArkWin w = WindowFrom(c);
            if (!w.Found)
            {
                data["Mode"] = "端游窗口已消失";
                data["Sent"] = false;
                data["TryPiece"] = "";
                data["LogLine"] = "";
                return data;
            }
            IntPtr hwnd = w.Handle;
            string mode;
            bool sent;
            if (GdWin.TryForeground(hwnd))
            {
                GdPoint pt = GdWin.ToScreen(hwnd, x, y);
                GdPoint old = GdWin.CursorPos();
                bool real = GdWin.RealClick(pt.X, pt.Y);
                System.Threading.Thread.Sleep(200);
                GdWin.SetCursorPos(old.X, old.Y);              // 鼠标放回原处，别留在游戏窗口里
                if (real) { mode = "真实点击"; sent = true; }
                else { mode = "PostMessage(没抢到前台，很可能点不动)"; sent = GdWin.ClickOnce(hwnd, x, y); }
            }
            else
            {
                mode = "PostMessage(没抢到前台，很可能点不动)";
                sent = GdWin.ClickOnce(hwnd, x, y);
            }
            data["Mode"] = mode;
            data["Sent"] = sent;
            // 旧实现在这里先打一行 WARN 日志、再把 "点位 X,Y(Mode)" 累进 tries 数组；两样都在这一枚出，
            // 编排侧只做"试完了用什么句子收尾"。
            data["TryPiece"] = "点位 " + x + "," + y + "(" + mode + ")";
            data["LogLine"] = "视觉兜底：" + what + " → 点" + name + " " + x + "," + y + "(" + why + ")";
            return data;
        }

        // ==================== verb: resolve-rescue-context(七道闸里那六道现场闸) ====================
        // 闸门顺序不许调换(lib\vision-rescue.ps1 的 Invoke-GdVisionRescue 头注明写)：
        // 后端在场 → 开关 → 权限 → key → Python → 对象在场。第七道"截图"与黑屏闸在 ark-window-shot 里，
        // 因为那一道必须先拍到东西才判得了。这一枚一个像素都不拍、也不起 python 探测以外的子进程。
        // skip 话术全部出自这里，编排侧一个字不改 —— 关开关那句必须带 settings 的键名，t13 按那几个字断言"关了确实一次都不试"。

        internal static object RescueContext(Dictionary<string, object> raw)
        {
            Dictionary<string, object> c = View(raw);
            string target = OptStr(c, "target", delegate { return "ark"; });
            string kind = OptStr(c, "kind", delegate { return target; }).ToLowerInvariant();  // New-GdVisionTarget-$($Kind.ToLowerInvariant())
            bool known = kind == "ark";

            Dictionary<string, object> data = new Dictionary<string, object>();
            data["Target"] = target;
            data["Kind"] = kind;
            data["Name"] = OptStr(c, "name", delegate { return known ? "端游窗口" : ""; });
            data["NeedsAdmin"] = OptFlag(c, "needsAdmin", delegate { return known; });
            data["AdminWhy"] = OptStr(c, "adminWhy", delegate { return known ? "端游是 requireAdministrator，这个权限下点它不会有任何反应" : ""; });
            data["Scene"] = OptStr(c, "scene", null);
            data["UserText"] = OptStr(c, "userText", null);
            data["Enabled"] = OptFlag(c, "enabled", delegate { return known ? ArkVisionOn() : true; });

            if (!OptFlag(c, "hasBuilder", delegate { return known; }))
                return No(data, "这棵树上没有 " + target + " 这一路的视觉兜底后端(没有 New-GdVisionTarget-" + target + " 这个函数)");
            if (!Gd.Truthy(data["Enabled"]))
                return No(data, OptStr(c, "why", delegate { return known ? "arkVisionRescue=false，视觉兜底已关闭" : ""; }));
            if (Gd.Truthy(data["NeedsAdmin"]) && !OptFlag(c, "isAdmin", delegate { return IsAdminNow(); }))
            {
                string why = Gd.Str(data["AdminWhy"]);
                if (why.Length == 0) why = "这一路的对象要管理员权限，这个权限下点它不会有任何反应";
                return No(data, "当前不是管理员权限：" + why + "，跳过兜底");
            }
            if (!OptFlag(c, "hasKey", delegate { return VisionKey().Length > 0; }))
                return No(data, "没配视觉模型 key(见 settings 里的 runtime.visionApiKey)，本轮不尝试兜底");
            string py = OptStr(c, "pythonExe", delegate { return PythonExe(); });
            data["HasPython"] = py.Length > 0;
            if (py.Length == 0)
                return No(data, "这台机器上没有可用的 Python 3(问模型那一层用它跑)：跑 setup.ps1 -WithPython 装一份到 state\\python\\，或把 runtime.pythonExe 指到已有的那份");

            bool alive = OptFlag(c, "alive", delegate { return known ? FindArk(ProcNameOf(c)).Found : true; });
            if (!alive) return No(data, Gd.Str(data["Name"]) + "已经不在了，兜底没有对象");

            data["CanAsk"] = true;
            data["Skip"] = "";
            return data;
        }

        static Dictionary<string, object> No(Dictionary<string, object> data, string why)
        {
            data["CanAsk"] = false;
            data["Skip"] = why;
            return data;
        }

        // arkVisionRescue 的开关语义是"没写就是开着"(旧实现：$S.runtime.arkVisionRescue -ne $false) ——
        // 只有明写成 false 才关，缺键/null/true 都算开。
        static bool ArkVisionOn()
        {
            object v = Runtime("arkVisionRescue");
            if (v is bool) return (bool)v;
            return true;
        }

        // ==================== verb: vision-ask-model(原 Invoke-GdVisionRaw + Invoke-GdVisionAsk) ====================
        // 一次"问模型"的完整往返：装请求 → 起 Python → 回扫 stdout → 顺手抠出 verdict。
        // 起子进程与 key 都只在这一侧，Rust 核不许知道它们；HTTP 分类与 429/5xx 退避重试仍归 tools\vision\ask.py。
        // 分工没变的那一半：抠 JSON 的规则仍在 Vision.cs(parse-vision-verdict)，这里直接**同装配内调**它，不 verb 套 verb。

        // 用户那句话 + 处境。截 200 字的规矩是"这一句是给模型听的处境说明，不是日志转储"(旧注释原话)，
        // 拼接串里那两个空格也是原样("$UserText  自动化这边的处境：")。
        internal static string AskUserText(string userText, string context)
        {
            string t = userText == null ? "" : userText;
            if (t.Length == 0) t = "这是当前画面。按规则只回一个 JSON 对象。";
            string ctx = context == null ? "" : context;
            if (ctx.Length > 0)
            {
                ctx = Regex.Replace(ctx, @"\s+", " ");
                if (ctx.Length > 200) ctx = ctx.Substring(0, 200) + "…";
                t = t + "  自动化这边的处境：" + ctx;
            }
            return t;
        }

        // 请求文件里的九个字段：顺序照旧实现的 [ordered]@{}(那是本层唯一的协议)。key 不在里面，一个都不许进。
        internal static Dictionary<string, object> BuildRequest(string imagePath, string scene, string userText,
            string model, int sec, bool allowReward, string baseUrl, string reqPath, string auditPath)
        {
            Dictionary<string, object> req = new Dictionary<string, object>();
            req["image"] = imagePath;
            req["system_prompt"] = Vision.BuildPrompt(scene, allowReward);
            req["user_text"] = userText;
            Dictionary<string, object> ep = Vision.Endpoint(Sub("base", baseUrl)) as Dictionary<string, object>;
            req["endpoint"] = ep == null ? "" : Gd.Str(ep["Url"]);
            req["model"] = model;
            req["temperature"] = 0;                 // 兜底要的是可复现，不是发挥
            req["max_tokens"] = 300;
            req["timeout_seconds"] = sec;
            req["audit"] = auditPath;
            return req;
        }

        // 子进程回来以后那团读数的解读：Code=-1 是"整段没回来"(超时)，其余按旧实现的三段兜次说人话；
        // stdout 逐行挑哪个对象仍住在 Vision.cs(parse-vision-reply)，包括"解释器先打了版本警告"这一格实测坑。
        internal static Dictionary<string, object> MapChild(int code, string outText, string errText, string reqPath, string nowText)
        {
            Dictionary<string, object> data = new Dictionary<string, object>();
            data["RequestPath"] = reqPath;
            object parsed = null;
            Dictionary<string, object> pick = Vision.Reply(Sub("out", outText == null ? "" : outText)) as Dictionary<string, object>;
            if (pick != null && Gd.Truthy(pick["Found"])) parsed = Gd.Get(pick, "Parsed");

            if (parsed == null)
            {
                string why = "Python 没回 JSON(退出码 " + code + ")";
                if (code == -1) why = "问模型超时(Python 侧 " + nowText + " 前没回)";
                else
                {
                    string tail = (errText == null ? "" : errText).Trim();
                    if (tail.Length > 0) why += "：" + tail;
                    else
                    {
                        string o = (outText == null ? "" : outText).Trim();
                        if (o.Length > 0) why += "：" + o;
                    }
                }
                data["Ok"] = false;
                data["Error"] = why;
                return data;
            }
            Dictionary<string, object> p = parsed as Dictionary<string, object>;
            data["Ok"] = Gd.Truthy(Gd.Get(p, "ok"));
            data["Text"] = StrOf(Gd.Get(p, "text"), "");
            data["Model"] = StrOf(Gd.Get(p, "model"), "");
            data["HttpStatus"] = Num(Gd.Get(p, "http_status"));
            data["ElapsedMs"] = Num(Gd.Get(p, "elapsed_ms"));
            data["Attempts"] = Num(Gd.Get(p, "attempts"));
            data["Error"] = StrOf(Gd.Get(p, "error"), "");
            return data;
        }

        internal static object VisionAsk(Dictionary<string, object> raw)
        {
            Dictionary<string, object> c = View(raw);
            string imagePath = OptStr(c, "imagePath", null);
            if (imagePath.Length == 0) throw new GdRuleException("vision-ask-model：没给 imagePath(问模型必须有一张已经落盘的截图)");
            string scene = OptStr(c, "scene", null);
            string context = OptStr(c, "context", null);

            // 三道前置与旧实现同一个顺序：key → ask.py → Python。
            if (!OptFlag(c, "hasKey", delegate { return VisionKey().Length > 0; }))
                return AskFail("没有视觉模型的 API key(settings.runtime.visionApiKey 或环境变量 DASHSCOPE_API_KEY)", "");
            string script = OptStr(c, "scriptPath", delegate { return Path.Combine(GdEnv.Root, @"tools\vision\ask.py"); });
            if (!File.Exists(script)) return AskFail("缺少 tools\\vision\\ask.py(问模型这一层的实现)，这一棵树上问不了模型", "");
            string py = OptStr(c, "pythonExe", delegate { return PythonExe(); });
            if (py.Length == 0)
                return AskFail("没找到可用的 Python 3(问模型这一层用它跑)：跑 setup.ps1 -WithPython 自动装一份到 state\\python\\，或把 runtime.pythonExe 指到自己装的那份", "");

            string model = OptStr(c, "model", delegate { return StrOf(Runtime("visionModel"), "qwen3.8-flash"); });
            // 缺键才去读 settings(理由见 OptStr 那条：自检不许被 settings 拖着走)；给了就按给的走同一条钳。
            int sec = SecOf(Given(c, "timeoutSeconds") ? Gd.Get(c, "timeoutSeconds") : Runtime("visionTimeoutSeconds"));
            string userText = AskUserText(OptStr(c, "userText", null), context);
            string dir = OptStr(c, "askDir", delegate { return Path.Combine(GdEnv.Root, @"state\vision"); });
            string reqPath = Path.Combine(dir, "ask-" + DateTime.Now.ToString("MMdd-HHmmss-fff", CultureInfo.InvariantCulture) + ".json");
            string audit = Path.Combine(dir, "calls.jsonl");

            Dictionary<string, object> data;
            if (OptFlag(c, "noSpawn", delegate { return false; }))
            {
                // 夹具档：不起子进程，直接把"子进程会给回的东西"手递进来(自检与对拍门都走这一格)。
                data = MapChild((int)OptLng(c, "fakeCode", 0), OptStr(c, "fakeOut", null), OptStr(c, "fakeErr", null),
                    reqPath, DateTime.Now.ToString("HH:mm:ss", CultureInfo.InvariantCulture));
                data["Model"] = model;
                data["SkippedSpawn"] = true;
            }
            else
            {
                GdJson.Write(reqPath, BuildRequest(imagePath, scene, userText, model, sec,
                    OptFlag(c, "allowReward", delegate { return Gd.Truthy(Runtime("visionAllowReward")); }),
                    OptStr(c, "baseUrl", delegate { return StrOf(Runtime("visionBaseUrl"), ""); }), reqPath, audit));
                data = RunChild(py, script, reqPath, sec * 3 + 30, Given(c, "hasKey"));
                if (Gd.Truthy(data["Ok"])) TrimAskDir(dir, 12);   // 问成了才清(旧实现同形；calls.jsonl 那本账不删)
            }
            if (!Gd.Truthy(data["Ok"]))
            {
                if (!Given(data, "Error")) data["Error"] = "";
                data["Verdict"] = null;
                return data;
            }
            Dictionary<string, object> v = null;
            string text = StrOf(Gd.Get(data, "Text"), "");
            if (text.Trim().Length > 0) v = Vision.Verdict(Sub("text", text)) as Dictionary<string, object>;
            data["Verdict"] = v;              // 空文本/抠不出 JSON → null，与旧实现"返回 $null"同形
            return data;
        }

        static Dictionary<string, object> AskFail(string err, string reqPath)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            d["Ok"] = false;
            d["Error"] = err;
            d["RequestPath"] = reqPath;
            d["Verdict"] = null;
            return d;
        }

        static int SecOf(object v)
        {
            int sec;
            try { sec = Convert.ToInt32(v == null ? 0 : v, CultureInfo.InvariantCulture); } catch { sec = 0; }
            return sec <= 0 ? 45 : sec;
        }

        // Invoke-GdChild 的现场版：只给这一个子进程加 DASHSCOPE_API_KEY，其余环境照父进程；
        // 总超时 = 单次超时 ×(重试次数+1)+ 启动余量，超时把进程杀掉并回 Code=-1 —— 旧实现同形。
        static Dictionary<string, object> RunChild(string py, string script, string reqPath, int timeoutSeconds, bool injectedKey)
        {
            ProcessStartInfo psi = new ProcessStartInfo(py);
            // 路径里有空格(用户名带空格太常见了)必须自己加引号 —— Invoke-GdChild 那条理由在这儿一样成立。
            psi.Arguments = GdAgentLink.QuoteArg(script) + " --request " + GdAgentLink.QuoteArg(reqPath);
            psi.UseShellExecute = false;
            psi.CreateNoWindow = true;
            psi.RedirectStandardOutput = true;
            psi.RedirectStandardError = true;
            try { psi.WorkingDirectory = GdEnv.Root; } catch { }
            try { psi.EnvironmentVariables["DASHSCOPE_API_KEY"] = injectedKey ? "(injected)" : VisionKey(); } catch { }
            try
            {
                using (Process p = Process.Start(psi))
                {
                    Task<string> tOut = p.StandardOutput.ReadToEndAsync();
                    Task<string> tErr = p.StandardError.ReadToEndAsync();
                    if (!p.WaitForExit(timeoutSeconds * 1000))
                    {
                        try { p.Kill(); } catch { }
                        return MapChild(-1, "", "timeout after " + timeoutSeconds + "s", reqPath,
                            DateTime.Now.ToString("HH:mm:ss", CultureInfo.InvariantCulture));
                    }
                    int code = p.ExitCode;
                    string outText, errText;
                    try { outText = tOut.Result; } catch { outText = ""; }
                    try { errText = tErr.Result; } catch { errText = ""; }
                    return MapChild(code, outText, errText, reqPath, DateTime.Now.ToString("HH:mm:ss", CultureInfo.InvariantCulture));
                }
            }
            catch (Exception ex)
            {
                // 起进程本身就失败(解释器被杀软拦/路径没了)：旧实现是整层上抛、由兜底 catch 成"模型调用失败"。
                // verb 不抛(抛了就成了 exit 1 的内部错)，照实回一句原话给编排去包那一层。
                Dictionary<string, object> d = new Dictionary<string, object>();
                d["Ok"] = false;
                d["Error"] = ex.Message;
                d["RequestPath"] = reqPath;
                return d;
            }
        }

        // Clear-GdVisionFiles 的同款：按最后写入时间留最近 Keep 个。Pattern 只吃 ask-*.json，
        // calls.jsonl 不在里面(它是账本，一行一条请求，不随兜底清)。
        internal static int TrimAskDir(string dir, int keep)
        {
            if (string.IsNullOrEmpty(dir) || !Directory.Exists(dir)) return 0;
            List<FileInfo> files = new List<FileInfo>();
            foreach (string f in Directory.GetFiles(dir, "ask-*.json"))
            {
                try { files.Add(new FileInfo(f)); } catch { }
            }
            files.Sort(delegate(FileInfo a, FileInfo b) { return b.LastWriteTime.CompareTo(a.LastWriteTime); });
            int gone = 0;
            for (int i = keep; i < files.Count; i++)
            {
                try { files[i].Delete(); gone++; } catch { }
            }
            return gone;
        }
    }
}
