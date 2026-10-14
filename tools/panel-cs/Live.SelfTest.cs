// tools/panel-cs/Live.SelfTest.cs —— --livetest：B-2b 现场件(Live.cs)七枚 verb 的纯规则段自检
// + 10-11 新增的第八枚 check-ark-launcher(启动器更新页五档相位，见本文件第 8 组)
// + 10-12 B-2d③ 的 mumu(adb) 这一路四枚(adb-connect/adb-game-pid/mumu-screen-shot/mumu-screen-click，见第 9 组)。
//
// 这一批和前十批不一样：Live.cs 里**大半代码天生测不了**(拍窗口要点真窗口、问模型要起真子进程、
// 抢前台会动别人的焦点)。所以这里定的纪律是"只钉话术与算术，不钉现场"：
//   * 不碰真窗口(不 PrintWindow、不 EnumWindows 拿真句柄用、不 SetCursorPos)、不起子进程、不联网；
//   * 不要求这台机器装了 Python、有 key、端游在跑、是管理员 —— 这些**全部以夹具注入**(旋钮身份见 Live.cs 头注)；
//   * 不调 settings：八档夹具里每一档都把现场量喂全，只要有一格漏喂，Settings()/PythonExe() 就会去读真配置，
//     那条断言立刻红(根没 Init，读会抛) —— 这正是"只读自检不许要求机器现场状态"的守门方式。
//   * 唯一写盘的那一格打在 %TEMP% 副本上(请求目录裁剪要有真文件才裁得动)，收尾整目录删掉，不落仓内。
//     10-12 起写盘的不止一处：mumu 的读图段要在 %TEMP% 造**真 PNG** 让 System.Drawing 真解码
//     (否则尺寸闸与亮度算术只能拿假读数比，那一格就钉不住移植)，收尾同样整目录删掉。
//
// 期望串逐字抄自 lib\vision-ark.ps1 / lib\vision-rescue.ps1(ark 那一路)与 lib\vision-mumu.ps1 / lib\mumu.ps1
// (mumu 那一路)的旧实现(生产接线之外它们仍是事实源)—— 这里红了就是移植走样，改 Live.cs，不改这里的期望(批次 1 起同一条纪律)。
// 两处专门钉住的原样怪癖：①"截图失败："双前缀异常路径会叠两次(ShotFail)；②处境串里那两个空格
// ("$UserText  自动化这边的处境：")。谁"顺手修好"了这里立刻红。
// 够不着的几样如实登记，不假装测过：黑屏闸的 Skip 只钉句子(常量件 BlackScreenSkip)、
// ark-window-shot / ark-window-click / ark-grab-foreground 的现场分支要等 B-2b④ 的门与一轮真跑；
// mumu 那两声 adb 失败(screencap/pull 的退出码句)与 RunAdb 的超时杀进程，同样只有真跑能验。

using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Text;

namespace GdPanel
{
    static class LiveSelfTest
    {
        sealed class Box
        {
            public int Total = 0;
            public int Fails = 0;
            public void Chk(string name, Func<bool> test)
            {
                Total++;
                bool ok = false;
                string detail = null;
                try { ok = test(); }
                catch (Exception ex) { ok = false; detail = ex.GetType().Name + ": " + ex.Message; }
                if (!ok)
                {
                    Fails++;
                    GdLog.Line("FAIL " + name + (detail != null ? "：" + detail : ""), "ERROR");
                }
            }
            public int Result(string tag)
            {
                if (Fails == 0) GdLog.Line(tag + " 全绿：" + Total + "/" + Total + " 项", "OK");
                else GdLog.Line(tag + " 有红项：" + Fails + "/" + Total, "ERROR");
                return Fails == 0 ? 0 : 1;
            }
        }

        static Dictionary<string, object> In(params object[] kv)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            for (int i = 0; i + 1 < kv.Length; i += 2) d[(string)kv[i]] = kv[i + 1];
            return d;
        }

        static Dictionary<string, object> Row(object o) { return o as Dictionary<string, object>; }
        static string S(object v) { return v == null ? "(null)" : Gd.Str(v); }
        static long L(object v) { return v == null ? long.MinValue : Convert.ToInt64(v, System.Globalization.CultureInfo.InvariantCulture); }

        // ---------- 六道闸门：绿档 = 全部现场量喂夹具，一格都不许现读 ----------
        static Dictionary<string, object> Green()
        {
            return In("target", "ark", "hasBuilder", true, "enabled", true, "isAdmin", true,
                      "hasKey", true, "pythonExe", @"C:\fake\python.exe", "alive", true);
        }

        static string SkipOf(Dictionary<string, object> knobs)
        {
            Dictionary<string, object> d = Row(Live.RescueContext(knobs));
            return d == null ? "(没有 data)" : S(d["Skip"]);
        }

        const string PythonSentence =
            "这台机器上没有可用的 Python 3(问模型那一层用它跑)：跑 setup.ps1 -WithPython 装一份到 state\\python\\，或把 runtime.pythonExe 指到已有的那份";

        // 问模型回执的三种 stdout(手写 JSON，不靠真 Python)
        const string OkJson =
            @"{""ok"":true,""text"":""好的"",""model"":""qwen3.8-flash"",""http_status"":200,""elapsed_ms"":812,""attempts"":1,""error"":""""}";
        const string VerdictJson =
            @"{""ok"":true,""text"":""{\""blocking\"":true,\""what\"":\""活动公告弹窗\"",\""clicks\"":[{\""x\"":880,\""y\"":210,\""why\"":\""右上角的 ×\""}]}\"""",""model"":""m""}";
        const string FailJson = @"{""ok"":false,""error"":""HTTP 429"",""http_status"":429,""attempts"":3}";

        public static int LiveTest()
        {
            Box b = new Box();
            Console.WriteLine("---- panel-cs --livetest(B-2b 现场件七枚 + 10-11 的 check-ark-launcher 的纯规则段，不碰真窗口/不起子进程/不联网) ----");
            string tmp = Path.Combine(Path.GetTempPath(), "gd-livetest-" + System.Diagnostics.Process.GetCurrentProcess().Id);
            try
            {
                // 夹具要的"现场存在性"就在这棵临时树里造齐：ask.py 只要"在"，问模型那一层的第二道闸才拦得住，
                // 而它**永远不会被执行**(夹具档 noSpawn)。
                Directory.CreateDirectory(tmp);
                File.WriteAllText(Path.Combine(tmp, "ask.py"), "");
            }
            catch (Exception ex)
            {
                GdLog.Line("--livetest 连临时树都建不起来(后面写盘那一格必红)：" + ex.Message, "ERROR");
            }

            // ==================== 1) 尺寸闸两分支 ====================
            b.Chk("尺寸正常 → 空串(不拦)", delegate { return Live.ShotSizeWhy(1600, 900, false) == ""; });
            b.Chk("恢复后仍最小化 → 说\"仍最小化\"并带尺寸", delegate
            {
                return Live.ShotSizeWhy(168, 29, true) == "端游窗口仍最小化(拍不到东西) 168x29";
            });
            b.Chk("非最小化而尺寸小 → 说\"窗口尺寸异常\"并带尺寸(旧实现两句不同，不许合成一句)", delegate
            {
                return Live.ShotSizeWhy(0, 0, false) == "窗口尺寸异常 0x0";
            });
            b.Chk("宽够高不够也拦(两边各自设阈 200x150)", delegate { return Live.ShotSizeWhy(1920, 149, false).Length > 0; });

            // ==================== 2) 六道闸门的话术与顺序 ====================
            b.Chk("六闸全绿 → CanAsk=true 且 Skip 空", delegate
            {
                Dictionary<string, object> d = Row(Live.RescueContext(Green()));
                return d != null && Gd.Truthy(d["CanAsk"]) && S(d["Skip"]) == "" && S(d["Kind"]) == "ark"
                       && S(d["Name"]) == "端游窗口" && Gd.Truthy(d["NeedsAdmin"]) && Gd.Truthy(d["HasPython"]);
            });
            b.Chk("闸①后端不在场：这句带得出 New-GdVisionTarget-<target>", delegate
            {
                Dictionary<string, object> k = Green();
                k["target"] = "mumu"; k["hasBuilder"] = false;
                return SkipOf(k) == "这棵树上没有 mumu 这一路的视觉兜底后端(没有 New-GdVisionTarget-mumu 这个函数)";
            });
            b.Chk("闸②开关：句子必须带得出 settings 的键名(t13 就按这几个字断言)", delegate
            {
                Dictionary<string, object> k = Green(); k["enabled"] = false;
                return SkipOf(k) == "arkVisionRescue=false，视觉兜底已关闭";
            });
            b.Chk("闸③权限：默认理由句取自靶子声明", delegate
            {
                Dictionary<string, object> k = Green(); k["isAdmin"] = false;
                return SkipOf(k) == "当前不是管理员权限：端游是 requireAdministrator，这个权限下点它不会有任何反应，跳过兜底";
            });
            b.Chk("闸③：adminWhy 空时回退那半句(旧实现的 if/else 同形)", delegate
            {
                Dictionary<string, object> k = Green(); k["isAdmin"] = false; k["adminWhy"] = "";
                return SkipOf(k) == "当前不是管理员权限：这一路的对象要管理员权限，这个权限下点它不会有任何反应，跳过兜底";
            });
            b.Chk("闸③：这一路不吃管理员就跳过这道(退到 key 那句)", delegate
            {
                Dictionary<string, object> k = Green(); k["needsAdmin"] = false; k["hasKey"] = false;
                return SkipOf(k) == "没配视觉模型 key(见 settings 里的 runtime.visionApiKey)，本轮不尝试兜底";
            });
            b.Chk("闸④key：原话(键名一字不改)", delegate
            {
                Dictionary<string, object> k = Green(); k["hasKey"] = false;
                return SkipOf(k) == "没配视觉模型 key(见 settings 里的 runtime.visionApiKey)，本轮不尝试兜底";
            });
            b.Chk("闸⑤Python：原话 + HasPython=false 也一起出", delegate
            {
                Dictionary<string, object> k = Green(); k["pythonExe"] = "";
                Dictionary<string, object> d = Row(Live.RescueContext(k));
                return S(d["Skip"]) == PythonSentence && !Gd.Truthy(d["HasPython"]);
            });
            b.Chk("闸⑥对象在场：那句用靶子名开头", delegate
            {
                Dictionary<string, object> k = Green(); k["alive"] = false;
                return SkipOf(k) == "端游窗口已经不在了，兜底没有对象";
            });
            b.Chk("闸门顺序不许调换：开关+权限+key 全坏时说出的仍是开关那句", delegate
            {
                Dictionary<string, object> k = Green();
                k["enabled"] = false; k["isAdmin"] = false; k["hasKey"] = false;
                return SkipOf(k) == "arkVisionRescue=false，视觉兜底已关闭";
            });
            b.Chk("target 大小写：Kind 走 ToLowerInvariant(旧实现 New-GdVisionTarget-$($Kind.ToLower()))", delegate
            {
                Dictionary<string, object> k = Green(); k["target"] = "ARK";
                Dictionary<string, object> d = Row(Live.RescueContext(k));
                return S(d["Kind"]) == "ark" && Gd.Truthy(d["CanAsk"]);
            });
            b.Chk("Scene/UserText 只是转达：喂进去什么就回什么(默认空串)", delegate
            {
                Dictionary<string, object> k = Green(); k["scene"] = "主界面"; k["userText"] = "找出挡路的弹窗";
                Dictionary<string, object> d = Row(Live.RescueContext(k));
                return S(d["Scene"]) == "主界面" && S(d["UserText"]) == "找出挡路的弹窗" &&
                       S(Row(Live.RescueContext(Green()))["Scene"]) == "";
            });

            // ==================== 3) 黑屏闸的句子 ====================
            b.Chk("黑屏那句带得出实测亮度(阈值 8 的判据在 Stats 那格)", delegate
            {
                return Live.BlackScreenSkip(3L) == "截图几乎是全黑(平均亮度 3)：桌面可能锁了或窗口没在渲染，问模型只会白花钱";
            });

            // ==================== 4) 亮度签名：算术全部在内存位图上跑 ====================
            b.Chk("整幅同色：平均亮度=灰阶本身，签名 144 格、只有采到的 4 格非零", delegate
            {
                Bitmap bmp = new Bitmap(14, 14);
                try
                {
                    for (int y = 0; y < 14; y++) for (int x = 0; x < 14; x++) bmp.SetPixel(x, y, Color.FromArgb(20, 20, 20));
                    Dictionary<string, object> st = Live.Stats(bmp, 14, 14, 7);
                    string[] parts = S(st["Sig"]).Split(',');
                    if (parts.Length != 144) return false;
                    if (L(st["AvgLum"]) != 20L) return false;
                    // 20/8 = 2.5 → 取偶成 2(PS 的 [int][math]::Round 同形)；采到的格是 (0,0)(0,7)(7,0)(7,7)
                    int nonzero = 0;
                    for (int i = 0; i < 144; i++) if (parts[i] != "0") { nonzero++; if (parts[i] != "2") return false; }
                    return nonzero == 4;
                }
                finally { bmp.Dispose(); }
            });
            b.Chk("均值 7.5 取偶 → 8(等于 8 就不算黑屏；截断会得 7 而误拦)", delegate
            {
                Bitmap bmp = new Bitmap(14, 14);
                try
                {
                    bmp.SetPixel(0, 0, Color.FromArgb(7, 7, 7));
                    bmp.SetPixel(0, 7, Color.FromArgb(7, 7, 7));
                    bmp.SetPixel(7, 0, Color.FromArgb(8, 8, 8));
                    bmp.SetPixel(7, 7, Color.FromArgb(8, 8, 8));
                    return L(Row2(Live.Stats(bmp, 14, 14, 7), "AvgLum")) == 8L;
                }
                finally { bmp.Dispose(); }
            });
            b.Chk("均值 7.33 → 7，落在黑屏闸那一侧", delegate
            {
                Bitmap bmp = new Bitmap(14, 21);
                try
                {
                    for (int y = 0; y < 21; y++) for (int x = 0; x < 14; x++) bmp.SetPixel(x, y, Color.FromArgb(7, 7, 7));
                    bmp.SetPixel(7, 14, Color.FromArgb(9, 9, 9));      // 采样点(0,0)(0,7)(0,14)(7,0)(7,7)(7,14)：35+9=44，44/6≈7.33
                    return L(Row2(Live.Stats(bmp, 14, 21, 7), "AvgLum")) == 7L;
                }
                finally { bmp.Dispose(); }
            });
            b.Chk("gy/gx 封顶：右下角那一格落在第 15 行 x 第 8 列(k=143) 而不是越界", delegate
            {
                Bitmap bmp = new Bitmap(100, 100);
                try
                {
                    for (int y = 0; y < 100; y++) for (int x = 0; x < 100; x++) bmp.SetPixel(x, y, Color.FromArgb(40, 40, 40));
                    string[] parts = S(Live.Stats(bmp, 100, 100, 7)["Sig"]).Split(',');
                    return parts.Length == 144 && parts[143] == "5";      // 40/8=5
                }
                finally { bmp.Dispose(); }
            });
            b.Chk("R*30+G*59+B*11 的加权：纯蓝 255 的亮度不是 255 而是 28", delegate
            {
                Bitmap bmp = new Bitmap(14, 14);
                try
                {
                    for (int y = 0; y < 14; y++) for (int x = 0; x < 14; x++) bmp.SetPixel(x, y, Color.FromArgb(0, 0, 255));
                    // (0*30+0*59+255*11)/100 = 28.05 → 取整 28
                    return L(Live.Stats(bmp, 14, 14, 7)["AvgLum"]) == 28L;
                }
                finally { bmp.Dispose(); }
            });

            // ==================== 5) 问模型那句话的装配 ====================
            b.Chk("没给 userText 用默认那句", delegate
            {
                return Live.AskUserText("", "") == "这是当前画面。按规则只回一个 JSON 对象。";
            });
            b.Chk("处境里的换行/多空格压平，拼接串那两个空格是原样", delegate
            {
                return Live.AskUserText("画面", "卡在\t弹窗\n上  三次了") == "画面  自动化这边的处境：卡在 弹窗 上 三次了";
            });
            b.Chk("处境截到 200 字并补省略号(那句是给模型听的处境说明，不是日志转储)", delegate
            {
                return Live.AskUserText("A", new string('x', 260)) ==
                       "A  自动化这边的处境：" + new string('x', 200) + "…";
            });
            b.Chk("请求文件九字段：key 一个都不许在里面", delegate
            {
                Dictionary<string, object> r = Live.BuildRequest(@"C:\shot.png", "【场景头】", "UT", "qwen3.8-flash", 45,
                                                                 false, "", @"C:\ask.json", @"C:\calls.jsonl");
                if (r.Count != 9) return false;
                foreach (string k in r.Keys) if (k.ToLowerInvariant().Contains("key")) return false;
                return S(r["image"]) == @"C:\shot.png" && S(r["user_text"]) == "UT" && S(r["model"]) == "qwen3.8-flash"
                       && L(r["temperature"]) == 0L && L(r["max_tokens"]) == 300L && L(r["timeout_seconds"]) == 45L
                       && S(r["audit"]) == @"C:\calls.jsonl";
            });
            b.Chk("给了 Scene 就原样当提示词头(裁尾在靶子定义处做过了)", delegate
            {
                Dictionary<string, object> r = Live.BuildRequest("i", "【场景头】", "u", "m", 1, false, "", "req", "audit");
                string sp = S(r["system_prompt"]);
                return sp.StartsWith("【场景头】") && sp.EndsWith("6. 不要输出坐标以外的多余字段。");
            });
            b.Chk("allowReward 真会改提示词，改完只差那一条规则", delegate
            {
                string on = Live.BuildRequest("i", "H", "u", "m", 1, true, "", "req", "audit")["system_prompt"].ToString();
                string off = Live.BuildRequest("i", "H", "u", "m", 1, false, "", "req", "audit")["system_prompt"].ToString();
                return on != off && on.Contains("领取日常奖励") && !off.Contains("这一轮已允许");
            });
            b.Chk("endpoint：空 base 回落到兼容模式，两种写法都得同一个 POST 地址", delegate
            {
                Dictionary<string, object> a = Live.BuildRequest("i", "H", "u", "m", 1, false, "", "req", "audit");
                Dictionary<string, object> c = Live.BuildRequest("i", "H", "u", "m", 1, false,
                    "https://dashscope.aliyuncs.com/compatible-mode/v1", "req", "audit");
                Dictionary<string, object> d = Live.BuildRequest("i", "H", "u", "m", 1, false,
                    "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions", "req", "audit");
                return S(a["endpoint"]) == "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
                       && S(c["endpoint"]) == S(a["endpoint"]) && S(d["endpoint"]) == S(a["endpoint"]);
            });

            // ==================== 6) 子进程回来以后那团读数的解读 ====================
            b.Chk("正常回执：六个字段各归各位", delegate
            {
                Dictionary<string, object> d = Live.MapChild(0, OkJson, "", @"C:\ask-1.json", "06:12:33");
                return Gd.Truthy(d["Ok"]) && S(d["Text"]) == "好的" && S(d["Model"]) == "qwen3.8-flash"
                       && L(d["HttpStatus"]) == 200L && L(d["ElapsedMs"]) == 812L && L(d["Attempts"]) == 1L
                       && S(d["Error"]) == "" && S(d["RequestPath"]) == @"C:\ask-1.json";
            });
            b.Chk("解释器先打了一行警告也挑得到那行 JSON(实测坑，别整段解析)", delegate
            {
                Dictionary<string, object> d = Live.MapChild(0, "Python site warning\n" + OkJson, "", "r", "06:12:33");
                return Gd.Truthy(d["Ok"]) && S(d["Text"]) == "好的";
            });
            b.Chk("第一行像 JSON 却解不动时继续往后挑", delegate
            {
                Dictionary<string, object> d = Live.MapChild(0, "{坏\n" + OkJson, "", "r", "06:12:33");
                return Gd.Truthy(d["Ok"]);
            });
            b.Chk("超时(Code=-1)说成\"问模型超时\"并带上那个时刻，不跟着报退出码", delegate
            {
                Dictionary<string, object> d = Live.MapChild(-1, "", "timeout after 165s", "r", "06:15:03");
                return !Gd.Truthy(d["Ok"]) && S(d["Error"]) == "问模型超时(Python 侧 06:15:03 前没回)";
            });
            b.Chk("非 0 退出：stderr 优先，其次 stdout", delegate
            {
                Dictionary<string, object> a = Live.MapChild(3, "", "Traceback: boom", "r", "06:12:33");
                Dictionary<string, object> c = Live.MapChild(2, "no json here", "", "r", "06:12:33");
                return S(a["Error"]) == "Python 没回 JSON(退出码 3)：Traceback: boom"
                       && S(c["Error"]) == "Python 没回 JSON(退出码 2)：no json here";
            });
            b.Chk("退出码 0 但回执 ok:false：照回执走，Error 用回执那句", delegate
            {
                Dictionary<string, object> d = Live.MapChild(0, FailJson, "", "r", "06:12:33");
                return !Gd.Truthy(d["Ok"]) && S(d["Error"]) == "HTTP 429" && L(d["HttpStatus"]) == 429L;
            });
            b.Chk("回执里 text 是空/空白 → Verdict 是 null(旧实现返回 $null，不是空对象)", delegate
            {
                Dictionary<string, object> d = Row(Live.VisionAsk(AskKnobs(@"{""ok"":true,""text"":"" ""}")));
                return d != null && d["Verdict"] == null && Gd.Truthy(d["Ok"]);
            });
            b.Chk("text 里裹着的 verdict JSON 当场抠出来(复用批 4 那枚，不 verb 套 verb)", delegate
            {
                Dictionary<string, object> d = Row(Live.VisionAsk(AskKnobs(VerdictJson)));
                Dictionary<string, object> v = d == null ? null : Row(d["Verdict"]);
                object[] clicks = v == null ? null : Gd.List(v["Clicks"]);
                return v != null && Gd.Truthy(v["Blocking"]) && S(v["What"]) == "活动公告弹窗"
                       && clicks != null && clicks.Length == 1;
            });
            b.Chk("三道前置的顺序与原话：key → ask.py → Python", delegate
            {
                Dictionary<string, object> k = In("imagePath", "c.png", "hasKey", false, "noSpawn", true);
                Dictionary<string, object> a = Row(Live.VisionAsk(k));
                Dictionary<string, object> b2 = In("imagePath", "c.png", "hasKey", true,
                    "scriptPath", Path.Combine(tmp, "并不存在.py"), "noSpawn", true);
                Dictionary<string, object> m = Row(Live.VisionAsk(b2));
                Dictionary<string, object> c2 = In("imagePath", "c.png", "hasKey", true,
                    "scriptPath", Path.Combine(tmp, "ask.py"), "pythonExe", "", "noSpawn", true);
                Dictionary<string, object> n = Row(Live.VisionAsk(c2));
                return S(a["Error"]) == "没有视觉模型的 API key(settings.runtime.visionApiKey 或环境变量 DASHSCOPE_API_KEY)"
                       && S(m["Error"]) == "缺少 tools\\vision\\ask.py(问模型这一层的实现)，这一棵树上问不了模型"
                       && S(n["Error"]) == "没找到可用的 Python 3(问模型这一层用它跑)：跑 setup.ps1 -WithPython 自动装一份到 state\\python\\，或把 runtime.pythonExe 指到自己装的那份";
            });
            b.Chk("没给 imagePath 是规则错(throw → 信封 exit 2)，不是默默回一句", delegate
            {
                try { Live.VisionAsk(In("noSpawn", true)); return false; }
                catch (GdRuleException) { return true; }
            });
            b.Chk("夹具档不许起进程：SkippedSpawn 明写在回执上", delegate
            {
                Dictionary<string, object> d = Row(Live.VisionAsk(AskKnobs(OkJson)));
                return d != null && Gd.Truthy(d["SkippedSpawn"]) && S(d["Model"]) == "m-fake";
            });

            // ==================== 7) 请求目录裁剪(唯一写盘的一格，打在 %TEMP%) ====================
            b.Chk("ask-*.json 留最近 12 份，calls.jsonl 不在裁剪范围", delegate
            {
                string dir = Path.Combine(tmp, "trim");
                Directory.CreateDirectory(dir);
                File.WriteAllText(Path.Combine(dir, "calls.jsonl"), "{}\n");
                DateTime baseTime = new DateTime(2026, 10, 8, 6, 0, 0);
                for (int i = 1; i <= 15; i++)
                {
                    string f = Path.Combine(dir, "ask-" + i.ToString("000") + ".json");
                    File.WriteAllText(f, "{}");
                    File.SetLastWriteTime(f, baseTime.AddMinutes(i));     // 越后面写入越新
                }
                int gone = Live.TrimAskDir(dir, 12);
                int left = Directory.GetFiles(dir, "ask-*.json").Length;
                bool ledger = File.Exists(Path.Combine(dir, "calls.jsonl"));
                // 删掉的必须是三份最旧的(001/002/003)
                bool oldestGone = !File.Exists(Path.Combine(dir, "ask-001.json")) &&
                                  !File.Exists(Path.Combine(dir, "ask-002.json")) &&
                                  !File.Exists(Path.Combine(dir, "ask-003.json")) &&
                                  File.Exists(Path.Combine(dir, "ask-004.json"));
                return gone == 3 && left == 12 && ledger && oldestGone;
            });
            b.Chk("目录不存在 → 0 份，不抛(兜底已经在异常路径上了)", delegate
            {
                return Live.TrimAskDir(Path.Combine(tmp, "没有这个目录"), 12) == 0 && Live.TrimAskDir("", 12) == 0;
            });

            // ==================== 8) 找窗口/看桌面：只喂夹具的那几档 ====================
            b.Chk("ark-find-window：假进程名 → Found=false，句柄和 pid 都给 0", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkFindWindow(In("procName", "gd-没有这个进程")));
                return d != null && !Gd.Truthy(d["Found"]) && L(d["Handle"]) == 0L && L(d["ProcId"]) == 0L && S(d["Title"]) == "";
            });
            b.Chk("ark-desktop-state：全量注入时一个字都不碰窗口调用", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkDesktopState(In("handle", 12345L, "desk", "Default",
                    "onFg", false, "fgProcess", "msedge", "iconic", false, "visible", true,
                    "rectText", "10,10 1600x900", "cursor", "5,5")));
                return S(d["Detail"]) == "输入桌面=Default(未锁屏)；端游在前台=否(前台是 msedge)；端游窗口状态=10,10 1600x900；光标在 5,5"
                       && Gd.Truthy(d["LockedKnown"]) && !Gd.Truthy(d["Locked"]) && S(d["FgProcess"]) == "msedge";
            });
            b.Chk("桌面名读不到 = \"不知道\"，不是\"锁屏了\"(Locked 出 null)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkDesktopState(In("handle", 0L, "desk", "",
                    "onFg", false, "fgProcess", "", "iconic", false, "visible", false, "rectText", "", "cursor", "")));
                return d["Locked"] == null && !Gd.Truthy(d["LockedKnown"])
                       && S(d["Detail"]) == "输入桌面=读不到(无法判断是否锁屏)；端游窗口没找到";
            });
            b.Chk("Winlogon 桌面那句是判据(06:00 那几轮靠它分辨\"桌面接管\")", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkDesktopState(In("handle", 0L, "desk", "Winlogon",
                    "onFg", false, "fgProcess", "", "iconic", false, "visible", false, "rectText", "", "cursor", "")));
                return Gd.Truthy(d["Locked"]) && S(d["Detail"]) ==
                       "输入桌面=Winlogon(会话不在正常桌面，鼠标键盘进不来)；端游窗口没找到";
            });

            // ==================== 8) check-ark-launcher：启动器更新页的五档相位(10-11) ====================
            // 这一组是**新增能力**的判据，旧实现在 lib 里没有真身可抄 —— 所以期望串就是这里定的那一套话术
            // (Live.cs 的注释与这里逐字对着)。纪律照旧：不碰真 PrintWindow、不起子进程；
            // 窗口那一格用"假句柄 + 注入的图读数"绕过，唯一碰真的地方是 no-window 那一档的进程枚举(只读、不碰窗口)。
            b.Chk("相位①no-window：窗口没找到就收工，一句都不问模型(没喂问模型夹具，真去问会当场红)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(In(
                    "procName", "gd-livetest-根本没有这一枚进程", "maxClicks", 6)));
                return d != null && S(d["Phase"]) == "no-window" && S(d["Level"]) == "WARN"
                       && S(d["LogLine"]) == "启动器检查：端游窗口没找到(可能已经关了)，这一轮不再检查更新页"
                       && S(d["Detail"]) == "端游窗口没找到(可能已经关了)";
            });
            b.Chk("相位②uncertain：拍不到画面(黑屏闸那句原样带出)也不问模型", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(In(
                    "handle", 987654321L, "maxClicks", 6,
                    "path", Path.Combine(Path.GetTempPath(), "gd-launcher-clamp.png"),
                    "shot", In("Ok", false, "CanContinue", false,
                               "Skip", Live.BlackScreenSkip(3), "Why", ""))));
                return S(d["Phase"]) == "uncertain" && S(d["Detail"]) == Live.BlackScreenSkip(3)
                       && S(d["LogLine"]) == "启动器检查：拍不到画面(" + Live.BlackScreenSkip(3) + ")，分不清是不是更新页，按原样放行";
            });
            b.Chk("拍图回执 Ok=true 但 CanContinue=false 也算拍不到(两格各自把关，不是只看 Ok)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(In(
                    "handle", 987654321L, "maxClicks", 6, "path", @"C:\gd-launcher-clamp.png",
                    "shot", FixShot("AvgLum", 7, "CanContinue", false, "Skip", Live.BlackScreenSkip(7)))));
                return S(d["Phase"]) == "uncertain" && S(d["Level"]) == "WARN"
                       && S(d["Detail"]) == Live.BlackScreenSkip(7);
            });
            b.Chk("相位③needs-click：blocking=true 且点位换算得动 → 出客户区坐标(千分比 500/820 → 800/738)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(LaunchKnobs(LClickJson, 0)));
                return S(d["Phase"]) == "needs-click" && S(d["Level"]) == "STEP"
                       && L(d["ClientX"]) == 800L && L(d["ClientY"]) == 738L
                       && S(d["Why"]) == "『更新』按钮" && S(d["Model"]) == "m-fake"
                       && S(d["What"]) == "资源更新页，要点更新";
            });
            b.Chk("needs-click 那句日志同时带出模型说法、点位与为什么(回看时要能认出点的是哪个按钮)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(LaunchKnobs(LClickJson, 0)));
                return S(d["LogLine"]) == "启动器检查：这一页要点一下(模型说：资源更新页，要点更新，点位 800,738 『更新』按钮)";
            });
            b.Chk("客户区不是整窗时点位要减去客户区原点(启动器是无边框窗时那格照样对)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(LaunchKnobs(LClickJson, 0,
                    "ClientLeft", 8, "ClientTop", 31)));
                return L(d["ClientX"]) == 792L && L(d["ClientY"]) == 707L;
            });
            b.Chk("相位④waiting：blocking=true 但 clicks 是空数组(下载中/拿不准) → 等，一处都不点", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(LaunchKnobs(LWaitJson, 0)));
                return S(d["Phase"]) == "waiting" && S(d["Level"]) == "" && L(d["ClientX"]) == 0L
                       && S(d["LogLine"]) == "启动器检查：这一页还不能进游戏(模型说：正在下载资源 47%)，没有该点的地方，继续等";
            });
            b.Chk("相位⑤clear：blocking=false → 放行(这一档不看点位)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(LaunchKnobs(LClearJson, 0)));
                return S(d["Phase"]) == "clear" && S(d["Level"]) == "OK"
                       && S(d["LogLine"]) == "启动器检查：已经不在更新页了(模型说：已经在游戏主界面)，可以开始了";
            });
            b.Chk("模型给的点位越界 → 当 waiting 而不是硬点(与兜底那一路同一把尺子、同一个宁可不动)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(LaunchKnobs(LOobJson, 0)));
                return S(d["Phase"]) == "waiting" && S(d["Level"]) == "WARN"
                       && S(d["Detail"]).Contains("模型给的点位用不了");
            });
            b.Chk("点击预算用尽 → uncertain 放行(不无限点『更新』，预算判据住 C#、计数在编排侧)", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(LaunchKnobs(LClickJson, 6)));
                return S(d["Phase"]) == "uncertain" && S(d["Level"]) == "WARN"
                       && S(d["LogLine"]) == "启动器检查：已经点过 6 次还在要点『更新』(不像是点一次就完的事)，按原样放行"
                       && L(d["ClicksAlready"]) == 6L && L(d["MaxClicks"]) == 6L;
            });
            b.Chk("maxClicks 写坏成 0 → 兜回 6(配置一坏不许变成\"一次都不许点\"的死闸)", delegate
            {
                Dictionary<string, object> knobs = LaunchKnobs(LClickJson, 5);
                knobs["maxClicks"] = 0;
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(knobs));
                return S(d["Phase"]) == "needs-click" && L(d["MaxClicks"]) == 6L;
            });
            b.Chk("问模型失败(回执 ok:false)→ uncertain 并把那句错误原样带进日志", delegate
            {
                Dictionary<string, object> knobs = LaunchKnobs(FailJson, 0);
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(knobs));
                return S(d["Phase"]) == "uncertain" && S(d["Error"]) == "HTTP 429"
                       && S(d["LogLine"]) == "启动器检查：问模型没成功(HTTP 429)，分不清是不是更新页，按原样放行";
            });
            b.Chk("回执 text 是空 → Verdict 抠不出 → uncertain(不许把\"没结论\"当成\"不用更新\")", delegate
            {
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(LaunchKnobs(LBlankJson, 0)));
                return S(d["Phase"]) == "uncertain" && S(d["Detail"]) == "模型回话里抠不出 JSON";
            });
            b.Chk("没有 key 那一档：uncertain 的日志带得出 settings 的键名(和兜底那一路同一句话)", delegate
            {
                Dictionary<string, object> knobs = LaunchKnobs(LClickJson, 0);
                knobs["hasKey"] = false;
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(knobs));
                return S(d["Phase"]) == "uncertain"
                       && S(d["Error"]).Contains("没有视觉模型的 API key(settings.runtime.visionApiKey");
            });
            b.Chk("启动器那一档问的是启动器话术(上下文里写明\"启动器/更新器\"，不是兜底那句\"MAA 卡在原地\")", delegate
            {
                string askDir = Path.Combine(tmp, "launcher-ask");
                Directory.CreateDirectory(askDir);
                Dictionary<string, object> knobs = LaunchKnobs(LClickJson, 0);
                knobs["noSpawn"] = false;             // 真走请求装配那一格：只写文件，进程起不来就照 uncertain 回
                knobs["pythonExe"] = @"C:\gd-livetest-并不存在的\python.exe";
                knobs["askDir"] = askDir;
                Dictionary<string, object> d = Row(Live.ArkLauncherCheck(knobs));
                string[] files = Directory.GetFiles(askDir, "ask-*.json");
                if (files.Length == 0) return false;
                string req = File.ReadAllText(files[0], Encoding.UTF8);
                // 走序列化回来比(JavaScriptSerializer 会把中文转成 \uXXXX，直接 Contains 原文必然假红)
                Dictionary<string, object> r = Row(GdJson.Deserialize(req));
                string sp = r == null ? "" : S(r["system_prompt"]);
                return S(d["Phase"]) != "" && sp.StartsWith("你是明日方舟端游**启动器/更新器**自动化流程")
                       && !sp.Contains("自动化助手(MAA)不认识界面上没收录过的弹窗");
            });
            b.Chk("BuildRequest 带 sceneId 才换话术；请求文件仍是九个字段(协议没为这一档扩面)", delegate
            {
                Dictionary<string, object> plain = Live.BuildRequest("i", "H", "u", "m", 1, false, "", "req", "audit");
                Dictionary<string, object> l2 = Live.BuildRequest("i", "H", "u", "m", 1, false, "", "req", "audit", "ark-launcher");
                return plain.Count == 9 && l2.Count == 9
                       && S(plain["system_prompt"]) == Vision.BuildPrompt("H", false)
                       && S(l2["system_prompt"]).StartsWith("你是明日方舟端游**启动器/更新器**");
            });

            // ==================== 9) B-2d③(10-12)：mumu(adb) 这一路 ====================
            // 这一组把 ark 那一路的**六闸期望一格都不改**(第 2 组那十格是移植的锚)，只往"按路派生"的默认值上加。
            // 现场段里够得着的只有读图段：夹具 localPng 递一张**真 PNG**(在 %TEMP% 现造)，System.Drawing 真解码
            // 走 Live.Stats —— 尺寸闸/黑屏闸/十五格回执因此钉得住，而 screencap/pull 那两次 adb 调用仍不碰
            // (那两句失败话术要等一轮真跑，第 8 组之外唯一起进程的地方还是"进程起不来就照实回")。
            b.Chk("mumu 绿档：六闸全过，Name/NeedsAdmin 与两份默认话术都取自靶子(话术逐字抄 vision-mumu.ps1:184-189)", delegate
            {
                Dictionary<string, object> d = Row(Live.RescueContext(GreenMumu()));
                if (d == null) return false;
                return Gd.Truthy(d["CanAsk"]) && S(d["Skip"]) == "" && S(d["Kind"]) == "mumu"
                       && S(d["Name"]) == "MuMu 模拟器画面" && !Gd.Truthy(d["NeedsAdmin"])
                       && S(d["AdminWhy"]) == "" && Gd.Truthy(d["HasPython"]) &&
                       S(d["Scene"]) == "你是《重返未来：1999》自动化流程里的\"看屏幕的人\"。游戏跑在 MuMu 模拟器里，自动化助手(M9A)只认\n" +
                           "自己模板集里的界面，遇到没收录过的弹窗或更新提示就会一直在原地重试。给你一张模拟器屏幕截图，\n" +
                           "请判断画面上有没有挡住主流程的东西(活动公告、更新/下载提示、登录弹窗、网络错误框这类)，并指出该点哪里把它关掉。" &&
                       S(d["UserText"]) == "这是模拟器里 1999 的当前画面。按规则只回一个 JSON 对象。";
            });
            b.Chk("mumu 不喂 hasBuilder：默认值按路派生(闸①不许把这一路当'树上没有')", delegate
            {
                Dictionary<string, object> k = GreenMumu();
                k.Remove("hasBuilder");
                Dictionary<string, object> d = Row(Live.RescueContext(k));
                return d != null && Gd.Truthy(d["CanAsk"]) && S(d["Skip"]) == "";
            });
            b.Chk("mumu 喂了 scene/userText 就用喂的(两份默认只在缺键时垫，转达不改写)", delegate
            {
                Dictionary<string, object> k = GreenMumu();
                k["scene"] = "就这一行"; k["userText"] = "找出挡路的弹窗";
                Dictionary<string, object> d = Row(Live.RescueContext(k));
                return S(d["Scene"]) == "就这一行" && S(d["UserText"]) == "找出挡路的弹窗";
            });
            b.Chk("闸②开关：mumu 那句带得出自己的键名，与 ark 那句不同(t13 那类断言按键名认)", delegate
            {
                Dictionary<string, object> k = GreenMumu(); k["enabled"] = false;
                return SkipOf(k) == "mumuVisionRescue=false，模拟器这一路的视觉兜底已关闭";
            });
            b.Chk("闸③管理员：mumu 这一路整个跳过(isAdmin=false 也不拦，退到后面那格)", delegate
            {
                Dictionary<string, object> k = GreenMumu(); k["isAdmin"] = false;
                Dictionary<string, object> d = Row(Live.RescueContext(k));
                return d != null && Gd.Truthy(d["CanAsk"]);
            });
            b.Chk("闸⑥对象在场：那句用 mumu 的靶子名开头(通用层拼的是 $t.Name)", delegate
            {
                Dictionary<string, object> k = GreenMumu(); k["alive"] = false;
                return SkipOf(k) == "MuMu 模拟器画面已经不在了，兜底没有对象";
            });
            b.Chk("mumu 的 alive 与端点**两样都没给** = 编程期抛(端点三源两正则不住这一侧，也不许猜一个'在'把兜底放行)", delegate
            {
                Dictionary<string, object> k = GreenMumu();
                k.Remove("alive");
                try { Live.RescueContext(k); return false; }
                catch (GdRuleException ex) { return S(ex.Message).Contains("要么把 adb/serial 喂进来"); }
            });
            // 10-12 实测改口那格的正面锚：喂了端点、不喂 alive ⇒ 由这一侧现查(与 ark 那一路"现枚举进程"同形)。
            // 无头安全：`adb` 指一棵不存在的树 ⇒ MumuAlive 在 File.Exists 那一步就回假，一趟 native 都不起
            // (§11.42 ①-9 那条射程边界：devices/connect 那两次真 adb 调用不在这一组的射程里)。
            b.Chk("mumu 喂了 adb/serial 而不喂 alive：现查走这一侧，adb 路径不在场即'对象不在场'(不许抛、也不起进程)", delegate
            {
                Dictionary<string, object> k = GreenMumu();
                k.Remove("alive");
                k["adb"] = @"C:\gd-livetest-并不存在的\adb.exe";
                k["serial"] = "127.0.0.1:7555";
                Dictionary<string, object> d = Row(Live.RescueContext(k));
                return d != null && !Gd.Truthy(d["CanAsk"])
                       && S(d["Skip"]) == "MuMu 模拟器画面已经不在了，兜底没有对象";
            });
            b.Chk("alive 覆盖格仍然优先：喂了端点也不自查(这一格只给无头自检与夹具用)", delegate
            {
                Dictionary<string, object> k = GreenMumu();
                k["adb"] = @"C:\gd-livetest-并不存在的\adb.exe";
                k["serial"] = "127.0.0.1:7555";
                Dictionary<string, object> d = Row(Live.RescueContext(k));
                return d != null && Gd.Truthy(d["CanAsk"]) && S(d["Skip"]) == "";
            });
            b.Chk("已拍那句两路共用一枚助手(ark 的十五格回执走的也是它 ⇒ 逐字符等值有单一事实源)", delegate
            {
                return Live.ShotLogLine("端游窗口", 1600, 900, 120) == "视觉兜底：已拍端游窗口(1600x900，亮度 120)，交给模型判断"
                       && Live.ShotLogLine("MuMu 模拟器画面", 1280, 720, 88) == "视觉兜底：已拍MuMu 模拟器画面(1280x720，亮度 88)，交给模型判断";
            });
            b.Chk("mumu 尺寸闸那句与 ark 两句**不同源**(阈值同为 200x150，话术各说各的，不许顺手统一)", delegate
            {
                return Live.MuMuSizeWhy(200, 150) == "" &&
                       Live.MuMuSizeWhy(120, 80) == "模拟器截屏尺寸异常 120x80(安卓那边还没起画面)" &&
                       Live.MuMuSizeWhy(120, 80) != Live.ShotSizeWhy(120, 80, false) &&
                       Live.MuMuSizeWhy(1920, 149).Length > 0;
            });
            b.Chk("adb-connect：devices 列出这台设备 → Ok=true(判定段复用 parse-adb-devices，同装配直调)", delegate
            {
                string fake = FakeAdbExe(tmp);
                Dictionary<string, object> d = Row(Live.AdbConnect(In("adb", fake, "serial", "127.0.0.1:7555",
                    "noSpawn", true, "fakeDevicesOut", "List of devices attached\n127.0.0.1:7555\tdevice\n")));
                return d != null && Gd.Truthy(d["Ok"]) && d.Count == 1;
            });
            b.Chk("adb-connect：devices 只列了另一台 → Ok=false(真身那侧连不上也只 WARN 一句'仍继续')", delegate
            {
                string fake = FakeAdbExe(tmp);
                Dictionary<string, object> d = Row(Live.AdbConnect(In("adb", fake, "serial", "127.0.0.1:7555",
                    "noSpawn", true, "fakeDevicesOut", "List of devices attached\nemulator-5554\tdevice\n")));
                return d != null && !Gd.Truthy(d["Ok"]);
            });
            b.Chk("adb 不在场 / 没给 adb：两处都是编程期抛，前者那句整字面抄 Connect-GdAdb 的 throw", delegate
            {
                try
                {
                    Live.AdbConnect(In("adb", Path.Combine(tmp, "并不存在\\adb.exe"), "serial", "127.0.0.1:7555", "noSpawn", true));
                    return false;
                }
                catch (GdRuleException ex)
                {
                    string msg = "adb 不存在：" + Path.Combine(tmp, "并不存在\\adb.exe");
                    if (S(ex.Message) != msg) return false;
                }
                try { Live.AdbConnect(In("serial", "127.0.0.1:7555")); return false; }
                catch (GdRuleException ex) { return S(ex.Message).StartsWith("adb-connect：没给 adb"); }
            });
            b.Chk("adb-game-pid：stdout 有数字 → Found/Pid 两格；空 stdout → Found=false(= 旧语义的 $null)", delegate
            {
                Dictionary<string, object> a = Row(Live.AdbGamePid(In("adb", @"C:\fake\adb.exe",
                    "serial", "127.0.0.1:7555", "package", "com.pape.m9a", "noSpawn", true, "fakeOut", " 1234\n")));
                Dictionary<string, object> b2 = Row(Live.AdbGamePid(In("adb", @"C:\fake\adb.exe",
                    "serial", "127.0.0.1:7555", "package", "com.pape.m9a", "noSpawn", true, "fakeOut", "")));
                return Gd.Truthy(a["Found"]) && L(a["Pid"]) == 1234L && !Gd.Truthy(b2["Found"]);
            });
            b.Chk("mumu-screen-shot：没给 path 是规则错；缺 adb 那一格回'拍不了屏'并照旧叠一层'截图失败：'前缀", delegate
            {
                try { Live.MuMuShot(In("adb", @"C:\fake\adb.exe", "serial", "127.0.0.1:7555")); return false; }
                catch (GdRuleException ex) { if (!S(ex.Message).StartsWith("mumu-screen-shot：没给 path")) return false; }
                Dictionary<string, object> d = Row(Live.MuMuShot(In("path", @"C:\gd-none.png", "serial", "127.0.0.1:7555")));
                return S(d["Why"]) == "adb 或序列号没定下来，拍不了屏" &&
                       S(d["Skip"]) == "截图失败：adb 或序列号没定下来，拍不了屏" &&
                       !Gd.Truthy(d["Ok"]) && !Gd.Truthy(d["CanContinue"]) && S(d["LogLine"]) == "";
            });
            b.Chk("mumu-screen-shot 读图段：真解一张亮 PNG → 十五格、整块屏即客户区、签名 144 格、LogLine 带靶子名", delegate
            {
                string png = WriteGrayPng(tmp, "mumu-bright.png", 300, 200, 20);
                Dictionary<string, object> d = Row(Live.MuMuShot(In("path", @"C:\gd-mumu\mumu-stall-1012060000-before.png",
                                                                   "localPng", png)));
                if (d == null) return false;
                if (d.Count != 15) return false;
                if (!Gd.Truthy(d["Ok"]) || !Gd.Truthy(d["CanContinue"]) || S(d["Skip"]) != "") return false;
                if (S(d["Path"]) != @"C:\gd-mumu\mumu-stall-1012060000-before.png") return false;   // Path 仍是编排给的那条
                if (L(d["ImgW"]) != 300L || L(d["ImgH"]) != 200L) return false;
                if (L(d["ClientLeft"]) != 0L || L(d["ClientTop"]) != 0L) return false;              // 真身:109 那四格
                if (L(d["ClientW"]) != 300L || L(d["ClientH"]) != 200L) return false;
                if (L(d["AvgLum"]) != 20L) return false;
                string[] parts = S(d["Sig"]).Split(',');
                if (parts.Length != 144) return false;
                int hit = 0;
                foreach (string p in parts) { if (p == "0") continue; if (p != "2") return false; hit++; }   // 20/8=2.5 → 取偶 2
                if (hit == 0) return false;
                if (S(d["Hash"]).Length != 32) return false;
                return S(d["LogLine"]) == "视觉兜底：已拍MuMu 模拟器画面(300x200，亮度 20)，交给模型判断";
            });
            b.Chk("mumu 也过黑屏闸与尺寸闸：暗 PNG(亮度 3)拦、120x80 拦，两句都是通用层那句/自家那句", delegate
            {
                Dictionary<string, object> dark = Row(Live.MuMuShot(In("path", @"C:\gd-none.png",
                    "localPng", WriteGrayPng(tmp, "mumu-dark.png", 260, 200, 3))));
                Dictionary<string, object> tiny = Row(Live.MuMuShot(In("path", @"C:\gd-none.png",
                    "localPng", WriteGrayPng(tmp, "mumu-tiny.png", 120, 80, 90))));
                if (!Gd.Truthy(dark["Ok"])) return false;                      // 暗图拍到了，只是不该问模型
                if (Gd.Truthy(dark["CanContinue"]) || S(dark["Skip"]) != Live.BlackScreenSkip(3)) return false;
                return !Gd.Truthy(tiny["Ok"]) && S(tiny["Why"]) == "模拟器截屏尺寸异常 120x80(安卓那边还没起画面)"
                       && S(tiny["Skip"]) == "截图失败：模拟器截屏尺寸异常 120x80(安卓那边还没起画面)";
            });
            b.Chk("mumu-screen-click 三句分派：0 → 点进虚拟机、非 0 → 带退出码、缺 serial → 通道断了(出口只 Mode/Sent 两格)", delegate
            {
                Dictionary<string, object> ok = Row(Live.MuMuClick(In("clientX", 880, "clientY", 210,
                    "adb", @"C:\fake\adb.exe", "serial", "127.0.0.1:7555", "fakeCode", 0)));
                Dictionary<string, object> bad = Row(Live.MuMuClick(In("clientX", 880, "clientY", 210,
                    "adb", @"C:\fake\adb.exe", "serial", "127.0.0.1:7555", "fakeCode", 1)));
                Dictionary<string, object> gone = Row(Live.MuMuClick(In("clientX", 1, "clientY", 2, "adb", @"C:\fake\adb.exe")));
                if (ok.Count != 2 || bad.Count != 2 || gone.Count != 2) return false;
                return S(ok["Mode"]) == "adb input tap(点进虚拟机，不碰宿主机窗口)" && Gd.Truthy(ok["Sent"])
                       && S(bad["Mode"]) == "adb input tap 失败(退出码 1)" && !Gd.Truthy(bad["Sent"])
                       && S(gone["Mode"]) == "adb 通道断了" && !Gd.Truthy(gone["Sent"]);
            });

            int rc = b.Result("--livetest");
            try { if (Directory.Exists(tmp)) Directory.Delete(tmp, true); } catch { }
            return rc;
        }

        // 这一枚与下面 LWaitJson／LClearJson／LBlankJson 三枚纯读数夹具改 internal，是给 --roundtest 用的
        // (B-3②-b2)：Chain.LauncherReady 的相位就靠这几枚钉住，判定本体仍一枚不在这儿重写。
        // ⚠ **只借这四枚，LaunchKnobs／AskKnobs 不借** —— 它们的 scriptPath 钉死 gd-livetest-<pid>\ask.py，
        // 而 --roundtest 是另一个进程、另一棵临时树，直接复用会让 VisionAsk 的第二道前置当场红
        // ("缺少 tools\vision\ask.py")、相位一律落 uncertain —— 那是自检验证不了的东西，不是 Chain 的缺陷。
        internal static Dictionary<string, object> FixShot(params object[] kv)
        {
            Dictionary<string, object> d = In("Ok", true, "CanContinue", true, "Path", @"C:\launcher.png",
                "ImgW", 1600, "ImgH", 900, "ClientLeft", 0, "ClientTop", 0, "ClientW", 1600, "ClientH", 900,
                "AvgLum", 120, "Skip", "", "Why", "", "Sig", "5,5,5,5", "Hash", "");
            for (int i = 0; i + 1 < kv.Length; i += 2) d[(string)kv[i]] = kv[i + 1];
            return d;
        }

        static Dictionary<string, object> LaunchKnobs(string fakeOut, int clicksAlready, params object[] shotKv)
        {
            Dictionary<string, object> knobs = AskKnobs(fakeOut);
            knobs["handle"] = 987654321L;                 // 假句柄：有它才不走枚举，也不会碰真窗口
            knobs["path"] = Path.Combine(Path.GetTempPath(), "gd-launcher-clamp.png");   // 不喂这一格就会去读 GdEnv.Root
            knobs["shot"] = shotKv == null || shotKv.Length == 0 ? FixShot() : FixShot(shotKv);
            knobs["clicksAlready"] = clicksAlready;
            knobs["maxClicks"] = 6;
            return knobs;
        }

        const string LClickJson =
            @"{""ok"":true,""text"":""{\""blocking\"":true,\""what\"":\""资源更新页，要点更新\"",\""clicks\"":[{\""x\"":500,\""y\"":820,\""why\"":\""『更新』按钮\""}]}\"""",""model"":""q""}";
        internal const string LWaitJson =
            @"{""ok"":true,""text"":""{\""blocking\"":true,\""what\"":\""正在下载资源 47%\"",\""clicks\"":[]}\"""",""model"":""q""}";
        internal const string LClearJson =
            @"{""ok"":true,""text"":""{\""blocking\"":false,\""what\"":\""已经在游戏主界面\"",\""clicks\"":[]}\"""",""model"":""q""}";
        const string LOobJson =
            @"{""ok"":true,""text"":""{\""blocking\"":true,\""what\"":\""更新页\"",\""clicks\"":[{\""x\"":1500,\""y\"":-20,\""why\"":\""越界的假坐标\""}]}\"""",""model"":""q""}";
        internal const string LBlankJson = @"{""ok"":true,""text"":"""",""model"":""q""}";

        // 取 Stats/子表的整数格(Stats 的 AvgLum 落 long，这里统一按 long 比)
        static long Row2(Dictionary<string, object> d, string k) { return Convert.ToInt64(d[k], System.Globalization.CultureInfo.InvariantCulture); }

        // 问模型夹具档：每一格都喂满，漏一格就会去读真 settings / 真 Python / 真目录
        static Dictionary<string, object> AskKnobs(string fakeOut)
        {
            string tmp = Path.Combine(Path.GetTempPath(), "gd-livetest-" + System.Diagnostics.Process.GetCurrentProcess().Id);
            return In("imagePath", @"C:\shot.png", "hasKey", true, "pythonExe", @"C:\fake\python.exe",
                      "scriptPath", Path.Combine(tmp, "ask.py"), "askDir", tmp,
                      "noSpawn", true, "model", "m-fake", "timeoutSeconds", 45, "scene", "H", "userText", "U",
                      "fakeCode", 0, "fakeOut", fakeOut, "fakeErr", "");
        }

        // ---------- B-2d③ mumu 这一路的夹具 ----------
        // 六道闸的 mumu 绿档：与 Green() 同一套"喂全"纪律，但**一枚 isAdmin 都不喂** ——
        // 这一路的 needsAdmin 默认就是 false，喂了反而钉不住"默认按路派生"那一格。
        static Dictionary<string, object> GreenMumu()
        {
            return In("target", "mumu", "hasBuilder", true, "enabled", true,
                      "hasKey", true, "pythonExe", @"C:\fake\python.exe", "alive", true);
        }

        // adb 的可执行文件只要"在"，三道现场闸才拦得住；夹具档 noSpawn 之下**永远不会被执行**。
        static string FakeAdbExe(string tmp)
        {
            string p = Path.Combine(tmp, "adb.exe");
            if (!File.Exists(p)) File.WriteAllText(p, "");
            return p;
        }

        // 一张纯灰 PNG：读图段要真解码(不是给一个假读数)，尺寸闸与亮度算术因此都走的是真身那套。
        static string WriteGrayPng(string tmp, string file, int w, int h, int gray)
        {
            string p = Path.Combine(tmp, file);
            Bitmap bmp = new Bitmap(w, h);
            try
            {
                for (int y = 0; y < h; y++) for (int x = 0; x < w; x++) bmp.SetPixel(x, y, Color.FromArgb(gray, gray, gray));
                bmp.Save(p, System.Drawing.Imaging.ImageFormat.Png);
            }
            finally { bmp.Dispose(); }
            return p;
        }
    }
}
