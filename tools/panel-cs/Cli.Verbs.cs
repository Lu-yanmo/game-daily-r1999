// tools/panel-cs/Cli.Verbs.cs —— verb 名册的**第二处**（数据面）。批次 C-0′「量尺」新立，见 docs\rust-core-plan.md 的 C-0′ 一节。
// 为什么要有这一份：Cli.cs 的分派是 84 条 `verb == "…"` 的 if-else 链，而"未知子命令"那句报错里的名册是**手写的散文长串**——
// 里面 15 枚只写了通配(`parse-asst-*` 一类)或纯计数("批次 1 判定四件")，且既有自检与 probe-cli-contract 都不钉 verb 枚数。
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
        public const int ExpectedCount = 84;

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
            "update-ark-farm-ledger"
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
