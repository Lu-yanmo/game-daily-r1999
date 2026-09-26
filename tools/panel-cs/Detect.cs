// tools/panel-cs/Detect.cs —— 批次 2：安装位置探测(detect) + 两个只读状态段落(队列行/实例行)。
//
// 单一事实源纪律：自本批起 lib\detect.ps1 的 Resolve-GdCandidate、lib\ark-status.ps1 的 Show-GdArkQueueState、
// lib\r1999-status.ps1 的 Show-GdR1999InstanceList 的**解析/探测本体**住这里，PS 侧只剩薄转发壳。
// 文案(Value/Source/行式)必须与 PS 原话逐字符一致 —— tools\probe-status-detect-golden.ps1 对拍，禁止放宽。
//
// 三个 verb 的输入形状(全是 --in JSON，值由 PS 壳组装)：
//   detect-installs:   { processNames[], exeNames[], nameHints[], uninstallLike[], pickProp,
//                        relExe, trimSegment, skipScan, roots[](可注入，空=本机固定盘) }
//     → data = { Found:false } 或 { Found:true, Value, Source }。
//     Candidates 不进信封：合体工程里唯一消费方 setup.ps1 只吃 .Value/.Source(批次 2 侦察确认)。
//   maa-queue-lines:   { cfgPath } → data = { Lines[] }   (MAA configuration.json → 展示行)
//   m9a-instance-lines: { dir, appFile } → data = { Lines[] } (M9A 实例目录 → 展示行)
//
// 与本机现场挂钩的部分怎么处理：
//   * 进程枚举/注册表/磁盘扫描是现场状态 —— 但老 PS 实现读的也是同一现场，双引擎对拍同机同时刻，
//     集合稳定即逐字相等；golden 的确定性场景一律用不存在的进程名/卸载表模式 + roots 注入假树。
//   * (Get-Date) 只出现在 running/uninstall 源的 LastWrite 上，而它不进输出文本(只影响排序，
//     同池内的相对先后由处理顺序决定，两侧同序) —— 所以不泄漏进对拍。
// 数字读回 Int32/Decimal 混发(批次 0 钉死)——一律 Gd.Lng/Gd.Int/Gd.Str，不裸强转。

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using Microsoft.Win32;

namespace GdPanel
{
    static class Detect
    {
        // 一个候选安装位置 —— 字段与旧 PS 的 pscustomobject 对齐(Dir/Exe/LastWrite/Junk/Via)。
        sealed class Cand
        {
            public string Dir = "";
            public string Exe = "";
            public DateTime LastWrite;
            public bool Junk;
            public string Via = "";
            public string Prop(string name) { return name == "Exe" ? Exe : Dir; }
        }

        // ==================== 路径过滤(逐字对齐 lib\detect.ps1 的两张表) ====================

        static readonly string[] SkipSegments = new string[] { "windows", "appdata", "programdata", "windowsapps", "$recycle.bin",
            "system volume information", "winsxs", "temp", "node_modules", ".git" };
        static readonly string[] JunkSegments = new string[] { "downloads", "desktop", "temp", "tmp", "inbox", "备份", "backup", "backups",
            "解压", "extract", "extracted" };

        // PS 的 -match "[\\/]seg[\\/]"(对整条路径小写后逐段查) → IgnoreCase 正则同形。
        static bool MatchSegment(string path, string[] segments)
        {
            foreach (string seg in segments)
                if (Regex.IsMatch(path, "[\\\\/]" + Regex.Escape(seg) + "[\\\\/]", RegexOptions.IgnoreCase)) return true;
            return false;
        }

        public static bool JunkPath(string path)
        {
            if (MatchSegment(path, JunkSegments)) return true;
            // "xx (2)\m9a.exe" 这类浏览器重命名的副本目录 —— 模式照抄(注意要求尾随分隔符，末尾就是 "(2)" 的路径不算)
            return Regex.IsMatch(path, "[\\\\/]\\(\\d+\\)[\\\\/]", RegexOptions.IgnoreCase);
        }

        public static bool SkipPath(string path) { return MatchSegment(path, SkipSegments); }

        // ==================== -like 通配 → 正则(全项目共用件：NameHints/DisplayName 匹配都是 -like 语义) ====================

        public static bool Like(string value, string pattern)
        {
            StringBuilder sb = new StringBuilder();
            foreach (char c in pattern)
            {
                if (c == '*') sb.Append(".*");
                else if (c == '?') sb.Append('.');
                else if (Array.IndexOf("[!]".ToCharArray(), c) >= 0) sb.Append('\\').Append(c);   // PS 里 [] 是字符类语法，这里按字面处理：实测数据无此用法
                else sb.Append(Regex.Escape(c.ToString()));
            }
            return Regex.IsMatch(value ?? "", "^" + sb + "$", RegexOptions.IgnoreCase);
        }

        // ==================== 候选来源 1：正在跑的进程(对齐 Get-GdRunningExeDir) ====================

        static List<Cand> RunningExeDirs(object[] processNames, string[] exeNames)
        {
            List<Cand> outList = new List<Cand>();
            foreach (object pnObj in processNames)
            {
                string pn = Gd.Str(pnObj);
                List<string> paths = new List<string>();
                bool failed = false;
                // PS 的 try 包住整条 `Get-Process | ForEach { $_.Path }`：一个受保护进程读不到 Path
                // 就把**这一进程名的整组**丢掉(catch{ $paths=@() })。这边逐进程读，出错即清空整组 —— 同语义。
                try
                {
                    foreach (System.Diagnostics.Process p in System.Diagnostics.Process.GetProcessesByName(pn))
                    {
                        string path;
                        try { path = p.MainModule != null ? p.MainModule.FileName : null; }
                        catch { failed = true; break; }
                        finally { try { p.Dispose(); } catch { } }
                        if (!string.IsNullOrEmpty(path)) paths.Add(path);
                    }
                }
                catch { failed = true; }
                if (failed) paths = new List<string>();
                foreach (string p in paths.Distinct(StringComparer.OrdinalIgnoreCase))
                {
                    string leaf = Path.GetFileName(p);
                    if (exeNames.Any(e => string.Equals(e, leaf, StringComparison.OrdinalIgnoreCase)))
                        outList.Add(new Cand { Dir = Path.GetDirectoryName(p), Exe = p, LastWrite = DateTime.Now, Junk = false });
                }
            }
            return outList;
        }

        // ==================== 候选来源 2：卸载表(对齐 Get-GdUninstallHint + Get-GdUninstallHintDir) ====================

        sealed class Hint { public string Name; public string Prop; public string Value; }

        static List<Hint> UninstallHints(object[] displayNameLike)
        {
            List<Hint> outList = new List<Hint>();
            // 三条键路径照抄 PS：HKLM 64 位视图直路径 + 字面 WOW6432Node 路径 + HKCU。
            // (PS 的 HKLM:\SOFTWARE\... 在 64 位 powershell 下就是 64 视图；WOW 那条是字面路径同样在 64 视图下打开 —— 这里同构)
            KeyValue[] roots = new KeyValue[] {
                new KeyValue(RegistryHive.LocalMachine, @"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", RegistryView.Registry64),
                new KeyValue(RegistryHive.LocalMachine, @"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall", RegistryView.Registry64),
                new KeyValue(RegistryHive.CurrentUser, @"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", RegistryView.Default) };
            foreach (KeyValue rk in roots)
            {
                List<KeyValuePair<string, string>> items = new List<KeyValuePair<string, string>>();
                try
                {
                    using (RegistryKey baseKey = RegistryKey.OpenBaseKey(rk.Hive, rk.View))
                    using (RegistryKey parent = baseKey.OpenSubKey(rk.Path))
                    {
                        if (parent != null)
                            foreach (string sub in parent.GetSubKeyNames())
                            {
                                try
                                {
                                    using (RegistryKey k = parent.OpenSubKey(sub))
                                    {
                                        if (k == null) continue;
                                        items.Add(new KeyValuePair<string, string>(sub, AsStr(k.GetValue("DisplayName"))));
                                    }
                                }
                                catch { }
                            }
                    }
                }
                catch { }
                foreach (KeyValuePair<string, string> it in items)
                {
                    string dn = it.Value;
                    foreach (object patObj in displayNameLike)
                    {
                        if (!Like(dn, Gd.Str(patObj))) continue;
                        foreach (string prop in new string[] { "InstallLocation", "InstallSource", "DisplayIcon" })
                        {
                            string v = "";
                            try
                            {
                                using (RegistryKey k = OpenOne(rk, it.Key))
                                    v = AsStr(k == null ? null : k.GetValue(prop));
                            }
                            catch { v = ""; }
                            if (v.Length > 0) outList.Add(new Hint { Name = dn, Prop = prop, Value = v });
                        }
                    }
                }
            }
            return outList;
        }

        struct KeyValue
        {
            public RegistryHive Hive; public string Path; public RegistryView View;
            public KeyValue(RegistryHive h, string p, RegistryView v) { Hive = h; Path = p; View = v; }
        }

        static RegistryKey OpenOne(KeyValue rk, string subName)
        {
            using (RegistryKey baseKey = RegistryKey.OpenBaseKey(rk.Hive, rk.View))
            using (RegistryKey parent = baseKey.OpenSubKey(rk.Path))
                return parent == null ? null : parent.OpenSubKey(subName);
        }

        // PS 的 [string]$x：null→""；数字按不变文化；MultiString 数组 PS 会拼成 "a b" —— 逐字对齐(批次 2 侦察定死)。
        static string AsStr(object v)
        {
            if (v == null) return "";
            string[] multi = v as string[];
            if (multi != null) return string.Join(" ", multi);
            return Convert.ToString(v, CultureInfo.InvariantCulture);
        }

        static List<Cand> UninstallHintDirs(object[] displayNameLike, string[] exeNames)
        {
            List<Cand> found = new List<Cand>();
            foreach (Hint h in UninstallHints(displayNameLike))
            {
                // $v -replace '^[^,;]*,','' → 去掉"XXX,"前缀(DisplayIcon 常是 "路径\exe.exe,0")；再去引号、Trim
                string cand = Regex.Replace(h.Value, "^[^,;]*,", "").Replace("\"", "").Trim();
                if (cand.Length == 0) continue;
                string dir = File.Exists(cand) ? Path.GetDirectoryName(cand) : cand;
                foreach (string exe in exeNames)
                {
                    string full = Path.Combine(dir, exe);
                    if (File.Exists(full))
                        found.Add(new Cand { Dir = dir, Exe = full, LastWrite = DateTime.Now, Junk = false, Via = "卸载表[" + h.Name + "]" });
                    string nested = Path.Combine(Path.Combine(dir, "nx_main"), exe);
                    if (File.Exists(nested))
                        found.Add(new Cand { Dir = dir, Exe = nested, LastWrite = DateTime.Now, Junk = false, Via = "卸载表[" + h.Name + "]" });
                }
            }
            return found;
        }

        // ==================== 候选来源 3：磁盘浅层 BFS(对齐 Find-GdInstallDir) ====================

        static string[] ChildDirs(string path)
        {
            // 跳过软链/junction(否则会绕圈)，权限目录整条忽略 —— 与 PS 的 Get-GdChildDir 同口径。
            string[] all;
            try { all = Directory.GetDirectories(path); } catch { return new string[0]; }
            List<string> ok = new List<string>();
            foreach (string d in all)
            {
                try
                {
                    FileAttributes at = File.GetAttributes(d);
                    if ((at & FileAttributes.ReparsePoint) == 0) ok.Add(d);
                }
                catch { ok.Add(d); }   // PS 里那句 catch { $_ } 是把原路径照样放行，这边同形
            }
            return ok.ToArray();
        }

        sealed class Node { public string Path; public int Depth; }

        static List<Cand> ScanInstallDirs(string[] exeNames, string[] nameHints, int maxDepth, string[] roots)
        {
            List<Cand> found = new List<Cand>();
            Queue<Node> queue = new Queue<Node>();
            foreach (string r in roots) queue.Enqueue(new Node { Path = r, Depth = 0 });
            int guard = 0;
            while (queue.Count > 0 && guard < 40000)
            {
                guard++;
                Node cur = queue.Dequeue();
                int depth = cur.Depth;
                foreach (string child in ChildDirs(cur.Path))
                {
                    if (SkipPath(child)) continue;
                    string leaf = Path.GetFileName(child.TrimEnd('\\'));
                    bool hit = false;
                    foreach (string hint in nameHints) if (Like(leaf, hint)) { hit = true; break; }
                    if (hit)
                    {
                        foreach (string exe in exeNames)
                        {
                            string full = Path.Combine(child, exe);
                            if (File.Exists(full))
                            {
                                DateTime ti;
                                try { ti = File.GetLastWriteTime(full); } catch { ti = DateTime.MinValue; }   // PS: catch { Get-Date 0 }
                                found.Add(new Cand { Dir = child, Exe = full, LastWrite = ti, Junk = JunkPath(child) });
                            }
                        }
                    }
                    // 原式两重 if 是同一条件(外层只是意图注释)，照抄语义：还能下潜就入队。
                    if ((depth + 1) < maxDepth) queue.Enqueue(new Node { Path = child, Depth = depth + 1 });
                }
            }
            return found;
        }

        // ==================== 汇合：Add-GdTrimmedDir / Select-GdUniqueDir / Format-GdCandidates / 稳定排序 ====================

        static List<Cand> Trimmed(List<Cand> pool, string trimSegment, string relExe)
        {
            List<Cand> outList = new List<Cand>();
            foreach (Cand c in pool)
            {
                if (!string.IsNullOrEmpty(c.Dir) &&
                    string.Equals(Path.GetFileName(c.Dir.TrimEnd('\\')), trimSegment, StringComparison.OrdinalIgnoreCase))
                {
                    string parent = Path.GetDirectoryName(c.Dir);
                    if (!string.IsNullOrEmpty(parent))
                    {
                        Cand t = new Cand();
                        t.Dir = parent;
                        t.Exe = Gd.Truthy(relExe) ? Path.Combine(parent, relExe) : c.Exe;
                        t.LastWrite = c.LastWrite;
                        t.Junk = c.Junk;
                        t.Via = c.Via.Length > 0 ? c.Via : "由子目录退回安装根";
                        outList.Add(t);
                        continue;
                    }
                }
                outList.Add(c);
            }
            return outList;
        }

        // Sort-Object -Stable：Junk 升序(false 先)、LastWrite 降序，平手保持进入顺序 —— 用序号兜底做全序，等价稳定排序。
        static List<Cand> StableSort(List<Cand> pool)
        {
            List<Cand> idx = pool.ToList();
            return idx
                .Select((c, i) => new { C = c, I = i })
                .OrderBy(x => x.C.Junk)
                .ThenByDescending(x => x.C.LastWrite)
                .ThenBy(x => x.I)
                .Select(x => x.C).ToList();
        }

        static List<Cand> UniqueDirs(List<Cand> pool)
        {
            HashSet<string> seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            List<Cand> outList = new List<Cand>();
            foreach (Cand c in pool)
            {
                string key = (c.Dir ?? "").TrimEnd('\\').ToLowerInvariant();
                if (key.Length == 0 || seen.Contains(key)) continue;
                seen.Add(key);
                outList.Add(c);
            }
            return outList;
        }

        // Format-GdCandidates：非空值、大小写不敏感去重、前 3 个、' ; ' 连接(Select-Object -Unique 对字符串默认不敏感)。
        static string FormatCandidates(List<Cand> pool, string prop)
        {
            List<string> vals = new List<string>();
            foreach (Cand c in pool)
            {
                string v = c.Prop(prop);
                if (!Gd.Truthy(v)) continue;
                if (vals.Any(x => string.Equals(x, v, StringComparison.OrdinalIgnoreCase))) continue;
                vals.Add(v);
                if (vals.Count >= 3) break;
            }
            return string.Join(" ; ", vals.ToArray());
        }

        public static string[] FixedRoots()   // public 只为 --statetest 能敲形状，别的业务方不需要
        {
            // Get-GdFixedRoot：固定盘且就绪，根目录去尾斜杠再加回一个 '\' —— 与 PS 的 TrimEnd+'\'+ 同形
            List<string> outList = new List<string>();
            try
            {
                foreach (DriveInfo d in DriveInfo.GetDrives())
                    if (d.DriveType == DriveType.Fixed && SafeReady(d)) outList.Add(d.RootDirectory.FullName.TrimEnd('\\') + "\\");
            }
            catch { }
            if (outList.Count == 0) outList.Add("C:\\");
            return outList.ToArray();
        }

        static bool SafeReady(DriveInfo d) { try { return d.IsReady; } catch { return false; } }

        // ==================== verb: detect-installs(原 Resolve-GdCandidate 主流程) ====================

        public static object Installs(Dictionary<string, object> inp)
        {
            object[] processNames = Gd.List(Gd.Get(inp, "processNames"));
            string[] exeNames = Arr(Gd.Get(inp, "exeNames"));
            string[] nameHints = Arr(Gd.Get(inp, "nameHints"));
            object[] uninstallLike = Gd.List(Gd.Get(inp, "uninstallLike"));
            string pickProp = Gd.Str(Gd.Get(inp, "pickProp"));
            if (pickProp.Length == 0) pickProp = "Dir";
            string relExe = Gd.Str(Gd.Get(inp, "relExe"));
            string trimSegment = Gd.Str(Gd.Get(inp, "trimSegment"));
            bool skipScan = Gd.Truthy(Gd.Get(inp, "skipScan"));
            string[] roots = Arr(Gd.Get(inp, "roots"));
            if (roots.Length == 0) roots = FixedRoots();

            List<Cand> pool = new List<Cand>();
            pool.AddRange(RunningExeDirs(processNames, exeNames));
            if (uninstallLike.Length > 0) pool.AddRange(UninstallHintDirs(uninstallLike, exeNames));
            if (!skipScan) pool.AddRange(ScanInstallDirs(relExe.Length > 0 ? new string[] { relExe } : exeNames, nameHints, 4, roots));
            if (trimSegment.Length > 0)
                pool = Trimmed(pool, trimSegment, relExe.Length > 0 ? relExe : (exeNames.Length > 0 ? exeNames[0] : ""));
            pool = StableSort(pool);
            if (trimSegment.Length > 0) pool = UniqueDirs(pool);
            if (pool.Count == 0) return Gd.Out("Found", false);

            Cand pick = pool[0];
            string shown = FormatCandidates(pool, pickProp);
            string picked = pick.Prop(pickProp);
            string src = "探测：候选 " + pool.Count.ToString(CultureInfo.InvariantCulture) + " 个，取「" + picked + "」(前几个：" + shown + ")";
            if (pick.Junk) src += " ← 这个位置像下载/备份副本，确认是不是你要用的那份";
            return Gd.Out("Found", true, "Value", picked, "Source", src);
        }

        static string[] Arr(object v)
        {
            List<string> outList = new List<string>();
            foreach (object o in Gd.List(v)) outList.Add(Gd.Str(o));
            return outList.ToArray();
        }

        // ==================== verb: maa-queue-lines(原 Show-GdArkQueueState 的读表+行式) ====================

        public static object MaaQueueLines(Dictionary<string, object> inp)
        {
            string cfgPath = Gd.Str(Gd.Get(inp, "cfgPath"));
            object cfg = GdJson.Read(cfgPath);   // 缺文件抛"找不到文件：…"，与 PS Read-GdJson 原话同形 → 壳的 catch 染红字
            List<string> lines = new List<string>();
            object taskQueue = Gd.Get(Gd.Get(Gd.Get(cfg, "Configurations"), "Default"), "TaskQueue");
            foreach (object qObj in Gd.List(taskQueue))
            {
                Dictionary<string, object> q = Gd.Map(qObj);
                if (q == null) continue;                                  // 原话 if (-not $q){continue}
                string plan = "";
                if (string.Equals(Gd.Str(Gd.Get(q, "TaskType")), "Fight", StringComparison.OrdinalIgnoreCase))
                {
                    List<string> ps = new List<string>();
                    foreach (object s in Gd.List(Gd.Get(q, "StagePlan")))
                        if (Gd.Truthy(s)) ps.Add(Gd.Str(s));
                    plan = " plan=" + string.Join(",", ps.ToArray());
                }
                // "-f {0,-10} {1,-12}" 的填充语义两边都是 .NET String.Format，逐字同形
                lines.Add(string.Format("  {0,-10} {1,-12} IsEnable={2}{3}",
                    Gd.Str(Gd.Get(q, "TaskType")), Gd.Str(Gd.Get(q, "Name")), StrVal(Gd.Get(q, "IsEnable")), plan));
            }
            object gui = Gd.Get(Gd.Get(Gd.Get(cfg, "Configurations"), "Default"), "Gui");
            lines.Add("  RunDirectly(启动即自动开始)= " + StrVal(Gd.Get(Gd.Get(gui, "StartUpSettings"), "RunDirectly")));
            lines.Add("  StartGame(MAA 自己开游戏)  = " + StrVal(Gd.Get(Gd.Get(gui, "RuntimeSettings"), "StartGame")));
            lines.Add("  连接方式 ConnectSettings.Config = " + StrVal(Gd.Get(Gd.Get(gui, "ConnectSettings"), "Config")));
            return Gd.Out("Lines", lines.ToArray());
        }

        // PS 的 "$($x)" 展开：null → 空串，bool → True/False，其余不变文化(与 Gd.Str 同口径，单点复用)。
        static string StrVal(object v) { return Gd.Str(v); }

        // ==================== verb: m9a-instance-lines(原 Show-GdR1999InstanceList 的读目录+行式) ====================

        public static object M9aInstanceLines(Dictionary<string, object> inp)
        {
            string dir = Gd.Str(Gd.Get(inp, "dir"));
            string appFile = Gd.Str(Gd.Get(inp, "appFile"));
            object app = GdJson.Read(appFile);
            // PS: $app.'Instances.LastActiveName' —— 整个点号串是**一个键名**(带引号属性访问)，不是两层。
            string lastActive = Gd.Str(Gd.Get(app, "Instances.LastActiveName"));
            List<string> lines = new List<string>();
            string[] files;
            // 有意的行为差(不进对拍)：目录不存在时旧 PS 是 Get-ChildItem 当场炸(消息还是引擎本地化的，
            // 双引擎下本来就不等形)，这边选择安静出空行表 —— 展示段落不值得带崩整次体检。
            try { files = Directory.GetFiles(dir, "*.json"); } catch { files = new string[0]; }
            Array.Sort(files, StringComparer.Ordinal);   // 与批次 1 r1999-instance 同一手：对齐 Get-ChildItem 的名序
            foreach (string f in files)
            {
                object c = GdJson.Read(f);
                string instName = Gd.Str(Gd.Get(c, "InstanceName"));
                // -eq(字符串) 大小写不敏感；mark 与行式逐字照抄
                string mark = string.Equals(instName, lastActive, StringComparison.OrdinalIgnoreCase) ? " <== 当前" : "";
                List<string> tasks = new List<string>();
                foreach (object t in Gd.List(Gd.Get(c, "CurrentTasks")))
                {
                    string[] seg = Regex.Split(Gd.Str(t), @"<\|\|\>");   // PS: -split '<\|\|\>' 是正则拆 '<||>'
                    tasks.Add(seg.Length > 0 ? seg[0] : "");
                }
                lines.Add("  " + instName + "(" + Path.GetFileNameWithoutExtension(f) + ")" + mark + " — " + string.Join("/", tasks.ToArray()));
            }
            return Gd.Out("Lines", lines.ToArray());
        }
    }
}
