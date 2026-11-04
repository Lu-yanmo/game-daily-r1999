// tools/panel-cs/Cli.Verbs.cs —— verb 名册的**第二处**（数据面）。批次 C-0′「量尺」新立，见 docs\rust-core-plan.md 的 C-0′ 一节。
// 为什么要有这一份：Cli.cs 的分派是 84 条 `verb == "…"` 的 if-else 链，而"未知子命令"那句报错里的名册是**手写的散文长串**——
// 里面 15 枚只写了通配(`parse-asst-*` 一类)或纯计数("批次 1 判定四件")，且既有自检与 probe-cli-contract 都不钉 verb 枚数（⚠ 后者已于 2026-10-26 甲档第七刀整件退役，"不钉"这句只剩历史效力——现钉法是第四行那句双向对账）。
// 后果＝新增 verb 时头注与报错文案**不会红**，只在报错里静默漏名（B-3②-b1/b2 两批名册 74→84 全靠人工现扫，就是这件事的证据）。
// 这一份把名册从"散文"改成"数据"：报错文案由它生成、枚数由它钉死、`Cli.SelfTest` 逐枚与 Cli.cs 源码对账（两个方向都咬）。
// 加一枚 verb 的规矩：分派链 + 本数组**同批改**（名册两处同批的既有纪律不变，只是第二处不再是手抄）。
using System;
using System.Collections.Generic;
using System.Text;

namespace GdPanel
{
    static class GdCliVerbs
    {
        // 钉死的枚数。加/减 verb 必须同时改这个数字 —— 它红了就说明名册没跟着走。
        // R9-a(2026-10-17 轮骨架四格收编)：84 → 88，新立四枚排在链尾(read-screensaver-config /
        // disable-screensaver-for-round / append-round-history / update-ark-event-probe)。
        // 乙档刀①(2026-10-23 面板摘要判定面进核)：88 → 89，链尾再加 emit-panel-summary。
        // 宿主批第二刀(2026-11-02 监督自举回路)：91 → 92，链尾再加 supervise-loop(Start 起的子进程自己跑的那一枚)。
        // 转接刀·时间换算族(2026-11-03)：92 → 94，链尾再加 convert-utc8-to-ms／format-unix-ms
        //   —— 这两枚不新写任何规则，只是把这一件里早就存在的 Gd.Utc8ToMs／Gd.Fmt 露成出口，
        //   给 PS 侧那两枚同规则真身出仓用；既有 verb 的回执形状一枚没动。
        // 刀① round-gate/stop 四臂直答(2026-11-03)：94 → 95，链尾再加 stop-mumu
        //   (Stop-GdMuMu 的 C# 直身，lib\r1999-module.ps1 Stop 钩子随臂进核的"跑完关模拟器"那半的出口)。
        // 刀② round-preview 两臂直答(2026-11-03)：95 → 97，链尾再加 mumu-state／read-m9a-connection
        //   (Get-GdMuMuState／Get-M9AConnection 的 C# 直身，round-preview:r1999 直答臂的 ADB 行与 MuMu 行经它们出口)。
        // 刀③ round-resolve 判定面进核(2026-11-03)：97 → 99，链尾再加 compose-decision-inputs／resolve-ark-farm-decision
        //   (Resolve-ArkPlan／Resolve-R1999Plan 的组值半段与 Resolve-GdArkFarmDecision 的活层——
        //   核臂组 decide-* body 缺的 settings 通路与 farm 半段的取数四件；既有 verb 的回执形状一枚没动)。
        // 刀④ status 族接线(2026-11-04)：99 → 101，链尾再加 resolve-status-env-rows／resolve-status-proc-names
        //   (lib\ark-status.ps1＋lib\r1999-status.ps1 两整件里四枚纯数据名册——EnvRows 行集三态与 ProcNames 写死名单——的直身；
        //   status.ps1 体检链自此问这两枚与屏保两枚核出口，那两本 lib 件随批出仓。既有 verb 的回执形状一枚没动)。
        public const int ExpectedCount = 101;

        // 顺序＝Cli.cs 分派链里的出现顺序（自检只比集合，不比顺序；顺序留着是为了肉眼对照分派链方便）。
        public static readonly string[] Names = new string[]
        {
            "echo",
            "decide-ark",
            "decide-r1999",
            "check-stages",
            "r1999-instance",
            "resolve-screensaver-who",
            "resolve-screensaver-verdict",
            "build-relaunch-args",
            "detect-installs",
            "maa-queue-lines",
            "m9a-instance-lines",
            "parse-asst-state",
            "parse-asst-hint",
            "plan-maa-queue",
            "m9a-launch-args",
            "parse-m9a-lines",
            "resolve-m9a-status",
            "parse-mumu-state",
            "parse-mumu-vms",
            "parse-adb-devices",
            "parse-adb-pidof",
            "build-vision-prompt",
            "parse-vision-verdict",
            "vision-click-point",
            "resolve-vision-endpoint",
            "shot-changed",
            "parse-vision-reply",
            "rescue-entry",
            "compose-ark-desktop-state",
            "parse-farm-target-spec",
            "parse-ark-drop-text",
            "resolve-ark-farm-plan",
            "apply-ark-daily-intent",
            "apply-r1999-daily-intent",
            "parse-maa-gui-lines",
            "split-task-args",
            "eval-supervisor-settings",
            "parse-supervisor-state",
            "resolve-supervisor-action",
            "compose-supervisor-detail",
            "decide-maa-run-tick",
            "decide-m9a-run-tick",
            "decide-daily-retry",
            "apply-maa-gui-events",
            "finish-maa-run",
            "apply-m9a-events",
            "detect-maa-fight-skip",
            "observe-maa-stage-open",
            "decide-ark-event-probe",
            "resolve-rescue-context",
            "ark-find-window",
            "ark-window-shot",
            "ark-window-click",
            "ark-grab-foreground",
            "ark-desktop-state",
            "vision-ask-model",
            "check-ark-launcher",
            "adb-connect",
            "adb-game-pid",
            "mumu-screen-shot",
            "mumu-screen-click",
            "resolve-round-context",
            "start-elevated-round",
            "send-round-notice",
            "start-run-lock",
            "stop-run-lock",
            "read-task-state",
            "stop-process-tree",
            "read-ark-activity",
            "read-r1999-activity",
            "read-decision-state",
            "set-ark-shop-state",
            "set-r1999-ack",
            "set-r1999-shop-state",
            "takeover-processes",
            "stop-maa-and-game",
            "stop-m9a",
            "read-maa-fight-skip",
            "read-maa-stage-observation",
            "start-ark-client",
            "start-mumu",
            "apply-maa-daily-config",
            "apply-m9a-instance",
            "update-ark-farm-ledger",
            // ---- R9-a(2026-10-17)：轮骨架四格收编新立的四枚，位置＝Cli.cs 链尾(C-0′ 起链尾追加是唯一允许的加法) ----
            //   前三枚住 Round.cs(屏保稳态读数／屏保只关不复／history.csv 那一行)，第四枚住 Round.Data.cs(活动图自证)。
            //   ⚠ 这四处新增**不改任何既有 verb 的回执形状**：改的是"谁来问"，不是"问回来什么"。
            "read-screensaver-config",
            "disable-screensaver-for-round",
            "append-round-history",
            "update-ark-event-probe",
            // ---- 乙档刀①(2026-10-23)：panel.ps1 的 -DumpSummary 判定面转写进核(tool 侧只有一份真身) ----
            //   消费方＝面板自检(--selftest 那七格摘要机读面)；生产链暂不接，接在刀③。
            "emit-panel-summary",
            // ---- 宿主批：监督 agent 起停的 C# 直身入口(宿主侧，不是轮骨架外发名册里的动作) ----
            "start-supervisor",
            "stop-supervisor",
            // ---- 宿主批第二刀：start-supervisor 起的那枚分离子进程的前台入口(阻塞到收工)，调用方不直接调它 ----
            "supervise-loop",
            // ---- 转接刀·时间换算族(2026-11-03)：把 Gd.Utc8ToMs／Gd.Fmt 露成出口，PS 侧那两枚真身因此出仓 ----
            "convert-utc8-to-ms",
            "format-unix-ms",
            // ---- 刀① round-gate/stop 四臂直答(2026-11-03)：Stop-GdMuMu 的 C# 直身，收尾钩子 stop:r1999 的
            //      "跑完关模拟器"那半经它出口(closeEmulatorAfter 的判据在核内直答臂，本件不判开关) ----
            "stop-mumu",
            // ---- 刀② round-preview 两臂直答(2026-11-03)：mumu-state 回执比真身多一枚 Index(MuMu 号牌，ctx 里没有 mumu 段)；
            //      read-m9a-connection 只读实例自己的 json(AdbDevice 三格，adb 空回落 mumuDir+mumuAdb) ----
            "mumu-state",
            "read-m9a-connection",
            // ---- 刀③ round-resolve 判定面进核(2026-11-03)：compose-decision-inputs 组判定接线值(dow 周日=7／
            //      now 毫秒／ack／领空折好的 shopDone／probe／ark·re1999 块与 paths 块)；resolve-ark-farm-decision
            //      是 Resolve-GdArkFarmDecision 的活层(三前置＋取数＋进程内调 FarmPlan) ----
            "compose-decision-inputs",
            "resolve-ark-farm-decision",
            // ---- 刀④ status 族接线(2026-11-04)：status 体检链问核的两枚纯数据出口(Detect.cs 直身)，
            //      两本 lib\*-status.ps1 的四枚写死名册随这两枚出仓 ----
            "resolve-status-env-rows",
            "resolve-status-proc-names"
        };

        // "未知子命令"那句报错用的名册串：逐枚全名，不再有通配与纯计数。
        public static string Roster()
        {
            return string.Join(" / ", Names);
        }

        // 给自检用的集合视图（大小写不敏感比较交给调用方，这里只出集合）。
        public static HashSet<string> NameSet()
        {
            HashSet<string> s = new HashSet<string>(StringComparer.Ordinal);
            for (int i = 0; i < Names.Length; i++) s.Add(Names[i]);
            return s;
        }
    }
}
