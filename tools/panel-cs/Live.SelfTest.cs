// tools/panel-cs/Live.SelfTest.cs —— --livetest：B-2b 现场件(Live.cs)七枚 verb 的纯规则段自检。
//
// 这一批和前十批不一样：Live.cs 里**大半代码天生测不了**(拍窗口要点真窗口、问模型要起真子进程、
// 抢前台会动别人的焦点)。所以这里定的纪律是"只钉话术与算术，不钉现场"：
//   * 不碰真窗口(不 PrintWindow、不 EnumWindows 拿真句柄用、不 SetCursorPos)、不起子进程、不联网；
//   * 不要求这台机器装了 Python、有 key、端游在跑、是管理员 —— 这些**全部以夹具注入**(旋钮身份见 Live.cs 头注)；
//   * 不调 settings：八档夹具里每一档都把现场量喂全，只要有一格漏喂，Settings()/PythonExe() 就会去读真配置，
//     那条断言立刻红(根没 Init，读会抛) —— 这正是"只读自检不许要求机器现场状态"的守门方式。
//   * 唯一写盘的那一格打在 %TEMP% 副本上(请求目录裁剪要有真文件才裁得动)，收尾整目录删掉，不落仓内。
//
// 期望串逐字抄自 lib\vision-ark.ps1 / lib\vision-rescue.ps1 的旧实现(那两个文件在生产接线 B-2c 之前仍是事实源)
// —— 这里红了就是移植走样，改 Live.cs，不改这里的期望(批次 1 起同一条纪律)。
// 两处专门钉住的原样怪癖：①"截图失败："双前缀异常路径会叠两次(ShotFail)；②处境串里那两个空格
// ("$UserText  自动化这边的处境：")。谁"顺手修好"了这里立刻红。
// 够不着的三样如实登记，不假装测过：黑屏闸的 Skip 只钉句子(常量件 BlackScreenSkip)、
// ark-window-shot / ark-window-click / ark-grab-foreground 的现场分支要等 B-2b④ 的门与一轮真跑。

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
            Console.WriteLine("---- panel-cs --livetest(B-2b 现场件七枚的纯规则段，不碰真窗口/不起子进程/不联网) ----");
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

            int rc = b.Result("--livetest");
            try { if (Directory.Exists(tmp)) Directory.Delete(tmp, true); } catch { }
            return rc;
        }

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
    }
}
