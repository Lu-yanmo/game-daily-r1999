// tools/panel-cs/Cli.SelfTest.cs —— --coretest / --echotest：批次 0 验收门 §6.2/§6.3。
// 纪律(迁移前就立好的老规矩)：
//   * 断言必须计入退出码 —— "结论 PASS 却印着 FAIL"那种事不许再发生；
//   * 全部喂 %TEMP% 下的临时树，不读真配置、不起真助手进程 —— 只读自检不许要求机器现场状态；
//   * JSS 的数字怪癖(int/double 混发)在这里**钉死**而不是祈祷：批次 1 的判定值要过这道形状关。

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.RegularExpressions;

namespace GdPanel
{
    static class GdCliSelfTest
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

        // ---------- 临时树 ----------
        static string MakeTempTree()
        {
            string tmp = Path.Combine(Path.GetTempPath(),
                "gd-" + Process.GetCurrentProcess().Id + "-" + DateTime.Now.ToString("yyyyMMdd-HHmmss") + "-"
                + Guid.NewGuid().ToString("N").Substring(0, 6));
            Directory.CreateDirectory(Path.Combine(tmp, "lib"));
            Directory.CreateDirectory(Path.Combine(tmp, "config"));
            Directory.CreateDirectory(tmp);   // logs/state/backups 由 Init 补
            return tmp;
        }

        static void CleanTemp(string tmp)
        {
            try { Directory.Delete(tmp, true); } catch { }
        }

        static object SampleData()
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            d["消息"] = "他说\"停\"，路径 C:\\x";
            d["n"] = 5;
            d["f"] = 1.2;
            d["t"] = true;
            d["nul"] = null;
            d["arr"] = new object[] { "CI", "CR" };
            Dictionary<string, object> nest = new Dictionary<string, object>();
            nest["s"] = new object[] { new Dictionary<string, object> { { "code", "CI" }, { "ap", 30 } } };
            d["nest"] = nest;
            return d;
        }

        static bool SameJson(object a, object b)
        {
            return GdJson.Serialize(a) == GdJson.Serialize(b);
        }

        static object EnvGet(Dictionary<string, object> env, string key)
        {
            object v;
            return env.TryGetValue(key, out v) ? v : null;
        }

        // ============================ --coretest ============================
        public static int CoreTest()
        {
            GdCli.TryAttachConsole();
            Box b = new Box();
            string tmp = MakeTempTree();
            try
            {
                // 只装方舟那一路的库：RequiredPathKeys 的口径就是"文件在才算装了那一路"
                File.WriteAllText(Path.Combine(tmp, "lib", "maa.ps1"), "#");
                File.WriteAllText(Path.Combine(tmp, "VERSION"), "9.9.9\n");
                Dictionary<string, object> paths = new Dictionary<string, object>();
                paths["maaDir"] = "";
                paths["m9aDir"] = Path.Combine(tmp, "no-such-dir");
                paths["mumuDir"] = Path.Combine(tmp, "lib");
                Dictionary<string, object> settings = new Dictionary<string, object>();
                settings["paths"] = paths;
                settings["runtime"] = new Dictionary<string, object> { { "arkVisionRescue", true } };
                GdJson.Write(Path.Combine(tmp, @"config\settings.json"), settings);

                GdEnv.InitForTest(tmp);

                b.Chk("VERSION 读数 = 临时树里那份", delegate { return GdEnv.Version == "9.9.9"; });
                b.Chk("VERSION 形状不对回 0.0.0 不抛", delegate
                {
                    File.WriteAllText(Path.Combine(tmp, "VERSION"), "v2026.09\n");
                    GdEnv.InitForTest(tmp);
                    return GdEnv.Version == "0.0.0";
                });
                File.WriteAllText(Path.Combine(tmp, "VERSION"), "9.9.9\n");
                GdEnv.InitForTest(tmp);

                b.Chk("JoinPath：空 base 返空串不抛(全工程靠这条显示『未填写』)", delegate
                {
                    return GdEnv.JoinPath("", "resource", "stages.json") == ""
                        && GdEnv.JoinPath(null, "a") == "";
                });
                b.Chk("JoinPath：多级拼接 + 跳过空段", delegate
                {
                    string want = Path.Combine(Path.Combine(tmp, "resource"), "stages.json");
                    return GdEnv.JoinPath(tmp, "", "resource", "stages.json") == want;
                });
                b.Chk("PathReady：空/不存在=false，文件与目录=true", delegate
                {
                    return !GdEnv.PathReady("")
                        && !GdEnv.PathReady(Path.Combine(tmp, "ghost"))
                        && GdEnv.PathReady(tmp)
                        && GdEnv.PathReady(Path.Combine(tmp, "lib", "maa.ps1"));
                });

                string roundPath = Path.Combine(tmp, "round.json");
                GdJson.Write(roundPath, SampleData());
                object back = GdJson.Read(roundPath);
                b.Chk("JSON 往返：中文/引号/反斜杠/嵌套/数组/布尔/null 全等", delegate { return SameJson(back, SampleData()); });
                b.Chk("写出无 BOM(对齐 Write-GdJson 的 UTF8Encoding($false))", delegate
                {
                    byte[] head = File.ReadAllBytes(roundPath);
                    return !(head.Length >= 3 && head[0] == 0xEF && head[1] == 0xBB && head[2] == 0xBF);
                });
                b.Chk("JSS 形状钉死(实测)：5→Int32 / 1.2→**Decimal** / 数组→Object[] —— 批次 1 取数一律 Convert.ToXxx，禁裸强转", delegate
                {
                    Dictionary<string, object> d = (Dictionary<string, object>)back;
                    bool nOk = d["n"] is int && (int)d["n"] == 5;
                    bool fOk = d["f"] is decimal && Convert.ToDouble(d["f"]) == 1.2;   // Convert 通道必须走得通，这是批次 1 的取数规矩
                    bool aOk = d["arr"] is object[] && ((object[])d["arr"]).Length == 2;
                    if (!(nOk && fOk && aOk))
                        throw new Exception("n=" + (d["n"] == null ? "null" : d["n"].GetType().Name)
                            + " f=" + (d["f"] == null ? "null" : d["f"].GetType().Name)
                            + " arr=" + (d["arr"] == null ? "null" : d["arr"].GetType().Name));
                    return true;
                });
                b.Chk("缺文件的读法说 PS 原话：找不到文件：<路径>", delegate
                {
                    try { GdJson.Read(Path.Combine(tmp, "ghost.json")); return false; }
                    catch (FileNotFoundException ex) { return ex.Message.StartsWith("找不到文件："); }
                });
                b.Chk("信封形状：proto/verb/ok/exit/data/error 齐、proto=1", delegate
                {
                    Dictionary<string, object> e = GdJson.Envelope("echo", true, 0, SampleData(), null);
                    return (int)e["proto"] == GdJson.Proto && (string)e["verb"] == "echo"
                        && (bool)e["ok"] && (int)e["exit"] == 0 && e["data"] != null && e["error"] == null;
                });

                b.Chk("RequiredPathKeys：只点这棵树上真装了库的那一路", delegate
                {
                    List<string> keys = GdSettings.RequiredPathKeys();
                    return keys.Count == 1 && keys[0] == "maaDir";
                });
                b.Chk("Get-GdSettings 的 Quiet 双人格：列缺不抛，missing 只收空路径项", delegate
                {
                    List<string> missing;
                    GdSettings.Load(out missing, true, null);
                    return missing.Count == 1 && missing[0] == "maaDir";   // m9aDir 是"填了指错"，另一类，只 WARN
                });
                b.Chk("缺配置文件的报错是 setup 文案，不是堆栈", delegate
                {
                    GdEnv.InitForTest(Path.Combine(tmp, "no-root-here"));
                    List<string> missing;
                    try { GdSettings.Load(out missing, true, new List<string>()); return false; }
                    catch (FileNotFoundException ex) { return ex.Message.Contains("setup.ps1"); }
                    finally { GdEnv.InitForTest(tmp); }
                });

                b.Chk("日志两层时间戳：落盘行 = [yyyy-MM-dd HH:mm:ss][HH:mm:ss][LEVEL]", delegate
                {
                    string logPath = Path.Combine(tmp, "fmt.log");
                    GdLog.LogFile = logPath;
                    GdLog.ToConsole = false;
                    try { GdLog.Line("你好", "WARN"); }
                    finally { GdLog.LogFile = null; GdLog.ToConsole = true; }
                    string line = File.ReadAllLines(logPath)[0];
                    return Regex.IsMatch(line, @"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]\[\d{2}:\d{2}:\d{2}\]\[WARN\] 你好$");
                });

                b.Chk("BackupFile：备份成功、内容一致、命名 <名>.<stamp>.bak", delegate
                {
                    string victim = Path.Combine(tmp, "victim.txt");
                    File.WriteAllText(victim, "A");
                    string bak = GdEnv.BackupFile(victim);
                    return bak != null && File.Exists(bak) && File.ReadAllText(bak) == "A"
                        && Path.GetFileName(bak).StartsWith("victim.txt.") && bak.EndsWith(".bak");
                });
            }
            catch (Exception ex)
            {
                b.Chk("coretest 整体没炸", delegate { throw ex; });
            }
            finally { CleanTemp(tmp); }
            return b.Result("coretest");
        }

        // ============================ --echotest ============================
        static int RunExe(string args)
        {
            ProcessStartInfo pi = new ProcessStartInfo(Assembly.GetExecutingAssembly().Location, args);
            pi.UseShellExecute = false;
            pi.CreateNoWindow = true;
            using (Process p = Process.Start(pi))
            {
                if (!p.WaitForExit(60000)) { try { p.Kill(); } catch { } return -99; }
                return p.ExitCode;
            }
        }

        static string Q(string s) { return "\"" + s + "\""; }

        public static int EchoTest()
        {
            GdCli.TryAttachConsole();
            Box b = new Box();
            string tmp = MakeTempTree();
            try
            {
                GdEnv.InitForTest(tmp);
                string inPath = Path.Combine(tmp, "in.json");
                string outPath = Path.Combine(tmp, "out.json");
                GdJson.Write(inPath, SampleData());
                object sampleRead = GdJson.Read(inPath);   // 两侧都过一遍 JSS，int/double 归一到同一形状再比

                b.Chk("回环：--in 的 data 原样进信封，退出码 0", delegate
                {
                    int code = GdCli.Run("echo", new string[] { "--in=" + inPath, "--out=" + outPath });
                    if (code != 0) return false;
                    Dictionary<string, object> env = GdJson.ReadEnvelope(outPath);
                    return (int)EnvGet(env, "proto") == 1 && (int)EnvGet(env, "exit") == 0
                        && (bool)EnvGet(env, "ok") && EnvGet(env, "error") == null
                        && (string)EnvGet(env, "verb") == "echo"
                        && SameJson((Dictionary<string, object>)EnvGet(env, "data"), sampleRead);
                });
                b.Chk("--exit=2：进程码和信封 exit 一致、ok=false、error 说清是模拟", delegate
                {
                    int code = GdCli.Run("echo", new string[] { "--out=" + outPath, "--exit=2" });
                    Dictionary<string, object> env = GdJson.ReadEnvelope(outPath);
                    return code == 2 && (int)EnvGet(env, "exit") == 2 && !(bool)EnvGet(env, "ok")
                        && ((string)EnvGet(env, "error")).Contains("模拟");
                });
                b.Chk("--no-out：违约=结果文件不在(转发壳要靠这一条报错，绝不面对'非 0 但不知道为什么')", delegate
                {
                    string ghost = Path.Combine(tmp, "ghost-out.json");
                    int code = GdCli.Run("echo", new string[] { "--out=" + ghost, "--no-out" });
                    return code == 0 && !File.Exists(ghost);
                });
                b.Chk("--garbage：违约=文件在但信封读不回来", delegate
                {
                    string g = Path.Combine(tmp, "g.json");
                    int code = GdCli.Run("echo", new string[] { "--out=" + g, "--garbage" });
                    if (code != 0 || !File.Exists(g)) return false;
                    try { GdJson.ReadEnvelope(g); return false; } catch { return true; }
                });
                b.Chk("--in 指空：退出码 1 且**仍写出**带 error 的信封", delegate
                {
                    string e2 = Path.Combine(tmp, "err.json");
                    int code = GdCli.Run("echo", new string[] { "--in=" + Path.Combine(tmp, "none.json"), "--out=" + e2 });
                    Dictionary<string, object> env = GdJson.ReadEnvelope(e2);
                    return code == 1 && !(bool)EnvGet(env, "ok")
                        && ((string)EnvGet(env, "error")).Contains("找不到文件：");
                });
                b.Chk("未知子命令 = 退出码 1(127 留给 cmd 层)", delegate
                {
                    return GdCli.Run("no-such-verb", new string[0]) == 1;
                });
                b.Chk("未知参数被点名拒绝而不是咽下", delegate
                {
                    string u = Path.Combine(tmp, "unknown-arg.json");
                    int code = GdCli.Run("echo", new string[] { "--oops=1", "--out=" + u });
                    Dictionary<string, object> env = GdJson.ReadEnvelope(u);
                    return code == 1 && ((string)EnvGet(env, "error")).Contains("未知参数");
                });
                b.Chk("--transcript 显式开才落 logs\\cli-echo-*.log", delegate
                {
                    string t = Path.Combine(tmp, "t.json");
                    try
                    {
                        GdCli.Run("echo", new string[] { "--out=" + t, "--transcript" });
                        bool found = false;
                        foreach (string f in Directory.GetFiles(Path.Combine(tmp, "logs"), "cli-echo-*.log"))
                        {
                            found = true;
                            string first = File.ReadAllLines(f)[0];
                            if (!Regex.IsMatch(first, @"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]\[\d{2}:\d{2}:\d{2}\]\[INFO\] 日志文件：")) return false;
                        }
                        return found;
                    }
                    finally { GdLog.LogFile = null; }   // 不落盘回去，后面的用例别都续写进这份临时日志
                });

                // 真子进程各走一遍 Main：证明 verb 分发不弹窗、退出码过进程边界仍成立。
                b.Chk("子进程：panel-cs.exe echo --out=… 退出码 0 + 信封可读", delegate
                {
                    string p = Path.Combine(tmp, "sub.json");
                    File.Delete(p);
                    int code = RunExe("echo --out=" + Q(p));
                    if (code != 0) return false;
                    Dictionary<string, object> env = GdJson.ReadEnvelope(p);
                    return (int)EnvGet(env, "proto") == 1 && (int)EnvGet(env, "exit") == 0;
                });
                b.Chk("子进程：未知 verb 退出码 1", delegate { return RunExe("no-such-verb") == 1; });
            }
            catch (Exception ex)
            {
                b.Chk("echotest 整体没炸", delegate { throw ex; });
            }
            finally { CleanTemp(tmp); }
            return b.Result("echotest");
        }
    }
}
