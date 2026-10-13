// tools/panel-cs/Round.SelfTest.cs —— --roundtest：B-3②-a(Round.cs)轮骨架现场七枚 + ②-b1(Round.Data.cs)
// 判定段/门表段数据面六枚的纯规则段自检。
//
// 体裁照 --livetest(§11.50 ②-1 与 Live.SelfTest.cs 头注那条纪律)：**只钉话术与算术，不钉现场**。
//   * 不真弹 UAC(noSpawn)、不真打 webhook(fakeWebhookErr)、不起气泡(noSend 或 fakeToastErr)、
//     不真杀进程(noKill)、不读真计划任务(fake 档)；
//   * ②-b1 那六枚同一条规矩：**不真打 MAA 活动接口**(apiJson/apiErr 两枚夹具)、
//     **缓存与活动数据读盘**要么给 cacheJson/dataJson 夹具、要么把路径指到临时树上的那份文件；
//     写盘只写临时树的 state\，收尾整目录删掉；
//   * 不要求这台机器装了 pwsh7、有管理员、有某个任务、有某个进程 —— 现场量一律夹具注入(旋钮身份见 Round.cs 头注)；
//   * 临时树里**故意不建 config\settings.json**：只要有一格漏喂 settings 夹具，SettingsNow()/SafeSettings()
//     就会去读那棵空树 ⇒ 当场红(那里还有一格专门钉这条守门，②-b1 的 read-ark-activity 同样有一格)。
//     唯一的例外是回落名册那一格，它喂 settings 而不靠现读；
//   * 写盘的几枚(state\last-notice.json、state\running.json、②-b1 的三本门表)全打在 InitForTest 的临时树上。
//
// 期望串逐字抄自 lib\common.ps1 的旧实现(Start-GdElevatedSelf / Send-GdNotice / Start-GdRunLock /
// Stop-GdRunLock / Get-GdTaskState / Stop-GdProcessTree / Get-GdElevateBatchLine)、lib\modules.ps1 的
// Invoke-GdDailyRound(通知标题那两句)，以及 ②-b1 的六份真身(Get-ArkEvent / Get-R1999Event /
// Set-GdArkShopState / Set-GdR1999Ack / Set-GdR1999ShopState / Get-Gd{Ack,ShopDone}Map)——
// 生产接线(②-d)之前它们仍是事实源，这里红了就是移植走样，
// 改 Round.cs / Round.Data.cs，不改这里的期望(批次 1 起同一条纪律)。
//
// 专门钉住的原样怪癖：① `-ne $false` 由**左侧类型**决定怎么折 $false ⇒ 字符串 "0" 算**开着**
// (FlagEnabled 第一版把数字 TryParse 摊到字符串上就判反了，这一格就是那次的守门)；
// ② Get-GdElevateBatchLine 的 '{0} {1}' 在空 ArgList 时留一个**尾随空格**，不 trim；
// ③ Stop-GdProcessTree 名曰树、实为按进程名，返回的是**匹配到的进程数**(Killed 与 Matched 同值就是这一格)；
// ④ activityCacheMaxHours 缺键＝0 ⇒ 缓存永远算过期，且那句 WARN 里插的是**原值**(缺键时是**空**，不是 0)；
// ⑤ 领空话术里的 dow 用原始 0-6(判定层用"周日=7")；⑥ 1999 确认话术那个箭头前没有空格；
// ⑦ 门表是**不分大小写**的表(PS 的 @{})；⑧ [math]::Round 与 Math.Round 同用银行家舍入(1.25→1.2)。
//
// 够不着的几样如实登记，不假装测过：
//   * 提权两支的真身(本机 pwsh runas / cmd 载体)与"读不到退出码"那一抛 —— 要等 ②-b 的 UAC 实弹；
//   * webhook 真 POST、气泡真弹、Schedule.Service COM 真读、Get-Process 真匹配 —— 夹具档一律绕开；
//   * 判活两把尺(unhandledMeansAlive)的分岔只在"读不到进程细节"那一档(提权轮/完整性级别不同)，
//     自检造不出那种进程，只钉"能读到时两把尺同值"；面板那侧的活体读回由 --exttest 五格随行自证。
//   * **真打 MAA 活动接口那一条 catch**(Invoke-WebRequest 的 Exception.Message 长什么样) —— 夹具只钉了话术的壳；
//   * **真 settings/真 state 盘**(临时树上的三本门表不等于玩家机器上那三本) —— 要等 ②-d 转接后的实跑；
//   * PS 那侧**还在按老路读写**这两张表(转接前双写不算，这里的"落盘"只证明形状与文件名对得上)。
// 本套件里唯一一次起子进程是 DeadPid() 那一发 `cmd /d /c exit`(瞬退、无窗)：残留那一格没有别的取法，
// 与 --exttest 的同名助手同一先例。

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Text;

namespace GdPanel
{
    static class RoundSelfTest
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
        static bool B(object v) { return v != null && Gd.Truthy(v); }
        static int I(object v) { return Gd.Int(v, int.MinValue); }
        static object Key(object data, string k) { return Gd.Get(Row(data), k); }
        static int Len(object v) { return Gd.List(v).Length; }
        // 门表回的是**字典**，Len() 对字典会算"一个元素"(Gd.List 那条防摊开的守卫)，比格数必须用这把尺：
        // 不是字典回 -1(形状错要红得明显)，是字典回格数(对齐 PS 的 $map.Count)。
        static int Cnt(object v)
        {
            Dictionary<string, object> d = v as Dictionary<string, object>;
            return d == null ? -1 : d.Count;
        }

        // ②-b1 数据面那六枚的回执里混着"小数(小时/天数)"与"话术数组"，这两把尺只为读它们而存在：
        //   Dbl = 取数(缺键回 NaN，好让"该有数却没了"红得明显)；Ln = 取 Lines 里第 i 句(越界回 null)。
        static double Dbl(object v)
        {
            if (v == null) return double.NaN;
            try { return Convert.ToDouble(v, CultureInfo.InvariantCulture); } catch { return double.NaN; }
        }
        static string Ln(object v, int i)
        {
            object[] a = Gd.List(v);
            return i < a.Length ? S(a[i]) : null;
        }
        // 抛出去的那句原话要逐字比(不只看"抛没抛")：Assert-GdGameId 的文案就是磁盘侧的用户可见话术。
        static object ThrownMsg(Func<object> run)
        {
            try { run(); return null; }
            catch (Exception ex) { return ex.Message; }
        }

        // 哨兵串：只许住在夹具里，**绝不许出现在回执里**(泄漏面自己先过一道)。
        const string KeySentinel = "SENTINEL-VISION-KEY";
        const string HookSentinel = "https://sentinel.webhook.invalid/notify";

        // 拿一个已经退出的 PID(残留那一格没有别的取法；与 Panel.SelfTest 的 DeadPid 同一先例)
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

        static bool Thrown<T>(Func<object> run) where T : Exception
        {
            try { run(); return false; }
            catch (Exception ex) { return ex is T; }
        }

        public static int RoundTest()
        {
            Box b = new Box();
            Console.WriteLine("---- panel-cs --roundtest(B-3②-a 轮骨架现场七枚 + ②-b1 判定段/门表段数据面六枚的纯规则段：不弹 UAC/不打 webhook/不起气泡/不真杀进程/不读真任务/不打活动接口) ----");
            int mine = Process.GetCurrentProcess().Id;
            string tmp = Path.Combine(Path.GetTempPath(), "gd-roundtest-" + mine);
            try { if (Directory.Exists(tmp)) Directory.Delete(tmp, true); } catch { }
            try { Directory.CreateDirectory(tmp); } catch (Exception ex)
            {
                GdLog.Line("--roundtest 连临时树都建不起来(后面写盘那几格必红)：" + ex.Message, "ERROR");
            }
            // 临时树里只造"回落名册"那一格要的 lib\maa.ps1；**故意不造 config\settings.json**。
            try
            {
                Directory.CreateDirectory(Path.Combine(tmp, "lib"));
                File.WriteAllText(Path.Combine(tmp, "lib", "maa.ps1"), "#");
            }
            catch { }
            GdEnv.InitForTest(tmp);
            string state = GdEnv.State;
            string lockFile = Path.Combine(state, "running.json");
            string noticeFile = Path.Combine(state, "last-notice.json");
            // ②-b1 数据面那五本账的落点：全在临时树的 state\ 上，收尾随整目录一起删。
            string ackR1999 = Path.Combine(state, "r1999-event-ack.json");
            string shopArk = Path.Combine(state, "ark-shop-done.json");
            string shopR1999 = Path.Combine(state, "r1999-shop-done.json");
            string probeFile = Path.Combine(state, "ark-event-probe.json");
            int dead = DeadPid();

            // ==================== 1) FlagEnabled：PS 的 `-ne $false` 语义 ====================
            b.Chk("缺键/null 算开着(清单没写 flag 的旧写法合法)", delegate { return Round.FlagEnabled(null); });
            b.Chk("空串算开着(\"\" 与 \"False\" 不等)", delegate { return Round.FlagEnabled(""); });
            b.Chk("真布尔 $false 才算关", delegate { return !Round.FlagEnabled(false) && Round.FlagEnabled(true); });
            b.Chk("字符串 false/FALSE(不分大小写)算关", delegate
            {
                return !Round.FlagEnabled("false") && !Round.FlagEnabled("FALSE");
            });
            b.Chk("原样怪癖：字符串 \"0\" 算**开着**(左侧是串时 $false 折成 \"False\"，不比数值)", delegate
            {
                return Round.FlagEnabled("0");
            });
            b.Chk("数字 0 / 0.0 / 0D 算关(左侧是数时 $false 折成 0)", delegate
            {
                return !Round.FlagEnabled(0) && !Round.FlagEnabled(0.0) && !Round.FlagEnabled(0m);
            });
            b.Chk("数字 1/2 与字符串 true 都算开着", delegate
            {
                return Round.FlagEnabled(1) && Round.FlagEnabled(2) && Round.FlagEnabled("true");
            });
            b.Chk("认不出的形状(数组)按开着收(旧路不会因此拦下任何一轮)", delegate
            {
                return Round.FlagEnabled(new object[] { "x" });
            });

            // ==================== 2) ParseLock：运行锁读侧唯一真身 ====================
            b.Chk("null／没有 pid 的标记 = 压根没标记(PS 那侧回 $null 那一格)", delegate
            {
                return !Round.ParseLock(null).Found
                    && !Round.ParseLock("{}").Found
                    && !Round.ParseLock(@"{""log"":""x""}").Found;
            });
            b.Chk("pid/started/games/tag 四格读回", delegate
            {
                Round.Lock lk = Round.ParseLock(@"{""pid"":4321,""started"":""2026-10-13 06:00:00"",""games"":""ark + r1999"",""tag"":""auto""}");
                return lk.Found && lk.Pid == 4321 && lk.Started == "2026-10-13 06:00:00"
                       && lk.Games == "ark + r1999" && lk.Tag == "auto";
            });
            b.Chk("键名不分大小写(ConvertFrom-Json 之后旧 -match 仍算同一枚)", delegate
            {
                Round.Lock lk = Round.ParseLock(@"{""PID"":7,""STARTED"":""s"",""GAMES"":""ark"",""TAG"":""t"",""LOG"":""l""}");
                return lk.Found && lk.Pid == 7 && lk.Started == "s" && lk.Games == "ark" && lk.Tag == "t" && lk.Log == "l";
            });
            b.Chk("log 的反斜杠是**双重转义还原**(还原后要能直接 File.Exists，不是 JSON 解析)", delegate
            {
                Round.Lock lk = Round.ParseLock(@"{""pid"":1,""log"":""C:\\logs\\run-20261013.log""}");
                return lk.Log == @"C:\logs\run-20261013.log";
            });
            b.Chk("pid 溢出(int.Parse 抛)判成没标记而不是把读侧带崩", delegate
            {
                return !Round.ParseLock(@"{""pid"":99999999999999,""games"":""ark""}").Found;
            });

            // ==================== 3) LockAt：判活 + 两把尺 ====================
            b.Chk("路径为空/文件不在 → 没标记(不抛)", delegate
            {
                return !Round.LockAt("", true).Found
                    && !Round.LockAt(Path.Combine(tmp, "ghost.json"), true).Found
                    && !Round.LockAt(null, false).Found;
            });
            b.Chk("活 PID(自己)判在跑：两把尺同值", delegate
            {
                string p = Path.Combine(tmp, "alive.json");
                File.WriteAllText(p, "{\"pid\":" + mine + ",\"games\":\"ark\"}", new UTF8Encoding(false));
                Round.Lock psRuler = Round.LockAt(p, false);
                Round.Lock panelRuler = Round.LockAt(p, true);
                return psRuler.Found && psRuler.Alive && panelRuler.Found && panelRuler.Alive && psRuler.Pid == mine;
            });
            if (dead > 0)
            {
                b.Chk("已退出的 PID 判残留=**没在跑**(否则一次硬杀把面板永久挡在门外)；两把尺这一格同值", delegate
                {
                    string p = Path.Combine(tmp, "dead.json");
                    File.WriteAllText(p, "{\"pid\":" + dead + ",\"games\":\"ark\"}", new UTF8Encoding(false));
                    return Round.LockAt(p, false).Found && !Round.LockAt(p, false).Alive
                        && Round.LockAt(p, true).Found && !Round.LockAt(p, true).Alive;
                });
            }
            else Console.WriteLine("  SKIP 拿不到一个已退出的 PID，残留那一格没测到(与 --exttest 同一条)");
            b.Chk("标记位置是个目录(读不动) → 判没标记不抛", delegate
            {
                string dir = Path.Combine(tmp, "weird-dir.json");
                Directory.CreateDirectory(dir);
                return !Round.LockAt(dir, true).Found;
            });
            Console.WriteLine("  登记：两把尺的分岔只在\"读不到进程细节\"那一档(提权轮/完整性级别不同)，自检造不出那种进程 ⇒ unobserved");

            // ==================== 4) OwnsText："只删自己登记的那一份" ====================
            b.Chk("{\"PID\":123} 也算自己那份(-match 不分大小写，K2 实测)", delegate
            {
                return Round.OwnsText(@"{""PID"":123,""games"":""ark""}", 123);
            });
            b.Chk("pid 123 不许认领 {\"pid\":1234}(尾随 [\\s,}] 那一格)", delegate
            {
                return !Round.OwnsText(@"{""pid"":1234,""games"":""ark""}", 123);
            });
            b.Chk("pid 123 也不许认领 {\"pid\":12}", delegate { return !Round.OwnsText(@"{""pid"":12}", 123); });
            b.Chk("自己那份：逗号收尾与右括号收尾都算", delegate
            {
                return Round.OwnsText(@"{""pid"":123,""tag"":""""}", 123) && Round.OwnsText(@"{""pid"":123}", 123);
            });
            b.Chk("null／没有 pid 的文本 = 不是自己的", delegate
            {
                return !Round.OwnsText(null, 123) && !Round.OwnsText("{}", 123);
            });

            // ==================== 5) resolve-round-context：逐枚点名取键 ====================
            // 名册现扫：回落那一格靠的就是临时树里只有 lib\maa.ps1 这一路库。
            b.Chk("点名的 runtime 键才回，缺的那枚回 null 但**键在场**", delegate
            {
                object d = Round.Context(In(
                    "settings", In("runtime", In("pythonExe", @"C:\fake\python.exe", "visionApiKey", KeySentinel)),
                    "runtimeKeys", new object[] { "pythonExe", "notThere" },
                    "isAdmin", false, "now", 0L));
                Dictionary<string, object> rt = Row(Key(d, "Runtime"));
                return rt != null && rt.Count == 2
                    && S(rt["pythonExe"]) == @"C:\fake\python.exe"
                    && rt.ContainsKey("notThere") && rt["notThere"] == null;
            });
            b.Chk("整块 runtime 绝不回给核：哨兵 key 在回执里搜不到(序列化后自证)", delegate
            {
                object d = Round.Context(In(
                    "settings", In("runtime", In("visionApiKey", KeySentinel, "pythonExe", @"C:\fake\python.exe")),
                    "runtimeKeys", new object[] { "pythonExe" },
                    "isAdmin", false, "now", 0L));
                return !GdJson.Serialize(d).Contains(KeySentinel);
            });
            b.Chk("动态 flag 在这一侧算完：显式 false 那枚落成 false，没点的落 true", delegate
            {
                object d = Round.Context(In(
                    "settings", In("runtime", In("arkRequiresElevation", false, "r1999RequiresElevation", "0")),
                    "elevFlags", new object[] { "arkRequiresElevation", "r1999RequiresElevation", "notThereFlag" },
                    "isAdmin", true, "now", 0L));
                Dictionary<string, object> el = Row(Key(d, "Elevation"));
                return el != null && el.Count == 3
                    && B(el["arkRequiresElevation"]) == false
                    && B(el["r1999RequiresElevation"])          // 原样怪癖：字符串 "0" 算开着
                    && B(el["notThereFlag"]);
            });
            b.Chk("IsAdmin 用夹具，不现读这台机器", delegate
            {
                object d = Round.Context(In("isAdmin", false, "now", 0L, "paths", In()));
                return Key(d, "IsAdmin") is bool && !(bool)Key(d, "IsAdmin");
            });
            b.Chk("在位性三档：在位→Ready／空串→Missing／填了但不在→Absent 带\"键 → 路径\"", delegate
            {
                string real = Path.Combine(tmp, "lib", "maa.ps1");
                string ghost = Path.Combine(tmp, "no-such-dir");
                object d = Round.Context(In(
                    "paths", In("maaDir", real, "m9aDir", "", "mumuDir", ghost),
                    "pathKeys", new object[] { "maaDir", "m9aDir", "mumuDir" },
                    "isAdmin", false, "now", 0L));
                Dictionary<string, object> ready = Row(Key(d, "Ready"));
                Dictionary<string, object> paths = Row(Key(d, "Paths"));
                object[] missing = Gd.List(Key(d, "Missing"));
                object[] absent = Gd.List(Key(d, "Absent"));
                return ready != null && (bool)ready["maaDir"] && !(bool)ready["m9aDir"] && !(bool)ready["mumuDir"]
                    && paths != null && paths.ContainsKey("m9aDir") && paths["m9aDir"] == null   // 空串那枚回 null
                    && missing.Length == 1 && S(missing[0]) == "m9aDir"
                    && absent.Length == 1 && S(absent[0]) == "mumuDir → " + ghost;
            });
            b.Chk("RequiredPathKeys 原样回点名那几枚(不新建第三棵名册)", delegate
            {
                object d = Round.Context(In("pathKeys", new object[] { "maaDir", "m9aDir" }, "isAdmin", false, "now", 0L, "paths", In()));
                object[] keys = Gd.List(Key(d, "RequiredPathKeys"));
                return keys.Length == 2 && S(keys[0]) == "maaDir" && S(keys[1]) == "m9aDir";
            });
            b.Chk("没点名 pathKeys → 回落这棵树真装了哪几路(临时树只有 lib\\maa.ps1)", delegate
            {
                object d = Round.Context(In("paths", In("maaDir", Path.Combine(tmp, "lib", "maa.ps1"), "m9aDir", ""),
                                            "isAdmin", false, "now", 0L));
                object[] keys = Gd.List(Key(d, "RequiredPathKeys"));
                return keys.Length == 1 && S(keys[0]) == "maaDir";
            });
            b.Chk("now 直通：NowMs 是喂进去的那枚，NowText/Dow 与它自洽(不现读挂钟)", delegate
            {
                long ms = 1760000000000L;
                object d = Round.Context(In("isAdmin", false, "now", ms, "paths", In()));
                long back = Convert.ToInt64(Key(d, "NowMs"), CultureInfo.InvariantCulture);
                DateTime local = DateTimeOffset.FromUnixTimeMilliseconds(ms).LocalDateTime;
                return back == ms
                    && S(Key(d, "NowText")) == local.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture)
                    && I(Key(d, "Dow")) == (int)local.DayOfWeek;
            });
            b.Chk("没喂 now 才现读挂钟：NowMs 落在现在±5 分钟，且与 NowText 同一时刻", delegate
            {
                object d = Round.Context(In("isAdmin", false, "paths", In()));
                long back = Convert.ToInt64(Key(d, "NowMs"), CultureInfo.InvariantCulture);
                long nowBack = Convert.ToInt64(
                    new DateTimeOffset(DateTime.Now).ToUnixTimeMilliseconds(), CultureInfo.InvariantCulture);
                bool near = Math.Abs(nowBack - back) < 300000L;
                DateTime parsed;
                bool shape = DateTime.TryParseExact(S(Key(d, "NowText")), "yyyy-MM-dd HH:mm:ss",
                    CultureInfo.InvariantCulture, DateTimeStyles.None, out parsed);
                return near && shape && Math.Abs((parsed - DateTimeOffset.FromUnixTimeMilliseconds(back).LocalDateTime).TotalSeconds) < 2.0;
            });
            b.Chk("守门：漏喂 settings 又要点 paths → 去读临时树那份(不在)当场抛，不静默给空 ctx", delegate
            {
                return Thrown<FileNotFoundException>(delegate
                {
                    return Round.Context(In("pathKeys", new object[] { "maaDir" }, "isAdmin", false, "now", 0L));
                });
            });

            // ==================== 6) NoticeTitle / BatchLine(两枚内部纯件) ====================
            b.Chk("标题两句照 modules.ps1：全成\"…日常已完成\"、否则\"…日常需要人工看一眼\"", delegate
            {
                return Round.NoticeTitle("方舟", true) == "方舟日常已完成"
                    && Round.NoticeTitle("方舟", false) == "方舟日常需要人工看一眼";
            });
            b.Chk("owner 为 null 也只丢 owner，后半句还在(${NoticeOwner} 那笔怪癖只有一个来源)", delegate
            {
                return Round.NoticeTitle(null, true) == "日常已完成";
            });
            b.Chk("BatchLine：不带空格不加引号，参数用一个空格拼", delegate
            {
                return Round.BatchLine(@"C:\f\pwsh.exe", new List<string> { "-File", "x.ps1" }) == @"C:\f\pwsh.exe -File x.ps1";
            });
            b.Chk("BatchLine：带空格整串加引号(Program Files 那一路)；参数**不另加引号**(旧路就没加)", delegate
            {
                return Round.BatchLine(@"C:\Program Files\PowerShell\7\pwsh.exe", new List<string> { "-File", @"C:\a b.ps1" })
                    == "\"C:\\Program Files\\PowerShell\\7\\pwsh.exe\" -File C:\\a b.ps1";
            });
            b.Chk("原样怪癖：ArgList 为空时留一个尾随空格('{0} {1}' 不 trim)", delegate
            {
                return Round.BatchLine("pwsh.exe", new List<string>()) == "pwsh.exe ";
            });

            // ==================== 7) start-elevated-round(noSpawn 档：一支都不真起) ====================
            b.Chk("noSpawn＋给了本机 pwsh → 载体 pwsh、Code=fakeCode、Lines 空", delegate
            {
                object d = Round.Elevate(In("noSpawn", true, "localPwsh", @"C:\f\pwsh.exe",
                                            "batchPwsh", @"C:\f\pwsh.exe", "fakeCode", 7,
                                            "argList", new object[] { "-File", @"C:\r\daily-clear.ps1" }));
                return I(Key(d, "Code")) == 7 && S(Key(d, "Carrier")) == "pwsh"
                    && Len(Key(d, "Lines")) == 0
                    && S(Key(d, "BatchLine")) == @"C:\f\pwsh.exe -File C:\r\daily-clear.ps1";
            });
            b.Chk("本机 pwsh 不在场(商店版被跳过就是这一格) → 载体落 cmd", delegate
            {
                object d = Round.Elevate(In("noSpawn", true, "localPwsh", "", "batchPwsh", "pwsh.exe",
                                            "argList", new object[] { "-File", "x.ps1" }, "fakeCode", 0));
                return S(Key(d, "Carrier")) == "cmd" && S(Key(d, "BatchLine")) == "pwsh.exe -File x.ps1";
            });
            b.Chk("给了 argList 就用它：**空数组也算给**(此时不许再要 script)", delegate
            {
                object d = Round.Elevate(In("noSpawn", true, "localPwsh", @"C:\f\pwsh.exe",
                                            "batchPwsh", @"C:\f\pwsh.exe", "argList", new object[0]));
                return S(Key(d, "BatchLine")) == @"C:\f\pwsh.exe " && I(Key(d, "Code")) == 0;
            });
            b.Chk("没给 argList → 回退 build-relaunch-args 同装配拼参(-Force 那几枚排在 -File 之后)", delegate
            {
                object d = Round.Elevate(In("noSpawn", true, "localPwsh", @"C:\f\pwsh.exe",
                                            "batchPwsh", @"C:\f\pwsh.exe",
                                            "script", @"C:\r\daily-clear.ps1",
                                            "force", new object[] { "-NoElevate" },
                                            "entries", new object[] { In("K", "Ids", "T", "string[]", "V", "ark") }));
                return S(Key(d, "BatchLine"))
                    == @"C:\f\pwsh.exe -NoProfile -ExecutionPolicy Bypass -File ""C:\r\daily-clear.ps1"" -NoElevate -Ids ""ark""";
            });
            b.Chk("既没 argList 也没 script → 前置话术(退出码 2 那一档)，不猜空参数就提权", delegate
            {
                return Thrown<GdRuleException>(delegate { return Round.Elevate(In("noSpawn", true)); });
            });
            b.Chk("batchPwsh 的默认值不许落宿主自己的 exe：要么落 pwsh，要么照旧那句抛(两种现场都算过)", delegate
            {
                // 这一格钉的是 CLI 冒烟抓到的一处移植走样：真身的第一级 (Get-Process -Id $PID).Path 在 PS 宿主里
                // 取到的是"跑脚本的解释器"，照搬到 panel-cs.exe 里就变成面板自己的绝对路径，
                // 批处理行会拼成 "panel-cs.exe -File x.ps1" —— 面板不认 -File，提权起来是一发空打。
                // 不许要求机器现场：这台有 pwsh 就断言落点里没宿主 exe；一个都没有就断言抛的是原话那句。
                try
                {
                    object d = Round.Elevate(In("noSpawn", true, "argList", new object[] { "-File", "x.ps1" }));
                    string line = S(Key(d, "BatchLine"));
                    string selfName = "panel-cs.exe";
                    try { selfName = Path.GetFileName(Process.GetCurrentProcess().MainModule.FileName); } catch { }
                    return line.IndexOf("panel-cs", StringComparison.OrdinalIgnoreCase) < 0
                        && line.IndexOf(selfName, StringComparison.OrdinalIgnoreCase) < 0
                        && line.IndexOf("pwsh", StringComparison.OrdinalIgnoreCase) >= 0;
                }
                catch (GdRuleException ex)
                {
                    return ex.Message != null && ex.Message.Contains("找不到 pwsh.exe");
                }
            });

            // ==================== 8) send-round-notice(全打临时树；noSend 挡气泡挡 POST) ====================
            b.Chk("默认档：owner 缺=每日、allOk 缺=全成 ⇒ 标题/等级走 OK 那支", delegate
            {
                object d = Round.Notice(In("noSend", true, "nowText", "2026-10-13 20:00:00"));
                return S(Key(d, "Title")) == "每日日常已完成" && S(Key(d, "Level")) == "OK"
                    && S(Key(d, "Time")) == "2026-10-13 20:00:00"
                    && S(Key(d, "FileKey")) == @"state\last-notice.json" && B(Key(d, "File"));
            });
            b.Chk("allOk=false 的标题与等级(需要人工看一眼 / WARN)", delegate
            {
                object d = Round.Notice(In("noSend", true, "owner", "方舟", "allOk", false, "text", "t"));
                return S(Key(d, "Title")) == "方舟日常需要人工看一眼" && S(Key(d, "Level")) == "WARN";
            });
            b.Chk("落盘是无条件的第一件事：两枚失败夹具都喂上，last-notice.json 仍写得出来且读得回", delegate
            {
                if (File.Exists(noticeFile)) File.Delete(noticeFile);
                object d = Round.Notice(In("noSend", false, "owner", "方舟", "allOk", true,
                                           "text", "方舟：fight\n详情", "nowText", "2026-10-13 20:00:01",
                                           "settings", In("notify", In("webhook", HookSentinel)),
                                           "fakeWebhookErr", "boom-hook", "fakeToastErr", "boom-toast"));
                Dictionary<string, object> back = Row(GdJson.Read(noticeFile));
                return S(Key(d, "File")) == "True" && back != null
                    && S(back["title"]) == "方舟日常已完成" && S(back["level"]) == "OK"
                    && S(back["text"]) == "方舟：fight\n详情" && S(back["time"]) == "2026-10-13 20:00:01";
            });
            b.Chk("落盘形状：我们这侧是单行(旧路 ConvertTo-Json 缩进版，差已在文件头登记)", delegate
            {
                string raw = File.ReadAllText(noticeFile);
                return raw.IndexOf('\n') < 0 && raw.StartsWith("{");
            });
            b.Chk("两把 try 分开：webhook 与气泡同时失败 → 两句都在，各判 failed(不许合并)", delegate
            {
                object d = Round.Notice(In("noSend", false, "text", "t", "nowText", "2026-10-13 20:00:02",
                                           "settings", In("notify", In("webhook", HookSentinel, "toast", true)),
                                           "fakeWebhookErr", "boom-hook", "fakeToastErr", "boom-toast"));
                object[] lines = Gd.List(Key(d, "Lines"));
                return S(Key(d, "Webhook")) == "failed" && S(Key(d, "Toast")) == "failed"
                    && lines.Length == 2
                    && S(lines[0]) == "webhook 推送失败：boom-hook"
                    && S(lines[1]) == "桌面通知发送失败(不影响日常本身)：boom-toast";
            });
            b.Chk("noSend 只把\"要打\"变成 skipped，失败夹具在它面前不参与(自检绝不联网)", delegate
            {
                object d = Round.Notice(In("noSend", true, "text", "t",
                                           "settings", In("notify", In("webhook", HookSentinel, "toast", true)),
                                           "fakeWebhookErr", "boom-hook", "fakeToastErr", "boom-toast"));
                return S(Key(d, "Webhook")) == "skipped" && S(Key(d, "Toast")) == "skipped"
                    && Len(Key(d, "Lines")) == 0;
            });
            b.Chk("没配 webhook = none(与 skipped 分得开：一个是没这回事，一个是有但不打)", delegate
            {
                object d = Round.Notice(In("noSend", false, "text", "t",
                                           "settings", In("notify", In("webhook", "", "toast", false))));
                return S(Key(d, "Webhook")) == "none" && S(Key(d, "Toast")) == "off";
            });
            b.Chk("toast 明确关掉 = off，noSend 也越不过它(真身 -not $settings -or toast 那一格)", delegate
            {
                object d = Round.Notice(In("noSend", true, "text", "t",
                                           "settings", In("notify", In("webhook", HookSentinel, "toast", false))));
                return S(Key(d, "Toast")) == "off" && S(Key(d, "Webhook")) == "skipped";
            });
            b.Chk("settings 读不到(临时树里没有那份)时按真身默认走：toast 视为开 ⇒ noSend 下 skipped", delegate
            {
                object d = Round.Notice(In("noSend", true, "text", "t"));
                return S(Key(d, "Toast")) == "skipped" && S(Key(d, "Webhook")) == "none";
            });
            b.Chk("安全边界：回执里搜不到 webhook URL(只有异常原话进 Lines)", delegate
            {
                object d = Round.Notice(In("noSend", false, "text", "t",
                                           "settings", In("notify", In("webhook", HookSentinel, "toast", true)),
                                           "fakeWebhookErr", "boom-hook", "fakeToastErr", "boom-toast"));
                return !GdJson.Serialize(d).Contains(HookSentinel);
            });

            // ==================== 9) start-run-lock / stop-run-lock(同一枚 running.json) ====================
            b.Chk("start-run-lock 必须显式给 pid：一次性 verb 把自己的 PID 登记成活锁=面板撞车保护失真", delegate
            {
                return Thrown<GdRuleException>(delegate { return Round.LockStart(In("games", "ark")); });
            });
            b.Chk("干净起跑：Wrote、键值原样、默认 games=all／log=当前日志(自检不落盘故空串)", delegate
            {
                if (File.Exists(lockFile)) File.Delete(lockFile);
                object d = Round.LockStart(In("pid", mine, "started", "2026-10-13 06:00:00", "games", "ark + r1999",
                                              "tag", "auto", "log", @"C:\logs\x.log"));
                Round.Lock lk = Round.LockAt(lockFile, false);
                return B(Key(d, "Wrote")) && !B(Key(d, "PrevAlive")) && Len(Key(d, "Lines")) == 0
                    && lk.Found && lk.Pid == mine && lk.Started == "2026-10-13 06:00:00"
                    && lk.Games == "ark + r1999" && lk.Tag == "auto" && lk.Log == @"C:\logs\x.log";
            });
            b.Chk("落盘单行、UTF-8 无 BOM(面板与 PS 都按原文正则读这一枚)", delegate
            {
                byte[] raw = File.ReadAllBytes(lockFile);
                string text = File.ReadAllText(lockFile);
                bool noBom = !(raw.Length > 2 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF);
                return noBom && text.IndexOf('\n') < 0 && text.StartsWith("{") && !Round.OwnsText(text, 999999);
            });
            b.Chk("已有一轮活着：**只说不拦**——那句 WARN 原话逐字、且照样登记自己(旧路不硬拦的理由是无人值守)", delegate
            {
                File.WriteAllText(lockFile, "{\"pid\":" + mine + ",\"started\":\"2026-10-13 05:00:00\",\"games\":\"ark\",\"tag\":\"auto\",\"log\":\"\"}",
                                  new UTF8Encoding(false));
                int other = dead > 0 ? dead : 999999;
                object d = Round.LockStart(In("pid", other, "started", "2026-10-13 06:00:00", "games", "all"));
                object[] lines = Gd.List(Key(d, "Lines"));
                bool wrote = B(Key(d, "Wrote")) && B(Key(d, "PrevAlive"));
                return wrote && lines.Length == 1
                    && S(lines[0]) == "已有一轮在跑(PID " + mine + "，2026-10-13 05:00:00 起，games=ark) —— 本轮照跑，两轮会抢同一个助手/端游"
                    && Round.LockAt(lockFile, false).Pid == other;      // 照跑=照覆盖
            });
            b.Chk("上一轮是残留(已退出)：不吭声、登记自己", delegate
            {
                if (dead > 0)
                {
                    File.WriteAllText(lockFile, "{\"pid\":" + dead + ",\"started\":\"2026-09-25 19:11:35\",\"games\":\"ark\",\"tag\":\"\",\"log\":\"\"}",
                                      new UTF8Encoding(false));
                    object d = Round.LockStart(In("pid", mine, "started", "2026-10-13 06:00:00", "games", "all"));
                    return B(Key(d, "Wrote")) && !B(Key(d, "PrevAlive")) && Len(Key(d, "Lines")) == 0;
                }
                return true;
            });
            b.Chk("stop-run-lock 也必须显式给 pid(撤锁只撤自己那一份)", delegate
            {
                return Thrown<GdRuleException>(delegate { return Round.LockStop(In()); });
            });
            b.Chk("撤自己那份：删得掉；键名大小写不同也算自己那份", delegate
            {
                File.WriteAllText(lockFile, "{\"PID\":" + mine + ",\"games\":\"ark\"}", new UTF8Encoding(false));
                object d = Round.LockStop(In("pid", mine));
                return B(Key(d, "Had")) && B(Key(d, "Removed")) && !File.Exists(lockFile);
            });
            b.Chk("别的轮已经覆盖了它 → Had 但**不删**(那等于把别人的撞车保护拆了)", delegate
            {
                File.WriteAllText(lockFile, "{\"pid\":" + mine + ",\"games\":\"ark\"}", new UTF8Encoding(false));
                object d = Round.LockStop(In("pid", 4321));
                return B(Key(d, "Had")) && !B(Key(d, "Removed")) && File.Exists(lockFile);
            });
            b.Chk("没有标记：Had=false/Removed=false，整枚 catch{} 不抛(收尾这一路绝不能被撤锁带崩)", delegate
            {
                try { if (File.Exists(lockFile)) File.Delete(lockFile); } catch { }
                object d = Round.LockStop(In("pid", mine));
                return !B(Key(d, "Had")) && !B(Key(d, "Removed"));
            });

            // ==================== 10) read-task-state(fake 档：一次 COM 都不碰) ====================
            b.Chk("read-task-state 必须给任务名(任务名属现场量)", delegate
            {
                return Thrown<GdRuleException>(delegate { return Round.TaskState(In("fake", In("state", "Ready"))); });
            });
            b.Chk("fake 档五格直通：Found/State/Last/Result/Next", delegate
            {
                object d = Round.TaskState(In("name", "GameDaily-Clear", "fake",
                    In("state", "Ready", "last", "2026-10-13 06:00:01", "result", 0, "next", "2026-10-14 06:00:00")));
                return B(Key(d, "Found")) && S(Key(d, "State")) == "Ready"
                    && S(Key(d, "Last")) == "2026-10-13 06:00:01" && I(Key(d, "Result")) == 0
                    && S(Key(d, "Next")) == "2026-10-14 06:00:00";
            });
            b.Chk("任务不在场 = Found:false(旧路回 $null 那一格)", delegate
            {
                object d = Round.TaskState(In("name", "Ghost-Task", "fake", In("found", false)));
                return !B(Key(d, "Found")) && S(Key(d, "State")) == "";
            });
            b.Chk("Result 读不出= null 而不是 0(旧路 try{[int]}catch{$null}：0 是\"成功\"的意思，别混)", delegate
            {
                object d = Round.TaskState(In("name", "T", "fake", In("state", "Unknown", "result", null)));
                return Key(d, "Result") == null;
            });
            b.Chk("TaskStateName 五档与 PS 的 TaskState 同一套序号", delegate
            {
                return Round.TaskStateName(0) == "Unknown" && Round.TaskStateName(1) == "Disabled"
                    && Round.TaskStateName(2) == "Queued" && Round.TaskStateName(3) == "Ready"
                    && Round.TaskStateName(4) == "Running";
            });
            b.Chk("越界的数原样回数字(PS 对枚举越界也回数字，这里不编新词)", delegate
            {
                return Round.TaskStateName(6) == "6" && Round.TaskStateName(-1) == "-1";
            });

            // ==================== 11) stop-process-tree(noKill 档：一枚进程都不碰) ====================
            b.Chk("stop-process-tree 必须给进程名(按进程名匹配，不给就是空打)", delegate
            {
                return Thrown<GdRuleException>(delegate { return Round.ProcTree(In("noKill", true)); });
            });
            b.Chk("noKill＋喂名册：Matched=Killed=匹配数(原样怪癖：返回的是\"匹配到的进程数\")", delegate
            {
                object d = Round.ProcTree(In("name", "pwsh", "noKill", true, "fakeProcs", new object[] { 1, 2, 3 }));
                return I(Key(d, "Matched")) == 3 && I(Key(d, "Killed")) == 3 && B(Key(d, "Skipped")) && !B(Key(d, "Force"));
            });
            b.Chk("Force 只是透传的那枚旗标，noKill 之下不改变计数", delegate
            {
                object d = Round.ProcTree(In("name", "pwsh", "noKill", true, "force", true,
                                             "fakeProcs", new object[] { 7 }));
                return I(Key(d, "Matched")) == 1 && I(Key(d, "Killed")) == 1 && B(Key(d, "Force"));
            });
            b.Chk("名册为空 = 0 枚(与真身\"$procs.Count -eq 0 → return 0\"同格)", delegate
            {
                object d = Round.ProcTree(In("name", "ghost-proc", "noKill", true, "fakeProcs", new object[0]));
                return I(Key(d, "Matched")) == 0 && I(Key(d, "Killed")) == 0;
            });

            // ==================== 12) CLI 接线：那七枚真的挂上了分发链(②-a 批) ====================
            b.Chk("resolve-round-context 走 --in/--out：退出码 0、信封 verb 对、data 里有 Runtime", delegate
            {
                string inPath = Path.Combine(tmp, "ctx-in.json");
                string outPath = Path.Combine(tmp, "ctx-out.json");
                GdJson.Write(inPath, In("settings", In("runtime", In("pythonExe", @"C:\fake\python.exe")),
                                        "runtimeKeys", new object[] { "pythonExe" },
                                        "isAdmin", false, "now", 0));
                int code = GdCli.Run("resolve-round-context", new string[] { "--in=" + inPath, "--out=" + outPath });
                Dictionary<string, object> env = GdJson.ReadEnvelope(outPath);
                Dictionary<string, object> data = Row(Gd.Get(env, "data"));
                return code == 0 && (bool)Gd.Get(env, "ok") && S(Gd.Get(env, "verb")) == "resolve-round-context"
                    && data != null && data.ContainsKey("Runtime") && data.ContainsKey("Elevation")
                    && data.ContainsKey("RequiredPathKeys") && data.ContainsKey("NowText");
            });
            b.Chk("start-run-lock 缺 pid 走 CLI：退出码 2(前置/规则那一档)＋原话进 error", delegate
            {
                string inPath = Path.Combine(tmp, "lock-in.json");
                string outPath = Path.Combine(tmp, "lock-out.json");
                GdJson.Write(inPath, In("games", "ark"));
                int code = GdCli.Run("start-run-lock", new string[] { "--in=" + inPath, "--out=" + outPath });
                Dictionary<string, object> env = GdJson.ReadEnvelope(outPath);
                return code == 2 && !Gd.Truthy(Gd.Get(env, "ok"))
                    && S(Gd.Get(env, "error")).Contains("必须显式给 pid");
            });
            b.Chk("send-round-notice 走 CLI：那份 last-notice.json 落在**临时树**的 state\\ 上(真跑才会落进真 state)", delegate
            {
                if (File.Exists(noticeFile)) File.Delete(noticeFile);
                string inPath = Path.Combine(tmp, "notice-in.json");
                string outPath = Path.Combine(tmp, "notice-out.json");
                GdJson.Write(inPath, In("owner", "方舟", "allOk", true, "text", "t", "noSend", true));
                int code = GdCli.Run("send-round-notice", new string[] { "--in=" + inPath, "--out=" + outPath });
                if (code != 0 || !File.Exists(noticeFile)) return false;
                return Path.GetFullPath(noticeFile).StartsWith(Path.GetFullPath(tmp), StringComparison.OrdinalIgnoreCase)
                    && S(Gd.Get(Row(GdJson.Read(noticeFile)), "title")) == "方舟日常已完成";
            });

            // ==================== 13) read-ark-activity：三档顺序 + 那两句 WARN(网络与读盘全走夹具) ====================
            b.Chk("接口夹具命中：Data=夹具、Source='MAA 活动接口'、CacheAgeHours 留空、一句 WARN 都不吭", delegate
            {
                object d = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityApiUrl", "http://fake.invalid/act")),
                    "apiUrl", "http://fake.invalid/act",
                    "apiJson", In("events", new object[] { "CE-5" })));
                return S(Key(d, "Source")) == "MAA 活动接口" && B(Key(d, "ApiUrlGiven")) && !B(Key(d, "Offline"))
                    && !B(Key(d, "CacheStale")) && Key(d, "CacheAgeHours") == null && Len(Key(d, "Lines")) == 0
                    && Key(d, "Data") != null;
            });
            b.Chk("offline 档：apiUrl 在场也不打接口，直接落本地缓存(source 里带'几小时前')", delegate
            {
                object d = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityApiUrl", "http://fake.invalid/act", "activityCacheMaxHours", 24)),
                    "offline", true, "cacheJson", In("a", 1), "cacheAgeHours", 3.0));
                return B(Key(d, "Offline")) && S(Key(d, "Source")) == "MAA 本地缓存(3 小时前)"
                    && Dbl(Key(d, "CacheAgeHours")) == 3.0 && !B(Key(d, "CacheStale")) && Len(Key(d, "Lines")) == 0;
            });
            b.Chk("原样怪癖：缓存小时数 Round(...,1) 与 PS 的 [math]::Round 同用**银行家舍入**(1.25→1.2，不是 1.3)", delegate
            {
                object d = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityCacheMaxHours", 24)),
                    "cacheJson", In("a", 1), "cacheAgeHours", 1.25));
                return S(Key(d, "Source")) == "MAA 本地缓存(1.2 小时前)" && Dbl(Key(d, "CacheAgeHours")) == 1.2;
            });
            b.Chk("apiErr 夹具：接口那句 WARN 原话逐字，然后照真身落到缓存档", delegate
            {
                object d = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityApiUrl", "http://fake.invalid/act", "activityCacheMaxHours", 24)),
                    "apiErr", "模拟断流", "cacheJson", In("a", 1), "cacheAgeHours", 2.0));
                return Ln(Key(d, "Lines"), 0) == "活动接口拉取失败(模拟断流)，改用本地缓存" && Len(Key(d, "Lines")) == 1
                    && S(Key(d, "Source")) == "MAA 本地缓存(2 小时前)";
            });
            b.Chk("接口与缓存两头落空 → '既拉不到…按长草期处理' 那句、Source='无数据'、Data 为 null", delegate
            {
                object d = RoundData.ArkActivity(In("settings", In("ark", In())));
                return S(Key(d, "Source")) == "无数据" && Key(d, "Data") == null && !B(Key(d, "ApiUrlGiven"))
                    && Len(Key(d, "Lines")) == 1 && Ln(Key(d, "Lines"), 0) == "既拉不到活动接口、也没有本地缓存 → 按长草期处理";
            });
            b.Chk("原样怪癖：activityCacheMaxHours 缺键＝0 ⇒ 缓存永远算过期，且那句 WARN 里插的是**原值**(空)，不是 0", delegate
            {
                object d = RoundData.ArkActivity(In(
                    "settings", In("ark", In()),                    // 整棵树没配这一项(真身 [int]$null = 0)
                    "cacheJson", In("a", 1), "cacheAgeHours", 1.25));
                return B(Key(d, "CacheStale")) && Len(Key(d, "Lines")) == 1
                    && Ln(Key(d, "Lines"), 0) == "活动缓存超过  小时未刷新，结论可能滞后(MAA 启动后会自行更新)";
            });
            b.Chk("配了 24 且没过 → 不吭；配了 1 且过了 → 那句里带上'1'", delegate
            {
                object ok = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityCacheMaxHours", 24)), "cacheJson", In("a", 1), "cacheAgeHours", 5.0));
                object bad = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityCacheMaxHours", 1)), "cacheJson", In("a", 1), "cacheAgeHours", 5.0));
                return !B(Key(ok, "CacheStale")) && Len(Key(ok, "Lines")) == 0
                    && B(Key(bad, "CacheStale"))
                    && Ln(Key(bad, "Lines"), 0) == "活动缓存超过 1 小时未刷新，结论可能滞后(MAA 启动后会自行更新)";
            });
            b.Chk("原样怪癖：'拉到东西了没有'按 PS 的 -not $data 判 —— 空对象算**有**、空数组算**没有**", delegate
            {
                object emptyObj = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityApiUrl", "http://fake.invalid/act")),
                    "apiUrl", "http://fake.invalid/act", "apiJson", In(), "offline", false));
                object emptyArr = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityApiUrl", "http://fake.invalid/act")),
                    "apiUrl", "http://fake.invalid/act", "apiJson", new object[0]));
                return S(Key(emptyObj, "Source")) == "MAA 活动接口" && S(Key(emptyArr, "Source")) == "无数据";
            });
            b.Chk("真读盘那半段：缓存文件落在**临时树**上，按 mtime 现算小时数(刚写的 ⇒ 0 小时前、没过 24 的线)", delegate
            {
                string cache = Path.Combine(tmp, "act-cache.json");
                GdJson.Write(cache, In("events", new object[] { "CE-5" }));
                object d = RoundData.ArkActivity(In(
                    "settings", In("ark", In("activityCacheMaxHours", 24)),
                    "paths", In("maaDir", tmp, "maaActivityCache", "act-cache.json")));
                return S(Key(d, "Source")) == "MAA 本地缓存(0 小时前)" && Key(d, "Data") != null
                    && !B(Key(d, "CacheStale")) && Len(Key(d, "Lines")) == 0;
            });
            b.Chk("守门：漏喂 settings 又没喂 paths → 去读临时树那份(不在)当场抛，不静默当'没活动'", delegate
            {
                return Thrown<FileNotFoundException>(delegate { return RoundData.ArkActivity(In("offline", true)); });
            });

            // ==================== 14) read-r1999-activity：读盘 + DataAge(天) ====================
            b.Chk("path 给了却不存在：Ready=false、Why='缺少 <路径>'、Reason 是那句'…，按长草处理'", delegate
            {
                string miss = Path.Combine(tmp, "没有这个文件.json");
                object d = RoundData.R1999Activity(In("path", miss));
                return !B(Key(d, "Ready")) && Key(d, "Data") == null && Key(d, "DataAgeDays") == null
                    && S(Key(d, "Why")) == "缺少 " + miss
                    && S(Key(d, "Reason")) == "缺少 " + miss + "，按长草处理";
            });
            b.Chk("m9aDir 没填 → 路径是空串，Why 换成那句人话(真身注释明写'缺少 ，按长草处理'玩家看不懂)", delegate
            {
                object d = RoundData.R1999Activity(In("settings", In("paths", In("m9aDir", "", "m9aActivityData", @"activity\cn.json"))));
                return !B(Key(d, "Ready")) && S(Key(d, "Path")) == ""
                    && S(Key(d, "Why")) == "M9A 目录还没填(先跑 setup.ps1)"
                    && S(Key(d, "Reason")) == "M9A 目录还没填(先跑 setup.ps1)，按长草处理";
            });
            b.Chk("真读盘：临时树上那份回读得到、DataAge 现算(mtime 刚写 ⇒ 0 天)且不吭'缺少'", delegate
            {
                string p = Path.Combine(tmp, "m9a-cn.json");
                GdJson.Write(p, In("version", 12, "events", new object[] { "ev1" }));
                object d = RoundData.R1999Activity(In("path", p));
                return B(Key(d, "Ready")) && S(Key(d, "Why")) == "" && Key(d, "Reason") == null
                    && Dbl(Key(d, "DataAgeDays")) < 0.01
                    && Gd.Map(Key(d, "Data")) != null && I(Gd.Get(Key(d, "Data"), "version")) == 12;
            });
            b.Chk("paths 拼接那半段：m9aDir+m9aActivityData 现拼(给了 path 就直答，不读 settings)", delegate
            {
                object joined = RoundData.R1999Activity(In(
                    "settings", In("paths", In("m9aDir", tmp, "m9aActivityData", "ghost.json"))));
                return S(Key(joined, "Path")) == Path.Combine(tmp, "ghost.json");
            });
            b.Chk("dataAgeDays 夹具原样带上回(那句'(活动数据 N 天前)'的**追加**动作属于判定，本 verb 不做——见头注双源第④处)", delegate
            {
                string p = Path.Combine(tmp, "m9a-cn.json");
                object d = RoundData.R1999Activity(In("path", p, "dataJson", In("version", 1), "dataAgeDays", 1.25));
                return B(Key(d, "Ready")) && Dbl(Key(d, "DataAgeDays")) == 1.2
                    && S(Key(d, "Reason")) == "(null)";
            });

            // ==================== 15) read-decision-state：五本 state 读 + 那把 flag 尺的读数 ====================
            b.Chk("空树三本：Ack/ShopDone 是空表(不是 null)、Probe 没探过=null、门默认开着", delegate
            {
                object d = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In())));
                // 门表回的是**字典**不是数组：Len() 走 Gd.List，它对字典回"一个元素"(那条防摊开的守卫)，
                // 所以比格数必须用 Cnt(对齐 PS 的 $map.Count)，否则空表会被数成一。
                return S(Key(d, "Game")) == "ark" && Cnt(Key(d, "Ack")) == 0 && Cnt(Key(d, "ShopDone")) == 0
                    && Key(d, "Probe") == null && B(Key(d, "ShopDoneGateOpen")) && Len(Key(d, "Want")) == 3;
            });
            b.Chk("want 逐枚点名：只要 ack 时 ShopDone/Probe 都不去碰(省两次读盘，也不替调用方多取)", delegate
            {
                object d = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In()), "want", new object[] { "ack" }));
                return Key(d, "Ack") != null && Key(d, "ShopDone") == null && Key(d, "Probe") == null
                    && Len(Key(d, "Want")) == 1;
            });
            b.Chk("盘上那本 shop-done 表回读得到，且键**不分大小写**(PS 的 @{} 就是这把尺，区分大小写会留孤儿格)", delegate
            {
                GdJson.Write(shopArk, In("done", In("CE-5", "2026-10-13 06:05")));
                object d = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In())));
                Dictionary<string, object> map = Row(Key(d, "ShopDone"));
                return map != null && map.ContainsKey("ce-5") && map.Count == 1 && S(map["CE-5"]) == "2026-10-13 06:05";
            });
            b.Chk("自证账本五键原样回读(Date/EventKey/Stage/Entered/Evidence)——判定层 Decisions.cs 就吃这五键", delegate
            {
                GdJson.Write(probeFile, In("Date", "10-13 06:12", "EventKey", "CE-5", "Stage", "CE-5/4",
                                           "Entered", true, "Evidence", "作战完成"));
                object d = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In()), "want", new object[] { "probe" }));
                Dictionary<string, object> pb = Row(Key(d, "Probe"));
                return pb != null && S(pb["Date"]) == "10-13 06:12" && S(pb["EventKey"]) == "CE-5"
                    && S(pb["Stage"]) == "CE-5/4" && B(pb["Entered"]) && S(pb["Evidence"]) == "作战完成";
            });
            b.Chk("EventKey 是空串 = '这一轮还没探'→ null(不能把空账本喂成一条结论)", delegate
            {
                GdJson.Write(probeFile, In("Date", "10-13 06:12", "EventKey", "", "Stage", "CE-5/4", "Entered", false));
                object d = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In()), "want", new object[] { "probe" }));
                return Key(d, "Probe") == null;
            });
            b.Chk("账本读坏了 → 照样 null(真身 Get-GdArkEventProbe 那一层有 try：坏账本不该把整轮带走)", delegate
            {
                File.WriteAllText(probeFile, "{这不是合法的 JSON", new UTF8Encoding(false));
                object d = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In()), "want", new object[] { "probe" }));
                return Key(d, "Probe") == null;
            });
            File.Delete(probeFile);
            b.Chk("r1999 读的是 r1999 那本盘：ack 表分游戏各走各的文件", delegate
            {
                GdJson.Write(ackR1999, In("acked", In("mainline-3", "2026-10-12 07:00")));
                object r = RoundData.DecisionState(In("game", "r1999", "settings", In("re1999", In()),
                                                      "want", new object[] { "ack" }));
                object a = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In()),
                                                      "want", new object[] { "ack" }));
                return S(Key(r, "Game")) == "r1999" && Cnt(Key(r, "Ack")) == 1 && Cnt(Key(a, "Ack")) == 0;
            });
            b.Chk("块名不许按 id 拼：id 是 r1999、settings 块叫 re1999(Get-GdSettingsBlock 的回落分支)", delegate
            {
                object open = RoundData.DecisionState(In("game", "r1999", "settings", In("re1999", In("shopDoneStopsEvent", false)),
                                                         "want", new object[] { "ack" }));
                object named = RoundData.DecisionState(In("game", "r1999", "settings", In("re1999", In("shopDoneStopsEvent", true)),
                                                           "settingsBlock", "re1999", "want", new object[] { "ack" }));
                return !B(Key(open, "ShopDoneGateOpen")) && B(Key(named, "ShopDoneGateOpen"));
            });
            b.Chk("那把 flag 尺复用 C# 侧唯一真身(Round.FlagEnabled)：字符串 \"0\" 算开着、显式 false 才算关", delegate
            {
                object zero = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In("shopDoneStopsEvent", "0")),
                                                         "want", new object[] { "ack" }));
                object off = RoundData.DecisionState(In("game", "ark", "settings", In("ark", In("shopDoneStopsEvent", false)),
                                                        "want", new object[] { "ack" }));
                return B(Key(zero, "ShopDoneGateOpen")) && !B(Key(off, "ShopDoneGateOpen"));
            });
            b.Chk("认不得的游戏 id 当场挡(它会被拼进状态文件名)，原话逐字对 Assert-GdGameId", delegate
            {
                return Thrown<GdRuleException>(delegate
                {
                    return RoundData.DecisionState(In("game", "__no_such__", "settings", In("ark", In())));
                }) && S(ThrownMsg(delegate
                {
                    return RoundData.DecisionState(In("game", "__no_such__", "settings", In("ark", In())));
                })) == "不认识的游戏 id：'__no_such__'(这棵树上认得：ark / r1999)";
            });
            b.Chk("名册可注入：将来模块清单多一款游戏由调用方点名 gameIds，不在这里硬编码第三路", delegate
            {
                object d = RoundData.DecisionState(In("game", "third", "gameIds", new object[] { "ark", "r1999", "third" },
                                                      "settings", In("third", In()), "want", new object[] { "ack" }));
                return S(Key(d, "Game")) == "third" && Cnt(Key(d, "Ack")) == 0;
            });

            // ==================== 16) set-ark-shop-state：四分支话术 + 真落盘(只落临时树) ====================
            b.Chk("没有进行中的活动(空 key)：只 WARN 那一句、Action=none、**盘上一个字都不写**", delegate
            {
                if (File.Exists(shopArk)) File.Delete(shopArk);
                object d = RoundData.ArkShopState(In("eventKey", "", "reason", "活动接口和缓存都拉不到"));
                return S(Key(d, "Action")) == "none" && !B(Key(d, "Written")) && I(Key(d, "Count")) == 0
                    && Ln(Key(d, "Lines"), 0) == "方舟当前没有进行中的活动可标记：活动接口和缓存都拉不到"
                    && !File.Exists(shopArk);
            });
            b.Chk("标记：Action=mark、时间戳用喂进来的那个、盘上 done 表里就是这一格、回执 Count=1", delegate
            {
                if (File.Exists(shopArk)) File.Delete(shopArk);
                object d = RoundData.ArkShopState(In("eventKey", "CE-5", "eventTip", "限时活动", "done", true,
                                                     "nowText", "2026-10-13 07:00", "idlePlan", "CE-5",
                                                     "settings", In("ark", In())));
                if (S(Key(d, "Action")) != "mark" || !B(Key(d, "Written")) || I(Key(d, "Count")) != 1) return false;
                if (Ln(Key(d, "Lines"), 0) != "已标记方舟活动 CE-5[限时活动] 商店领空 → 之后剩余理智只刷 CE-5") return false;
                Dictionary<string, object> obj = Row(GdJson.Read(shopArk));
                Dictionary<string, object> done = Row(Gd.Get(obj, "done"));
                return done != null && S(done["CE-5"]) == "2026-10-13 07:00";
            });
            b.Chk("原样怪癖：回落那句人话里的 dow 用**原始 0-6**(周日读 planByWeekday.\"0\")，不是判定层那把'周日=7'", delegate
            {
                object sun = RoundData.ArkShopState(In("eventKey", "K", "eventTip", "t", "done", true, "nowText", "T",
                                                       "dow", 0, "settings", In("ark", In("planByWeekday", In("1", "CE-5"),
                                                                                          "defaultStage", "1-7"))));
                object mon = RoundData.ArkShopState(In("eventKey", "K", "eventTip", "t", "done", true, "nowText", "T",
                                                       "dow", 1, "settings", In("ark", In("planByWeekday", In("1", "CE-5"),
                                                                                          "defaultStage", "1-7"))));
                return Ln(Key(sun, "Lines"), 0).EndsWith("只刷 1-7") && Ln(Key(mon, "Lines"), 0).EndsWith("只刷 CE-5");
            });
            b.Chk("整表写回不丢别的格：连标两枚 ⇒ 盘上两格都在、Count=2(真身是取表→盖一格→整表写回)", delegate
            {
                // 从空表起：Count 是**整表**格数，前一格(CE-5/K)留下的账会把它抬上去，那不是在验"写回丢不丢"。
                if (File.Exists(shopArk)) File.Delete(shopArk);
                RoundData.ArkShopState(In("eventKey", "K1", "eventTip", "t", "done", true, "nowText", "07:00",
                                          "idlePlan", "P", "settings", In("ark", In())));
                object d = RoundData.ArkShopState(In("eventKey", "K2", "eventTip", "t", "done", true, "nowText", "07:05",
                                                     "idlePlan", "P", "settings", In("ark", In())));
                Dictionary<string, object> done = Row(Gd.Get(Row(GdJson.Read(shopArk)), "done"));
                return I(Key(d, "Count")) == 2 && done != null && done.Count == 2
                    && S(done["K1"]) == "07:00" && S(done["K2"]) == "07:05";
            });
            b.Chk("撤销：Action=reset、话术'…恢复自动刷活动代币'、盘上那一格真没了(别的格留下)", delegate
            {
                object d = RoundData.ArkShopState(In("eventKey", "K1", "done", false, "settings", In("ark", In())));
                Dictionary<string, object> done = Row(Gd.Get(Row(GdJson.Read(shopArk)), "done"));
                return S(Key(d, "Action")) == "reset" && B(Key(d, "Written")) && I(Key(d, "Count")) == 1
                    && Ln(Key(d, "Lines"), 0) == "已撤销 K1 的领空标记 → 恢复自动刷活动代币"
                    && done != null && !done.ContainsKey("K1") && done.ContainsKey("K2");
            });
            b.Chk("撤销落空(本来就没有)：Action=miss、Written=false、只 WARN 一句、**不建文件**(真身那句 return 在 Remove 之前)", delegate
            {
                File.Delete(shopArk);
                object d = RoundData.ArkShopState(In("eventKey", "K9", "done", false, "settings", In("ark", In())));
                return S(Key(d, "Action")) == "miss" && !B(Key(d, "Written")) && I(Key(d, "Count")) == 0
                    && Ln(Key(d, "Lines"), 0) == "K9 本来就没有领空标记" && !File.Exists(shopArk);
            });

            // ==================== 17) set-r1999-ack：只有'标记'一档(这道门没有撤销口) ====================
            b.Chk("没有待确认活动：只 WARN 那句、不写盘", delegate
            {
                if (File.Exists(ackR1999)) File.Delete(ackR1999);
                object d = RoundData.R1999Ack(In("eventKey", "", "reason", "这一版没有主线活动"));
                return S(Key(d, "Action")) == "none" && !B(Key(d, "Written"))
                    && Ln(Key(d, "Lines"), 0) == "1999 当前没有待确认活动：这一版没有主线活动" && !File.Exists(ackR1999);
            });
            b.Chk("原样怪癖：确认话术里那个箭头前**没有空格**(真身原文如此，ark 那两句却有空格，不许统一)", delegate
            {
                if (File.Exists(ackR1999)) File.Delete(ackR1999);
                object d = RoundData.R1999Ack(In("eventKey", "mainline-3", "eventTip", "第三章", "nowText", "2026-10-13 07:10"));
                if (Ln(Key(d, "Lines"), 0) != "已确认 1999 活动 mainline-3[第三章]→ 日常将使用活动方案") return false;
                Dictionary<string, object> acked = Row(Gd.Get(Row(GdJson.Read(ackR1999)), "acked"));
                return acked != null && S(acked["mainline-3"]) == "2026-10-13 07:10";
            });
            b.Chk("重复确认同一格：Count 不增(整表写回是**覆盖**不是追加)", delegate
            {
                object d = RoundData.R1999Ack(In("eventKey", "mainline-3", "eventTip", "第三章", "nowText", "07:20"));
                Dictionary<string, object> acked = Row(Gd.Get(Row(GdJson.Read(ackR1999)), "acked"));
                return I(Key(d, "Count")) == 1 && acked.Count == 1 && S(acked["mainline-3"]) == "07:20";
            });

            // ==================== 18) set-r1999-shop-state：与 ark 那枚同构，差两句人话 + 回落值是实例名 ====================
            b.Chk("标记：回落值从 re1999.planToInstance.idle **现读**(不是关卡名)，落的是 r1999 那本盘", delegate
            {
                if (File.Exists(shopR1999)) File.Delete(shopR1999);
                object d = RoundData.R1999ShopState(In("eventKey", "ev1", "eventTip", "活动一", "done", true,
                                                       "nowText", "07:30",
                                                       "settings", In("re1999", In("planToInstance", In("idle", "长草实例")))));
                if (Ln(Key(d, "Lines"), 0) != "已标记 1999 活动 ev1[活动一] 商店领空 → 之后跑[长草实例]实例") return false;
                Dictionary<string, object> done = Row(Gd.Get(Row(GdJson.Read(shopR1999)), "done"));
                return done != null && S(done["ev1"]) == "07:30" && !File.Exists(shopArk);
            });
            b.Chk("idleInstance 直答优先于现读(转接期壳里已经算好了就别再读一遍 settings)", delegate
            {
                object d = RoundData.R1999ShopState(In("eventKey", "ev2", "eventTip", "t", "done", true,
                                                       "nowText", "07:31", "idleInstance", "指定实例"));
                return Ln(Key(d, "Lines"), 0).EndsWith("跑[指定实例]实例");
            });
            b.Chk("撤销那两句与 ark 同构但**结尾那句不一样**('恢复活动方案' vs '恢复自动刷活动代币')", delegate
            {
                object r = RoundData.R1999ShopState(In("eventKey", "ev2", "done", false));
                object m = RoundData.R1999ShopState(In("eventKey", "ev9", "done", false));
                return S(Key(r, "Action")) == "reset" && Ln(Key(r, "Lines"), 0) == "已撤销 ev2 的领空标记 → 恢复活动方案"
                    && S(Key(m, "Action")) == "miss" && Ln(Key(m, "Lines"), 0) == "ev9 本来就没有领空标记"
                    && !File.Exists(shopArk);
            });

            // ==================== 19) CLI 接线：这六枚真的挂上了分发链(名册两处同批的自证) ====================
            b.Chk("read-decision-state 走 --in/--out：退出码 0、信封 verb 对、data 里五键齐", delegate
            {
                string inPath = Path.Combine(tmp, "ds-in.json");
                string outPath = Path.Combine(tmp, "ds-out.json");
                GdJson.Write(inPath, In("game", "ark", "settings", In("ark", In())));
                int code = GdCli.Run("read-decision-state", new string[] { "--in=" + inPath, "--out=" + outPath });
                Dictionary<string, object> env = GdJson.ReadEnvelope(outPath);
                Dictionary<string, object> data = Row(Gd.Get(env, "data"));
                return code == 0 && Gd.Truthy(Gd.Get(env, "ok")) && S(Gd.Get(env, "verb")) == "read-decision-state"
                    && data != null && data.ContainsKey("Ack") && data.ContainsKey("ShopDone")
                    && data.ContainsKey("ShopDoneGateOpen") && data.ContainsKey("Probe") && data.ContainsKey("Game");
            });
            b.Chk("set-ark-shop-state 走 CLI：那份表落在**临时树**的 state\\ 上(真跑才会落进真 state)", delegate
            {
                if (File.Exists(shopArk)) File.Delete(shopArk);
                string inPath = Path.Combine(tmp, "shop-in.json");
                string outPath = Path.Combine(tmp, "shop-out.json");
                GdJson.Write(inPath, In("eventKey", "CLI-1", "eventTip", "t", "done", true,
                                         "nowText", "08:00", "idlePlan", "P"));
                int code = GdCli.Run("set-ark-shop-state", new string[] { "--in=" + inPath, "--out=" + outPath });
                if (code != 0 || !File.Exists(shopArk)) return false;
                return Path.GetFullPath(shopArk).StartsWith(Path.GetFullPath(tmp), StringComparison.OrdinalIgnoreCase)
                    && S(Gd.Get(Row(Gd.Get(Row(GdJson.Read(shopArk)), "done")), "CLI-1")) == "08:00";
            });
            b.Chk("read-ark-activity 走 CLI 且漏喂 settings：退出码 2(前置那一档)＋原话进 error，不静默回'无数据'", delegate
            {
                string inPath = Path.Combine(tmp, "act-in.json");
                string outPath = Path.Combine(tmp, "act-out.json");
                GdJson.Write(inPath, In("offline", true));
                int code = GdCli.Run("read-ark-activity", new string[] { "--in=" + inPath, "--out=" + outPath });
                Dictionary<string, object> env = GdJson.ReadEnvelope(outPath);
                return code == 2 && !Gd.Truthy(Gd.Get(env, "ok"))
                    && S(Gd.Get(env, "error")).Contains("还没有 config\\settings.json");
            });
            b.Chk("认不得的游戏 id 走 CLI：退出码 2＋规则原话(GdRuleException 与前置同档)", delegate
            {
                string inPath = Path.Combine(tmp, "bad-in.json");
                string outPath = Path.Combine(tmp, "bad-out.json");
                GdJson.Write(inPath, In("game", "__no_such__", "settings", In("ark", In())));
                int code = GdCli.Run("read-decision-state", new string[] { "--in=" + inPath, "--out=" + outPath });
                Dictionary<string, object> env = GdJson.ReadEnvelope(outPath);
                return code == 2 && S(Gd.Get(env, "error")).Contains("不认识的游戏 id：'__no_such__'");
            });
            b.Chk("名册第二处(usage 长串)随行自证：未知子命令那句报错里必须同时数到本批六枚", delegate
            {
                string outPath = Path.Combine(tmp, "usage-out.json");
                int code = GdCli.Run("no-such-verb", new string[] { "--out=" + outPath });
                string err = S(Gd.Get(GdJson.ReadEnvelope(outPath), "error"));
                return code == 1 && err.Contains("read-ark-activity") && err.Contains("read-r1999-activity")
                    && err.Contains("read-decision-state") && err.Contains("set-ark-shop-state")
                    && err.Contains("set-r1999-ack") && err.Contains("set-r1999-shop-state");
            });

            // 收尾：临时树整目录删掉(纪律＝门/自检的临时件不落仓内)
            try { if (File.Exists(lockFile)) File.Delete(lockFile); } catch { }
            try { Directory.Delete(tmp, true); }
            catch (Exception ex) { GdLog.Line("--roundtest 没能清掉临时树 " + tmp + "：" + ex.Message, "WARN"); }

            int rc = b.Result("--roundtest");
            if (rc == 0)
                Console.WriteLine("结论: PASS(②-a 那七枚 + ②-b1 这六枚的话术/算术/夹具闸都对；提权实弹、真 webhook/气泡/真任务/真杀进程、真打活动接口与真 settings/真 state 盘仍登记 unobserved)");
            return rc;
        }
    }
}
