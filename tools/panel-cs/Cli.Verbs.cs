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
        public const int ExpectedCount = 89;

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
            "emit-panel-summary"
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
