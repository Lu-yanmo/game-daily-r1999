# 重返未来：1999 每日清体力（M9A + MuMu 模拟器 12，无人值守）

每天定时把 1999 的日常跑完：**启动模拟器 → 拉起 M9A → 按当前版本自动选方案（长草 / 当期活动 / 复刻）→ 跑完任务队列 → 关游戏、关 M9A、关模拟器**。

用的还是 **M9A** 本身 —— 这里只做「今天该用哪个方案 → 切实例 → 用 M9A 自己的命令行入口启动 → 看它的日志判断结果 → 收尾」，
不需要点按钮、不需要热键、不需要 UI 自动化。

---

## 0. 用之前先看这三条

- **本仓库只有 PowerShell 脚本，不含 M9A、不含 MuMu、不含游戏客户端或素材。** M9A 是 AGPL-3.0 的第三方软件，MuMu 是网易商业软件，都要你自己从官方渠道装（许可见 [第 8 节](#8-许可免责与隐私)）。脚本只是启动它们、读它们的日志和配置文件。
- **游戏用户协议禁止脚本/自动化。** 被默认容忍 ≠ 官方许可；处罚风险由使用者自行承担，建议只在个人账号上用。
- **判定逻辑和 M9A 版本强耦合。** M9A 升级后改了日志文案、或实例重新导入导致实例 id 变了，判定就会失效。所以脚本在「一条认识的标志都没读到」时明确报 `unknown-format`，并且**不会**据此判定体力已清完 —— 按 [第 6 节](#6-出问题怎么查) 对一遍日志再更新 `lib\m9a.ps1` 里的正则。

**还有一条硬编码偏好**：出了新活动但你还没手动通一遍时，脚本**不会**自动进未确认的新主线/活动，
本轮回落到长草方案（第 3 节门 1）。

## 1. 四个入口

| 想做什么 | 怎么做 |
| --- | --- |
| 看今天会怎么跑（**什么都不改、不启动**） | `pwsh -File status.ps1` 或 `pwsh -File daily-clear.ps1 -DryRun` |
| 手动跑一轮 | 双击 `run-daily.cmd`，等价于 `pwsh -File run-via-task.ps1` |
| 每天按 `config\settings.json` 里 `schedule.times` 的时间自动跑 | `pwsh -File install-task.ps1` |
| 不想记命令，只想瞟一眼今天怎么样 | 双击 `panel.cmd`（图形状态面板，见第 1.1 节） |

### 1.1 图形状态面板（`panel.cmd` / `tools\panel.ps1`）

一屏显示：今天会刷哪个方案、用的是哪个 M9A 实例、两道手动门的状态、定时任务下次几点跑、上一轮结果；
按钮上留着：今天会跑什么（预演）/ 开始跑一轮 / 环境体检 / 我打过活动了 / 商店已领空 / 打开日志目录。
左边那条导航（总览 / 1999）只是换个角度看同一批读数 —— 点它不改变要跑什么，环境那一卡会换成这一路自己的路径探针。

面板**只是外壳**：每个按钮都是去调上面那几个脚本，不自己算结论，所以面板里看到的和命令行跑出来的是同一个。
「开始跑一轮」要连点两次才真的开跑。不想用鼠标的人可以 `pwsh -File tools\panel.ps1 -SelfTest`（不显示窗口，
把状态摘要、参数探测、布局检查打出来）。打包成 exe 的场合对应动词是 `--panel`。

另有一款**黑白双主题的 C# 面板**（`tools\panel-cs\Panel.cs`）随源码发布：直接双击 `panel.cmd`
（或不带参数开 exe）就会**自动现编**成 `tools\panel-cs\panel-cs.exe` 并打开它 —— 用的是 Windows
自带的 .NET 编译器，不用装任何东西；编不出来(极精简系统)才退回 PowerShell 面板，两边看到的结论一模一样。
想手动预编：`pwsh -File tools\panel-cs\build.ps1`。C# 面板同样只转达 `tools\panel.ps1` 的结论
（通过 `panel.ps1 -DumpSummary` 的机器协议），自己不判定任何东西。
面板上那张「实验功能」卡目前只有方舟一路有内容(AI 辅助 / 视觉兜底)，这一仓里它会写明"没有可配置的实验功能"。


命令行常用姿势：

```powershell
.\daily-clear.ps1 -DryRun              # 预览：今天判定成什么方案、将要用哪个实例
.\daily-clear.ps1                      # 真跑一轮
.\daily-clear.ps1 -Ack                 # 「新版本的活动我手动打过了」→ 之后每天自动跑活动方案
.\daily-clear.ps1 -ShopDone            # 「活动商店我已换空」→ 之后回落到长草方案
.\daily-clear.ps1 -ShopReset           # 误标了 → 撤销，恢复跑活动
.\daily-clear.ps1 -Plan rerun          # 临时强制某个方案（idle / event / rerun）
.\daily-clear.ps1 -SkipCleanup         # 跑完不关模拟器（自己接着玩）
```

这一路**不需要管理员权限**（不像端游那一路要提权）。退出码：`0` 完成 / `1` 部分完成 / `2` 失败。
运行日志在 `logs\`，历史汇总在 `state\history.csv`。

## 2. 每天到底刷什么（自动判定 + M9A 命令行入口）

**方案判定**：读 M9A 自带的活动日历 `data\activity\cn.json`，按当前版本窗口判断，然后自动选 M9A 实例：

| 判定 | 用的 M9A 实例 |
| --- | --- |
| 长草 | `日常-长草` |
| 当期活动 | `日常-活动` |
| 复刻活动 | `日常-复刻` |

实例名 → id 的映射在 `config\settings.json` 的 `re1999.planToInstance`，你在 M9A 里建的实例名不一样就改这里。
**每个实例里勾哪些任务由你在 M9A 里配**，脚本不碰任务列表 —— 它只负责选对实例、把队列跑完。

**启动方式**用的是 M9A 自己的脚本化入口：

```
m9a.exe --instance <实例id> --autostart --quit-after-run --force-start
```

帮助原文（从 `libs\MFAAvalonia.Core.dll` 里挖出来的）：`--instance` 指定启动的实例；`--autostart` 自动开始任务；
`--force-start` 配合 `--instance`，停掉现有任务、按命令行重新连接并开始，不读上次任务状态；
`--quit-after-run` 配合 `--instance`，任务结束后退出程序 —— 注释里写着「用于脚本化调度」。
M9A 已经在跑时这条命令会被**转发**给正在跑的实例，所以冷启动和在跑两种情况都能用。

> **坑（务必用长参数）**：短参数 `-a` 会被 M9A 原样记成 key `a`（日志：`启动参数：数量=4，内容=[[instance, …], [a, ], …]`），
> 它的自动开始判定匹配不上，队列根本不提交 —— 表现就是「连上模拟器 + 起完 Agent，10 秒后报『任务已全部完成』、一个任务都没跑」；
> 换成 `--autostart` 才打出 `用户操作：启动任务` → `准备执行任务队列：任务数量=N`。

相关开关在 `config\settings.json` 的 `runtime` 里：`m9aCliArgs`（关掉就回到裸启动）、`m9aInstanceArg`（`id` 或 `name`）、
`m9aForceStart`（带不带 `--force-start`；它会接管当时正在跑的任务，手动时段可自行关掉）。

**判定完成用的是 `>>> 任务 启动任务完成（异步）。`**，不是界面上那句「任务已全部完成！」——
`--quit-after-run` 下 M9A 打完 `启动任务完成` + `停止前状态：SUCCEEDED` 就「应用已关闭」（实测相隔 0.3 秒），
根本来不及写「任务已全部完成」。而 `启动任务开始/完成` 这一对是**括住整条链路**的（手工日志里相隔 23 分钟），
所以只有「一条『开始任务：X』都没见过」时它才表示只连上了模拟器。

## 3. 两道手动门：活动确认 + 商店领空

**门 1：活动确认** —— 版本更新出了新活动、你还没手动通一遍时，1999 回落到长草方案，不进未确认的新主线/活动。

```powershell
.\daily-clear.ps1 -Ack -DryRun    # 只登记确认，不跑
.\daily-clear.ps1 -Ack            # 登记确认 + 顺手跑一轮
```

确认状态存在 `state\r1999-event-ack.json`（按活动 id 记）。不想用这道门：`re1999.mainStoryRequiresAck` 改 `false`。

**门 2：商店领空** —— 活动还在开、但代币商店已经被你搬空时，再刷就是纯浪费体力。

```powershell
.\daily-clear.ps1 -ShopDone       # 标记当前活动商店已领空
.\daily-clear.ps1 -ShopReset      # 恢复
```

之后每一轮改用 `日常-长草` 实例；标记按活动 id / 版本号记（`state\r1999-shop-done.json`），
新版本或新活动自动不适用，**不用记得回来取消**。不想用这道门：`re1999.shopDoneStopsEvent` 改 `false`。

这道门必须手动：M9A 的活动日历只说活动在不在开，看不到你的兑换进度，猜出来的只会误判。

## 4. 首次使用（新机器上一次性）

0. **生成配置**：仓库里只有 `config\settings.example.json`（`settings.json` 被 `.gitignore` 排除，见第 8 节）。

   ```powershell
   pwsh -File setup.ps1 -DryRun     # 只看探测到什么，不写文件
   pwsh -File setup.ps1             # 探测 + 写入 config\settings.json
   pwsh -File setup.ps1 -M9aDir "D:\Tools\M9A" -MuMuDir "D:\Programs\MuMuPlayer-12.0" -VmIndex 0
   ```

   探测优先级：**命令行指定 > 现有 `settings.json` 里仍有效的路径 > 正在运行的进程 > 卸载表 > 固定磁盘浅层扫描**。
   `MuMu` 的虚拟机索引：只有一台虚拟机时自动填；**有多台时不替你猜**，必须用 `-VmIndex` 指定装了 1999 的那台。
   `Downloads\`、`xx (2)\` 这类副本目录会被排到最后并标注「像下载/备份副本」。

1. **M9A 里先把三个实例建好**（长草 / 当期活动 / 复刻），每个实例里勾好任务；脚本每轮会把 `NoAutoStart` 压成 `False`，
   并在启动时把「当前实例」切成判定出来的那个（切前在 `backups\` 留副本）。
2. **MuMu 里保持登录**、1999 能在模拟器内自动进游戏。卡在登录页/更新页的话这一轮会 `not-started` 或 `stalled`。
   首次跑请盯一眼：`runtime.r1999StartTimeoutMinutes`（默认 10 分钟）内没跑任何任务就返回 `not-started`。
3. **不要锁屏**：M9A 靠截图识别，锁屏 = 桌面不可用 = 失败。机器保持登录、允许熄屏即可
   （运行期间脚本临时顶住睡眠和熄屏，跑完恢复；`runtime.keepScreenOn`）。
4. 偶发的 MuMu 开屏广告会挡住自动化流程 —— 目前脚本**不处理**它，遇到就这一轮失败重试。

## 5. 计划任务

```powershell
pwsh -File install-task.ps1 -DryRun            # 先看要建什么
pwsh -File install-task.ps1                    # 注册两个每日任务 + 一个按需任务
pwsh -File install-task.ps1 -Times 07:30,19:30
pwsh -File install-task.ps1 -Uninstall
```

任务跑在**当前用户会话**里（M9A 要连本机 GUI 和模拟器，所以「只在登录时运行」）。
错过时间点会尽快补跑（`StartWhenAvailable`），单次最长 3 小时被强杀。

## 6. 出问题怎么查

- `pwsh -File status.ps1` —— 环境、进程、今天判定、M9A 实例、MuMu 状态、确认状态、最近记录，一次看完。
- `pwsh -File tools\test-r1999.ps1` —— 纯逻辑自检（只读：不改配置、不启动任何程序），87 条断言，`powershell.exe -File` 同样能跑。
  改过 settings 或判定代码后跑一遍最放心。另有 `-History` 一组（7 条）会回放**作者本机**某天的真实 M9A 日志，
  断言的是那一次真跑的内容，别人的机器上内容不同，所以默认不跑。
- `pwsh -File tools\test-modules.ps1` —— 框架层自检（这一路的游戏模块清单齐不齐、入口开关名对不对、判定/预览钩子调不调得动），只读，`powershell.exe -File` 同样能跑。这份仓库里只有 1999 一路，所以是 79 条；它和上面那 87 条是两套东西，缺哪个都不算过。
- `pwsh -File tools\probe-m9a-history.ps1` —— 统计 M9A 历史日志里每个实例真正开始过几个任务（卡住时先看这个，实例 id 变了也在这里露出来）。
- `pwsh -File tools\replay-m9a-log.ps1` —— 把某一轮的真实 M9A 日志按时间窗口回放给判定逻辑（默认整份「今天的日志」全量回放；`-Log` 换历史日志，
  `-From`/`-To` 只写时刻就按那份日志当天算。**实例 id 不用你知道**：日志每行带 `[inst=实例名/id]`，脚本取窗口里出现最多的那个并在开头打印，
  认错了再用 `-InstanceId` 指名）。
  **改了判定之后不用重跑十几分钟游戏**，用已经发生过的那一轮的字节流验证结论。
- M9A 自己的日志：`<m9aDir>\logs\log-YYYYMMDD.log`（目录见 `config\settings.json` 的 `paths`）。
- 被脚本改过的配置都在 `backups\` 留了最近 20 份，出问题直接覆盖回去即可。

判定规则小结：

| | 成功 | 卡住 |
| --- | --- | --- |
| M9A | 当天日志（按实例 id 过滤）里 `准备执行任务队列：任务数量=N` → 若干 `开始任务：X` → `>>> 任务 启动任务完成（异步）。`；跑满 N 个后 M9A 因 `--quit-after-run` 自己退出也算成功 | 只有「启动任务完成」却一条任务没有（`not-started`／`empty-queue`，秒判不硬等）；进程中途退出且任务没跑满（`error`）；22 分钟无新日志（`stalled`）；超时（`timeout`）；**一条认识的标志都没有 → `unknown-format`** |

`unknown-format` 是唯一「结果不可信」的状态：它不猜原因（到底是 M9A 改了文案、实例 id 变了，还是压根没写日志），
只告诉你别按这一轮判定体力是否清完，并给出下一步命令。

失败会自动重试一次（`runtime.retryOnce`），但 `not-started` / `empty-queue` / `unknown-format` 不重试（重试也是白烧 10～20 分钟）；
再失败就收尾 + 弹通知，不会整晚挂着。

## 7. 文件结构

```
  setup.ps1            第一次用：探测 M9A / MuMu 装在哪、列虚拟机索引 → 生成 config\settings.json
  daily-clear.ps1      编排：判定 → 切实例 → 起模拟器 + M9A → 重试 → 收尾 → 通知
  status.ps1           只读体检
  install-task.ps1     计划任务注册/卸载
  run-daily.cmd        双击手动跑
  panel.cmd            双击打开图形状态面板（转调上面那些脚本，见第 1.1 节）
  config\settings.example.json  配置模板（入仓）
  config\settings.json  唯一需要配的文件（不入仓）
  lib\common.ps1       日志/JSON/备份/防睡眠/进程/通知
  lib\detect.ps1       安装位置探测：进程 → 卸载表 → 磁盘浅层扫描
  lib\decisions.ps1    今天用哪个方案：版本窗口、确认门、实例选择
  lib\m9a.ps1          M9A 实例切换 + 命令行启动 + logs\log-*.log 判定
  lib\mumu.ps1         MuMuManager 启停、adb 探活
  tools\panel.ps1      面板本体（-SelfTest 不显示窗口，给自动化验证用）
  tools\               自检 / 探测脚本（含 replay-m9a-log）
  logs\ state\ backups\  运行时生成（不入仓）
```

依赖：MuMu 模拟器 12（自带 `MuMuManager.exe` 和 `adb.exe`，不需要你另装 adb）。
PowerShell 用 7（`pwsh`）最省事，但**没装也行**——本仓库的脚本 Windows PowerShell 5.1（系统自带）和 7 都能跑，
`run-daily.cmd` 会自己挑一个能用的引擎；下文示例里的 `pwsh` 换成 `powershell` 是同一条命令。
不放心可以跑 `tools\check-ps51.ps1` 自检（在 5.1 里跑出来的结论才算数）。

## 8. 许可、免责与隐私

**许可**：本仓库脚本按 MIT 发布（见 `LICENSE`），只含原创 PowerShell 代码。

**不含第三方程序**：M9A（AGPL-3.0）、MuMu 模拟器（网易商业软件）、重返未来：1999 客户端及素材/名称
都**不**在本仓库内，也不由本仓库分发，请自行从官方渠道安装。M9A 的许可条款以它自己的仓库为准，
本仓库的 MIT 不改变也不覆盖它们的要求。

**免责**：软件按「现状」提供，不含任何明示或暗示的保证。游戏用户协议禁止脚本与自动化，
使用本仓库可能带来账号处罚、时间损失或数据损失，风险由使用者自行承担。

**隐私与脱敏**：`.gitignore` 排除了这几类内容，因为里面是你的机器和个人信息 ——
`config\settings.json`（绝对路径、MuMu 虚拟机索引、M9A 实例名、`notify.webhook`）、
`logs\`（每轮时间点/耗时/失败原因，攒起来就是你几点玩游戏的作息表）、
`state\`（历史汇总、你参与过的活动 id 与版本号）、`backups\`（M9A 配置历史副本，可能含账号字段）。
克隆下来只会得到模板，跑 `setup.ps1` 生成自己的配置。**别把这些目录重新加进版本库，也别把日志原样贴进 issue**，
需要贴的先自己打码（M9A 实例名、绝对路径、活动 id）。
