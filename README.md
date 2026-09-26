# 每日清体力日常（明日方舟 端游 + 重返未来：1999）

一条命令（或两个定时点）把两款的日常跑完：方舟走 **MAA + 端游客户端**，1999 走 **M9A + MuMu 模拟器 12**，
跑完自动收尾（关助手、关端游、关模拟器）。

工具本身还是用你已经在用的 MAA / M9A —— 这里只做「判断今天该跑什么 + 改配置 + 启动 + 看日志判断结果 + 收尾」。

---

## 0. 用之前先看这三条

- **本仓库只有 PowerShell 脚本，不含任何助手或游戏的二进制/素材。** MAA、M9A、MuMu 模拟器、游戏客户端都要你自己装（许可见 [第 8 节](#8-许可免责与隐私)）。脚本做的事就是启动这些程序、读它们的日志和配置文件。
- **游戏用户协议普遍禁止脚本/自动化。** 这类工具在圈子里被默认容忍 ≠ 官方许可；封号或被处罚的后果由使用者自己承担，建议只在个人账号上、并且清楚自己在做什么的情况下使用。
- **判定逻辑和助手版本是强耦合的。** MAA / M9A 升级后改了日志文案或配置字段，判定就可能失效。所以脚本在「一条认识的标志都没读到」时会明确报 `unknown-format`（而不是猜一个失败原因），并且**不会**据此判定日常已清完 —— 看到这状态请按 [第 6 节](#6-出问题怎么查) 对一下日志再更新 `lib\` 里的正则。

---

## 1. 日常操作：双击 `panel.cmd`（主控台）

面板是**面向用户的主控台**，日常不需要记命令：今天跑什么、为什么这么判、改配置、改时间点、翻历史、打包诊断，全在界面里点。
双击 `panel.cmd` 会先用系统自带的 .NET 编译器把 `tools\panel-cs\` 里那几份 `.cs`（同一个 `partial class` 按职责拆的 11 份，`Panel.cs` 是入口那份）现编成 `tools\panel-cs\panel-cs.exe` 再打开（黑白双主题，右上角切深/浅）；
编不出来（极精简系统）才回落到 `tools\panel.ps1` 那版 PowerShell 面板 —— 兜底版，功能一致。

左边导航：上面「总览 / 方舟 / 1999」按模块注册表长出来，只换"看哪一路的读数"；下面四页是操作台：

| 页 | 干什么 | 写到哪儿去 |
| --- | --- | --- |
| 配 置 | 助手路径（MAA / 端游 / M9A / MuMu / 虚拟机序号）+ 视觉兜底与通知 | 路径改动代跑 `setup.ps1`（探测与合并的规则归它）；标量项面板行内写 `settings.json`，写前备份到 `backups\` |
| 定 时 | `schedule.times` 与机器上真存在的 `GameDaily-*` 任务并排对齐，改时间点 / 注册 / 卸载 | 代跑 `tools\reschedule-daily.ps1`、`install-task.ps1` |
| 历 史 | `state\history.csv` 最近 12 轮，带原因的行可以点，整句进日志区；一键打开 `logs\` | 只读 |
| 诊 断 | 读数 + 体检 + 抹掉 key 的配置摘要 + history.csv + 最近 5 个运行日志 → 桌面 zip | 只读 + 打包 |

总览页上还有一张「实验功能」卡：视觉兜底的三档开关（端游那路 / 模拟器那路 / 要不要允许领日常奖励，默认**不允许**）和一颗「只问不点」，
它写的是 `settings.json` 里那三个布尔键（详见 6.1）。老 settings 里没写过的键，卡片会替你在 `runtime` 段补出那一行：落盘前先自己核对整份文件的形状（引号配平、括号配平、不留收尾逗号），形状认不出来就**不写**；写完再把这一行读回来核对，对不上就当没改、当着人把勾退回原位并说明原因。至于"补出来的那一行 `ConvertFrom-Json` 到底读不读得回来"，由面板自检 `--exptest` 替你验（见第 6 节），不在点勾那一刻起子进程。

**面板不自己判定任何东西**：界面每行结论都转达自判定层（`tools\panel.ps1 -DumpSummary` 的协议行、以及判定层逐字写进
`state\history.csv` 的那句原因）；视觉 Key 只落这台机器的 `settings.json`，不进命令行参数、不进日志、不进仓库。
读数第一行是 `PROTO|<版本>`：判定层和面板对不上版本时，面板会在日志区明说「判定层协议 v几 / 我只认 v几」，
而不是让某个格子静默变空 —— 读不懂必须说出来。
自检：`tools\panel-cs\panel-cs.exe --selftest`（布局/主题）、`--consoletest`（四页取数 + 只在 settings 临时副本上写 + 诊断包不含 key）
与 `--exptest`（「实验功能」那三个布尔键的读写回环：翻一次读得回来、只有那一行动过、补出来的行要让 `ConvertFrom-Json` 复述一遍、
认不出的形状必须拒着不写 —— 全程打在 `%TEMP%` 副本上，收尾逐字节比对确认原件没动），
兜底面板 `pwsh -File tools\panel.ps1 -SelfTest` / `-HeadlessCheck`。

### 1.1 命令行（兜底与自动化）

面板没覆盖的边角、以及定时任务和脚本化调用走命令行。四个入口分别是：

| 想做什么 | 怎么做 |
| --- | --- |
| 不想记命令，一步步选 | 双击 `start.cmd` —— 中文菜单，选数字就行 |
| 看今天会怎么跑（**什么都不改、不启动**） | `pwsh -File status.ps1` 或 `pwsh -File daily-clear.ps1 -DryRun` |
| 手动跑一轮（**注册过任务后不再弹 UAC**） | 双击 `run-daily.cmd`，等价于 `pwsh -File run-via-task.ps1` |
| 每天两个时间点自动跑（默认 08:00 / 20:00，改法见第 5 节） | `pwsh -File install-task.ps1`（注册计划任务，弹一次 UAC） |

`start.cmd` / `panel.cmd` / `run-daily.cmd` 会自己挑引擎：装了 PowerShell 7 就用 `pwsh`，没装就用系统自带的
`powershell.exe`（5.1）。仓库里的脚本 5.1 和 7 都能跑，下面示例里的 `pwsh` 换成 `powershell` 也一样。
不放心可以用 `pwsh -File tools\check-ps51.ps1` 自检（在 5.1 里跑出来的结论才算数）。

**为什么需要管理员**：明日方舟端游客户端要管理员权限，MAA 未提权时连不上它（Windows UIPI 挡住输入注入），
表现就是 MAA 一直停在「正在连接模拟器……」。有三条路拿到管理员权限：

1. **计划任务通道（推荐，注册一次以后不再弹）**：`install-task.ps1` 除了 08:00 / 20:00 两个每日任务，还会注册一个
   无触发器的按需任务 `GameDaily-Manual`（同样是最高权限）。`run-via-task.ps1` 只负责 `Start-ScheduledTask` 点火、
   跟日志、回传退出码 —— 普通权限就能触发自己账号下的最高权限任务，所以**不弹 UAC**。按需任务不存在时会自动退回直跑。
2. **自我提权**：直接跑 `daily-clear.ps1` 时检测到自己不是管理员，就用同样的参数以管理员身份重启自己，等子轮结束、
   把退出码原样返回（弹一次 UAC）。注意 pwsh 若是 Microsoft Store(WindowsApps) 版，系统不允许直接对它提权，
   脚本会自动改用 `System32\cmd.exe` 做提权载体（UAC 上显示的是 cmd.exe，属正常）。
3. **只跑 1999**（`-Games r1999`）不需要提权；不想自动提权加 `-NoElevate`；`-DryRun` 永远不弹。


命令行常用姿势：

```powershell
.\daily-clear.ps1 -DryRun              # 预览：今天判定成什么、将要改哪些配置
.\daily-clear.ps1                      # 完整一轮（方舟 + 1999）
.\daily-clear.ps1 -Games ark           # 只跑方舟
.\daily-clear.ps1 -AckArk              # 「新活动我手动打过了」→ 之后每天自动刷活动图
.\daily-clear.ps1 -Ack1999             # 同上，1999 主线/活动
.\daily-clear.ps1 -ArkShopDone         # 「活动商店我已领空」→ 之后不再刷活动，剩余理智转长草
.\daily-clear.ps1 -RShopDone           # 同上，1999 活动商店
.\daily-clear.ps1 -ArkShopReset        # 误标了/活动又开新一批兑换 → 撤销领空标记，恢复刷活动
.\daily-clear.ps1 -RShopReset          # 同上，1999
.\daily-clear.ps1 -Games ark -ArkStage SK-5   # 临时指定刷某个关卡
.\daily-clear.ps1 -Games r1999 -RPlan rerun   # 临时强制某个 1999 方案
.\daily-clear.ps1 -SkipCleanup         # 跑完不关模拟器/端游（自己接着玩）
```

退出码：`0` 全部完成 / `1` 部分完成 / `2` 失败。运行日志在 `logs\`，历史汇总在 `state\history.csv`。

## 2. 每天到底刷什么（自动判定）

**明日方舟**：读 MAA 官方活动接口（拉不到就退回 MAA 本地缓存 `cache\gui\StageActivityV2.json`）。

- 有进行中的活动关卡 → 理智全刷活动图（活动代币优先）；标记过「商店已领空」的活动会被跳过，见第 3 节；
- 没有活动（长草）→ 按 `config\settings.json` 里 `ark.defaultStage` / `planByWeekday` 刷，默认 `CE-5,LS-5`（龙门币 + 经验，都是每天开放的常驻本，MAA 的「剩余理智」会按列表顺序把理智刷干）；
- **不吃理智药**（脚本会把 `UseMedicine / UseStone / UseExpiringMedicine` 一律压成 false）。

**重返未来：1999**：读 M9A 自带的活动日历 `data\activity\cn.json`，按当前版本窗口判断，然后自动选实例：

| 判定 | 用的 M9A 实例 |
| --- | --- |
| 长草 | `日常-长草` |
| 当期活动 | `日常-活动` |
| 复刻活动 | `日常-复刻` |

> **1999 也已经全自动了（2026-09-23 打通）。** 用的是 M9A 自己的命令行脚本化入口，不用点按钮、不用热键、不用 UI 自动化：
>
> ```
> m9a.exe --instance <实例id> --autostart --quit-after-run --force-start
> ```
>
> 实例 id 是 M9A 给每个实例生成的短 hash，**每台机器都不一样**（我机器上那个不写在这里：`tools\test-modules.ps1` 现在会拿本机的实例 id 扫源码和文案，写进来就红）；
> `pwsh -File status.ps1` 的 M9A 段会把「方案 → 实例名/id」列出来，`tools\probe-m9a-history.ps1` 也能看到历史用过的 id。
>
> 帮助原文（从 `libs\MFAAvalonia.Core.dll` 里挖出来的）：
> `-i/--instance` 指定启动的实例；`-a/--autostart` 自动开始任务；
> `-f/--force-start` 配合 `--instance`，停掉现有任务、按命令行重新连接并开始，不读上次任务状态；
> `-q/--quit-after-run` 配合 `--instance`，任务结束后退出程序 —— 注释里写着「用于脚本化调度」。
> M9A 已经在跑时这条命令会被**转发**给正在跑的实例，所以冷启动和在跑两种情况都能用。
>
> **坑（踩过，务必用长参数）**：短参数 `-a` 会被 M9A 原样记成 key `a`（日志：`启动参数：数量=4，内容=[[instance, …], [a, ], …]`），
> 它的自动开始判定匹配不上，队列根本不提交 —— 表现就是「连上模拟器 + 起完 Agent，10 秒后报『任务已全部完成』、一个任务都没跑」；
> 换成 `--autostart` 才打出 `用户操作：启动任务` → `准备执行任务队列：任务数量=10`。`-i/-q/-f` 同理一律不用。
>
> 相关开关都在 `config\settings.json` 的 `runtime` 里：`m9aCliArgs`（关掉就回到裸启动）、`m9aInstanceArg`（`id` 或 `name`，实测 id 可用）、
> `m9aForceStart`（带不带 `--force-start`；它会接管当时正在跑的任务，手动时段可自行关掉）。
>
> **判定完成用的是 `>>> 任务 启动任务完成（异步）。`**，不是界面上那句「任务已全部完成！」——
> `--quit-after-run` 下 M9A 打完 `启动任务完成` + `停止前状态：SUCCEEDED` 就「应用已关闭」（实测 09-23 17:43:33.977 → 17:43:34.286），
> 根本来不及写「任务已全部完成」。而 `启动任务开始/完成` 这一对是**括住整条链路**的（08-30 手工日志里相隔 23 分钟），
> 所以只有「一条『开始任务：X』都没见过」时它才表示只连上了模拟器。
> 2026-09-23 首次真跑（17:36→17:43，长草 10 个任务）就是因为这条判定写错被误判 `error`、白重试了一轮，现已修正。
>
> 想复核判定逻辑，不用再花十几分钟重跑游戏：`pwsh -File tools\replay-m9a-log.ps1` 会把真实日志按时间窗口喂进同一套解析/状态机
> （默认整份「今天的日志」全量回放，`-Log/-From/-To` 换任意历史轮次，含 08-30 的手工成功轮；只写时刻不写日期就按那份日志当天算。
> 实例 id 不用填：日志每行带 `[inst=名字/id]`，脚本取窗口里出现最多的那个并在开头打出来，认错了才用 `-InstanceId` 指名）。
> 想看历史每实例真跑过几个任务：`pwsh -File tools\probe-m9a-history.ps1`。

## 3. 两道手动门：活动确认 + 商店领空

**门 1：活动确认（「版本更新了但我还没打活动关卡就先别刷」）**

版本更新出了新活动、但你还没手动通一遍时，脚本**不会**自动烧理智：

- 方舟：本轮只跑 公招 / 基建 / 信用 / 领奖，理智留着；通知会提示你确认。
- 1999：回落到长草方案，不进未确认的新主线/活动。

你自己打过之后确认一次即可，之后每天自动刷：

```powershell
.\daily-clear.ps1 -AckArk -DryRun      # 只登记确认，不跑
.\daily-clear.ps1 -AckArk              # 登记确认 + 顺手跑一轮
```

确认状态存在 `state\ark-event-ack.json` / `state\r1999-event-ack.json`（按活动 id 记，一个活动确认一次）。
临时想越过的话用 `-Force`。

**门 2：商店领空（「活动商店换完了就别再刷活动了」）**

活动还在开、但代币兑换商店已经被你搬空时，再刷活动图就是纯浪费理智。这时候打个点：

```powershell
.\daily-clear.ps1 -ArkShopDone         # 方舟：标记当前活动商店已领空
.\daily-clear.ps1 -RShopDone           # 1999：同上
```

之后的每一轮：

- 方舟：该活动被从候选里剔除，剩余理智改刷长草计划（`CE-5,LS-5` 或你配的 `planByWeekday`）；
  同时还有别的已确认活动时会自动改刷那个，提示里写清「另有 N 个活动商店已领空，已跳过」。
- 1999：改用 `日常-长草` 实例，不再跑 `日常-活动` 的代币刷取。
- 标记按活动 id / 版本号记（`state\ark-shop-done.json`、`state\r1999-shop-done.json`），
  下一个新版本或新活动自动不适用，**不用记得回来取消**；领空领错了想恢复刷活动：`-ArkShopReset` / `-RShopReset`。
- 不想用这道门：把 `config\settings.json` 的 `ark.shopDoneStopsEvent` / `re1999.shopDoneStopsEvent` 改成 `false`。

这道门必须手动：MAA 的活动接口只给「活动在不在开、哪些关掉代币」，看不到你的商店兑换进度；
游戏里也没有可靠的「商店已空」信号（MAA 日志里根本没有活动商店相关记录），猜出来的只会误判。

## 4. 首次使用（新机器上一次性）

0. **生成配置**：仓库里只有 `config\settings.example.json`（`settings.json` 被 `.gitignore` 排除，见第 8 节）。

   ```powershell
   pwsh -File setup.ps1 -DryRun     # 只看探测到什么，不写文件
   pwsh -File setup.ps1             # 探测 + 写入 config\settings.json
   ```

   探测优先级：**命令行指定 > 现有 `settings.json` 里仍然有效的路径 > 正在运行的进程 > 卸载表 > 各固定磁盘浅层扫描（深度 ≤ 4）**，
   每项都会打印来源，磁盘副本目录（`Downloads\`、`xx (2)\` 这类）会被排到最后并标注「像下载/备份副本」。
   探测不到就用参数手工指定（相对路径按当前目录解析）：

   ```powershell
   pwsh -File setup.ps1 -MaaDir "D:\Tools\MAA" -M9aDir "D:\Tools\M9A" -MuMuDir "D:\Programs\MuMuPlayer-12.0" `
                        -ArkExe "D:\Games\Arknights\Arknights.exe" -VmIndex 0
   ```

   退出码 `0` = 必填项齐了，`2` = 还有项没填（输出里会列出来，手填 `settings.json` 也可以）。
   **脚本只保证「路径存在」，不保证那是你想用的那份助手配置**，所以探测完一定看一眼输出，再跑 `status.ps1`。
   `-SkipScan` 只走进程/卸载表（快，适合没装在哪都能猜到的机器），`-Force` 按模板整体重建（会丢掉你手改过的其他字段）。

1. **允许一次 UAC**：跑 `install-task.ps1` 注册计划任务时点「是」，之后定时跑和双击 `run-daily.cmd` 都不再弹（任务自带最高权限）。
2. `config\settings.json` 的路径块（MAA / M9A / 端游 / MuMu）由上面第 0 步填好，换目录只改这里。
3. **端游由脚本拉起**：MAA 的配置里其实没存端游路径（`StartGame=true` 也只会报「未找到窗口」），所以 `daily-clear.ps1`
   会先启动 `paths.arknightsExe` 指向的端游、等窗口出现、再把 `StartGame` 关掉后启动 MAA 去连。
   端游退出后要重新登录/更新的话日常会卡在这一步（脚本报 `not-started` 并通知你，超时由 `runtime.arkStartTimeoutMinutes` 控制）。
4. **不要锁屏**：定时点以 `config\settings.json` 的 `schedule.times` 为准（默认 08:00 / 20:00），机器保持登录、允许熄屏即可（脚本运行期间顶住睡眠和熄屏；屏保自 2026-09-26 起策略为**只关不复**——每轮起跑时关掉后不再恢复，若会话已被屏保/锁屏桌面接管，方舟一路直接早退 `not-started` 并写明原因，不再白烧十几分钟）。设置了「无人操作自动锁屏」的话请先关掉，锁屏 = 桌面不可用 = 截图失败。
5. 1999 这一路走 M9A 自己的命令行入口（见第 2 节），脚本每次都会把 `NoAutoStart` 压成 `False`。第一次跑请盯一眼：`runtime.r1999StartTimeoutMinutes`（默认 10 分钟）内没跑任何任务就返回 `not-started`，一个标志都没读到则返回 `unknown-format`。
6. MuMu 的虚拟机索引由 `setup.ps1` 探测填写（本机是 `5`，对应 `127.0.0.1:16544`）。你在 MuMu 里多开的话，这个 index 要对应装了 1999 的那个 —— 有多台时 `setup.ps1` 不会替你猜，会让你用 `-VmIndex` 指定。

## 5. 计划任务

```powershell
pwsh -File install-task.ps1 -DryRun            # 先看要建什么
pwsh -File install-task.ps1                    # 注册 GameDaily-0800 / GameDaily-2000
pwsh -File install-task.ps1 -Times 07:30,19:30
pwsh -File install-task.ps1 -Uninstall
```

**改时间点用 `tools\reschedule-daily.ps1`，别只跑 `install-task.ps1 -Times`** —— 后者只更新/新建目标时间点的任务，
**不会删掉旧时间点的**，从 `08:00,20:00` 换成 `06:00,18:00` 只跑它的话机器上会同时留着四个任务，一天跑四遍。

```powershell
pwsh -File tools\reschedule-daily.ps1 -Times 06:00,18:00 -DryRun   # 先看会注册什么、会删哪几个旧任务
pwsh -File tools\reschedule-daily.ps1 -Times 06:00,18:00           # 等当前轮次跑完 → 写 settings.schedule.times → 重注册 → 删旧任务(弹一次 UAC)
```

它会等正在跑的一轮结束（最多 `150` 分钟，超时就放弃、什么都不改），把新时间点写回 `config\settings.json`，
注册新任务，再删掉不在新列表里的 `GameDaily-*`（`GameDaily-Manual` 保留），最后打印验证表。

任务跑在**当前用户会话**里（助手要连本机 GUI，所以用「只在登录时运行」），并且以**最高权限**运行（端游/MAA 要管理员，注册这种任务本身需要提权，脚本会自动弹 UAC）。错过时间点会尽快补跑（`StartWhenAvailable`），单次最长 3 小时被强杀。

## 6. 出问题怎么查

- `pwsh -File status.ps1` —— 环境、进程、今天判定、MAA 队列、M9A 实例、确认状态、最近记录，七段一起看。
- `pwsh -File tools\test-decisions.ps1` —— 纯逻辑自检（只读：不改配置、不启动任何程序，`powershell.exe -File` 同样能跑）。方舟那一路 178 条、1999 那一路 151 条、框架层（模块清单齐不齐、注册表对不对、框架有没有掺进业务知识、本机专属值有没有混进源码和文案）178 条，其中公共库那 25 条两边各跑一次，合计 507 条。
  1999 那一路另有 7 条要读本机某天的真实 M9A 日志（断言的是我那次真跑的内容，换台机器不适用），加 `-History` 才跑，合计 514 条。
  只查一路就单跑 `tools\test-ark.ps1` / `tools\test-r1999.ps1` / `tools\test-modules.ps1`（或 `test-decisions.ps1 -Only ark`；`-Only both` 是老的"只跑两款游戏"写法）。改过 settings 或判定代码后跑一遍最放心。
- `pwsh -File tools\probe-m9a-history.ps1` —— 统计 M9A 历史日志里每个实例真正开始过几个任务（1999 卡住时先看这个，实例 id 变了也在这里露出来）。
- `pwsh -File tools\replay-m9a-log.ps1` —— 把某一轮的真实 M9A 日志按时间窗口回放给判定逻辑（默认整份今天的日志、实例自动认，`-Log`/`-From`/`-To`/`-InstanceId` 可覆盖），改完判定不用重跑游戏。
- `pwsh -File tools\probe-decisions.ps1` —— 把活动接口逐条窗口打出来，怀疑判定不对时看这个。
- `pwsh -File tools\vision-ask.ps1 -Game ark` —— 把现在的屏幕交给视觉模型、把它怎么说的原样打出来，**一次点击都不做**。想知道"兜底为什么没救活"或者"它到底看见了什么"时用这条（详见 6.1）。
- 助手自己的日志：`<maaDir>\debug\gui.log`、`<m9aDir>\logs\log-YYYYMMDD.log`（两个目录见 `config\settings.json` 的 `paths`）。
- 被脚本改过的配置都在 `backups\` 留了最近 20 份，出问题直接覆盖回去即可。

判定规则小结（脚本靠这些信号判断一轮跑完）：

| | 成功 | 卡住 |
| --- | --- | --- |
| MAA | `gui.log` 出现 `任务已全部完成！` | MAA 自己的「日志无更新/停滞」告警，或 22 分钟无新日志（`stalled`）；日志在涨却一条标志都没认出、也没到该开始的时间点 → `unknown-format` |
| M9A | 当天日志（按实例 id 过滤）里 `准备执行任务队列：任务数量=N` → 若干 `开始任务：X` → `>>> 任务 启动任务完成（异步）。`；跑满 N 个后 M9A 因 `--quit-after-run` 自己退出也算成功 | 只有「启动任务完成」却一条任务没有（`not-started`／`empty-queue`，秒判不硬等）；进程中途退出且任务没跑满（`error`）；22 分钟无新日志（`stalled`）；超过 85 分钟没跑完（`timeout`）；**一条认识的标志都没有 → `unknown-format`** |

`unknown-format` 是唯一「结果不可信」的状态：它不猜原因（到底是助手改了日志文案、实例重新导入导致 id 变了，还是助手压根没写日志），
只告诉你别按这一轮判定体力是否清完，并给出下一步命令（`probe-m9a-history.ps1` 看实例 id → `replay-m9a-log.ps1` 对着日志复现 → 改 `lib\` 里的正则）。

一轮失败会自动重试一次（`runtime.retryOnce`），但 `not-started` / `empty-queue` / `unknown-format` 不重试（重试也是白烧 10～20 分钟）；再失败就收尾 + 弹通知，不会整晚挂着。
重试的完整动作是「关干净 → 等 25 秒 → 重新拉端游 → 再跑助手」，所以日志里会看到两次「启动端游」。这条 2026-09-24 早上修过一次：拉端游那句原来写在重试闭包外面，第一轮被判 `stalled` 后端游已经被关掉了，第二次只有 MAA 起来、游戏没起来，直接白跑一轮。

### 6.1 卡住时谁帮忙看屏幕：视觉兜底

助手偶尔会被"屏幕上一个谁都不认识的弹窗"堵住（端游的活动页盖住整屏、弹窗吃掉点击）。这时候脚本会**看一眼屏幕**：截图 → 交给视觉模型 → 模型只准回答"该点哪个关闭按钮" → 点它 → 继续跑。

两条路的取屏和点击方式不同，用的是**同一个模型、同一把 key**：

| 这一路 | 截图 | 点一下 | 要管理员 |
| --- | --- | --- | --- |
| 方舟（端游窗口） | `PrintWindow` 截那个窗口自己（被全屏游戏挡住时 `CopyFromScreen` 截到的是游戏，所以不用它） | 发给端游窗口的消息点击 | 要（端游本身就要提权） |
| 1999（MuMu 虚拟机） | `adb exec-out screencap` | `adb shell input tap`（消息进虚拟机，**碰不到宿主机上你正在用的窗口**） | 不要 |

**什么时候出手**：不需要等 22 分钟超时。MAA 在同一个子任务上反复点同一处、一条链路都没完成，这就是"点击没被吃进去"的特征，脚本当场判定卡住 → 兜底一次 → 立刻早退交给外层那一次重试。只点了一两次数不算卡住（正常推进也会点）。

**它被硬约束成只会做一件事**：提示词里只准它挑"会关掉这个东西"的按钮，最多回 3 个候选点位并按把握排序，脚本逐个试、点通了画面真的变了才继续。画面没变就当它没说对，不会重复点同一处。

**默认绝不让它领东西**：「领取 / 购买 / 确认」这类按钮在提示词里是禁词 —— 判错一次就是拿账号换一次不确定。要放开这一档，得自己在面板「实验功能」卡里勾「允许领日常奖励」（写 `runtime.visionAllowReward`，默认不勾）。

三档开关各管一段，互不牵连（面板「实验功能」卡上就是这三颗勾 + 一个「只问不点」按钮）：

| 键 | 默认 | 管什么 |
| --- | --- | --- |
| `runtime.arkVisionRescue` | 开（没写这行就是开） | 端游那一路要不要兜底 |
| `runtime.mumuVisionRescue` | 开（同上） | 模拟器那一路要不要兜底 |
| `runtime.visionAllowReward` | **关**（没写这行就是关） | 要不要把「领日常奖励」这类按钮放给它点 |

每次救火都记在 `state\rescue.json`（哪一路、什么时候、救活没有，保留最近 60 条）——"这一轮到底兜了几次、有没有用"是能查的，不是感觉。

**Python 只做"问模型"这一层**：`tools\vision\ask.py` 收一个由 PowerShell 写好的请求文件（里面是 PNG 路径和要问的那句话），把模型回的原文交出去。**截图和点击都还在 PowerShell 这边**，Python 碰不到、也不需要碰你的账号和窗口。这台机器没有 Python 也能跑：`pwsh -File setup.ps1 -WithPython` 会按需下载一个 embeddable 包到 `state\python\`（不进仓库）；你自己装过 Python 3 就把 `runtime.pythonExe` 指过去。没有可用的 Python 时，兜底会说明原因后放弃，日常主流程不受影响。

想只问不点、看看它到底看见了什么：`pwsh -File tools\vision-ask.ps1 -Game ark`（面板上那颗「只问不点」就是代跑这一条）。它把现屏交给模型、把回答原样打出来，一次点击都不做 —— 想知道"兜底为什么没救活"，先跑这条比改代码便宜。

## 7. 文件结构

```
game-daily\
  setup.ps1            第一次用：探测助手装在哪 → 生成 config\settings.json
  daily-clear.ps1      合体入口：收参数(-AckArk 这类带游戏名的) → 转调下面两路，收尾 + 通知
  status.ps1           只读体检(外壳，段落函数在两路的 *-status.ps1 里)
  install-task.ps1     计划任务注册/卸载
  run-daily.cmd        双击手动跑
  start.cmd            双击开中文菜单(不认命令行的用法，全部动作菜单里选)
  panel.cmd            双击开主控台(现编 C# 面板；编不出来回落 PowerShell 面板，见第 1 节)
  config\settings.example.json 配置模板(入仓)
  config\settings.json 唯一需要配的文件(路径 + 策略 + 超时 + 时间点，不入仓)
  lib\common.ps1       日志/JSON/备份/防睡眠/进程/通知/接管关闭
  lib\detect.ps1       安装位置探测：进程 → 卸载表 → 磁盘浅层扫描
  lib\modules.ps1      框架层：模块注册表 + 一轮日常的公共流程(不认识任何一款游戏，只按清单调钩子)
  lib\ark-module.ps1   方舟的模块清单：显示名/要不要管理员/设置块 + 七个钩子怎么转调 ark-*.ps1
  lib\r1999-module.ps1 1999 的模块清单：同上(转调 r1999-*.ps1 / m9a.ps1 / mumu.ps1)
  lib\decisions.ps1    判定层门面：按"文件在不在"点进下面三个，本体一个都不写在这
  lib\decisions-common.ps1  两路共用的底座：UTC+8 时间换算、活动确认门/商店领空标记读写
  lib\ark-decisions.ps1     方舟：活动窗口 → 刷哪关、关卡代号校验
  lib\r1999-decisions.ps1   1999：版本窗口 → 长草/活动/复刻方案、M9A 实例查找
  lib\ark-daily.ps1        方舟这一路的动作：标记状态、定方案、预览、跑一轮
  lib\r1999-daily.ps1      1999 这一路的动作：同上(MuMu + M9A)
  lib\ark-status.ps1       方舟的体检段落(路径/进程/今日判定/MAA 队列)
  lib\r1999-status.ps1     1999 的体检段落(路径/进程/MuMu/实例列表)
  lib\maa.ps1          MAA 队列改写 + 启动 + gui.log 判定
  lib\m9a.ps1          M9A 实例切换 + 启动 + logs\log-*.log 判定
  lib\mumu.ps1         MuMuManager 启停、adb 探活
  lib\vision-rescue.ps1     视觉兜底的公共层：什么时候出手、抠模型回的 JSON、千分比换算点位、"画面变没变"、救火记账(见 6.1)
  lib\vision-ark.ps1        兜底的端游后端：PrintWindow 截窗口 + 发消息点击(要提权)
  lib\vision-mumu.ps1       兜底的模拟器后端：adb 拍屏 + adb tap(不要提权，也碰不到宿主机窗口)
  tools\menu.ps1       菜单本体(start.cmd 调它)
  tools\panel.ps1      面板本体(panel.cmd 调它，判定仍转调上面的脚本；左侧导航按模块注册表长出来；-DumpSummary 出机器协议行)
  tools\panel-cs\      C# 主控台(Panel.cs 是入口，其余 Panel.*.cs 是同一个 partial class 的分工；build.ps1 按目录扫 *.cs 用系统 csc 现编；读 panel.ps1 -DumpSummary 的协议行，不自己判定；四页操作台代跑 setup/install-task/reschedule)
  tools\vision-ask.ps1 只问不点：把现屏交给模型、原样打印它的回答，一次点击都不做
  tools\vision\ask.py  "问模型"那一层(PowerShell 写请求文件 → 它回模型原文)；截图与点击都不在这里
  tools\               自检 / 探测脚本(test-ark、test-r1999、test-modules(框架层)、跑全部的 test-decisions、
                       公共脚手架 test-lib、probe-*、replay-m9a-log、check-syntax、check-ps51、reschedule-daily)
  .gitattributes       行尾规矩：.ps1 = LF + UTF-8 BOM，.cmd = CRLF 无 BOM(5.1 读不了无 BOM 的中文 .ps1)
  logs\ state\ backups\ 运行时生成(不入仓) —— state 里另有 history.csv(每轮结论) 与 rescue.json(兜底记账)
```

依赖：PowerShell 7（`pwsh`，本机用的 7.6.6）**或**系统自带的 Windows PowerShell 5.1 —— 两边都测过
（`tools\check-ps51.ps1` 静态守卫 + 两个引擎跑同一套脚本对比输出）。方舟一路不需要 adb，1999 一路用 MuMu 自带的 `adb.exe`。

## 8. 许可、免责与隐私

**许可**：本仓库的脚本按 MIT 许可发布（见 `LICENSE`），只包含作者原创的 PowerShell 代码。

**不含第三方程序**：MAA（AGPL-3.0）、M9A（AGPL-3.0）、MuMu 模拟器（网易商业软件）、以及两款游戏客户端/素材/名称
都**不**在本仓库内，也不由本仓库分发，需要你自己从各自官方渠道安装。脚本只是启动这些程序、读它们的日志与配置文件。
MAA / M9A 各自的许可条款以它们自己的仓库为准；本仓库的 MIT 不改变、也不覆盖它们的要求。

**免责**：软件按「现状」提供，不含任何明示或暗示的保证。游戏用户协议普遍禁止脚本与自动化，
使用本仓库可能带来账号处罚、时间损失或数据损失，风险由使用者自行承担；
作者不承担因使用本仓库产生的任何损失，也不提供任何游戏内结果承诺。

**隐私与脱敏**：`.gitignore` 已把这几类内容排除在仓库之外，因为它们里面是你的机器和个人信息 ——

| 排除项 | 为什么排除 |
| --- | --- |
| `config\settings.json` | 你机器的绝对路径、MuMu 虚拟机索引、助手实例名、`notify.webhook`（填了就是往自己地址 POST 每轮结果） |
| `logs\` | 每一轮的时间点、耗时、失败原因，攒起来就是你几点玩游戏的作息表 |
| `state\` | `history.csv` 同上；`*-event-ack.json` / `*-shop-done.json` 带你参与过的活动 id 与版本号 |
| `backups\` | MAA / M9A 配置的历史副本，可能含账号相关字段 |

所以克隆下来只会得到模板 `config\settings.example.json`，跑 `setup.ps1` 生成自己的配置即可（第 4 节第 0 步）。
反过来说：**别把这些目录重新加进版本库、也别把日志贴到 issue 里**，需要先自己把路径和实例名打码。

光靠 `.gitignore` 挡不住"顺手把本机路径粘进源码注释/自检样本/README"这种事，所以还有一道扫描：
`lib\common.ps1` 的 `Get-GdLocalOnlyValues` 运行时从本机配置里取出"只在这台机器成立"的值（四条安装路径、
MuMu 虚拟机名、M9A 每个实例的短 hash、视觉 Key、webhook、`%USERPROFILE%`），
`tools\test-modules.ps1` 的 `[m8]` 段就拿这套值扫**源码和文案**，`tools\publish-split.ps1` 发出去之前扫**导出目录**，
两边共用同一个函数。红的时候只报"哪个文件 <- 哪一项"，值本身不进日志（日志会进诊断包递给人看）。
这套判据前移的当天就抓到一处：本地 README 里写着某个真实实例 id。
