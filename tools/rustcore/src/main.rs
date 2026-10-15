// gdprobe —— 换宿主方案(Rust 核)的探针。契约号见下面的 `CONTRACT`。
//
// 这个文件起初只做两件不需要碰游戏、不碰助手实例的事：
//   1) 从**内容**读版本（DLL 自己报的版本号 + 装包里 interface.json 的资源轴字段），
//   2) 用 LoadLibrary + GetProcAddress 确认「本轮要调的函数在不在导出表里」。
// 连接、追加任务、start/stop 那半**不在这里**：按 docs\rust-core-plan.md §4 第 6 项，
// 那半会动助手侧现场，只在 GUI 确认不在场的窗口手动跑，且要留"改了哪些现场"的说明。
//
// R1 起多一层**事件流归一**（`--events`）：只吃内联夹具、不加载任何 DLL，
// 它给的是"序号对账 + 终态判据 + 出口裁剪"这套**方法**；现场那一半仍等连接段另两档的窗口样本。
// 按 ADR-2 那条硬门：清单里还有 `unobserved` 格子，这一层就**不许**替换任何生产判据。
//
// R2 起再加一层**监督纯判定搬家**（`--sv`）：把 tools\panel-cs\Supervisor.cs 的四段纯规则
// （四件开关取值 / 证据→状态 / 状态→动作 / 详情拼句）逐字复刻到这边，48 档内联夹具一次跑全。
// 它与 C# 那份的等值由 tools\rustcore\probe-sv-rust-golden.ps1 三侧逐字对拍钉死（old=7b07650 旧 PS /
// new=现树 C# 转发壳 / rust=这里）。同样是旁路对拍面：生产入口在 R3 解锁三件到位前**一律不接线**（方案 §8.3）。
//
// R2② 续一层**farm/intent 五段搬家**（`--farm-intent`）：目标清单解析 / 掉落文本 / 补刷判表 /
// 方舟本轮意图 / 1999 本轮意图，逐字复刻 tools\panel-cs\Orchestration.cs 那五段，46 档内联夹具一次跑全。
// 等值由 tools\rustcore\probe-farm-intent-rust-golden.ps1 三侧逐字对拍钉死（old=a2fdd67 旧 PS / new=现树壳 / rust=这里）。
// 同 `--sv`：旁路对拍面，生产不接线；信封**不带 date 格**（这一档不吃时钟，门也不做日期对账）。
//
// 纪律（写在这里给下一个改这个文件的人）：
//   - 零外部 crate，Win32 直接 FFI；
//   - 本机绝对路径一律由调用方(PS 薄壳)从 settings 取好传进来，**这个文件里不许出现任何一条本机路径字面量**；
//   - 打印默认只带目录的**最后一段**当标签，完整路径要 `--show-path` 才印，免得读数表顺手把路径抄进 docs；
//   - 退出码沿用现有 verb 信封那套：0 正常 / 2 前置不满足 / 1 用法或内部错。新核是消费方，不另起一套。

#![allow(non_snake_case)]

use std::env;
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::exit;

// 升号依据（ADR-3：号只在"出口形状变了"的那一批动）：`RustCore-R7.0` → `RustCore-R8.0`，
// 变的是**同一处形状**的两半，所以只升一次（§11.52 ④ 把这批钉成 ②-b3，理由在此）：
// ① 新出口 `--daily-round`（role 取值域多出第三枚 `B3b-round-daily`，ask 的 action 名册是
//    十六格链体之外的**十段轮骨架**，拒跑闸多一道 `--root`）；
// ② 答手形状变（`DriveFx.direct` 从"有/没有闭包"两档变成"闭包可能答 None ⇒ 照旧 ask"三档，
//    回执类型 `Result<Option<DirectOut>, String>`）。
// 两半是同一处对外可见的形状变：没有 ①，② 只是内部重构不该升号；没有 ②，① 那十段一格都答不动。
// ⚠ 改号必须与 `lib\common.ps1` 的 `Get-GdRustContract`、`tools\build-exe.ps1` 的内嵌名册、
// `tools\rustcore\probe-connect.ps1`、`docs\r0-keyfacts.json` 顶层**同批**动，
// 由 `probe-events-golden.ps1` 的 H3 半逐一对齐（壳侧字面量还要求只剩一种拼法）。
const CONTRACT: &str = "RustCore-R8.0";
const ROLE: &str = "R0-readonly";

// ---------------------------------------------------------------- Win32 直调

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(lp_lib_file_name: *const u16, h_file: *mut c_void, dw_flags: u32) -> *mut c_void;
    fn GetProcAddress(h_module: *mut c_void, lp_proc_name: *const i8) -> *mut c_void;
    fn FreeLibrary(h_module: *mut c_void) -> i32;
    fn GetLastError() -> u32;
    fn GetCurrentThreadId() -> u32;
    fn GetSystemTimePreciseAsFileTime(lp_system_time_as_file_time: *mut u64);
    fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut c_void;
    fn Process32FirstW(h_snapshot: *mut c_void, lp_entry: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(h_snapshot: *mut c_void, lp_entry: *mut ProcessEntry32W) -> i32;
    fn CloseHandle(h_object: *mut c_void) -> i32;
    fn GetLocalTime(lp_system_time: *mut SystemTime);
    // B-3②-b3：睡眠屏蔽。这三枚标志的生效范围是**调用它的那根线程/进程**（句柄语义，见
    // tools\gdinterop\GdPower.cs 的注释与 lib\common.ps1 的 Enter-GdNoSleep / Exit-GdNoSleep），
    // 所以它天生不是 verb —— 外发给壳答，等于让另一个进程去设一个对本进程无效的开关。
    // 这是 B-3 里唯一一处「该在核内新立 Win32 面」的例外，依据写在 docs\rust-core-plan.md §11.50 ①-8。
    // ⚠ 这里只做 ES_CONTINUOUS / ES_SYSTEM_REQUIRED / ES_DISPLAY_REQUIRED 三枚**设置**与一枚**查询**；
    //   屏保那一格（user32 的 SystemParametersInfo）仍留壳侧 —— panel-cs 的 resolve-screensaver-verdict
    //   与 tools\gdinterop\GdPower.cs 已经管着它，核不越界去抢（同 §11.52 ④「既有 verb 形状一字不动」）。
    fn SetThreadExecutionState(es_flags: u32) -> u32;
}

/// `SetThreadExecutionState` 的三枚设置标志 + 一枚查询标志，值取自 winbase.h，
/// 与 tools\gdinterop\GdPower.cs:19-21 那三枚**逐字同值**（两处同源，改一处必改两处）。
/// 查询形态是 `ES_CONTINUOUS | ES_QUERY_ONLY`：只回读当前状态、不改，返回值即当前位型。
/// ⚠ ES_QUERY_ONLY 的数值**在本仓任何一份文档里都没有钉过**（§11.50 ①-8 只说"用 ES_QUERY 复验"），
///   所以这里按头文件写、并在自证那一发当场测；测不到就照口径记 unobserved。
///   自证只读 system/display 两位（`round_answer` 的 nosleep 那格），`ES_USER_PRESENT` 不参与判据，
///   留在名册里只为"读到的位型能对回头文件"这一件事。
const ES_CONTINUOUS: u32 = 0x8000_0000;
const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;
#[allow(dead_code)]
const ES_USER_PRESENT: u32 = 0x0000_0004;
const ES_QUERY_ONLY: u32 = 0x0000_0010;

/// 本地时间戳（B-1a 已搬动作 mark/obsrec 用）——GetLocalTime 的 SYSTEMTIME 布局。
#[repr(C)]
struct SystemTime {
    w_year: u16,
    w_month: u16,
    w_day_of_week: u16,
    w_day: u16,
    w_hour: u16,
    w_minute: u16,
    w_second: u16,
    w_milliseconds: u16,
}

// 管理员判定（B-1a 已搬动作 admin 用）——advapi32 的 SID/令牌三件套。
// 照 PS 真身（Test-GdAdmin）语义：任何一步失败 → false，绝不抛。
#[link(name = "advapi32")]
extern "system" {
    fn AllocateAndInitializeSid(
        p_identifier_authority: *const SidIdentifierAuthority,
        n_sub_authority_count: u8,
        dw_sub_authority0: u32,
        dw_sub_authority1: u32,
        dw_sub_authority2: u32,
        dw_sub_authority3: u32,
        dw_sub_authority4: u32,
        dw_sub_authority5: u32,
        dw_sub_authority6: u32,
        dw_sub_authority7: u32,
        p_sid: *mut *mut c_void,
    ) -> i32;
    fn FreeSid(p_sid: *mut c_void) -> *mut c_void;
    fn CheckTokenMembership(token_handle: *mut c_void, sid_to_check: *mut c_void, is_member: *mut i32) -> i32;
}

#[repr(C)]
struct SidIdentifierAuthority {
    value: [u8; 6],
}

/// 进程快照用（连接段的环境闸）。INVALID_HANDLE 用 -1 的位型判。
const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
const INVALID_HANDLE_ISIZE: isize = -1;

/// `PROCESSENTRY32W` 的**现代布局**（含末尾字段）。
/// 这里只有**字段偏移**是按头文件写的；喂给内核的 `dwSize` 不是 `size_of` 而是下面那枚常量，
/// 因为 `repr(C)` 会把整个结构体按 8 对齐补齐到 576，而内核要的是 568 —— 偏一字节都不给读。
/// 10-12 实测分布（同一枚快照句柄上逐个试出来的，不是推算的）：
///   `dwSize` = 560 → `Process32FirstW` 回 0 / err 24；**568 → 回 1、首条名字读得到**；
///   572 → 0/24；576（＝本结构的 `size_of`）→ 0/24；584 → 0/24。
/// err 24 = `ERROR_BAD_LENGTH`。两条教训记在这里：
///   1) 早先"少写尾字段会因 dwSize 对不上失败"是真的，但补上尾字段**并不等于对上了**；
///   2) 失败是**静默**的 —— 快照句柄照样拿到、`scan_forbidden_processes` 照样回 `Ok(空)`、
///      环境闸照样报 `proc_read_ok:true`，而 `real_proc_running` 对任何名字恒 false。
///      现场表现＝ 10-11 run 进核之后连报「MAA 未能启动(进程未出现)」，而 MAA 自己的日志
///      显示它起来了并把整条队列跑完。自检里那枚"自身进程必然在场"的正例就是钉这一格的。
#[repr(C)]
struct ProcessEntry32W {
    dw_size: u32,
    cnt_usage: u32,
    th32_process_id: u32,
    th32_default_heap_id: usize,
    th32_module_id: u32,
    cnt_threads: u32,
    th32_parent_process_id: u32,
    pc_pri_class_base: i32,
    dw_flags: u32,
    sz_exe_file: [u16; 260],
    th32_memory_base: u32,
    th32_check_sum: u32,
}

/// 快照条目要递给内核的 `dwSize`：实测唯一被接受的那个数（见上面那串分布）。
/// 用常量而不是 `size_of::<ProcessEntry32W>()`，后者是 576、内核不认。
const PROCESSENTRY32W_DWSIZE: u32 = 568;

/// 让 DLL 的依赖从**它自己所在目录**解析。不传这个旗标的话，MaaCore.dll 依赖的
/// DirectML.dll 之类会先去找我们的 exe 目录，那是一定找不到的。
const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x0000_0008;

/// 两侧版本 getter 的形状一致：返回 UTF-8 的 const char*，指向 DLL 自己的静态存储。
type VersionFn = unsafe extern "C" fn() -> *const i8;

fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

// ---------------------------------------------------------------- 两侧装包的形状

struct Kind {
    name: &'static str,
    /// 相对装包根目录的候选路径，按序试，第一个能加载的就是它。
    dll_candidates: &'static [&'static str],
    /// 本轮判定为"必须有"的导出符号。缺一个就是版本不匹配，不是"少个可选函数"。
    expect: &'static [&'static str],
    /// 判定为"必须没有"的导出符号 —— 这两组 forbid 就是 §2 那条"两侧 ABI 不对称"的可复跑证据。
    forbid: &'static [&'static str],
    /// 框架/程序自报版本用的 getter 名（一定在 expect 里）。
    version_fn: &'static str,
    /// 资源轴要不要顺带读同目录的 interface.json。
    reads_interface_json: bool,
    note: &'static str,
    /// **连接段要用的符号清单**：只查存在性（GetProcAddress），不取指针、不调用。
    /// 单独列一份而不是塞进 expect，是因为"这枚导出在"和"本轮判版本必须靠它"是两件事。
    connect_symbols: &'static [&'static str],
    /// 本地**签名源**文件名（相对装包，只写文件名不写盘符）。没有签名源的函数一律不许调。
    signature_source: &'static str,
    /// 有签名源、连接段真的允许调用的子集。`connect_symbols` 里不在这份的另一半 = 只查不调。
    callable: &'static [&'static str],
}

// ark 侧签名源 = 装包自带的 Python\asst\asst.py（免上游 header）。顺序照那份文件里出现的次序。
const ARK_EXPECT: &[&str] = &[
    "AsstGetVersion",
    "AsstSetUserDir",
    "AsstLoadResource",
    "AsstSetStaticOption",
    "AsstSetConnectionExtras",
    "AsstGetImage",
    "AsstCreate",
    "AsstCreateEx",
    "AsstDestroy",
    "AsstSetInstanceOption",
    "AsstConnect",
    "AsstAsyncConnect",
    "AsstAppendTask",
    "AsstSetTaskParams",
    "AsstStart",
    "AsstStop",
    "AsstRunning",
    "AsstLog",
];

// M9A 侧核心 DLL 的必需符号。**这一份是 09-30 实测出来的，不是猜的**：
// 第一版按"应该是这些名字"写了 13 条，跑本机真 DLL 直接缺 8 条 —— 差的不是版本对不上，
// 是名字根本不存在（MaaStartup / MaaShutdown / MaaGetErrorString / MaaBuffer* / MaaPostTask / MaaStatus
// 在 MaaFramework v5.14 的导出表里一个都没有；真名是 MaaTaskerPostTask / MaaTaskerStatus / MaaStringBuffer*）。
// 权威来源 = dumpbin //exports 的 173 条导出，那份清单落在 docs 的 R0 读数表里。
const MF_EXPECT: &[&str] = &[
    "MaaVersion",
    "MaaSetGlobalOption",
    "MaaGlobalSetOption",
    "MaaResourceCreate",
    "MaaResourceDestroy",
    "MaaResourceLoaded",
    "MaaResourceGetHash",
    "MaaResourcePostPipeline",
    "MaaResourceAddSink",
    "MaaTaskerCreate",
    "MaaTaskerDestroy",
    "MaaTaskerInited",
    "MaaTaskerBindResource",
    "MaaTaskerBindController",
    "MaaTaskerAddSink",
    "MaaTaskerPostTask",
    "MaaTaskerGetTaskDetail",
    "MaaTaskerStatus",
    "MaaTaskerWait",
    "MaaControllerDestroy",
    "MaaControllerConnected",
    "MaaControllerAddSink",
    "MaaStringBufferCreate",
    "MaaStringBufferGet",
    "MaaStringBufferDestroy",
    "MaaImageBufferCreate",
    "MaaImageBufferGetEncoded",
    "MaaImageBufferDestroy",
];

/// Win32 桌面窗口控制器**不在核心 DLL 里**：实测它在同目录那枚 `MaaWin32ControlUnit.dll`，
/// 而那枚 DLL 只导出三条（Create / Destroy / GetVersion）。
/// 这条读数是 R0 最实在的一条：直连要驱动桌面窗口，得先吃下"另加载一枚控制单元"这件事。
const W32CU_EXPECT: &[&str] = &[
    "MaaWin32ControlUnitCreate",
    "MaaWin32ControlUnitDestroy",
    "MaaWin32ControlUnitGetVersion",
];

// ---------------------------------------------------------------- 连接段清单（09-30 实测得来，不是设想）
//
// ark 侧的连接入口**不是** AsstConnect：当日 asst.log 里那条链是
//   asst::Assistant::attach_window → ctrl_attach_window → Controller::attach_window
//   → Win32Controller::attach → Win32ControlUnitLoader::load/create
// 也就是端游走**桌面窗口挂载**。挂载那两枚（AsstAttachWindow / AsstAsyncAttachWindow）
// 确实在 MaaCore.dll 的导出表里，但装包里的 asst.py **没声明它们** —— 按 §2 那条纪律
// 「没有本地签名源 = 不许调」，连接段对 ark 侧**只走 adb 形状**（asst.py 有 AsstConnect），
// 桌面窗口挂载这一半必须等上游 header。这条限制要能在读数里自己说话，所以写成代码判据。
const ARK_CONNECT: &[&str] = &[
    "AsstSetUserDir",
    "AsstLoadResource",
    "AsstCreateEx",
    "AsstDestroy",
    "AsstConnect",
    "AsstAsyncConnect",
    "AsstAppendTask",
    "AsstSetTaskParams",
    "AsstStart",
    "AsstStop",
    "AsstRunning",
    "AsstConnected",
    "AsstAttachWindow",
    "AsstAsyncAttachWindow",
];

// asst.py 里真声明了 argtypes/restype 的那批 —— 只有这份能调。
// AsstConnected / AsstAttachWindow / AsstAsyncAttachWindow 都在导出表里但**不在签名源里**，
// 所以它们进 connect 清单（查存在性）、不进 callable（不调）。
const ARK_CALLABLE: &[&str] = &[
    "AsstSetUserDir",
    "AsstLoadResource",
    "AsstCreateEx",
    "AsstDestroy",
    "AsstConnect",
    "AsstAsyncConnect",
    "AsstAppendTask",
    "AsstSetTaskParams",
    "AsstStart",
    "AsstStop",
    "AsstRunning",
];

// mf 侧：连接/事件面的签名源是装包自带的 python 绑定（只读参考，**一个字节都不拷进仓**）。
// 实测到的关键形状有两条：
//   1) 事件回调是 `MaaEventCallback = WINFUNCTYPE(None, void* handle, char* message, char* details_json, void* arg)`
//      —— **四参、message 是字符串**；和 ark 侧的 `(int message, char* details_json, void* arg)` 三参**并不同构**。
//      方案 §4 第 4 项原本写的"同构"是设想，这里是实测，第五条自我纠正就是它。
//   2) 控制器创建按类型分枚（MaaAdbControllerCreate / …Win32… 在不在导出表里由本轮实测给结论）。
const MF_CONNECT: &[&str] = &[
    "MaaResourceCreate",
    "MaaResourceDestroy",
    "MaaResourceAddSink",
    "MaaResourceRemoveSink",
    "MaaResourcePostPipeline",
    "MaaResourceLoaded",
    "MaaTaskerCreate",
    "MaaTaskerDestroy",
    "MaaTaskerAddSink",
    "MaaAdbControllerCreate",
    "MaaWin32ControllerCreate",
    "MaaControllerDestroy",
    "MaaControllerAddSink",
    "MaaControllerPostConnection",
    "MaaControllerConnected",
];

const MF_CALLABLE: &[&str] = &[
    "MaaResourceCreate",
    "MaaResourceDestroy",
    "MaaResourceAddSink",
    "MaaResourceRemoveSink",
    "MaaResourcePostPipeline",
    "MaaResourceLoaded",
    "MaaTaskerCreate",
    "MaaTaskerDestroy",
    "MaaTaskerAddSink",
    "MaaAdbControllerCreate",
    "MaaControllerDestroy",
    "MaaControllerAddSink",
    "MaaControllerPostConnection",
    "MaaControllerConnected",
];

/// 连接段拒跑的进程名（大小写不敏感）。**判据是"在就不抢方向盘"**：
/// 命中一个就不加载、不连接、直接退 2，且**绝不**替人关进程（§8(a) 的硬口径）。
const FORBIDDEN_PROCESSES: &[&str] = &[
    "MAA.exe",
    "MAA.Updater.exe",
    "M9A.exe",
    "MFAAvalonia.exe",
];

const KINDS: &[Kind] = &[
    Kind {
        name: "ark",
        dll_candidates: &["MaaCore.dll"],
        expect: ARK_EXPECT,
        // 判据的核心：这枚 MaaCore.dll 对外仍是遗留 Asst* 面，MaaFramework 那套入口**不该在**。
        forbid: &["MaaStartup", "MaaCreate", "MaaTaskerCreate", "MaaControllerCreate"],
        version_fn: "AsstGetVersion",
        reads_interface_json: false,
        note: "MAA：内容自报版本，装包目录名不作数",
        connect_symbols: ARK_CONNECT,
        signature_source: "Python/asst/asst.py",
        callable: ARK_CALLABLE,
    },
    Kind {
        name: "mf",
        dll_candidates: &[
            "runtimes\\win-x64\\native\\MaaFramework.dll",
            "bin\\MaaFramework.dll",
            "MaaFramework.dll",
        ],
        expect: MF_EXPECT,
        // 实测这三条在 v5.14 里**不存在**；写成 forbid 不是笔误，是要让它变成一条会自己报警的判据：
        // 哪天对方把它们加回来了（版本形状变了），探针就该停下来重判，而不是继续按老形状装配。
        forbid: &["AsstGetVersion", "AsstCreate", "AsstAppendTask", "MaaStartup", "MaaShutdown"],
        version_fn: "MaaVersion",
        reads_interface_json: true,
        note: "M9A：真 MaaFramework；程序轴与资源轴都不在这枚 DLL 上，要另读",
        connect_symbols: MF_CONNECT,
        signature_source: "python/Lib/site-packages/maa/{define,resource,tasker,controller}.py",
        callable: MF_CALLABLE,
    },
    Kind {
        name: "w32cu",
        dll_candidates: &[
            "runtimes\\win-x64\\native\\MaaWin32ControlUnit.dll",
            "plugins\\win-x64\\MaaWin32ControlUnit.dll",
            "MaaWin32ControlUnit.dll",
        ],
        expect: W32CU_EXPECT,
        forbid: &["AsstAttachWindow", "MaaWin32ControllerCreate"],
        // 空串 = 不调任何函数，只查导出。GetVersion 那枚的签名我没核过，
        // 拿不准确的签名去调对方导出函数是 UB，宁可不读这个版本号。
        version_fn: "",
        reads_interface_json: false,
        note: "Win32 控制器单元：核心 DLL 不导出它，直连驱动桌面窗口要另加载这枚",
        connect_symbols: W32CU_EXPECT,
        signature_source: "",
        // 空 = 一枚都不许调。这枚的签名两侧都没有本地来源，连接段对它只查导出。
        callable: &[],
    },
];

fn find_kind(name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.name == name)
}

// ---------------------------------------------------------------- JSON 输出

/// 手写转义就够了，不想为这个再引一个 serde。控制字符一律 \u 形式，
/// 免得上面有非法字节时把整行读数打成不可解析的东西。
fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn jopt_str(o: &Option<String>) -> String {
    match o {
        Some(v) => jstr(v),
        None => "null".to_string(),
    }
}

fn jarr(items: &[String]) -> String {
    let v: Vec<String> = items.iter().map(|s| jstr(s)).collect();
    format!("[{}]", v.join(","))
}

// ---------------------------------------------------------------- 资源轴：从 interface.json 里抠 "version"

/// 只做**定向**提取：顶层扫 `"version"` 这个键，冒号后跟一个字符串就收下。
/// 一个都不收（只收第一个）是不行的 —— 装包里可能有别的 version，
/// 收到几条就报几条，多于一条时把歧义如实写进读数，**不替人挑一个**。
fn extract_interface_versions(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let key = b"\"version\"";
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + key.len() <= b.len() {
        if &b[i..i + key.len()] == key {
            // 键左边必须是 JSON 分隔符，否则是 "interface_version" 这类同后缀键混进来。
            let left_ok = i == 0 || matches!(b[i - 1], b'{' | b',' | b' ' | b'\n' | b'\r' | b'\t');
            if left_ok {
                let mut j = i + key.len();
                while j < b.len() && matches!(b[j], b' ' | b'\n' | b'\r' | b'\t') {
                    j += 1;
                }
                if j < b.len() && b[j] == b':' {
                    j += 1;
                    while j < b.len() && matches!(b[j], b' ' | b'\n' | b'\r' | b'\t') {
                        j += 1;
                    }
                    if j < b.len() && b[j] == b'"' {
                        j += 1;
                        let start = j;
                        while j < b.len() && b[j] != b'"' {
                            if b[j] == b'\\' && j + 1 < b.len() {
                                j += 1;
                            }
                            j += 1;
                        }
                        if j < b.len() {
                            out.push(String::from_utf8_lossy(&b[start..j]).into_owned());
                        }
                    }
                }
                i += key.len();
                continue;
            }
        }
        i += 1;
    }
    out
}

/// 顺带把 interface_version 那个整数键读出来（它和资源轴不是一回事，分开报）。
fn extract_interface_schema_version(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let key = b"\"interface_version\"";
    let mut i = 0usize;
    while i + key.len() <= b.len() {
        if &b[i..i + key.len()] == key {
            let mut j = i + key.len();
            while j < b.len() && matches!(b[j], b' ' | b'\n' | b'\r' | b'\t') {
                j += 1;
            }
            if j < b.len() && b[j] == b':' {
                j += 1;
                // 冒号后面可以有空格/换行，不跳的话 `{"interface_version": 2}` 这种就永远读不出来
                // （第一版就栽在这，被 --selftest 的夹具抓到）。
                while j < b.len() && matches!(b[j], b' ' | b'\n' | b'\r' | b'\t') {
                    j += 1;
                }
                let start = j;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                if j > start {
                    return Some(String::from_utf8_lossy(&b[start..j]).into_owned());
                }
            }
            return None;
        }
        i += 1;
    }
    None
}

fn read_text_lossy(path: &str) -> Option<String> {
    let raw = std::fs::read(path).ok()?;
    let body = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &raw[3..]
    } else {
        &raw[..]
    };
    Some(String::from_utf8_lossy(body).into_owned())
}

// ---------------------------------------------------------------- 探针主体

struct Report {
    ok: bool,
    why: String,
    kind: String,
    dir: String,
    dll: Option<String>,
    loaded: bool,
    load_err: u32,
    self_reported_version: Option<String>,
    interface_versions: Vec<String>,
    interface_schema_version: Option<String>,
    present: Vec<String>,
    missing: Vec<String>,
    forbidden_found: Vec<String>,
    connect_present: Vec<String>,
    connect_missing: Vec<String>,
    /// 在导出表里、但本地没有签名源的那半 —— 查得到、**不许调**，读数里必须点名。
    refused: Vec<String>,
    sig_source: String,
    freed: bool,
}

impl Report {
    fn to_json(&self) -> String {
        let mut p: Vec<String> = Vec::new();
        p.push(format!("\"ok\":{}", if self.ok { "true" } else { "false" }));
        p.push(format!("\"why\":{}", jstr(&self.why)));
        p.push(format!("\"contract\":{}", jstr(CONTRACT)));
        p.push(format!("\"role\":{}", jstr(ROLE)));
        p.push(format!("\"kind\":{}", jstr(&self.kind)));
        p.push(format!("\"dir\":{}", jstr(&self.dir)));
        p.push(format!("\"dll\":{}", jopt_str(&self.dll)));
        p.push(format!("\"loaded\":{}", if self.loaded { "true" } else { "false" }));
        p.push(format!("\"load_err\":{}", self.load_err));
        p.push(format!("\"self_reported_version\":{}", jopt_str(&self.self_reported_version)));
        p.push(format!("\"interface_json_versions\":{}", jarr(&self.interface_versions)));
        p.push(format!("\"interface_schema_version\":{}", jopt_str(&self.interface_schema_version)));
        p.push(format!("\"symbols_present\":{}", jarr(&self.present)));
        p.push(format!("\"symbols_missing\":{}", jarr(&self.missing)));
        p.push(format!("\"forbidden_found\":{}", jarr(&self.forbidden_found)));
        p.push(format!("\"connect_present\":{}", jarr(&self.connect_present)));
        p.push(format!("\"connect_missing\":{}", jarr(&self.connect_missing)));
        p.push(format!("\"refused_no_signature_source\":{}", jarr(&self.refused)));
        p.push(format!("\"signature_source\":{}", jstr(self.sig_source.as_str())));
        p.push(format!("\"freed\":{}", if self.freed { "true" } else { "false" }));
        format!("{{{}}}", p.join(","))
    }
}

/// 目录标签：只取最后一段。整条路径默认不出现在读数里。
fn dir_label(dir: &str, show_path: bool) -> String {
    if show_path {
        return dir.to_string();
    }
    let trimmed = dir.trim_end_matches(|c| c == '/' || c == '\\');
    match trimmed.rsplit(|c| c == '/' || c == '\\').next() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => trimmed.to_string(),
    }
}

fn probe(kind: &'static Kind, dir: &str, show_path: bool, do_free: bool) -> Report {
    let mut r = Report {
        ok: false,
        why: String::new(),
        kind: kind.name.to_string(),
        dir: dir_label(dir, show_path),
        dll: None,
        loaded: false,
        load_err: 0,
        self_reported_version: None,
        interface_versions: Vec::new(),
        interface_schema_version: None,
        present: Vec::new(),
        missing: Vec::new(),
        forbidden_found: Vec::new(),
        connect_present: Vec::new(),
        connect_missing: Vec::new(),
        refused: Vec::new(),
        sig_source: kind.signature_source.to_string(),
        freed: false,
    };

    if !std::path::Path::new(dir).is_dir() {
        r.why = "装包目录不在场（前置不满足，不是代码错）".to_string();
        return r;
    }

    // 资源轴：先在磁盘上找，跟 DLL 能不能加载无关，两件事分开报。
    if kind.reads_interface_json {
        let ij = format!("{}\\interface.json", dir.trim_end_matches(|c| c == '/' || c == '\\'));
        match read_text_lossy(&ij) {
            Some(text) => {
                r.interface_versions = extract_interface_versions(&text);
                r.interface_schema_version = extract_interface_schema_version(&text);
            }
            None => r.interface_schema_version = None,
        }
    }

    let mut chosen: Option<String> = None;
    let mut hmod: *mut c_void = std::ptr::null_mut();
    let mut last_err = 0u32;
    for cand in kind.dll_candidates {
        let full = format!("{}\\{}", dir.trim_end_matches(|c| c == '/' || c == '\\'), cand);
        if !std::path::Path::new(&full).exists() {
            continue;
        }
        let w = wide(&full);
        let h = unsafe { LoadLibraryExW(w.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
        if h.is_null() {
            last_err = unsafe { GetLastError() };
            continue;
        }
        chosen = Some(cand.to_string());
        hmod = h;
        break;
    }

    let Some(cand) = chosen else {
        r.load_err = last_err;
        r.why = if last_err == 0 {
            "候选路径一个都不存在（这台机器上的装包形状与本文假设不符）".to_string()
        } else {
            format!("文件在但加载失败，GetLastError={}（多半是依赖没跟着解析到）", last_err)
        };
        return r;
    };
    r.dll = Some(cand);
    r.loaded = true;

    // 自报版本：只调 version_fn 这一个函数，别的一律不碰。
    if kind.version_fn.is_empty() {
        // 这枚的形状只够查导出、不够安全调用（签名没核过，乱调是 UB）——那就如实这么报。
        r.ok = true;
        r.why = "签名未核，本轮只查导出、不调任何函数".to_string();
    } else {
        let vk = wide_cstr(kind.version_fn);
        let vp = unsafe { GetProcAddress(hmod, vk.as_ptr()) };
        if vp.is_null() {
            r.missing.push(kind.version_fn.to_string());
            r.why = format!("导出表里没有 {}，版本读不出来", kind.version_fn);
        } else {
            let f: VersionFn = unsafe { std::mem::transmute(vp) };
            let got = unsafe { f() };
            if got.is_null() {
                r.why = format!("{} 返回空指针", kind.version_fn);
            } else {
                r.self_reported_version = Some(unsafe { std::ffi::CStr::from_ptr(got) }
                    .to_string_lossy()
                    .into_owned());
                r.why = "ok".to_string();
                r.ok = true;
            }
        }
    }

    // 符号存在性：只 GetProcAddress，**不取函数指针、不调用**。
    for name in kind.expect {
        if *name == kind.version_fn {
            // 上面已经查过，别重复记
            if !r.missing.iter().any(|m| m == name) && !r.present.iter().any(|p| p == name) {
                r.present.push((*name).to_string());
            }
            continue;
        }
        let w = wide_cstr(name);
        let p = unsafe { GetProcAddress(hmod, w.as_ptr()) };
        if p.is_null() {
            r.missing.push((*name).to_string());
        } else {
            r.present.push((*name).to_string());
        }
    }
    for name in kind.forbid {
        let w = wide_cstr(name);
        let p = unsafe { GetProcAddress(hmod, w.as_ptr()) };
        if !p.is_null() {
            r.forbidden_found.push((*name).to_string());
        }
    }

    // 连接段清单：**仍然只查存在性**（这一档随时能跑，不需要窗口、不动任何现场）。
    // 同时在读数里把"导出在但没有本地签名源"的那半单独点名 —— 那是 §2 那条纪律的可复跑形态，
    // 免得将来有人看到 AsstAttachWindow 在导出表里，就以为可以照着名字猜签名调它。
    for name in kind.connect_symbols {
        let w = wide_cstr(name);
        let p = unsafe { GetProcAddress(hmod, w.as_ptr()) };
        if p.is_null() {
            r.connect_missing.push((*name).to_string());
        } else {
            r.connect_present.push((*name).to_string());
            if !kind.callable.iter().any(|c| c == name) {
                r.refused.push((*name).to_string());
            }
        }
    }

    if !r.missing.is_empty() {
        r.ok = false;
        r.why = format!(
            "缺 {} 个必需导出符号：{}（报版本不匹配并降级回现有链，不是半路崩）",
            r.missing.len(),
            r.missing.join(",")
        );
    }
    if !r.forbidden_found.is_empty() {
        r.ok = false;
        r.why = format!(
            "{}；本侧「不该在」的符号出现了：{}（§2 那条 ABI 不对称的前提在这枚装包上不成立，停下来重判）",
            r.why,
            r.forbidden_found.join(",")
        );
    }

    // 卸载这件事本身就是 R0 的一条读数：默认**不卸**，让进程退出统一回收。
    if do_free {
        let c = unsafe { FreeLibrary(hmod) };
        r.freed = c != 0;
    }
    r
}

/// 符号名是 ASCII，直接堆成以 0 结尾的 i8 数组给 GetProcAddress。
fn wide_cstr(name: &str) -> Vec<i8> {
    name.bytes().map(|b| b as i8).chain(std::iter::once(0i8)).collect()
}

// ---------------------------------------------------------------- 连接段（R0 第五件出口要的那半）
//
// 与只读段的分界线在这里：**加载对方 DLL 里的资源、起句柄、发连接**都算动现场。
// 所以这一档不是"随手能跑"，而是三道闸 + 一份现场说明 + 一份还原清单齐了才跑（§8(a)）。
// 闸本身写成纯逻辑函数，好让 --selftest 在没有装包、没有 GUI 的机器上也能验它。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// 一次连接段里收到的全部事件。对方在自己的线程上回调我们，所以这里必须上锁。
#[derive(Clone)]
struct Event {
    seq: u64,
    abi: &'static str,
    code: String,
    message: String,
    thread: u32,
    rel_ms: i64,
    raw_len: usize,
    raw_hash: u64,
    details: String,
}

static EVENTS: OnceLock<Mutex<Vec<Event>>> = OnceLock::new();
static T0_FILETIME: OnceLock<u64> = OnceLock::new();
static CB_PANICS: AtomicU32 = AtomicU32::new(0);

fn events() -> &'static Mutex<Vec<Event>> {
    EVENTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn filetime_now() -> u64 {
    let mut t: u64 = 0;
    unsafe { GetSystemTimePreciseAsFileTime(&mut t) };
    t
}

/// 相对起点毫秒。时间戳口径这条读数的**唯一来源**就是它 —— 两侧回调都不自带时间戳，
/// 所以事件流必须自己打，且要打标（ADR-2 的 ts_source）。
/// ⚠ 基线是 `OnceLock` **首次调用才落**的，所以调用方必须在"想量从哪儿开始"那一刻先叫它一次；
/// 连接段在 10-02 之前就漏了这一步，于是首枚事件的 `rel_ms` **恒为 0**（那条实跑里唯一的回执是销毁事件，
/// 0 会被读成"零毫秒就到了"，其实它只证"这是第一枚"）。修法在 `connect_run` 入口，判据在那儿钉着。
fn rel_ms() -> i64 {
    let t0 = *T0_FILETIME.get_or_init(filetime_now);
    filetime_now().saturating_sub(t0) as i64 / 10_000
}

/// 挂钟等值的唯一算法（FILETIME 一格 = 100ns ⇒ /10_000 成毫秒；倒退或不齐就饱和成 0，不绕回大数）。
/// 拆成纯函数是为了让自检能拿确定夹具钉它，而不是去掐现场时间。
fn elapsed_ms(entry: u64, now: u64) -> u64 {
    now.saturating_sub(entry) / 10_000
}

/// `waited_ms` 的**唯一 setter**。原先只在执行段末尾赋一次，于是任何提前 return 的档（闸没过、
/// 目录不在场、DLL 加载不了……）这格都是结构体初始值 0 —— 而 0 会被读成"没等"，其实它只证
/// "没走到那行"（10-02 那两遍假绿的 `waited_ms=0` 就是这么来的）。现在每条退出路都过这里，
/// 口径统一成**从进连接段到退出**的挂钟，不再掺"执行段"与"整段"两套含义。
fn stamp(mut r: ConnectReport) -> ConnectReport {
    r.waited_ms = elapsed_ms(r.entry_filetime, filetime_now());
    r
}

/// 稳定哈希（FNV-1a 64）：对拍要比"同一条事件是不是同一条"，但原文可能带本机路径，
/// 所以读数里放**哈希 + 长度**当等值判据，放**脱敏后的截断原文**给人看。
fn fnv1a64(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn cstr_bytes(s: &str) -> Vec<i8> {
    let mut v: Vec<i8> = s.as_bytes().iter().map(|b| *b as i8).collect();
    v.push(0);
    v
}

unsafe fn cstr_to_lossy(p: *const i8) -> String {
    if p.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
}

/// 脱敏：任何"盘符:\" 起头的连续段 + 两个环境变量值，一律换成标签。
/// 对方回调里的 JSON 很可能带完整装包路径，读数是要落 docs / 进 git 的，**默认就当它会漏**。
fn scrub(s: &str) -> String {
    let mut out = s.to_string();
    for (var, label) in [("USERPROFILE", "<用户目录>"), ("TEMP", "<临时目录>"), ("LOCALAPPDATA", "<本地应用数据>")] {
        if let Ok(v) = env::var(var) {
            if v.len() >= 3 {
                out = out.replace(v.as_str(), label);
            }
        }
    }
    let b: Vec<char> = out.chars().collect();
    let mut res = String::with_capacity(out.len());
    let mut i = 0usize;
    while i < b.len() {
        let is_path_start = i + 2 < b.len()
            && b[i].is_ascii_alphabetic()
            && b[i + 1] == ':'
            && (b[i + 2] == '\\' || b[i + 2] == '/');
        if is_path_start {
            let mut j = i + 2;
            while j < b.len() && !matches!(b[j], ' ' | '"' | '\'' | '\t' | '\n' | '\r' | ',' | ';' | ')' | '}' | ']') {
                j += 1;
            }
            res.push_str("<本机路径>");
            i = j;
            continue;
        }
        res.push(b[i]);
        i += 1;
    }
    res
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn push_event(abi: &'static str, code: String, message: String, raw: &str) {
    let hash = fnv1a64(raw.as_bytes());
    let len = raw.chars().count();
    let body = truncate_chars(&scrub(raw), 240);
    let th = unsafe { GetCurrentThreadId() };
    let mut guard = match events().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let seq = guard.len() as u64 + 1;
    guard.push(Event {
        seq,
        abi,
        code,
        message,
        thread: th,
        rel_ms: rel_ms(),
        raw_len: len,
        raw_hash: hash,
        details: body,
    });
}

/// ark 侧回调消息号的具名表（来源：装包 Python/asst/utils.py 的 Message 枚举）。
fn ark_msg_name(m: i32) -> &'static str {
    match m {
        0 => "InternalError",
        1 => "InitFailed",
        2 => "ConnectionInfo",
        3 => "AllTasksCompleted",
        4 => "AsyncCallInfo",
        5 => "Destroyed",
        10000 => "TaskChainError",
        10001 => "TaskChainStart",
        10002 => "TaskChainCompleted",
        10003 => "TaskChainExtraInfo",
        10004 => "TaskChainStopped",
        20000 => "SubTaskError",
        20001 => "SubTaskStart",
        20002 => "SubTaskCompleted",
        20003 => "SubTaskExtraInfo",
        20004 => "SubTaskStopped",
        30000 => "ReportRequest",
        _ => "UnknownMessage",
    }
}

/// 遗留 Asst ABI 的回调：(int message, char* details_json, void* arg)。
unsafe extern "system" fn ark_cb(message: i32, details: *const i8, _arg: *mut c_void) {
    // §11.5：panic 不许越过这条边界。对方在自己的线程上调我们，崩在这里就是把宿主崩了。
    let got = catch_unwind(AssertUnwindSafe(|| {
        let raw = cstr_to_lossy(details);
        push_event("asst", message.to_string(), ark_msg_name(message).to_string(), &raw);
    }));
    if got.is_err() {
        CB_PANICS.fetch_add(1, Ordering::SeqCst);
    }
}

/// framework ABI 的回调：**四参、message 是字符串**（实测自装包的 python 绑定 define.py 那行
/// MaaEventCallback）。与上面那条并不同构 —— 方案 §4 原本写"同构"是设想，这条是纠正。
unsafe extern "system" fn mf_cb(_handle: *mut c_void, message: *const i8, details: *const i8, _arg: *mut c_void) {
    let got = catch_unwind(AssertUnwindSafe(|| {
        let m = cstr_to_lossy(message);
        let raw = cstr_to_lossy(details);
        push_event("framework", String::new(), m, &raw);
    }));
    if got.is_err() {
        CB_PANICS.fetch_add(1, Ordering::SeqCst);
    }
}

// ---- R1：事件流归一层（方案 §4 第 4 项；统一发生在**我方内部表示**，不在 ABI 层）
//
// ADR-2 字段名那一维的口径：**不许自造字段名**。对方给的原名逐字留在 `raw_name`（和 `raw_code`）里，
// `name` 是我方的归一映射、`domain` / `state` 是我方的分档 —— 三份各存各的，映射名单独一档、单独挂号。
// 两份 adapter 就是上面那两个 extern fn（ark_cb 三参整数号 / mf_cb 四参字符串名，**不同构**），
// adapter 之上才是同一条流；不要去求"一个 callback 类型同时满足两侧"的写法，那个类型不存在。

/// 归一后的一条事件。
///
/// **这里没有 `raw_len` / `raw_hash`**：实测 `raw_len` 就是资源落点路径长度的直接读数
/// （失败侧 38 / 成功侧 43，差 5 恰为 `\base`），`raw_hash` 在 `Failed` 与 `Succeeded` 两侧**都给**
/// （它是内容指纹不是标志位）。按 `docs\r0-adrs.md` §6-3 第 ①② 条，对外这份事件流**在出口处裁掉这两枚**；
/// 它们仍留在环内的 `Event` 上，供本机连接段信封（`--connect`）做逐条等值对照。
#[derive(Clone)]
struct NormEvent {
    seq: u64,
    abi: &'static str,
    raw_code: String,
    raw_name: String,
    name: String,
    domain: String,
    state: String,
    ts_source: &'static str,
    on_main_thread: bool,
    rel_ms: i64,
    details: String,
}

fn last_seg(s: &str) -> String {
    match s.rfind('.') {
        Some(i) => s[i + 1..].to_string(),
        None => s.to_string(),
    }
}

fn first_seg(s: &str) -> String {
    match s.find('.') {
        Some(i) => s[..i].to_string(),
        None => String::new(),
    }
}

/// 遗留侧的号段分档（**我方分档，不是对方的字段**）：0..5 归 `core`、1xxxx `task_chain`、
/// 2xxxx `sub_task`、3xxxx `report`，其余 `unknown`。
fn asst_domain(code: &str) -> &'static str {
    let n: i32 = match code.parse::<i32>() {
        Ok(v) => v,
        Err(_) => return "unknown",
    };
    match n {
        0..=5 => "core",
        10000..=19999 => "task_chain",
        20000..=29999 => "sub_task",
        30000..=39999 => "report",
        _ => "unknown",
    }
}

fn state_of(name: &str) -> String {
    for s in ["Error", "Start", "Completed", "Stopped", "ExtraInfo", "Info"].iter() {
        if name.ends_with(*s) {
            return (*s).to_string();
        }
    }
    last_seg(name)
}

/// 环里的一条 → 归一的一条。`main_thread` 由调用方给（实测线程号**每跑一次都换**，
/// 所以只许当"是不是主线程"的布尔用，不许当识别依据）。
fn normalize(e: &Event, main_thread: u32) -> NormEvent {
    let domain = if e.abi == "asst" {
        asst_domain(&e.code).to_string()
    } else {
        first_seg(&e.message)
    };
    NormEvent {
        seq: e.seq,
        abi: e.abi,
        raw_code: e.code.clone(),
        raw_name: e.message.clone(),
        name: e.message.clone(),
        domain,
        state: if e.abi == "asst" { state_of(&e.message) } else { last_seg(&e.message) },
        // 两侧回调都不自带时间戳（实测负向读数：`details` 里只有 hash/path/res_id/type，没有任何时间字段），
        // 所以这一格恒为 local 并**必须打标** —— 不打标就会把"我补的"当"它测的"拿去对拍。
        ts_source: "local",
        on_main_thread: e.thread == main_thread,
        rel_ms: e.rel_ms,
        details: e.details.clone(),
    }
}

/// 序号对账的读数（丢失语义那一维**唯一**能现在就建起来的那半：方法先有，读数等窗口）。
#[derive(Clone)]
struct Loss {
    received: u64,
    expected: Option<u64>,
    contiguous: bool,
    gaps: Vec<u64>,
    first_seq: u64,
    verdict: &'static str,
}

/// **连续不等于没丢**。没有"已知应到数"时判 `unverified`，绝不判"没丢"——
/// 这正是 `loss_semantics` 那一格在 R0 跑两遍都没被替换的原因（两遍各收到自己那 2 条，没有参照流）。
fn loss_check(seqs: &[u64], expected: Option<u64>) -> Loss {
    let mut gaps: Vec<u64> = Vec::new();
    let mut i = 1usize;
    while i < seqs.len() {
        let prev = seqs[i - 1];
        let cur = seqs[i];
        if cur > prev + 1 {
            let mut m = prev + 1;
            while m < cur {
                gaps.push(m);
                m += 1;
            }
        }
        i += 1;
    }
    let contiguous = seqs.windows(2).all(|w| w[1] == w[0] + 1);
    let received = seqs.len() as u64;
    let verdict: &'static str = if !contiguous {
        "gapped"
    } else if expected.is_none() {
        "unverified"
    } else if received == expected.unwrap_or(0) {
        "expected-met"
    } else {
        "expected-mismatch"
    };
    Loss {
        received,
        expected,
        contiguous,
        gaps,
        first_seq: if seqs.is_empty() { 0 } else { seqs[0] },
        verdict,
    }
}

/// 终态：**只认归一后的事件名**。framework 侧 `code` 恒为空串 ⇒ 按 code 判必错；
/// `details.hash` 成败两侧都给 ⇒ 按 hash 判必错。取**环序里最后命中的那条**（实测到达顺序 Starting 先、终态后）。
fn terminal_of(name: &str) -> Option<&'static str> {
    match name {
        "AllTasksCompleted" => Some("succeeded"),
        "TaskChainError" | "InitFailed" => Some("failed"),
        "TaskChainStopped" => Some("stopped"),
        _ => {
            if name.ends_with(".Succeeded") {
                Some("succeeded")
            } else if name.ends_with(".Failed") {
                Some("failed")
            } else if name.ends_with(".Stopped") {
                Some("stopped")
            } else {
                None
            }
        }
    }
}

fn event_verdict(events: &[NormEvent]) -> (&'static str, String) {
    let mut verdict = "no-terminal";
    let mut why = String::from("环里没有终态事件：只有起始不见收尾时不许判成败（'ok 从不证资源'那条判读错的镜像）");
    for e in events {
        if let Some(v) = terminal_of(&e.name) {
            verdict = v;
            why = format!("末条终态命中：{}", e.name);
        }
    }
    (verdict, why)
}

/// 事件流的对外信封 —— 这个函数体里**不许出现**被裁掉的那两枚读数，`--selftest` 有一条门数着它。
fn render_events_json(fixture: &str, main_thread: u32, cb_panics: u32, events: &[NormEvent], loss: &Loss) -> String {
    let mut p: Vec<String> = Vec::new();
    p.push(format!("\"ok\":{}", if events.is_empty() { "false" } else { "true" }));
    p.push(format!("\"contract\":{}", jstr(CONTRACT)));
    p.push(format!("\"role\":\"R1-events\""));
    p.push(format!("\"fixture\":{}", jstr(fixture)));
    p.push(format!("\"event_count\":{}", events.len()));
    p.push(format!("\"main_thread\":{}", main_thread));
    p.push(format!("\"callback_panics\":{}", cb_panics));
    p.push(format!("\"loss\":{{\"first_seq\":{},\"received\":{},\"expected\":{},\"contiguous\":{},\"gaps\":{},\"verdict\":{}}}",
        loss.first_seq,
        loss.received,
        match loss.expected {
            Some(v) => v.to_string(),
            None => "\"not-attempted\"".to_string(),
        },
        if loss.contiguous { "true" } else { "false" },
        jarr(&loss.gaps.iter().map(|g| g.to_string()).collect::<Vec<_>>()),
        jstr(loss.verdict)));
    let (v, w) = event_verdict(events);
    p.push(format!("\"verdict\":{}", jstr(v)));
    p.push(format!("\"why\":{}", jstr(&w)));
    let ev: Vec<String> = events
        .iter()
        .map(|e| {
            format!(
                "{{\"seq\":{},\"abi\":{},\"raw_code\":{},\"raw_name\":{},\"name\":{},\"domain\":{},\"state\":{},\"ts_source\":{},\"on_main_thread\":{},\"rel_ms\":{},\"details\":{}}}",
                e.seq,
                jstr(e.abi),
                jstr(&e.raw_code),
                jstr(&e.raw_name),
                jstr(&e.name),
                jstr(&e.domain),
                jstr(&e.state),
                jstr(e.ts_source),
                if e.on_main_thread { "true" } else { "false" },
                e.rel_ms,
                jstr(&e.details)
            )
        })
        .collect();
    p.push(format!("\"events\":[{}]", ev.join(",")));
    format!("{{{}}}", p.join(","))
}

/// 内联夹具：形状取自连接段两次实跑**读到的形状**，`details` 原文里**没有一个字节的现场数据**
/// （路径写成 `<本机路径>` 标签，计数与 id 是编的）。所以这些**不是观测读数** ——
/// 出口里带 `fixture` 标，对拍门拿它钉的是**装配形状与判据**，不是"该到几条"。
///
/// framework 那对故意把两条的 `details` 原文写成**一模一样**：两枚 hash 因此相同，
/// 而终态必须一败一成判得开 —— 这就是"按 hash 判成败"的证伪样本。
///
/// R3①b 又加两档**终态负例**（`asst-stopped` / `asst-failed`）：都是"起始 + 终态"两枚，
/// 只换终态那一条（TaskChainStopped / TaskChainError）—— 对应六类负例里"任务取消 / 任务失败"
/// 在事件流面的最小样本，把 verdict 从"恒 succeeded"里拽出来。
fn fixture_events(which: &str) -> Vec<(&'static str, String, String, String)> {
    let asst_three: Vec<(&'static str, String, String, String)> = vec![
        (
            "asst",
            "2".to_string(),
            ark_msg_name(2).to_string(),
            r#"{"what":"ScreencapCost","avg":56}"#.to_string(),
        ),
        (
            "asst",
            "10001".to_string(),
            ark_msg_name(10001).to_string(),
            r#"{"chain":"MainLine"}"#.to_string(),
        ),
        (
            "asst",
            "3".to_string(),
            ark_msg_name(3).to_string(),
            r#"{"status":"success"}"#.to_string(),
        ),
    ];
    let mf_pair: Vec<(&'static str, String, String, String)> = vec![
        (
            "framework",
            String::new(),
            "Resource.Loading.Starting".to_string(),
            r#"{"path":"<本机路径>","res_id":2096769248,"type":"file","count":1}"#.to_string(),
        ),
        (
            "framework",
            String::new(),
            "Resource.Loading.Succeeded".to_string(),
            r#"{"path":"<本机路径>","res_id":2096769248,"type":"file","count":1}"#.to_string(),
        ),
    ];
    let asst_stopped: Vec<(&'static str, String, String, String)> = vec![
        (
            "asst",
            "10001".to_string(),
            ark_msg_name(10001).to_string(),
            r#"{"chain":"MainLine"}"#.to_string(),
        ),
        (
            "asst",
            "10004".to_string(),
            ark_msg_name(10004).to_string(),
            r#"{"chain":"MainLine"}"#.to_string(),
        ),
    ];
    let asst_failed: Vec<(&'static str, String, String, String)> = vec![
        (
            "asst",
            "10001".to_string(),
            ark_msg_name(10001).to_string(),
            r#"{"chain":"MainLine"}"#.to_string(),
        ),
        (
            "asst",
            "10000".to_string(),
            ark_msg_name(10000).to_string(),
            r#"{"chain":"MainLine"}"#.to_string(),
        ),
    ];
    match which {
        "asst" => asst_three,
        "framework" => mf_pair,
        "both" => {
            let mut v = asst_three;
            v.extend(mf_pair);
            v
        }
        "asst-stopped" => asst_stopped,
        "asst-failed" => asst_failed,
        other => {
            eprintln!("未知 fixture：{}（只有 asst / framework / both / asst-stopped / asst-failed）", other);
            exit(1);
        }
    }
}

/// 取环里 `from` 之后新加的那一段并归一。环在喂事件**之前**就必须就绪（keyfacts 里那条硬约束）。
fn normalize_ring(main_thread: u32, from: usize) -> Vec<NormEvent> {
    let guard = match events().lock() {
        Ok(g) => g.clone(),
        Err(p) => p.into_inner().clone(),
    };
    guard.iter().skip(from).map(|e| normalize(e, main_thread)).collect()
}

/// `--events` 那一档：把内联夹具**从 adapter 同一处入口**灌进环（不是抄一份事件形状的副本），
/// 再走归一 → 序号对账 → 终态判据 → 对外信封。全程不加载任何 DLL、不问机器现场。
fn run_events(fixture: &str, expected: Option<u64>, drop_at: u64, out: &Option<String>) -> i32 {
    // 这一档的事件是**我方自己**在调用线程上推进环的，所以 `on_main_thread` 恒为 true ——
    // 这是构造出来的，不是读数。连接段那两次实跑读到的是**跨线程**（32304 vs 主线程 32520），
    // 那条读数只能在真回调里拿，夹具造不出来；`--selftest` 里第 12 节用一个手搓的 `Event`
    // 单独钉住"线程号不等 ⇒ false"这一支，免得这一档全 true 把判据的空转当成通过。
    let main_thread = unsafe { GetCurrentThreadId() };
    let before = events().lock().map(|g| g.len()).unwrap_or(0);
    for (abi, code, message, raw) in fixture_events(fixture) {
        push_event(abi, code, message, &raw);
    }
    let mut list = normalize_ring(main_thread, before);
    if drop_at > 0 && list.len() >= 2 {
        // 造缺口：从**环的副本**里抽掉第 N 条，seq 因此断号 —— 判据必须报 gapped 并指出缺的是哪个号。
        let idx = ((drop_at as usize) - 1).min(list.len() - 1);
        list.remove(idx);
    }
    let seqs: Vec<u64> = list.iter().map(|e| e.seq).collect();
    let loss = loss_check(&seqs, expected);
    let json = render_events_json(fixture, main_thread, CB_PANICS.load(Ordering::SeqCst), &list, &loss);
    emit(&json, out);
    if list.is_empty() {
        return 2;
    }
    0
}

// ---- R2：监督纯判定四段（照 tools\panel-cs\Supervisor.cs 逐字复刻；等值由三侧对拍门钉）
//
// 这一层的四个函数是 C# 那份的**逐字复刻**：话术、判据顺序、PS 等值口径（`$x -ne $false` 那套）
// 一处都不许"顺手优化"。夹具（48 档，抄自 tools\probe-sv-golden.ps1 的场景清单）内联在下面，
// 只吃夹具、不加载任何 DLL、不碰任何现场；日期戳由 --date 注入（C# 侧同样不读时钟，golden 才可复现）。
// `JVal` 是这一层的通用值树：形状对齐 JavaScriptSerializer 反序列化的产物（对象/数组/字符串/数字/布尔/null），
// 语义工具（truthy/get/str/int/list）对齐 C# 的 Gd.* 工具，一行一套、不掺 IO。

#[derive(Clone, Debug, PartialEq)]
enum JVal {
    Null,
    B(bool),
    I(i64),
    F(f64),
    S(String),
    A(Vec<JVal>),
    O(Vec<(String, JVal)>),
}

fn jv_s(s: &str) -> JVal {
    JVal::S(s.to_string())
}

fn jv_o(kv: Vec<(&str, JVal)>) -> JVal {
    JVal::O(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// `Gd.Get`：null 映射、缺键、非对象 → Null（键冲突取首个，夹具面不出现重复键）。
fn jv_get(v: &JVal, key: &str) -> JVal {
    if let JVal::O(o) = v {
        for (k, val) in o {
            if k == key {
                return val.clone();
            }
        }
    }
    JVal::Null
}

/// `Gd.Truthy`：null/空串/0/空集合为假，其余为真（判定层大量依赖，别按"非 null 即真"的直觉）。
fn jv_truthy(v: &JVal) -> bool {
    match v {
        JVal::Null => false,
        JVal::B(b) => *b,
        JVal::S(s) => !s.is_empty(),
        JVal::A(a) => !a.is_empty(),
        JVal::O(o) => !o.is_empty(),
        JVal::I(i) => *i != 0,
        JVal::F(f) => *f != 0.0,
    }
}

/// PS 等值口径：`$x -ne $false` 不是 C# 的 x != false —— bool 看值、数字转成 0/1、
/// 字符串把 $false 换成 "False" 做大小写不敏感比较（'False' 字符串在 PS 里就是关）。
/// 其余类型现实中不出现在 supervisor 键上，退化成真值判断（照 Supervisor.cs 注释口径）。
fn sv_ne_false(v: &JVal) -> bool {
    match v {
        JVal::Null => true,
        JVal::B(b) => *b,
        JVal::S(s) => !s.eq_ignore_ascii_case("False"),
        JVal::I(i) => *i != 0,
        JVal::F(f) => *f != 0.0,
        _ => jv_truthy(v),
    }
}

/// `Gd.Str`：null → ""；数字按不变文化。数组支 = `Convert.ToString(object[])` → 类型名
/// "System.Object[]"（10-05 twotgt 实锤：畸形嵌套数组元素经 Gd.Str 就是这个串，甲侧 S/R
/// 两段同形印证）；对象支在夹具与生产字面里仍不可达，留空串兜底。
fn jv_str(v: &JVal) -> String {
    match v {
        JVal::Null => String::new(),
        JVal::S(s) => s.clone(),
        JVal::B(b) => if *b { "True" } else { "False" }.to_string(),
        JVal::I(i) => i.to_string(),
        JVal::F(f) => format!("{}", f),
        JVal::A(_) => "System.Object[]".to_string(),
        JVal::O(_) => String::new(),
    }
}

/// C# `Convert.ToInt32(double)` 的银行家舍入（到偶数）；夹具面用不到小数进 [int] 的格，
/// 但口径写死在这里，省得将来某一档静默按"四舍五入"漂过去。
fn round_half_even(f: f64) -> f64 {
    let r = f.round();
    if ((f - f.trunc()).abs() - 0.5).abs() < f64::EPSILON && (r % 2.0) != 0.0 {
        r - f.signum()
    } else {
        r
    }
}

/// `Gd.Int`：null → def；转换失败 → def（C# 侧对非法值也是差一条 throw 与 0 的区别，见 Supervisor.cs 已知局限注）。
fn jv_int(v: &JVal, def: i64) -> i64 {
    match v {
        JVal::Null => def,
        JVal::I(i) => *i,
        JVal::F(f) => round_half_even(*f) as i64,
        JVal::B(b) => if *b { 1 } else { 0 },
        JVal::S(s) => s.trim().parse::<i64>().unwrap_or(def),
        _ => def,
    }
}

/// 状态判据的 `ToD`：null → 0；转不动 → 0（只用于和 quietSec 比大小）。
fn jv_to_d(v: &JVal) -> f64 {
    match v {
        JVal::Null => 0.0,
        JVal::I(i) => *i as f64,
        JVal::F(f) => *f,
        JVal::B(b) => if *b { 1.0 } else { 0.0 },
        JVal::S(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `Gd.List`：null → 空；数组摊开；字符串/对象/标量都算"一个元素"（PS 的 @() 语义）。
fn jv_list(v: &JVal) -> Vec<&JVal> {
    match v {
        JVal::Null => Vec::new(),
        JVal::A(a) => a.iter().collect(),
        other => vec![other],
    }
}

/// 出口投影：只要合法 JSON。字符串走既有 jstr；输出面不出现 F（真出现也按最小数字打印）。
fn jv_to_json(v: &JVal) -> String {
    match v {
        JVal::Null => "null".to_string(),
        JVal::B(b) => if *b { "true" } else { "false" }.to_string(),
        JVal::I(i) => i.to_string(),
        JVal::F(f) => format!("{}", f),
        JVal::S(s) => jstr(s),
        JVal::A(a) => format!("[{}]", a.iter().map(jv_to_json).collect::<Vec<_>>().join(",")),
        JVal::O(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, val)| format!("{}:{}", jstr(k), jv_to_json(val)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// pwsh 7 `ConvertTo-Json` 的字符串转义口径（B-1a 已搬动作写出的文件要与 PS 真身逐字节同）：
/// 短五枚走名字转义（\b \t \n \f \r）；C0 其余 27 枚 + U+0085/U+2028/U+2029 → `\u` 四位小写；
/// 0x7F、C1 其余、U+00A0、U+FEFF、非 BMP 一律原样。口径来源=实测读数表（state\gd-b1a-probe-out.txt）。
fn ps_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            '\u{85}' | '\u{2028}' | '\u{2029}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// pwsh 7 `ConvertTo-Json` 的美化输出（`-Depth 100` 下对象/数组全展开）。形状口径：
/// CRLF 换行、**尾无换行**、缩进 2 空格一层、`"key": value`；**单元素数组仍多行**；
/// 空数组 `[]` / 空对象 `{}` 内联。写文件时编码 UTF-8 无 BOM（调用方负责）。
fn ps_to_json_pretty(v: &JVal) -> String {
    let mut out = String::new();
    ps_write_pretty(v, 0, &mut out);
    out
}

fn ps_write_pretty(v: &JVal, indent: usize, out: &mut String) {
    match v {
        JVal::Null => out.push_str("null"),
        JVal::B(b) => out.push_str(if *b { "true" } else { "false" }),
        JVal::I(i) => out.push_str(&i.to_string()),
        JVal::F(f) => out.push_str(&format!("{}", f)),
        JVal::S(s) => out.push_str(&ps_str(s)),
        JVal::A(a) => {
            if a.is_empty() {
                out.push_str("[]");
            } else {
                out.push_str("[\r\n");
                for (i, item) in a.iter().enumerate() {
                    if i > 0 {
                        out.push_str(",\r\n");
                    }
                    out.push_str(&"  ".repeat(indent + 1));
                    ps_write_pretty(item, indent + 1, out);
                }
                out.push_str("\r\n");
                out.push_str(&"  ".repeat(indent));
                out.push(']');
            }
        }
        JVal::O(o) => {
            if o.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{\r\n");
                for (i, (k, val)) in o.iter().enumerate() {
                    if i > 0 {
                        out.push_str(",\r\n");
                    }
                    out.push_str(&"  ".repeat(indent + 1));
                    out.push_str(&ps_str(k));
                    out.push_str(": ");
                    ps_write_pretty(val, indent + 1, out);
                }
                out.push_str("\r\n");
                out.push_str(&"  ".repeat(indent));
                out.push('}');
            }
        }
    }
}

/// 本地时刻一次取全（GetLocalTime）——下面几枚格式串都从这一枚出，保证同一进程里
/// 相邻两次取名义上同一秒（真身 `Get-Date` 每次调用自己取，这也是它两侧能逐字对拍的原因：
/// 对拍时两侧喂的是同一个现取值，而不是各取各的时钟）。
fn gd_now() -> SystemTime {
    let mut st = SystemTime {
        w_year: 0,
        w_month: 0,
        w_day_of_week: 0,
        w_day: 0,
        w_hour: 0,
        w_minute: 0,
        w_second: 0,
        w_milliseconds: 0,
    };
    unsafe {
        GetLocalTime(&mut st);
    }
    st
}

/// 本地日期串 `yyyy-MM-dd`（GetLocalTime）——PS 真身 `(Get-Date).ToString('yyyy-MM-dd')` 的同机同日口径。
fn gd_local_date() -> String {
    let st = gd_now();
    format!("{:04}-{:02}-{:02}", st.w_year, st.w_month, st.w_day)
}

/// `Get-Date -Format 'yyyyMMdd'` —— M9A 日志文件名里那一截（`Get-M9ADailyLog`，m9a.ps1:126-130：
/// `Join-Path $P.LogDir "log-$(Get-Date -Format 'yyyyMMdd').log"`）。**没有横线**，跟上面那枚
/// `yyyy-MM-dd` 不是一套形状；每轮重算 ⇒ 跨 00:00 就换一个文件，偏移必须按路径 keyed。
/// 剧本侧不吃这一枚：那边由夹具喂的日期表按虚拟钟查（`day_lookup`），两边在投影上同记一条 `D|`。
fn gd_day_stamp() -> String {
    let st = gd_now();
    format!("{:04}{:02}{:02}", st.w_year, st.w_month, st.w_day)
}

/// `Get-Date -Format 'MMdd-HHmmss'` —— 兜底截图的文件名后缀用（真身 vision-rescue.ps1 里那枚 `$stamp`）。
/// 注意月在前、横线后是时分秒，`H` 是 24 小时制且补零，跟通用的 `yyyy-MM-dd HH:mm:ss` 不是一套。
/// 命名刻意不叫 stamp：connect_run 那枚形状门数的是全文里那个词根带左括号的出现次数
/// （8 = 定义 1 + 出口 7），兜底这层借用同一个词会把那条计数顶成假红 —— 形状门只当辅助镜，
/// 但既然能避让就避让，别让它变成"看见红先怀疑门"的那种红。
fn gd_shot_suffix() -> String {
    let st = gd_now();
    format!(
        "{:02}{:02}-{:02}{:02}{:02}",
        st.w_month, st.w_day, st.w_hour, st.w_minute, st.w_second
    )
}

/// `Get-Date -Format 'yyyy-MM-dd HH:mm:ss'` —— 救火账本里 `time`／`updated` 两格的形状。
/// 真身在 `Add-GdRescueRecord` 里取两次（一次给事件、一次给账本头），所以同一轮里这两格
/// 可以差一秒；这边照样各取各的，别"顺手"合并成一次。
fn gd_ledger_time() -> String {
    let st = gd_now();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        st.w_year, st.w_month, st.w_day, st.w_hour, st.w_minute, st.w_second
    )
}

/// 从 `yyyy-M-d`（月/日可一位或两位）解星期几，0=周日 .. 6=周六（Sakamoto）。
/// 照 PS 真身 `Get-Date ([string]$Obs.Date)` 的口径：解不动/日期不存在 → None（调用侧当失败处理）。
/// 只认 `yyyy-M-d` 一种形状 —— 观测账本里的 Date 是 PS 侧自己按这个形状写进去的。
fn gd_dow_from_date(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.trim().split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    if y < 1 || !(1..=12).contains(&m) || d < 1 {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mdays = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if d > mdays[(m - 1) as usize] {
        return None;
    }
    let t = [0i64, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yy = if m < 3 { y - 1 } else { y };
    let w = (yy + yy / 4 - yy / 100 + yy / 400 + t[(m - 1) as usize] + d) % 7;
    Some(w as u32)
}

/// 迷你 JSON 解析器（只为本 crate 自用：读 panel-cs.exe 的 --out 信封，R3② 装配回放档要用）。
/// 口径：RFC 8259 的常规子集 —— 对象 / 数组 / 字符串（含 \uXXXX 与代理对）/ 数字 / true / false / null；
/// 顶层值后只许剩空白。失败带"第几个字符"，信封读不动时能把坏点说清楚。
/// 手写而不是引 serde：零依赖是本 crate 的既有决定（见 Cargo.toml 头注），且面对的输入是
/// 我们自家 C# 侧的输出，形状可控。
fn jv_parse(s: &str) -> Result<JVal, String> {
    let cs: Vec<char> = s.chars().collect();
    let mut p = Jp { cs: &cs, i: 0 };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.i != p.cs.len() {
        return Err(format!("第 {} 个字符起有多余内容", p.i + 1));
    }
    Ok(v)
}

struct Jp<'a> {
    cs: &'a [char],
    i: usize,
}

impl Jp<'_> {
    fn peek(&self) -> Option<char> {
        self.cs.get(self.i).copied()
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    fn value(&mut self) -> Result<JVal, String> {
        match self.peek() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => Ok(JVal::S(self.string()?)),
            Some('t') => {
                self.lit("true")?;
                Ok(JVal::B(true))
            }
            Some('f') => {
                self.lit("false")?;
                Ok(JVal::B(false))
            }
            Some('n') => {
                self.lit("null")?;
                Ok(JVal::Null)
            }
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("第 {} 个字符「{}」不是合法值开头", self.i + 1, c)),
            None => Err("读到串尾，值还没出来".to_string()),
        }
    }
    fn lit(&mut self, w: &str) -> Result<(), String> {
        for c in w.chars() {
            if self.peek() == Some(c) {
                self.i += 1;
            } else {
                return Err(format!("第 {} 个字符：期望 `{}`", self.i + 1, w));
            }
        }
        Ok(())
    }
    fn object(&mut self) -> Result<JVal, String> {
        self.i += 1; // '{'
        let mut kv = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.i += 1;
            return Ok(JVal::O(kv));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some('"') {
                return Err(format!("第 {} 个字符：对象键必须是字符串", self.i + 1));
            }
            let k = self.string()?;
            self.skip_ws();
            if self.peek() != Some(':') {
                return Err(format!("第 {} 个字符：键后缺冒号", self.i + 1));
            }
            self.i += 1;
            self.skip_ws();
            let v = self.value()?;
            kv.push((k, v));
            self.skip_ws();
            match self.peek() {
                Some(',') => self.i += 1,
                Some('}') => {
                    self.i += 1;
                    return Ok(JVal::O(kv));
                }
                _ => return Err(format!("第 {} 个字符：对象里期望逗号或右花括号", self.i + 1)),
            }
        }
    }
    fn array(&mut self) -> Result<JVal, String> {
        self.i += 1; // '['
        let mut a = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.i += 1;
            return Ok(JVal::A(a));
        }
        loop {
            self.skip_ws();
            a.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(',') => self.i += 1,
                Some(']') => {
                    self.i += 1;
                    return Ok(JVal::A(a));
                }
                _ => return Err(format!("第 {} 个字符：数组里期望逗号或右方括号", self.i + 1)),
            }
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // '"'
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err("字符串没闭合".to_string()),
                Some('"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some('\\') => {
                    self.i += 1;
                    match self.peek() {
                        Some('"') => {
                            out.push('"');
                            self.i += 1;
                        }
                        Some('\\') => {
                            out.push('\\');
                            self.i += 1;
                        }
                        Some('/') => {
                            out.push('/');
                            self.i += 1;
                        }
                        Some('b') => {
                            out.push('\u{8}');
                            self.i += 1;
                        }
                        Some('f') => {
                            out.push('\u{c}');
                            self.i += 1;
                        }
                        Some('n') => {
                            out.push('\n');
                            self.i += 1;
                        }
                        Some('r') => {
                            out.push('\r');
                            self.i += 1;
                        }
                        Some('t') => {
                            out.push('\t');
                            self.i += 1;
                        }
                        Some('u') => {
                            self.i += 1;
                            let u = self.hex4()?;
                            if (0xD800..=0xDBFF).contains(&u) {
                                // 高代理：后邻必须是 \uDC00-DFFF 的低代理，组合成一个字符
                                if self.peek() == Some('\\') && self.cs.get(self.i + 1) == Some(&'u') {
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    if (0xDC00..=0xDFFF).contains(&lo) {
                                        let c = 0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00);
                                        match char::from_u32(c) {
                                            Some(ch) => out.push(ch),
                                            None => return Err("代理对拼不出字符".to_string()),
                                        }
                                    } else {
                                        return Err("高代理后面不是低代理".to_string());
                                    }
                                } else {
                                    return Err("高代理没跟低代理".to_string());
                                }
                            } else if (0xDC00..=0xDFFF).contains(&u) {
                                return Err("孤立低代理".to_string());
                            } else {
                                match char::from_u32(u) {
                                    Some(ch) => out.push(ch),
                                    None => return Err("\\u 值不是合法字符".to_string()),
                                }
                            }
                        }
                        _ => return Err(format!("第 {} 个字符：反斜杠后是不认识的转义", self.i + 1)),
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
    }
    fn hex4(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let c = self.peek().ok_or("\\u 后面不足 4 位")?;
            let d = c
                .to_digit(16)
                .ok_or_else(|| format!("第 {} 个字符：「{}」不是十六进制位", self.i + 1, c))?;
            v = v * 16 + d;
            self.i += 1;
        }
        Ok(v)
    }
    fn number(&mut self) -> Result<JVal, String> {
        let start = self.i;
        if self.peek() == Some('-') {
            self.i += 1;
        }
        let mut is_float = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.i += 1;
            } else if c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
                is_float = true;
                self.i += 1;
            } else {
                break;
            }
        }
        let text: String = self.cs[start..self.i].iter().collect();
        if text.is_empty() || text == "-" {
            return Err(format!("第 {} 个字符：数字是空的", start + 1));
        }
        if is_float {
            text.parse::<f64>()
                .map(JVal::F)
                .map_err(|e| format!("数字「{}」读不动：{}", text, e))
        } else {
            match text.parse::<i64>() {
                Ok(v) => Ok(JVal::I(v)),
                Err(_) => text
                    .parse::<f64>()
                    .map(JVal::F)
                    .map_err(|e| format!("数字「{}」读不动：{}", text, e)),
            }
        }
    }
}

// ==================== verb: eval-supervisor-settings（原四件开关取值） ====================
// 输入是四个**原始值**（缺键=null），一次算全：Enabled=「没显式 false 就是开」；
// IntervalSec <15 归 60（拍太快费日志）；QuietSec=分钟×60、下限 5 分钟；MaxInterventions 缺省 1、负数钳 0。
fn sv_settings(inp: &JVal) -> JVal {
    let sup = jv_get(inp, "supervisor");
    let enabled = matches!(sup, JVal::Null) || sv_ne_false(&sup);

    let mut v = 0i64;
    let raw_i = jv_get(inp, "intervalSeconds");
    if jv_truthy(&raw_i) {
        v = jv_int(&raw_i, 0);
    }
    if v < 15 {
        v = 60;
    }

    let mut m = 30i64;
    let raw_q = jv_get(inp, "quietMinutes");
    if jv_truthy(&raw_q) {
        m = jv_int(&raw_q, 0);
    }
    if m < 5 {
        m = 5;
    }

    let raw_m = jv_get(inp, "maxInterventions");
    let max = if matches!(raw_m, JVal::Null) {
        1
    } else {
        let t = jv_int(&raw_m, 0);
        if t < 0 { 0 } else { t }
    };

    jv_o(vec![
        ("Enabled", JVal::B(enabled)),
        ("IntervalSec", JVal::I(v)),
        ("QuietSec", JVal::I(m * 60)),
        ("MaxInterventions", JVal::I(max)),
    ])
}

// ==================== verb: parse-supervisor-state（原 Get-GdSupervisorState） ====================
// 规则心跳，零 token：done/dead/quiet/busy 四态。判据顺序是认真的（注释随旧实现搬来）：
// done 排最前（完成线看到了，进程在不在都不重要）；dead 只在"曾经见过助手活着、现在没了"时才判；
// quiet 要求**两本日志都**安静**严格大于**阈值；asst_age/daily_age 缺失按 0 收。
fn sv_state(inp: &JVal) -> JVal {
    let ev = jv_get(inp, "evidence");
    if jv_truthy(&jv_get(&ev, "Done")) {
        return jv_o(vec![("State", jv_s("done"))]);
    }
    let alive = jv_truthy(&jv_get(&ev, "AssistantAlive"));
    if jv_truthy(&jv_get(inp, "everAlive")) && !alive {
        return jv_o(vec![("State", jv_s("dead"))]);
    }
    let quiet = jv_int(&jv_get(inp, "quietSec"), 1800) as f64;
    let a = jv_to_d(&jv_get(&ev, "AsstAgeSec"));
    let d = jv_to_d(&jv_get(&ev, "DailyAgeSec"));
    if a > quiet && d > quiet {
        return jv_o(vec![("State", jv_s("quiet"))]);
    }
    jv_o(vec![("State", jv_s("busy"))])
}

// ==================== verb: resolve-supervisor-action（原 Get-GdSupervisorAction 决策表） ====================
// 干预决策表：quiet 要**连续两拍**才算数（一拍可能是日志轮转/时间抖动）；
// 画面 blocking=true → 不干预，那是主流程兜底的活场；blocking=false（没弹窗却全线静止）
// 且这一路允许重启且还有额度 → restart_assistant；其余异常一律 notify，
// 且每种状态一轮只通知一次（调用方记 alreadyNotified）。
// done/busy 之外的 state 一律按 quiet 流程走（照抄旧实现的 fall-through）；字符串比较按
// PS -eq 的大小写不敏感口径。四句 Why 是话术本体，逐字禁改（golden 拍全文）。
fn sv_action(inp: &JVal) -> JVal {
    let state = jv_str(&jv_get(inp, "state"));
    if state.eq_ignore_ascii_case("done") || state.eq_ignore_ascii_case("busy") {
        return jv_o(vec![("Kind", jv_s("none")), ("Why", jv_s(""))]);
    }
    let notified = jv_truthy(&jv_get(inp, "alreadyNotified"));
    if state.eq_ignore_ascii_case("dead") {
        if notified {
            return jv_o(vec![("Kind", jv_s("none")), ("Why", jv_s(""))]);
        }
        return jv_o(vec![
            ("Kind", jv_s("notify")),
            ("Why", jv_s("助手进程不见了，而这一轮还没宣布完成")),
        ]);
    }
    if jv_int(&jv_get(inp, "quietStreak"), 0) < 2 {
        return jv_o(vec![("Kind", jv_s("none")), ("Why", jv_s(""))]);
    }
    let scr = jv_get(inp, "screen");
    if !matches!(scr, JVal::Null) && jv_truthy(&jv_get(&scr, "Ok")) {
        if jv_truthy(&jv_get(&scr, "Blocking")) {
            return jv_o(vec![
                ("Kind", jv_s("none")),
                (
                    "Why",
                    JVal::S(format!(
                        "画面被「{}」挡着，交给主流程兜底",
                        jv_str(&jv_get(&scr, "What"))
                    )),
                ),
            ]);
        }
        if jv_truthy(&jv_get(inp, "canRestart"))
            && jv_int(&jv_get(inp, "interventions"), 0)
                < jv_int(&jv_get(inp, "maxInterventions"), 1)
        {
            return jv_o(vec![
                ("Kind", jv_s("restart_assistant")),
                (
                    "Why",
                    JVal::S(format!(
                        "连续静止且画面没有挡路的东西({})，判定层没醒，重启助手",
                        jv_str(&jv_get(&scr, "What"))
                    )),
                ),
            ]);
        }
        if notified {
            return jv_o(vec![("Kind", jv_s("none")), ("Why", jv_s(""))]);
        }
        return jv_o(vec![
            ("Kind", jv_s("notify")),
            ("Why", jv_s("连续静止、画面没有弹窗，但这一路不许自动重启(或额度用完)")),
        ]);
    }
    if notified {
        return jv_o(vec![("Kind", jv_s("none")), ("Why", jv_s(""))]);
    }
    let mut why = String::from("连续静止，且看不清画面");
    if !matches!(scr, JVal::Null) && jv_truthy(&jv_get(&scr, "Why")) {
        why.push('(');
        why.push_str(&jv_str(&jv_get(&scr, "Why")));
        why.push(')');
    }
    jv_o(vec![("Kind", jv_s("notify")), ("Why", JVal::S(why))])
}

// ==================== verb: compose-supervisor-detail（原 Get-GdSupervisorDetailLines） ====================
// 把 supervision.json 里**已经算好**的东西念成给人看的展开文本。纯函数、一个字都不判：
// state/action/screen/why 都是采集与判据那两层写进去的值，这里只按固定字段取值、按固定说法拼句子。
// 日期戳（Get-Date 现场）由壳以 date 参数注入（C# 侧同样，golden 才可复现）；行数刻意收敛
// （最近 MaxBeats 拍×每路一行+现值头+介入记录）。纪律：输出行不许含 '|'（面板协议分隔符，
// test-modules 钉着）；空串字段说"没问/none"，asst_age=-1 说"读不到"；beats 被上限挤掉时
// 不许顺手 return 吞掉介入记录（旧实现注释原话）。
fn sv_detail(inp: &JVal) -> JVal {
    let mut lines: Vec<JVal> = Vec::new();
    let sv = jv_get(inp, "sv");
    if matches!(sv, JVal::Null) {
        return jv_o(vec![("Lines", JVal::A(lines))]);
    }

    let stopped = if jv_truthy(&jv_get(&sv, "stopped")) {
        format!(" · 收工 {}", jv_str(&jv_get(&sv, "stopped")))
    } else {
        String::new()
    };
    lines.push(JVal::S(format!(
        "现值：PID {} · 盯的路线 {} · 上岗 {} · 最后一拍 {}{}",
        jv_str(&jv_get(&sv, "pid")),
        jv_str(&jv_get(&sv, "games")),
        jv_str(&jv_get(&sv, "started")),
        jv_str(&jv_get(&sv, "updated")),
        stopped
    )));
    lines.push(JVal::S(format!(
        "流水：logs\\supervision-{}.jsonl(一行一拍，留 7 天)",
        jv_str(&jv_get(inp, "date"))
    )));

    let beats_val = jv_get(&sv, "beats");
    let beats = jv_list(&beats_val);
    if beats.is_empty() {
        // 只说"还没落下第一拍"，不许就此结束：拍被挤掉、介入记录还在。
        lines.push(jv_s("这一枚监督进程还没落下第一拍(起拍间隔默认 60 秒)。"));
    } else {
        let mut mb = jv_int(&jv_get(inp, "maxBeats"), 6);
        if mb < 1 {
            mb = 1;
        }
        let shown = (mb as usize).min(beats.len());
        lines.push(JVal::S(format!(
            "最近 {} 拍(字段：状态 / 助手日志静默 / 进程 / 完成线 / 画面 / 动作)：",
            shown
        )));
        for &b in &beats[beats.len() - shown..] {
            let mut ts = jv_str(&jv_get(b, "ts"));
            let tsc: Vec<char> = ts.chars().collect();
            if tsc.len() >= 19 {
                // C# 的 Substring(11,8) 按 UTF-16 单元；这里按字符取，ASCII 日期戳两口径等值。
                ts = tsc[11..19].iter().collect();
            }
            let games_val = jv_get(b, "games");
            for g in jv_list(&games_val) {
                let mut age = jv_str(&jv_get(g, "asst_age"));
                age = if age == "-1" {
                    "读不到".to_string()
                } else {
                    age + "秒"
                };
                lines.push(JVal::S(format!(
                    "  {} {}={} 助手日志静默 {} 进程 {} 完成线 {} 画面 {} 动作 {}{}",
                    ts,
                    jv_str(&jv_get(g, "game")),
                    jv_str(&jv_get(g, "state")),
                    age,
                    if jv_truthy(&jv_get(g, "alive")) { "在" } else { "没" },
                    if jv_truthy(&jv_get(g, "done")) { "见过" } else { "没有" },
                    if jv_truthy(&jv_get(g, "screen")) {
                        jv_str(&jv_get(g, "screen"))
                    } else {
                        "没问".to_string()
                    },
                    if jv_truthy(&jv_get(g, "action")) {
                        jv_str(&jv_get(g, "action"))
                    } else {
                        "none".to_string()
                    },
                    if jv_truthy(&jv_get(g, "why")) {
                        format!(" 原因 {}", jv_str(&jv_get(g, "why")))
                    } else {
                        String::new()
                    }
                )));
            }
        }
    }

    let acts_val = jv_get(&sv, "actions");
    let acts = jv_list(&acts_val);
    if !acts.is_empty() {
        lines.push(JVal::S(format!("介入记录({} 次)：", acts.len())));
        for a in acts {
            lines.push(JVal::S(format!(
                "  {} {} {} {} — {}",
                jv_str(&jv_get(a, "ts")),
                jv_str(&jv_get(a, "game")),
                jv_str(&jv_get(a, "kind")),
                if jv_truthy(&jv_get(a, "ok")) { "成" } else { "没成" },
                jv_str(&jv_get(a, "why"))
            )));
        }
    } else {
        lines.push(jv_s("这一轮没有介入过(它唯一的干预是重启助手，重启过会记在这里)。"));
    }
    jv_o(vec![("Lines", JVal::A(lines))])
}

// ---- R2 夹具与出口：48 档场景（12 开关 + 11 状态 + 17 动作 + 8 详情），逐条抄自
//      tools\probe-sv-golden.ps1 的场景清单；名字与那边一字不差（三侧门按名做机器对账）。
// 输入一律按**转发壳归一后的 verb 输入形状**给：settings 四键恒在（缺=$null）、
// state 恒带 everAlive（壳缺省 $true —— 这一枚必须显式带上，C# 读法"缺键=null"是假，
// 不带的话 rust 侧会跟 old/new 分家）、action 七键恒在（缺省值按壳填好）、d/ 首行带注入日期。
fn sv_fixture_cases(date: &str) -> Vec<(&'static str, JVal)> {
    let mut cases: Vec<(&'static str, JVal)> = Vec::new();

    // ---------- set/ 12 档：eval-supervisor-settings ----------
    let setv = |sup: JVal, iv: JVal, qm: JVal, mx: JVal| -> JVal {
        jv_o(vec![
            ("supervisor", sup),
            ("intervalSeconds", iv),
            ("quietMinutes", qm),
            ("maxInterventions", mx),
        ])
    };
    let nul = || JVal::Null;
    cases.push(("set/empty", setv(nul(), nul(), nul(), nul())));
    cases.push(("set/false", setv(JVal::B(false), nul(), nul(), nul())));
    cases.push(("set/true", setv(JVal::B(true), nul(), nul(), nul())));
    cases.push(("set/zero", setv(JVal::I(0), nul(), nul(), nul())));
    cases.push(("set/strFalse", setv(jv_s("False"), nul(), nul(), nul())));
    cases.push(("set/strlower", setv(jv_s("false"), nul(), nul(), nul())));
    cases.push(("set/strYes", setv(jv_s("yes"), nul(), nul(), nul())));
    cases.push(("set/low", setv(nul(), JVal::I(5), JVal::I(1), JVal::I(-3))));
    cases.push(("set/ok", setv(nul(), JVal::I(90), JVal::I(45), JVal::I(2))));
    cases.push(("set/edge", setv(nul(), JVal::I(15), JVal::I(5), JVal::I(0))));
    cases.push(("set/strnum", setv(nul(), jv_s("90"), jv_s("5"), jv_s("2"))));
    cases.push(("set/nulls", setv(nul(), nul(), nul(), nul())));

    // ---------- st/ 11 档：parse-supervisor-state ----------
    let stv = |done: JVal, alive: bool, aa: JVal, da: JVal, qs: i64, ever: bool| -> JVal {
        jv_o(vec![
            (
                "evidence",
                jv_o(vec![
                    ("Done", done),
                    ("AssistantAlive", JVal::B(alive)),
                    ("AsstAgeSec", aa),
                    ("DailyAgeSec", da),
                ]),
            ),
            ("quietSec", JVal::I(qs)),
            ("everAlive", JVal::B(ever)),
        ])
    };
    cases.push(("st/done", stv(JVal::B(true), false, JVal::I(9999), JVal::I(9999), 1800, true)));
    cases.push(("st/dead", stv(JVal::B(false), false, JVal::I(10), JVal::I(10), 1800, true)));
    cases.push(("st/early", stv(JVal::B(false), false, JVal::I(9999), JVal::I(10), 1800, false)));
    cases.push(("st/quiet", stv(JVal::B(false), true, JVal::I(9999), JVal::I(9999), 1800, true)));
    cases.push(("st/asst-old", stv(JVal::B(false), true, JVal::I(9999), JVal::I(10), 1800, true)));
    cases.push(("st/daily-old", stv(JVal::B(false), true, JVal::I(10), JVal::I(9999), 1800, true)));
    cases.push(("st/edge", stv(JVal::B(false), true, JVal::I(1800), JVal::I(1800), 1800, true)));
    cases.push(("st/just", stv(JVal::B(false), true, JVal::I(301), JVal::I(301), 300, true)));
    cases.push(("st/nulls", stv(JVal::Null, true, JVal::Null, JVal::Null, 1800, true)));
    cases.push(("st/decimal", stv(JVal::B(false), true, JVal::F(1810.5), JVal::F(3000.25), 1800, true)));
    cases.push(("st/defaults", stv(JVal::B(false), true, JVal::I(2000), JVal::I(2000), 1800, true)));

    // ---------- a/ 17 档：resolve-supervisor-action ----------
    let act = |state: &str, qs: i64, scr: JVal, can: bool, intr: i64, maxi: i64, notif: bool| -> JVal {
        jv_o(vec![
            ("state", JVal::S(state.to_string())),
            ("quietStreak", JVal::I(qs)),
            ("screen", scr),
            ("canRestart", JVal::B(can)),
            ("interventions", JVal::I(intr)),
            ("maxInterventions", JVal::I(maxi)),
            ("alreadyNotified", JVal::B(notif)),
        ])
    };
    let scr_clear = || jv_o(vec![("Ok", JVal::B(true)), ("Blocking", JVal::B(false)), ("What", jv_s("主界面"))]);
    let scr_block = || jv_o(vec![("Ok", JVal::B(true)), ("Blocking", JVal::B(true)), ("What", jv_s("活动公告"))]);
    let scr_fail_why = || jv_o(vec![("Ok", JVal::B(false)), ("Why", jv_s("截图没成功"))]);
    let scr_fail_bare = || jv_o(vec![("Ok", JVal::B(false))]);
    cases.push(("a/busy", act("busy", 0, JVal::Null, false, 0, 1, false)));
    cases.push(("a/done", act("done", 0, JVal::Null, false, 0, 1, false)));
    cases.push(("a/DONE", act("DONE", 0, JVal::Null, false, 0, 1, false)));
    cases.push(("a/dead", act("dead", 0, JVal::Null, false, 0, 1, false)));
    cases.push(("a/dead-notified", act("dead", 0, JVal::Null, false, 0, 1, true)));
    cases.push(("a/quiet-1", act("quiet", 1, scr_clear(), true, 0, 1, false)));
    cases.push(("a/blocking", act("quiet", 2, scr_block(), false, 0, 1, false)));
    cases.push(("a/restart", act("quiet", 2, scr_clear(), true, 0, 1, false)));
    cases.push(("a/quota", act("quiet", 2, scr_clear(), true, 1, 1, false)));
    cases.push(("a/norestart", act("quiet", 2, scr_clear(), false, 0, 1, false)));
    cases.push(("a/norestart-not", act("quiet", 2, scr_clear(), false, 0, 1, true)));
    cases.push(("a/fail-why", act("quiet", 2, scr_fail_why(), true, 0, 1, false)));
    cases.push(("a/fail-bare", act("quiet", 2, scr_fail_bare(), true, 0, 1, false)));
    cases.push(("a/fail-notified", act("quiet", 3, scr_fail_why(), false, 0, 1, true)));
    cases.push(("a/screen-null", act("quiet", 2, JVal::Null, false, 0, 1, false)));
    cases.push(("a/unknown-fall", act("weird", 1, JVal::Null, false, 0, 1, false)));
    cases.push(("a/unknown-fall2", act("unknown", 2, scr_fail_bare(), false, 0, 1, false)));

    // ---------- d/ 8 档：compose-supervisor-detail ----------
    let beat2 = jv_o(vec![
        ("ts", jv_s("2026-09-26 13:05:00")),
        (
            "games",
            JVal::A(vec![
                jv_o(vec![
                    ("game", jv_s("t1")),
                    ("state", jv_s("quiet")),
                    ("alive", JVal::B(true)),
                    ("asst_age", JVal::I(1810)),
                    ("done", JVal::B(false)),
                    ("screen", jv_s("clear")),
                    ("action", jv_s("notify")),
                    ("why", jv_s("连续静止，额度用完")),
                ]),
                jv_o(vec![
                    ("game", jv_s("t2")),
                    ("state", jv_s("dead")),
                    ("alive", JVal::B(false)),
                    ("asst_age", JVal::I(-1)),
                    ("done", JVal::B(true)),
                    ("screen", JVal::Null),
                    ("action", jv_s("none")),
                    ("why", jv_s("")),
                ]),
            ]),
        ),
    ]);
    let doc1 = jv_o(vec![
        ("pid", JVal::I(4321)),
        ("owner", JVal::I(1)),
        ("started", jv_s("2026-09-26 13:00:00")),
        ("games", jv_s("t1+t2")),
        ("updated", jv_s("2026-09-26 13:05:00")),
        ("stopped", jv_s("")),
        ("beats", JVal::A(vec![beat2.clone()])),
        ("actions", JVal::A(vec![])),
    ]);
    let doc_stopped = jv_o(vec![
        ("pid", JVal::I(4321)),
        ("owner", JVal::I(1)),
        ("started", jv_s("2026-09-26 13:00:00")),
        ("games", jv_s("t1+t2")),
        ("updated", jv_s("2026-09-26 13:05:00")),
        ("stopped", jv_s("2026-09-26 13:20:00")),
        ("beats", JVal::A(vec![beat2.clone()])),
        (
            "actions",
            JVal::A(vec![jv_o(vec![
                ("ts", jv_s("13:06:00")),
                ("game", jv_s("t1")),
                ("kind", jv_s("restart_assistant")),
                ("ok", JVal::B(false)),
                ("why", jv_s("重启没成")),
            ])]),
        ),
    ]);
    // 刻意**没有** stopped / owner 键：旧侧属性缺失=$null 与新侧 JSON 缺键必须是同一句话
    let doc_no_beats = jv_o(vec![
        ("pid", JVal::I(1)),
        ("games", jv_s("all")),
        ("started", jv_s("x")),
        ("updated", jv_s("")),
        ("beats", JVal::A(vec![])),
        (
            "actions",
            JVal::A(vec![jv_o(vec![
                ("ts", jv_s("13:06:00")),
                ("game", jv_s("t1")),
                ("kind", jv_s("restart_assistant")),
                ("ok", JVal::B(true)),
                ("why", jv_s("已重启")),
            ])]),
        ),
    ]);
    // games 是单对象不是数组（5.1 的 ConvertTo-Json 就这样塌）—— 两侧 @() 语义必须一致
    let doc_short = jv_o(vec![
        ("pid", JVal::I(7)),
        ("games", jv_s("t9")),
        ("started", jv_s("s")),
        ("updated", jv_s("u")),
        ("stopped", jv_s("")),
        (
            "beats",
            JVal::A(vec![jv_o(vec![
                ("ts", jv_s("13:05")),
                (
                    "games",
                    jv_o(vec![
                        ("game", jv_s("t9")),
                        ("state", jv_s("busy")),
                        ("alive", JVal::B(true)),
                        ("asst_age", JVal::I(5)),
                        ("done", JVal::B(false)),
                        ("screen", jv_s("")),
                        ("action", jv_s("")),
                        ("why", jv_s("")),
                    ]),
                ),
            ])]),
        ),
        ("actions", JVal::A(vec![])),
    ]);
    let mk_b = |ts: &str, state: &str, age: i64| -> JVal {
        jv_o(vec![
            ("ts", JVal::S(ts.to_string())),
            (
                "games",
                JVal::A(vec![jv_o(vec![
                    ("game", jv_s("t1")),
                    ("state", JVal::S(state.to_string())),
                    ("alive", JVal::B(true)),
                    ("asst_age", JVal::I(age)),
                    ("done", JVal::B(false)),
                    ("screen", jv_s("")),
                    ("action", jv_s("none")),
                    ("why", jv_s("")),
                ])]),
            ),
        ])
    };
    let doc3 = jv_o(vec![
        ("pid", JVal::I(1)),
        ("games", jv_s("t1")),
        ("started", jv_s("s")),
        ("updated", jv_s("u")),
        ("stopped", jv_s("")),
        (
            "beats",
            JVal::A(vec![
                mk_b("2026-09-26 13:01:00", "busy", 5),
                mk_b("2026-09-26 13:02:00", "busy", 5),
                mk_b("2026-09-26 13:03:00", "quiet", 30),
            ]),
        ),
        ("actions", JVal::A(vec![])),
    ]);
    let det = |sv: JVal, mb: i64| -> JVal {
        jv_o(vec![
            ("sv", sv),
            ("maxBeats", JVal::I(mb)),
            ("date", JVal::S(date.to_string())),
        ])
    };
    cases.push(("d/null", det(JVal::Null, 6)));
    cases.push(("d/full", det(doc1, 6)));
    cases.push(("d/stopped", det(doc_stopped, 6)));
    cases.push(("d/nobeats", det(doc_no_beats, 6)));
    cases.push(("d/shortts", det(doc_short, 6)));
    cases.push(("d/max2", det(doc3.clone(), 2)));
    cases.push(("d/max0", det(doc3.clone(), 0)));
    cases.push(("d/max99", det(doc3, 99)));

    cases
}

/// `--sv` 的信封装配（纯函数；`run_sv` 与 `--selftest` 共用同一处 —— 自检复现的就是真出口）。
fn render_sv_json(date: &str) -> String {
    let cases = sv_fixture_cases(date);
    let items: Vec<String> = cases
        .iter()
        .map(|(name, inp)| {
            let data = if name.starts_with("set/") {
                sv_settings(inp)
            } else if name.starts_with("st/") {
                sv_state(inp)
            } else if name.starts_with("a/") {
                sv_action(inp)
            } else {
                sv_detail(inp)
            };
            format!("{{\"name\":{},\"data\":{}}}", jstr(name), jv_to_json(&data))
        })
        .collect();
    format!(
        "{{\"ok\":true,\"why\":\"只吃内联夹具的四段监督纯判定（48 档）；这不是对方行为的读数\",\"contract\":{},\"role\":\"R2-supervisor\",\"fixture\":\"supervisor\",\"date\":{},\"count\":{},\"cases\":[{}]}}",
        jstr(CONTRACT),
        jstr(date),
        items.len(),
        items.join(",")
    )
}

/// `--sv` 那一档：同 `--events` 的出口纪律 —— 只吃内联夹具、不加载任何 DLL、不碰任何现场；
/// 日期戳由 `--date` 注入（不给=空串，那是"没注入"的显式读数，探针永不读时钟）。
fn run_sv(date: &str, out: &Option<String>) -> i32 {
    let json = render_sv_json(date);
    emit(&json, out);
    0
}

// ==================== verb: farm-intent（R2②：farm/intent 五段纯规则） ====================
// 转写自 tools\panel-cs\Orchestration.cs 的五段（parse-farm-target-spec / parse-ark-drop-text /
// resolve-ark-farm-plan / apply-ark-daily-intent / apply-r1999-daily-intent），零外部依赖 ⇒
// 八处正则全部手写等价扫描。\s ≡ char::is_whitespace；\d 按 ASCII 数字落（.NET 默认是 Unicode
// 数位面，差异面=全角数字这类非 ASCII 位，夹具不出现，注释在案）。对拍门在
// tools\rustcore\probe-farm-intent-rust-golden.ps1（三侧：旧 PS 真身 / 现树壳 / 本出口）。

/// 跳 `\s*`：从字节位置 `i`（须在 char 边界）起跳过空白，返回下一个非空白或串尾的位置。
fn fi_skip_ws(s: &str, mut i: usize) -> usize {
    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        if !c.is_whitespace() {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// `-split '[,，、;；\s]+'` 后去空串：中英逗号/顿号/分号与空白都是关卡分隔。
fn fi_split_stages(s: &str) -> Vec<String> {
    s.split(|c: char| c == ',' || c == '，' || c == '、' || c == ';' || c == '；' || c.is_whitespace())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

/// `^\d{3,8}$`：全 ASCII 数字且长度 3..=8（.NET 的 \d 是 Unicode 数位面，差异面见段头注）。
fn fi_is_digits3to8(s: &str) -> bool {
    s.len() >= 3 && s.len() <= 8 && s.bytes().all(|b| b.is_ascii_digit())
}

/// `int.Parse` 的夹具面替身：调用方已保证是 ASCII 数字串；超 i32::MAX 饱和（C# 那份超界会抛
/// OverflowException，夹具不出现超长串，饱和只是防御性落点）。
fn fi_parse_digits(s: &str) -> i32 {
    let mut n: i64 = 0;
    for b in s.bytes() {
        n = n * 10 + (b - b'0') as i64;
        if n > i32::MAX as i64 {
            return i32::MAX;
        }
    }
    n as i32
}

/// `int.TryParse(s, NumberStyles.Integer)` 的替身：前后空白、可选 +/-、全 ASCII 数字，须落进 i32。
fn fi_try_i32(v: &JVal) -> Option<i32> {
    let s = jv_str(v);
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (neg, digits) = if let Some(r) = t.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = t.strip_prefix('+') {
        (false, r)
    } else {
        (false, t)
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut n: i64 = 0;
    for b in digits.bytes() {
        n = n.checked_mul(10)?.checked_add((b - b'0') as i64)?;
    }
    i32::try_from(if neg { -n } else { n }).ok()
}

/// `Gd.Map` 的只读投影：对象 → Some(键值切片)（空对象也是对象）；其余 → None。
fn fi_as_map(v: &JVal) -> Option<&[(String, JVal)]> {
    match v {
        JVal::O(o) => Some(o.as_slice()),
        _ => None,
    }
}

/// 键查值（缺键 → Null，与 Gd.Get 同口径）。
fn fi_kv(o: &[(String, JVal)], k: &str) -> JVal {
    for (kk, vv) in o {
        if kk == k {
            return vv.clone();
        }
    }
    JVal::Null
}

/// `ContainsValue`（Add-Member 前的查重）：Gd.Str 等值。
fn fi_list_has(list: &[JVal], v: &JVal) -> bool {
    let s = jv_str(v);
    list.iter().any(|x| jv_str(x) == s)
}

/// `JoinObjects`：各元素 Gd.Str 后按 sep 拼。
fn fi_join(list: &[JVal], sep: &str) -> String {
    list.iter().map(jv_str).collect::<Vec<_>>().join(sep)
}

/// `Math.Round(x, 1)` 的替身（先乘 10 再银行家舍入——1.875 与 2.4 这两颗夹具值在 double 里
/// 乘 10 都精确，与 .NET 的十进制舍入等价；半分界行为见 round_half_even）。
fn fi_round1(f: f64) -> f64 {
    round_half_even(f * 10.0) / 10.0
}

/// `d.ToString("R")` 的串面：最短往返串（1.0 打成 "1"，与 .NET "R" 一致）。
fn fi_round_str(f: f64) -> String {
    format!("{}", f)
}

/// 主正则 `^(?<k>.+?)\s*[=＝]\s*(?<n>\d+)(?:\s*@\s*(?<s>.+))?$` 的手写等价扫描：
/// k 从左往右试长度（`.+?` 懒惰）；k 不跨 '\n'（`.` 默认不匹配换行）。返回值 `(Key, Target, Stages)`，
/// Stages=None=@ 分支未匹配（正则组 s 缺席）。t 已在调用方 Trim ⇒ `$` 的"串尾 \n 之前"分支不可达。
fn fi_spec_scan(t: &str) -> Option<(String, i32, Option<Vec<String>>)> {
    if t.is_empty() {
        return None;
    }
    let first_nl = t.find('\n').unwrap_or(t.len());
    let mut ends: Vec<usize> = Vec::new();
    for (i, _) in t.char_indices() {
        if i > 0 && i <= first_nl {
            ends.push(i);
        }
    }
    if first_nl == t.len() {
        ends.push(t.len());
    }
    for &e in &ends {
        let p = fi_skip_ws(t, e);
        let mut q = if t[p..].starts_with('=') {
            p + 1
        } else if t[p..].starts_with('＝') {
            p + '＝'.len_utf8()
        } else {
            continue;
        };
        q = fi_skip_ws(t, q);
        let bytes = t.as_bytes();
        let mut m = q;
        while m < t.len() && bytes[m].is_ascii_digit() {
            m += 1;
        }
        if m == q {
            continue;
        }
        let after = fi_skip_ws(t, m);
        let mut stages: Option<Vec<String>> = None;
        let mut matched = false;
        if after < t.len() && bytes[after] == b'@' {
            let q2 = fi_skip_ws(t, after + 1);
            if q2 < t.len() && !t[q2..].contains('\n') {
                stages = Some(fi_split_stages(&t[q2..]));
                matched = true;
            }
        }
        if !matched && fi_skip_ws(t, m) == t.len() {
            matched = true;
        }
        if !matched {
            continue;
        }
        return Some((t[..e].trim().to_string(), fi_parse_digits(&t[q..m]), stages));
    }
    None
}

/// `parse-farm-target-spec`：一行目标文本 → {Found;Spec?}。吃 "名字=数量[@关卡…]" 与 json 对象
/// {item/target/stages} 各写法；解析不动的给 Bad 说明（调用方汇总着说，不静默丢）。
fn fi_target_spec(raw: &JVal) -> JVal {
    if matches!(raw, JVal::Null) {
        return jv_o(vec![("Found", JVal::B(false))]);
    }
    if let JVal::S(s0) = raw {
        let t = s0.trim();
        if t.is_empty() {
            return jv_o(vec![("Found", JVal::B(false))]);
        }
        let (key, target, stages, bad) = match fi_spec_scan(t) {
            Some((k, n, st)) => (k, n, st, String::new()),
            None => (
                String::new(),
                0,
                None,
                format!("看不懂「{}」，要写成 材料名=数量[@关卡1,关卡2]", t),
            ),
        };
        let st = match stages {
            Some(v) => JVal::A(v.into_iter().map(JVal::S).collect()),
            None => JVal::A(Vec::new()),
        };
        return jv_o(vec![
            ("Found", JVal::B(true)),
            ("Spec", jv_o(vec![
                ("Key", JVal::S(key)),
                ("Target", JVal::I(target as i64)),
                ("Stages", st),
                ("Raw", JVal::S(t.to_string())),
                ("Bad", JVal::S(bad)),
            ])),
        ]);
    }
    let obj = match fi_as_map(raw) {
        Some(o) => o,
        None => return jv_o(vec![("Found", JVal::B(false))]),
    };
    let mut k = jv_str(&fi_kv(obj, "item"));
    if k.is_empty() {
        k = jv_str(&fi_kv(obj, "name"));
    }
    let mut n = fi_try_i32(&fi_kv(obj, "target")).unwrap_or(0);
    if n == 0 {
        n = fi_try_i32(&fi_kv(obj, "count")).unwrap_or(0);
    }
    let stages_v = fi_kv(obj, "stages");
    let mut s: Vec<JVal> = jv_list(&stages_v).into_iter().cloned().collect();
    if s.len() == 1 {
        if let JVal::S(ss) = &s[0] {
            s = fi_split_stages(ss).into_iter().map(JVal::S).collect();
        }
    }
    if k.is_empty() {
        return jv_o(vec![
            ("Found", JVal::B(true)),
            ("Spec", jv_o(vec![
                ("Key", JVal::S(String::new())),
                ("Target", JVal::I(0)),
                ("Stages", JVal::A(Vec::new())),
                ("Raw", JVal::S(jv_to_json(raw))),
                ("Bad", JVal::S("条目里没有材料名/代号(item)".to_string())),
            ])),
        ]);
    }
    if n <= 0 {
        return jv_o(vec![
            ("Found", JVal::B(true)),
            ("Spec", jv_o(vec![
                ("Key", JVal::S(k.clone())),
                ("Target", JVal::I(0)),
                ("Stages", JVal::A(s)),
                ("Raw", JVal::S(k.clone())),
                ("Bad", JVal::S(format!("{} 的目标数量没写或不是正整数", k))),
            ])),
        ]);
    }
    jv_o(vec![
        ("Found", JVal::B(true)),
        ("Spec", jv_o(vec![
            ("Key", JVal::S(k.clone())),
            ("Target", JVal::I(n as i64)),
            ("Stages", JVal::A(s)),
            ("Raw", JVal::S(format!("{}={}", k, n))),
            ("Bad", JVal::S(String::new())),
        ])),
    ])
}

/// `\r?\n` 断行：只按 '\n' 切、回溯剥行尾 '\r'。
fn fi_split_lines(t: &str) -> Vec<&str> {
    t.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect()
}

/// `^\[[^\]]*\]\s*` 剥前缀：以 '[' 开头且存在 ']' 时剥到首个 ']' 后并跳空白；否则原样。
fn fi_strip_bracket_prefix(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix('[') {
        if let Some(i) = rest.find(']') {
            let after = &rest[i + 1..];
            return &after[fi_skip_ws(after, 0)..];
        }
    }
    s
}

/// `^<\d+>\s*` 剥前缀：'<' + ASCII 数字（至少 1 位）+ '>' + 跳空白；否则原样。
fn fi_strip_angle(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix('<') {
        let b = rest.as_bytes();
        let mut i = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i > 0 && i < b.len() && b[i] == b'>' {
            let after = &rest[i + 1..];
            return &after[fi_skip_ws(after, 0)..];
        }
    }
    s
}

/// `([A-Za-z0-9][A-Za-z0-9\-]*)\s*掉落统计`：最左匹配，组1 = 关卡代号。
/// 正则里 `[A-Za-z0-9\-]*` 的回退在此不可达（组1 全 ASCII，回退一字符后 `\s*` 跳 0 个、
/// 下一字符仍是组1 字符，接不上"掉落统计"字面）⇒ 吃尽 + 跳空白即是全部候选。
fn fi_find_head(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let n = b.len();
    for pos in 0..n {
        if !b[pos].is_ascii_alphanumeric() {
            continue;
        }
        let mut e = pos + 1;
        while e < n && (b[e].is_ascii_alphanumeric() || b[e] == b'-') {
            e += 1;
        }
        let p = fi_skip_ws(s, e);
        if s[p..].starts_with("掉落统计") {
            return Some(s[pos..e].to_string());
        }
    }
    None
}

/// `([^\s:：,，、;()（）+]+)\s*[::]\s*(\d+)(?:\s*[（(]\s*\+(\d+)\s*[）)])?` 的手写扫描：
/// 返回 `(Name, Num, Delta)`（组3 成功=Delta 取解析值，否则 0）。组1 负集只含 ASCII ';'
/// ——全角 '；' 是合法名字字符（照抄 C# 负集）；空白按 char::is_whitespace 判。
fn fi_scan_pairs(s: &str) -> Vec<(String, i32, i32)> {
    let cs: Vec<char> = s.chars().collect();
    let n = cs.len();
    let is_neg = |c: char| {
        c.is_whitespace()
            || matches!(c, ':' | '：' | ',' | '，' | '、' | ';' | '(' | ')' | '（' | '）' | '+')
    };
    let mut out: Vec<(String, i32, i32)> = Vec::new();
    let mut i = 0usize;
    while i < n {
        if is_neg(cs[i]) {
            i += 1;
            continue;
        }
        let mut e1 = i + 1;
        while e1 < n && !is_neg(cs[e1]) {
            e1 += 1;
        }
        // 组1 回退不可达（组1 不含空白，回退一字符后 `\s*` 跳 0 个、下一字符仍是组1 字符，
        // 接不上 ':'）⇒ 吃尽 + 跳空白见冒号即是唯一候选。
        let mut g = e1;
        while g < n && cs[g].is_whitespace() {
            g += 1;
        }
        if g >= n || !(cs[g] == ':' || cs[g] == '：') {
            i += 1;
            continue;
        }
        let mut h = g + 1;
        while h < n && cs[h].is_whitespace() {
            h += 1;
        }
        if h >= n || !cs[h].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut m = h;
        while m < n && cs[m].is_ascii_digit() {
            m += 1;
        }
        let num1: String = cs[h..m].iter().collect();
        // 可选组整体匹配才取组3（`\d+` 回退到数字面恒接不上 ')'，与正则同）；否则 Delta=0、匹配尾=数字尾。
        let mut delta = 0i32;
        let mut next = m;
        let mut m2 = m;
        while m2 < n && cs[m2].is_whitespace() {
            m2 += 1;
        }
        if m2 < n && (cs[m2] == '(' || cs[m2] == '（') {
            let mut m3 = m2 + 1;
            while m3 < n && cs[m3].is_whitespace() {
                m3 += 1;
            }
            if m3 < n && cs[m3] == '+' {
                let mut m4 = m3 + 1;
                while m4 < n && cs[m4].is_whitespace() {
                    m4 += 1;
                }
                if m4 < n && cs[m4].is_ascii_digit() {
                    let mut m5 = m4;
                    while m5 < n && cs[m5].is_ascii_digit() {
                        m5 += 1;
                    }
                    let num3: String = cs[m4..m5].iter().collect();
                    let mut m6 = m5;
                    while m6 < n && cs[m6].is_whitespace() {
                        m6 += 1;
                    }
                    if m6 < n && (cs[m6] == ')' || cs[m6] == '）') {
                        delta = fi_parse_digits(&num3);
                        next = m6 + 1;
                    }
                }
            }
        }
        out.push((cs[i..e1].iter().collect(), fi_parse_digits(&num1), delta));
        i = next;
    }
    out
}

/// `parse-ark-drop-text`：gui.log 片段 → {Blocks:[{Stage;Items:[{Name;Delta}]}]}。
/// 只认 (+N) 增量；"当前次数/理智/用时"这类同名不同物的行靠拒绝表挡掉；块是连续的，
/// 出现不像配额的行就算块结束（后面的行不再入账）。
fn fi_drop_text(t: &str) -> JVal {
    const REJECTS: [&str; 6] = ["当前次数", "理智", "剩余理智", "用时", "预计", "累计理智"];
    let mut blocks: Vec<(String, Vec<JVal>)> = Vec::new();
    let mut cur: Option<usize> = None;
    if !t.trim().is_empty() {
        for raw in fi_split_lines(t) {
            let l0 = raw.trim();
            if l0.is_empty() {
                continue;
            }
            let line = fi_strip_angle(fi_strip_bracket_prefix(l0));
            if let Some(stage) = fi_find_head(line) {
                blocks.push((stage, Vec::new()));
                cur = Some(blocks.len() - 1);
                continue;
            }
            let idx = match cur {
                Some(x) => x,
                None => continue,
            };
            let pairs = fi_scan_pairs(line);
            if pairs.is_empty() {
                cur = None;
                continue;
            }
            for (nm, _num, delta) in pairs {
                if REJECTS.iter().any(|r| *r == nm) {
                    continue;
                }
                blocks[idx]
                    .1
                    .push(jv_o(vec![("Name", JVal::S(nm)), ("Delta", JVal::I(delta as i64))]));
            }
        }
    }
    let arr: Vec<JVal> = blocks
        .into_iter()
        .map(|(stage, items)| jv_o(vec![("Stage", JVal::S(stage)), ("Items", JVal::A(items))]))
        .collect();
    jv_o(vec![("Blocks", JVal::A(arr))])
}

/// 从可选 map 上取键（map 缺位或键缺 → Null）。
fn fi_map_kv(m: Option<&[(String, JVal)]>, k: &str) -> JVal {
    match m {
        Some(o) => fi_kv(o, k),
        None => JVal::Null,
    }
}

fn fi_plan_none(why: &str) -> JVal {
    jv_o(vec![
        ("Active", JVal::B(false)),
        ("Reason", JVal::S(why.to_string())),
        ("ItemId", JVal::S(String::new())),
        ("ItemName", JVal::S(String::new())),
        ("Have", JVal::I(0)),
        ("Target", JVal::I(0)),
        ("Gap", JVal::I(0)),
        ("Stages", JVal::A(Vec::new())),
        ("Times", JVal::I(0)),
    ])
}

/// `resolve-ark-farm-plan`：目标清单 + 账本 + 学习表 → 本轮补刷方案。
/// 一次只锁"还差最多"的一个目标；刷哪关：显式填的 > 学习表挑（平均每场掉最多的优先、
/// 并列取见得多的，最多 2 关）> 不刷并说为什么；次数=缺口（保守口径：每场至少 1 折算，
/// 凑满目标为止，封顶 MaxTimes）。输出键序逐字照抄旧 pscustomobject。
/// 宽容注：C# 那份对 nameToId/dropTable 等**缺键**会 NRE，壳侧保证不喂那种形状；
/// 这里按空 map/Null 宽容处理，夹具面两者不可分。
fn fi_plan(inp: &JVal) -> JVal {
    let o0 = fi_as_map(inp).unwrap_or(&[]);
    let targets_v = fi_kv(o0, "targets");
    let targets = jv_list(&targets_v);
    let n2i_v = fi_kv(o0, "nameToId");
    let name_to_id = fi_as_map(&n2i_v);
    let i2n_v = fi_kv(o0, "idToName");
    let id_to_name = fi_as_map(&i2n_v);
    let inv_v = fi_kv(o0, "inventory");
    let inventory = fi_as_map(&inv_v);
    let org_v = fi_kv(o0, "origin");
    let origin = fi_as_map(&org_v);
    let dt_v = fi_kv(o0, "dropTable");
    let drop_table = fi_as_map(&dt_v);
    let ks_v = fi_kv(o0, "knownStages");
    let known = fi_as_map(&ks_v);
    let mut max_times = jv_int(&fi_kv(o0, "maxTimes"), 12);
    if max_times < 1 {
        max_times = 1;
    }
    if targets.is_empty() {
        return fi_plan_none("没配 ark.farmTargets → 材料补刷未启动");
    }

    let mut notes: Vec<String> = Vec::new();
    let mut best: Option<JVal> = None;
    let mut best_gap: i64 = 0;
    for t0 in targets {
        let t = match fi_as_map(t0) {
            Some(m) => m,
            None => continue,
        };
        let bad = jv_str(&fi_kv(t, "Bad"));
        if !bad.is_empty() {
            notes.push(format!("跳过「{}」：{}", jv_str(&fi_kv(t, "Raw")), bad));
            continue;
        }
        let mut nm = jv_str(&fi_kv(t, "Key"));
        let key = nm.clone();
        let mut id = String::new();
        if fi_is_digits3to8(&key) {
            id = key.clone();
            if let Some(m) = id_to_name {
                if m.iter().any(|(k, _)| *k == id) {
                    nm = jv_str(&fi_kv(m, &id));
                }
            }
        } else if let Some(m) = name_to_id {
            if m.iter().any(|(k, _)| *k == key) {
                id = jv_str(&fi_kv(m, &key));
            }
        }
        if id.is_empty() {
            notes.push(format!(
                "跳过「{}」：材料名在 MAA 材料表(item_index)里找不到",
                jv_str(&fi_kv(t, "Raw"))
            ));
            continue;
        }
        let have = if let Some(m) = inventory {
            if m.iter().any(|(k, _)| *k == id) {
                jv_int(&fi_kv(m, &id), 0)
            } else {
                0
            }
        } else {
            0
        };
        let target_n = jv_int(&fi_kv(t, "Target"), 0);
        let gap = target_n - have;
        if gap <= 0 {
            notes.push(format!("{} 已够({}/{})", nm, have, jv_str(&fi_kv(t, "Target"))));
            continue;
        }

        let stages_v = fi_kv(t, "Stages");
        // 保真保留元素原形（10-05 twotgt 实锤：stages 里可能挂着嵌套数组这种畸形形状，
        // C# 把原对象一路带到 stOk/输出；串化只在比较与 Reason 处按 Gd.Str 现做）。
        let mut stages: Vec<JVal> = jv_list(&stages_v).into_iter().cloned().collect();
        if stages.is_empty() {
            // 没填关卡 → 从学习表挑：平均每场掉得最多的优先，并列取见得多的；最多挑 2 关
            let mut learned: Vec<(String, f64, i64)> = Vec::new();
            let by_stage_v = fi_map_kv(drop_table, &id);
            if let Some(by_stage) = fi_as_map(&by_stage_v) {
                for (stage_key, rec) in by_stage {
                    let runs = jv_int(&fi_map_kv(fi_as_map(rec), "runs"), 0);
                    let qty = jv_int(&fi_map_kv(fi_as_map(rec), "qty"), 0);
                    if runs > 0 && qty > 0 {
                        learned.push((stage_key.clone(), qty as f64 / runs as f64, runs));
                    }
                }
            }
            // OrderByDescending(Avg).ThenByDescending(Runs) 是稳定排序（相等保枚举序）；Take(2)
            learned.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.2.cmp(&a.2))
            });
            learned.truncate(2);
            stages = learned.into_iter().map(|(s, _, _)| JVal::S(s)).collect();
            if stages.is_empty() {
                notes.push(format!(
                    "{} 还差 {}，但不知道该刷哪关(清单里没填、掉落账本也没教过) → 把它写成 {}@CE-5 这种带关卡的形式",
                    nm,
                    gap,
                    jv_str(&fi_kv(t, "Raw"))
                ));
                continue;
            }
        }
        let mut bad_st: Vec<JVal> = Vec::new();
        if let Some(ks) = known {
            if !ks.is_empty() {
                for st in &stages {
                    // C#：knownStages.ContainsKey(Gd.Str(st)) —— 键比较走 GD.Str 串口径
                    if !ks.iter().any(|(k, _)| k == &jv_str(st)) {
                        bad_st.push(st.clone());
                    }
                }
            }
        }
        // C#：ContainsValue(badSt, st) —— 同样 Gd.Str 等值；stOk 保留**原对象**（不重塑形）
        let st_ok: Vec<JVal> = stages
            .iter()
            .filter(|st| !bad_st.iter().any(|b| jv_str(b) == jv_str(st)))
            .cloned()
            .collect();
        if st_ok.is_empty() {
            notes.push(format!(
                "{} 还差 {}，但关卡 {} 不被 MAA 关卡表认识 → 先更 MAA 资源或改 farmTargets",
                nm,
                gap,
                fi_join(&bad_st, "/")
            ));
            continue;
        }

        // 每场平均掉落：挑关仍按它（掉得多的打头），Reason 里如实报数；但次数不再用它折算
        let mut tq: i64 = 0;
        let mut tr: i64 = 0;
        let by_id_v = fi_map_kv(drop_table, &id);
        if let Some(by_stage) = fi_as_map(&by_id_v) {
            for st in &st_ok {
                let rec = fi_kv(by_stage, &jv_str(st));
                if !matches!(rec, JVal::Null) {
                    tq += jv_int(&fi_map_kv(fi_as_map(&rec), "qty"), 0);
                    tr += jv_int(&fi_map_kv(fi_as_map(&rec), "runs"), 0);
                }
            }
        }
        let mut avg: f64 = 1.0;
        let mut avg_note = "掉落率未知，按每场 1 估".to_string();
        if tr > 0 {
            avg = ((tq as f64) / (tr as f64)).max(0.1);
            avg_note = format!(
                "按每场掉 1 保守给次数、凑满目标为止(学习到的每场约 {})",
                fi_round_str(fi_round1(avg))
            );
        }
        let mut times = gap;
        let mut cap_note = String::new();
        if times > max_times {
            times = max_times;
            cap_note = format!("(单轮封顶 {}，剩下的明天接着刷)", max_times);
        }

        let cand = jv_o(vec![
            ("Active", JVal::B(true)),
            ("ItemId", JVal::S(id.clone())),
            ("ItemName", JVal::S(nm.clone())),
            ("Have", JVal::I(have)),
            ("Target", JVal::I(target_n)),
            ("Gap", JVal::I(gap)),
            ("Stages", JVal::A(st_ok.clone())),
            ("Times", JVal::I(times)),
            ("Avg", JVal::F(avg)),
            ("AvgNote", JVal::S(avg_note)),
            ("CapNote", JVal::S(cap_note)),
            ("BadStages", JVal::A(bad_st.clone())),
        ]);
        if best.is_none() || gap > best_gap {
            best_gap = gap;
            best = Some(cand);
        }
    }
    if best.is_none() {
        let mut why = "本轮没有要补刷的材料".to_string();
        if !notes.is_empty() {
            why = format!("本轮没有要补刷的材料：{}", notes.join("；"));
        }
        return fi_plan_none(&why);
    }
    let best = best.unwrap();
    let bm = fi_as_map(&best).unwrap();
    let b_id = jv_str(&fi_kv(bm, "ItemId"));
    let origin_note = if jv_str(&fi_map_kv(origin, &b_id)) == "depot" {
        "(现值来自库存缓存标定，是估算)"
    } else {
        ""
    };
    let bad_note = {
        let bv = fi_kv(bm, "BadStages");
        let list: Vec<String> = jv_list(&bv).iter().map(|v| jv_str(*v)).collect();
        if list.is_empty() {
            String::new()
        } else {
            format!("；已剔除不认识的关卡 {}", list.join("/"))
        }
    };
    let stages_str = {
        let sv = fi_kv(bm, "Stages");
        let list: Vec<String> = jv_list(&sv).iter().map(|v| jv_str(*v)).collect();
        list.join(",")
    };
    let reason = format!(
        "补刷 {}×{} @{}：目标 {}，现有 {}{}，还差 {}，{}{}{}",
        jv_str(&fi_kv(bm, "ItemName")),
        jv_str(&fi_kv(bm, "Times")),
        stages_str,
        jv_str(&fi_kv(bm, "Target")),
        jv_str(&fi_kv(bm, "Have")),
        origin_note,
        jv_str(&fi_kv(bm, "Gap")),
        jv_str(&fi_kv(bm, "AvgNote")),
        jv_str(&fi_kv(bm, "CapNote")),
        bad_note
    );
    jv_o(vec![
        ("Active", JVal::B(true)),
        ("ItemId", JVal::S(b_id)),
        ("ItemName", fi_kv(bm, "ItemName")),
        ("Have", fi_kv(bm, "Have")),
        ("Target", fi_kv(bm, "Target")),
        ("Gap", fi_kv(bm, "Gap")),
        ("Stages", fi_kv(bm, "Stages")),
        ("Times", fi_kv(bm, "Times")),
        ("Reason", JVal::S(reason)),
        ("Notes", JVal::A(notes.iter().map(|s| JVal::S(s.clone())).collect())),
    ])
}

/// 键值字面 → owned 键值表（键序=书写序）。
fn fi_kvv(kv: Vec<(&str, JVal)>) -> Vec<(String, JVal)> {
    kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// `Overwrite`（Add-Member -Force 同语义）：旧键原位覆盖、新键排末尾。
fn fi_overwrite(src: &[(String, JVal)], kv: Vec<(&str, JVal)>) -> Vec<(String, JVal)> {
    let mut o: Vec<(String, JVal)> = src.to_vec();
    for (k, v) in kv {
        if let Some(pos) = o.iter().position(|(kk, _)| kk == k) {
            o[pos].1 = v;
        } else {
            o.push((k.to_string(), v));
        }
    }
    o
}

/// `apply-ark-daily-intent`（原 Resolve-GdArkDecision 的构造段）：命令行意图 → 本轮方舟方案。
/// 五格 {Ark;FarmMode;HasFarmFields;FarmSet;Warnings}；FarmSet=本次场景给了可补方案
/// （farmOpt 非空，旧门 Farm=SET/NULL 同一件事）；Warnings 由门侧拼两段调用的话术。
fn fi_ark_intent(inp: &JVal) -> JVal {
    let o0 = fi_as_map(inp).unwrap_or(&[]);
    let ark_v = fi_kv(o0, "ark");
    let mut ark: Vec<(String, JVal)> = fi_as_map(&ark_v).map(|m| m.to_vec()).unwrap_or_default();
    let stage = jv_str(&fi_kv(o0, "stage"));
    let force = jv_truthy(&fi_kv(o0, "force"));
    let farm = jv_truthy(&fi_kv(o0, "farm"));
    let farm_opt = fi_kv(o0, "farmOpt");
    let mut warnings: Vec<JVal> = Vec::new();

    if !stage.is_empty() {
        ark = fi_kvv(vec![
            ("Mode", JVal::S("manual".to_string())),
            ("Stage", JVal::S(stage.clone())),
            ("StageHint", JVal::S(stage.clone())),
            ("ClearSanity", JVal::B(true)),
            ("Reason", JVal::S(format!("命令行指定刷 {}", stage))),
            ("EventKey", JVal::Null),
            ("EventTip", JVal::Null),
            ("Source", JVal::S("manual".to_string())),
        ]);
    }
    if force && jv_str(&fi_kv(&ark, "Mode")) == "event-pending" {
        let hint = jv_str(&fi_kv(&ark, "StageHint"));
        ark = fi_kvv(vec![
            ("Mode", JVal::S("event".to_string())),
            ("Stage", JVal::S(hint.clone())),
            ("StageHint", JVal::S(hint.clone())),
            ("ClearSanity", JVal::B(true)),
            ("Reason", JVal::S(format!("(-Force) 不管今天的自证结论(进得去与否)，强行刷 {}", hint))),
            ("EventKey", fi_kv(&ark, "EventKey")),
            ("EventTip", fi_kv(&ark, "EventTip")),
            ("Source", fi_kv(&ark, "Source")),
        ]);
        warnings.push(JVal::S("-Force：本轮忽略活动图自证结论".to_string()));
    }

    let (farm_stages, farm_reason): (Vec<JVal>, String) = if matches!(farm_opt, JVal::Null) {
        (Vec::new(), String::new())
    } else {
        let sv = fi_map_kv(fi_as_map(&farm_opt), "Stages");
        let st: Vec<JVal> = jv_list(&sv).iter().map(|v| (*v).clone()).collect();
        (st, jv_str(&fi_map_kv(fi_as_map(&farm_opt), "Reason")))
    };
    let mut farm_mode = String::new();
    let mut has_farm_fields = false;

    if farm {
        has_farm_fields = true;
        if !matches!(farm_opt, JVal::Null) {
            farm_mode = "dedicated".to_string();
            let joined = fi_join(&farm_stages, ",");
            ark = fi_kvv(vec![
                ("Mode", JVal::S("farm".to_string())),
                ("Stage", JVal::S(joined.clone())),
                ("StageHint", JVal::S(joined.clone())),
                ("ClearSanity", JVal::B(true)),
                ("Reason", JVal::S(format!("材料补刷专用轮 — {}", farm_reason))),
                ("EventKey", fi_kv(&ark, "EventKey")),
                ("EventTip", fi_kv(&ark, "EventTip")),
                ("Source", fi_kv(&ark, "Source")),
            ]);
        } else {
            warnings.push(JVal::S("-Farm 本轮没有可补的材料 → 按原方案跑".to_string()));
        }
    } else if !matches!(farm_opt, JVal::Null) {
        has_farm_fields = true;
        farm_mode = "append".to_string();
        // 需求优先档：farm 关排最前（去重），长草关在后（也去重）
        let mut merged: Vec<JVal> = Vec::new();
        for st in &farm_stages {
            if !fi_list_has(&merged, st) {
                merged.push(st.clone());
            }
        }
        for st in fi_split_stages(&jv_str(&fi_kv(&ark, "Stage"))) {
            let stv = JVal::S(st);
            if !fi_list_has(&merged, &stv) {
                merged.push(stv);
            }
        }
        let reason_new = format!(
            "{}；需求优先 {}",
            jv_str(&fi_kv(&ark, "Reason")),
            farm_reason
        );
        ark = fi_overwrite(
            &ark,
            vec![
                ("Stage", JVal::S(fi_join(&merged, ","))),
                ("Reason", JVal::S(reason_new)),
            ],
        );
    }

    jv_o(vec![
        ("Ark", JVal::O(ark)),
        ("FarmMode", JVal::S(farm_mode)),
        ("HasFarmFields", JVal::B(has_farm_fields)),
        ("FarmSet", JVal::B(!matches!(farm_opt, JVal::Null))),
        ("Warnings", JVal::A(warnings)),
    ])
}

/// `apply-r1999-daily-intent`：-Force 跳确认门与 -Plan 手动指定两段构造（Plan≠auto 无条件
/// 覆盖 Force 那段的结果）→ {Decision;Warnings}。
fn fi_r1999_intent(inp: &JVal) -> JVal {
    let o0 = fi_as_map(inp).unwrap_or(&[]);
    let d_v = fi_kv(o0, "decision");
    let mut d: Vec<(String, JVal)> = fi_as_map(&d_v).map(|m| m.to_vec()).unwrap_or_default();
    let plan = jv_str(&fi_kv(o0, "plan"));
    let force = jv_truthy(&fi_kv(o0, "force"));
    let mut warnings: Vec<JVal> = Vec::new();

    if force && jv_str(&fi_kv(&d, "Mode")) == "event-pending" {
        d = fi_kvv(vec![
            ("Mode", JVal::S("event".to_string())),
            ("Instance", JVal::S("event".to_string())),
            ("Version", fi_kv(&d, "Version")),
            ("VersionName", fi_kv(&d, "VersionName")),
            ("EventKey", fi_kv(&d, "EventKey")),
            ("EventTip", fi_kv(&d, "EventTip")),
            ("Reason", JVal::S("(-Force) 跳过 1999 活动确认门，直接用活动方案".to_string())),
            ("DataAge", fi_kv(&d, "DataAge")),
        ]);
        warnings.push(JVal::S("-Force：1999 本轮忽略活动确认门".to_string()));
    }
    if plan != "auto" {
        d = fi_kvv(vec![
            ("Mode", JVal::S(plan.clone())),
            ("Instance", JVal::S(plan.clone())),
            ("Version", JVal::S("manual".to_string())),
            ("VersionName", JVal::S("命令行指定".to_string())),
            ("EventKey", JVal::Null),
            ("EventTip", JVal::Null),
            ("Reason", JVal::S(format!("命令行指定 {}", plan))),
            ("DataAge", JVal::Null),
        ]);
    }
    jv_o(vec![
        ("Decision", JVal::O(d)),
        ("Warnings", JVal::A(warnings)),
    ])
}

/// 门用 ark 场景的两步调用（旧序刻意保留：先 manual/force 一步，再拿变换后的 ark 过 farm 档
/// 一步），farmOpt 由场景注入；Warnings=两段依序合计。
fn fi_ark_case(inp: &JVal) -> JVal {
    let o0 = fi_as_map(inp).unwrap_or(&[]);
    let ark0 = fi_kv(o0, "ark");
    let stage = fi_kv(o0, "stage");
    let force = fi_kv(o0, "force");
    let farm = fi_kv(o0, "farm");
    let farm_opt = fi_kv(o0, "farmOpt");

    let d1 = fi_ark_intent(&jv_o(vec![
        ("ark", ark0),
        ("stage", stage),
        ("force", force),
        ("farm", JVal::B(false)),
        ("farmOpt", JVal::Null),
    ]));
    let ark1 = fi_kv(fi_as_map(&d1).unwrap_or(&[]), "Ark");
    let d2 = fi_ark_intent(&jv_o(vec![
        ("ark", ark1),
        ("stage", JVal::S(String::new())),
        ("force", JVal::B(false)),
        ("farm", farm),
        ("farmOpt", farm_opt.clone()),
    ]));
    let d2m = fi_as_map(&d2).unwrap_or(&[]);
    let mut w: Vec<JVal> = Vec::new();
    for src in [&d1, &d2] {
        let sm = fi_as_map(src).unwrap_or(&[]);
        let wv = fi_kv(sm, "Warnings");
        for x in jv_list(&wv) {
            w.push(x.clone());
        }
    }
    jv_o(vec![
        ("Ark", fi_kv(d2m, "Ark")),
        ("FarmMode", fi_kv(d2m, "FarmMode")),
        ("HasFarmFields", fi_kv(d2m, "HasFarmFields")),
        ("FarmSet", JVal::B(!matches!(farm_opt, JVal::Null))),
        ("Warnings", JVal::A(w)),
    ])
}

/// 46 档夹具（名册顺序与旧门 probe-orch-golden.ps1 的 Run 顺序逐字对齐）：
/// spec 13 / drop 6 / plan 14 / ark 9 / r19 4。输入值照旧门字节（ark 用旧门版 IdleArk
/// Stage='CE-5,LS-5,CE-5' 与 PendArk）——自检另有 C# SelfTest 版输入，两版勿混。
fn fi_fixture_cases() -> Vec<(&'static str, JVal)> {
    let n2i = || {
        jv_o(vec![
            ("D32钢", JVal::S("30135".to_string())),
            ("或铝石", JVal::S("30115".to_string())),
        ])
    };
    let i2n = || {
        jv_o(vec![
            ("30135", JVal::S("D32钢".to_string())),
            ("30115", JVal::S("或铝石".to_string())),
        ])
    };
    let known = || jv_o(vec![("CE-5", JVal::B(true)), ("CE-4", JVal::B(true))]);
    let dt = || {
        jv_o(vec![(
            "30135",
            jv_o(vec![
                ("CE-5", jv_o(vec![("runs", JVal::I(5)), ("qty", JVal::I(12))])),
                ("CE-4", jv_o(vec![("runs", JVal::I(3)), ("qty", JVal::I(3))])),
            ]),
        )])
    };
    let ft = |key: &str, target: i64, stages: Vec<&str>| -> JVal {
        jv_o(vec![
            ("Key", JVal::S(key.to_string())),
            ("Target", JVal::I(target)),
            ("Stages", JVal::A(stages.into_iter().map(|s| JVal::S(s.to_string())).collect())),
            ("Raw", JVal::S(format!("{}={}", key, target))),
            ("Bad", JVal::S(String::new())),
        ])
    };
    let plan_in = |targets: Vec<JVal>, inv: JVal, org: JVal, dtv: JVal, ksv: JVal, mt: i64| -> JVal {
        jv_o(vec![
            ("targets", JVal::A(targets)),
            ("nameToId", n2i()),
            ("idToName", i2n()),
            ("inventory", inv),
            ("origin", org),
            ("dropTable", dtv),
            ("knownStages", ksv),
            ("maxTimes", JVal::I(mt)),
        ])
    };
    let inv_of = |n: i64| jv_o(vec![("30135", JVal::I(n))]);
    let raw = |s: &str| jv_o(vec![("raw", JVal::S(s.to_string()))]);
    let text = |s: &str| jv_o(vec![("text", JVal::S(s.to_string()))]);
    let idle = || {
        jv_o(vec![
            ("Mode", JVal::S("idle".to_string())),
            ("Stage", JVal::S("CE-5,LS-5,CE-5".to_string())),
            ("StageHint", JVal::S("CE-5,LS-5".to_string())),
            ("ClearSanity", JVal::B(true)),
            ("Reason", JVal::S("长草".to_string())),
            ("EventKey", JVal::Null),
            ("EventTip", JVal::Null),
            ("Source", JVal::S("weekday".to_string())),
        ])
    };
    let pend = || {
        jv_o(vec![
            ("Mode", JVal::S("event-pending".to_string())),
            ("Stage", JVal::Null),
            ("StageHint", JVal::S("CT-1".to_string())),
            ("ClearSanity", JVal::B(false)),
            ("Reason", JVal::S("等新活动确认".to_string())),
            ("EventKey", JVal::S("act1".to_string())),
            ("EventTip", JVal::S("sidestory".to_string())),
            ("Source", JVal::S("event".to_string())),
        ])
    };
    let fo1 = || {
        jv_o(vec![
            ("Stages", JVal::A(vec![JVal::S("PR-D-2".to_string()), JVal::S("CE-5".to_string())])),
            ("Reason", JVal::S("补刷 特种芯片组×8 @PR-D-2".to_string())),
        ])
    };
    let fo2 = || {
        jv_o(vec![
            ("Stages", JVal::A(vec![JVal::S("PR-D-2".to_string())])),
            ("Reason", JVal::S("补刷 D32钢×5 @CE-5".to_string())),
        ])
    };
    let fcase = |ark0: JVal, stage: &str, force: bool, farm: bool, fo: JVal| -> JVal {
        jv_o(vec![
            ("ark", ark0),
            ("stage", JVal::S(stage.to_string())),
            ("force", JVal::B(force)),
            ("farm", JVal::B(farm)),
            ("farmOpt", fo),
        ])
    };
    let pendr = || {
        jv_o(vec![
            ("Mode", JVal::S("event-pending".to_string())),
            ("Instance", JVal::S("idle".to_string())),
            ("Version", JVal::S("v4.7.1".to_string())),
            ("VersionName", JVal::S("测试版本".to_string())),
            ("EventKey", JVal::S("act123".to_string())),
            ("EventTip", JVal::S("活动".to_string())),
            ("Reason", JVal::S("待确认".to_string())),
            ("DataAge", JVal::Null),
        ])
    };
    let rcase = |plan: &str, force: bool| -> JVal {
        jv_o(vec![
            ("decision", pendr()),
            ("plan", JVal::S(plan.to_string())),
            ("force", JVal::B(force)),
        ])
    };
    vec![
        // ---- spec 13 ----
        ("spec/plain", raw("D32钢=30@CE-5")),
        ("spec/code", raw("30135=12@CE-5,CE-4")),
        ("spec/seps", raw("30135=12@CE-5、CE-4；LS-5")),
        ("spec/fullwidth", raw("D32钢＝30 @ CE-5")),
        ("spec/nostage", raw("D32钢=30")),
        ("spec/obj", jv_o(vec![(
            "raw",
            jv_o(vec![
                ("item", JVal::S("D32钢".to_string())),
                ("target", JVal::I(20)),
                ("stages", JVal::A(vec![JVal::S("CE-5".to_string())])),
            ]),
        )])),
        ("spec/obj-str", jv_o(vec![(
            "raw",
            jv_o(vec![
                ("item", JVal::S("D32钢".to_string())),
                ("target", JVal::I(20)),
                ("stages", JVal::A(vec![JVal::S("CE-5，CE-4".to_string())])),
            ]),
        )])),
        ("spec/obj-alt", jv_o(vec![(
            "raw",
            jv_o(vec![
                ("name", JVal::S("或铝石".to_string())),
                ("count", JVal::I(7)),
            ]),
        )])),
        ("spec/obj-badn", jv_o(vec![(
            "raw",
            jv_o(vec![("item", JVal::S("D32钢".to_string()))]),
        )])),
        ("spec/obj-nok", jv_o(vec![("raw", jv_o(vec![("target", JVal::I(5))]))])),
        ("spec/garbage", raw("想刷点火腿")),
        ("spec/null", jv_o(vec![("raw", JVal::Null)])),
        ("spec/blank", raw("   ")),
        // ---- drop 6 ----
        // prefix/nohead/nostage 三档复刻"子进程实收单段"：块里 `-Text '头行' + "`n" + '对'` 没加括号，
        // 裸 `+` 拼接在 PS 参数模式里只把第一段绑进 -Text、其余 token 坠入 $args 被丢（10-05
        // comma-probe 双引擎实锤：`T1 -Text 'x1' + 'x2' + 'x3'` → Text=[x1]、argN=4）。
        ("drop/two", text("[08:50:26] CE-5 掉落统计:\nD32钢 : 2 (+2)   或铝石 : 1 (+1)   当前次数 : 3\n[08:52:10] LS-5 掉落统计:\n采购凭证 : 30 (30)")),
        ("drop/prefix", text("<12> PR-B-2 掉落统计:")),
        ("drop/stop", text("CE-4 掉落统计:\nD32钢 : 1 (+1)\n这是一句普通说明\n30135 : 9 (+9)")),
        ("drop/nohead", text("掉落统计:")),
        ("drop/empty", text("")),
        ("drop/nostage", text("CE5X 掉落统计:")),
        // ---- plan 14 ----
        ("plan/none", jv_o(vec![("targets", JVal::A(Vec::new()))])),
        ("plan/basic", plan_in(vec![ft("D32钢", 30, vec!["CE-5"])], inv_of(18), JVal::O(Vec::new()), dt(), known(), 12)),
        ("plan/cap", plan_in(vec![ft("D32钢", 99, vec!["CE-5"])], JVal::O(Vec::new()), JVal::O(Vec::new()), dt(), known(), 4)),
        ("plan/enough", plan_in(vec![ft("D32钢", 10, vec!["CE-5"])], inv_of(12), JVal::O(Vec::new()), dt(), known(), 12)),
        ("plan/badst", plan_in(vec![ft("D32钢", 30, vec!["CE-9"])], JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), known(), 12)),
        ("plan/unknm", plan_in(vec![ft("点火腿", 5, vec![])], JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), 12)),
        ("plan/nowhere", plan_in(vec![ft("D32钢", 5, vec![])], JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), 12)),
        ("plan/depot", plan_in(vec![ft("D32钢", 8, vec!["CE-5"])], inv_of(4), jv_o(vec![("30135", JVal::S("depot".to_string()))]), JVal::O(Vec::new()), JVal::O(Vec::new()), 12)),
        ("plan/twotgt", plan_in(
            // 单目标畸形形：块里写的是 `@(Ft 'D32钢' 20 @('CE-5'), Ft '或铝石' 100 @('CE-4'))`，
            // 逗号把 `@('CE-5')` 与裸词 `Ft` 粘成 Stages 的第 2 元素、余下三枚 token 坠入 $args 被丢
            // ——第二个目标「或铝石」从未进入调用（10-05 comma-probe 双引擎实锤：stagesN=2、
            // E0=[System.Object[]]、E1=[Ft]；甲侧 S/R 两段都印 System.Object[],Ft）。逐字复刻实收形状。
            vec![jv_o(vec![
                ("Key", JVal::S("D32钢".to_string())),
                ("Target", JVal::I(20)),
                ("Stages", JVal::A(vec![
                    JVal::A(vec![JVal::S("CE-5".to_string())]),
                    JVal::S("Ft".to_string()),
                ])),
                ("Raw", JVal::S("D32钢=20".to_string())),
                ("Bad", JVal::S(String::new())),
            ])],
            jv_o(vec![("30135", JVal::I(18)), ("30115", JVal::I(90))]),
            JVal::O(Vec::new()),
            JVal::O(Vec::new()),
            JVal::O(Vec::new()),
            12,
        )),
        ("plan/learn", plan_in(vec![ft("D32钢", 30, vec![])], inv_of(18), JVal::O(Vec::new()), dt(), known(), 12)),
        ("plan/code", plan_in(vec![ft("30115", 20, vec!["CE-4"])], JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), 12)),
        ("plan/badnote", plan_in(
            vec![
                jv_o(vec![
                    ("Key", JVal::S("点火腿".to_string())),
                    ("Target", JVal::I(0)),
                    ("Stages", JVal::A(Vec::new())),
                    ("Raw", JVal::S("想刷点火腿".to_string())),
                    ("Bad", JVal::S("看不懂「想刷点火腿」，要写成 材料名=数量[@关卡1,关卡2]".to_string())),
                ]),
                ft("D32钢", 20, vec!["CE-5"]),
            ],
            inv_of(18),
            JVal::O(Vec::new()),
            JVal::O(Vec::new()),
            JVal::O(Vec::new()),
            12,
        )),
        ("plan/zerot", plan_in(vec![ft("D32钢", 3, vec!["CE-5"])], JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), JVal::O(Vec::new()), 0)),
        ("plan/mixst", plan_in(vec![ft("D32钢", 12, vec!["CE-5", "CE-9"])], inv_of(18), JVal::O(Vec::new()), dt(), known(), 12)),
        // ---- ark 9 ----
        ("ark/plain", fcase(idle(), "", false, false, JVal::Null)),
        ("ark/manual", fcase(pend(), "OF-1", false, false, JVal::Null)),
        ("ark/force", fcase(pend(), "", true, false, JVal::Null)),
        ("ark/force-idle", fcase(idle(), "", true, false, JVal::Null)),
        ("ark/farm-ded", fcase(idle(), "", false, true, fo1())),
        ("ark/farm-none", fcase(idle(), "", false, true, JVal::Null)),
        ("ark/append", fcase(idle(), "", false, false, fo1())),
        ("ark/manual-farm", fcase(idle(), "OF-1", false, false, fo2())),
        ("ark/force-farm", fcase(pend(), "", true, false, fo2())),
        // ---- r19 4 ----
        ("r19/plain", rcase("auto", false)),
        ("r19/force", rcase("auto", true)),
        ("r19/plan", rcase("idle", false)),
        ("r19/both", rcase("idle", true)),
    ]
}

/// `--farm-intent` 的信封装配（纯函数；`run_farm_intent` 与 `--selftest` 共用同一处 —— 自检
/// 复现的就是真出口）。**无 date 格**：这档不吃时钟，对应门也不做日期注入。
fn render_farm_intent_json() -> String {
    let cases = fi_fixture_cases();
    let items: Vec<String> = cases
        .iter()
        .map(|(name, inp)| {
            let data = if name.starts_with("spec/") {
                // 夹具的 body 是 {raw:<条目>} 包装（与 drop 的 {text:…} 同款）；壳吃的是**条目本身**
                // ——这里剥一层再进纯判定（10-05 三侧门首跑抓到的 13 档 spec 全红就是这个根因：
                // 旧码把整包 body 当条目，于是 R= 打印出 {"raw":…}、Bad=「没有材料名/代号」）。
                let rv = fi_kv(fi_as_map(inp).unwrap_or(&[]), "raw");
                fi_target_spec(&rv)
            } else if name.starts_with("drop/") {
                let t = fi_kv(fi_as_map(inp).unwrap_or(&[]), "text");
                fi_drop_text(&jv_str(&t))
            } else if name.starts_with("plan/") {
                fi_plan(inp)
            } else if name.starts_with("ark/") {
                fi_ark_case(inp)
            } else if name.starts_with("r19/") {
                fi_r1999_intent(inp)
            } else {
                JVal::Null // 名册由自检钉死五族，兜底不可达
            };
            format!("{{\"name\":{},\"data\":{}}}", jstr(name), jv_to_json(&data))
        })
        .collect();
    format!(
        "{{\"ok\":true,\"why\":\"只吃内联夹具的 farm/intent 五段纯判定（46 档）；这不是对方行为的读数\",\"contract\":{},\"role\":\"R2-farm-intent\",\"fixture\":\"farm-intent\",\"count\":{},\"cases\":[{}]}}",
        jstr(CONTRACT),
        items.len(),
        items.join(",")
    )
}

/// `--farm-intent` 那一档：同 `--events`/`--sv` 的出口纪律 —— 只吃内联夹具、不加载任何 DLL、
/// 不碰任何现场；这档与时钟无关（信封不带日期格，探针永不读时钟）。
fn run_farm_intent(out: &Option<String>) -> i32 {
    let json = render_farm_intent_json();
    emit(&json, out);
    0
}

// ==================== ark 认关止损链装配回放（R3②：Start-GdArkDaily 整函数转写） ====================
// 转写源 = lib\ark-daily.ps1 的 Start-GdArkDaily（基线 251bd3c 的 106-255；B 档观测采集段落码后
// 该函数未再动，且此时壳已是"回放日志 + 落地活"的批 8 形态）。逐句转写、文案逐字，谁都不许在
// 这一侧"顺手化简"（§7 同一条纪律：对拍红了回修这里，禁止改期望）。
//
// 活的判定不在本段：重试分流的跳过清单与两句文案住在 panel-cs.exe（Orchestration.cs 的
// decide-daily-retry）——本档按生产同一条协议**真跑它**（--in/--out 信封，经 --panel-cs 注入路径，
// 就是 Invoke-GdCli 的那套校验：verb / 退出码 / ok 三关）。"判定不重编第二份"在这一刀里靠的就是这个。
// 其余外部调用（接管/写配置/拉端游/跑 MAA/认关读数/认栽标记/记账）都在桩世界里回放：
// 与 tools\probe-live-golden.ps1 第六路同一套桩口径 —— 调用序与入参进 seq、队列消费、固定返回、
// Test-GdAdmin 恒 false。8 档场景与 live 门第六路同源同量（arksk-1..8）。
//
// 对拍门在 tools\rustcore\probe-arkskip-rust-golden.ps1（三侧：旧 PS 真身转写 / 现树直调 / 本出口）。

/// PS 的 `-split '[,，、;；\s]+'` 等价：按半角/全角逗号、顿号、分号与空白切分，滤掉空段
/// （正则 `+` 合并连续分隔符，两端空段由调用处的 `if ($st …)` 滤除 —— 滤空后直接等价）。
fn arksk_split_stages(s: &str) -> Vec<String> {
    s.split(|c: char| c == ',' || c == '，' || c == '、' || c == ';' || c == '；' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// PS 的 `-split ','` + `Where-Object { $_ }`（认栽标记的关卡串；非正则、只按半角逗号）。
fn arksk_split_commas(s: &str) -> Vec<String> {
    s.split(',').filter(|t| !t.is_empty()).map(|t| t.to_string()).collect()
}

/// 真身 191 行那句 `$farmStages -notcontains [string]$st` 的大小写口径复刻：PS 的 -notcontains
/// **不区分大小写**（R3② 转写用的 Vec::contains 是区分大小写的——8 档拼写全同、投影没显形，
/// 但混拼档位（'ce-5' vs 'CE-5'）会被漏剔/错剔）。剔关重试的关卡串就从这个函数出。
fn arksk_lean_stages(stage: &str, farm: &[String]) -> Vec<String> {
    arksk_split_stages(stage)
        .into_iter()
        .filter(|st| !farm.iter().any(|f| f.eq_ignore_ascii_case(st)))
        .collect()
}

/// 记录行里"有没有这个键"——jv_get 把"缺键"与"值是 null"读成同一件事，而驱动档的
/// journal/res 恰恰要靠"data 键在不在"（takeover 的 data 就是 null）来判行形态，所以单立一把。
fn jv_has(v: &JVal, key: &str) -> bool {
    if let JVal::O(o) = v {
        return o.iter().any(|(k, _)| k == key);
    }
    false
}

/// 键名按 PS 阅读习惯取（`$ark.Mode` 对哈希键**不区分大小写**）：驱动档的 decision 从壳侧
/// 序列化过来，键名拼写不能当契约读者，只有"这一格在不在"才是。
fn jv_get_ci(v: &JVal, key: &str) -> JVal {
    if let JVal::O(o) = v {
        for (k, val) in o {
            if k.eq_ignore_ascii_case(key) {
                return val.clone();
            }
        }
    }
    JVal::Null
}

/// 一档的认关读数：`{Skipped;Plan;Evidence}` 或 `{Throw}`（与 live 门夹具同形）。
enum ArkskSkipItem {
    R {
        skipped: bool,
        plan: &'static [&'static str],
        evidence: &'static [&'static str],
    },
    T(&'static str),
}

/// 一档的配置队列项：Set-MaaDailyConfig 桩的固定返回 `{Changes;ExpectedChains}`。
struct ArkskCfg {
    changes: &'static [&'static str],
    chains: &'static [&'static str],
}

/// 一档场景（纯数据；对应 live 门第六路的 `@{}` 哈希表）。
struct ArkskScen {
    name: &'static str,
    retry_once: bool,
    launch: bool,
    /// 合成 decision 的 Source（8 档恒 'auto'）。真身里这枚是 $Decision.Source —— 决定主支
    /// 配置记账归属的那一格；驱动档腿长在壳侧（壳拿 ctx 里的原 $Decision），桩世界由本字段供。
    dec_source: &'static str,
    dec_mode: &'static str,
    dec_stage: &'static str,
    dec_farm: Option<&'static [&'static str]>,
    run_q: &'static [(&'static str, &'static str)], // (Status, Detail)
    skip_q: &'static [ArkskSkipItem],
    cfg_q: &'static [ArkskCfg],
    desk: &'static str,   // Get-GdInputDesktopName 桩返回（恒 'Default'；屏保支只在扩档时用）
    sv_txt: &'static str, // 屏保读数夹具（恒空；同上）
}

/// 两枚固定配置桩（主轮 / 剔关轮）——用 const 而不是局部变量：`&[MAIN_CFG, …]` 是常量表达式，
/// 会被常量提升成 'static 引用；局部变量则只能移动进一枚临时数组，装不进 `&'static [ArkskCfg]` 的字段。
const MAIN_CFG: ArkskCfg = ArkskCfg { changes: &["chg-main"], chains: &["Fight", "Shop"] };
const LEAN_CFG: ArkskCfg = ArkskCfg { changes: &["chg-lean"], chains: &["Fight"] };

/// 8 档场景 —— 逐档抄自 tools\probe-live-golden.ps1 第六路的 Get-ArkSkipScenarios。
fn arkskip_scenarios() -> Vec<ArkskScen> {
    vec![
        // 1) 跳过&剔关&恢复：check1 跳 → 剔 CE-5 重跑 → r2 ok、check2 没跳 → recovered
        ArkskScen {
            name: "arksk-1-recover",
            retry_once: false,
            launch: true,
            dec_source: "auto",
            dec_mode: "default",
            dec_stage: "1-7,CE-5",
            dec_farm: Some(&["CE-5"]),
            run_q: &[("ok", "首轮收尾正常"), ("ok", "剔关轮收尾正常")],
            skip_q: &[
                ArkskSkipItem::R { skipped: true, plan: &["1-7"], evidence: &["计划修剪实录A"] },
                ArkskSkipItem::R { skipped: false, plan: &["1-7"], evidence: &[] },
            ],
            cfg_q: &[MAIN_CFG, LEAN_CFG],
            desk: "Default",
            sv_txt: "",
        },
        // 2) 二查仍跳 → giveUp + 认栽标记 + Detail 拼 if 支(Detail 非空) + 二查计划'空'支
        ArkskScen {
            name: "arksk-2-still-skip",
            retry_once: false,
            launch: true,
            dec_source: "auto",
            dec_mode: "default",
            dec_stage: "1-7,CE-5",
            dec_farm: Some(&["CE-5"]),
            run_q: &[("ok", "首轮收尾正常"), ("ok", "剔关后收尾正常")],
            skip_q: &[
                ArkskSkipItem::R { skipped: true, plan: &["1-7"], evidence: &["x"] },
                ArkskSkipItem::R { skipped: true, plan: &[], evidence: &["y"] },
            ],
            cfg_q: &[MAIN_CFG, LEAN_CFG],
            desk: "Default",
            sv_txt: "",
        },
        // 3) 二查没跳但 r2 非 ok → "说不死关卡开没开"支，不记标记(本轮以该状态收场)
        ArkskScen {
            name: "arksk-3-r2-notok",
            retry_once: false,
            launch: true,
            dec_source: "auto",
            dec_mode: "default",
            dec_stage: "1-7,CE-5",
            dec_farm: Some(&["CE-5"]),
            run_q: &[("ok", "首轮收尾正常"), ("stalled", "剔关轮卡住")],
            skip_q: &[
                ArkskSkipItem::R { skipped: true, plan: &["1-7"], evidence: &["x"] },
                ArkskSkipItem::R { skipped: false, plan: &["1-7"], evidence: &[] },
            ],
            cfg_q: &[MAIN_CFG, LEAN_CFG],
            desk: "Default",
            sv_txt: "",
        },
        // 4) 无 Farm → 条件假 → 直接认栽；Detail 空串 → tailNote 走 else 支(独立成句)
        ArkskScen {
            name: "arksk-4-no-lean",
            retry_once: false,
            launch: true,
            dec_source: "auto",
            dec_mode: "default",
            dec_stage: "CE-5",
            dec_farm: None,
            run_q: &[("ok", "")],
            skip_q: &[ArkskSkipItem::R { skipped: true, plan: &["CE-5"], evidence: &["x"] }],
            cfg_q: &[MAIN_CFG],
            desk: "Default",
            sv_txt: "",
        },
        // 5) 检查了但没跳 → 认关链整块不动
        ArkskScen {
            name: "arksk-5-not-skipped",
            retry_once: false,
            launch: true,
            dec_source: "auto",
            dec_mode: "default",
            dec_stage: "1-7,CE-5",
            dec_farm: Some(&["CE-5"]),
            run_q: &[("ok", "一切正常")],
            skip_q: &[ArkskSkipItem::R { skipped: false, plan: &["1-7"], evidence: &[] }],
            cfg_q: &[MAIN_CFG],
            desk: "Default",
            sv_txt: "",
        },
        // 6) 首查抛 → WARN "认关检查没做起来：…(不影响本轮)"，链不进来
        ArkskScen {
            name: "arksk-6-check1-throw",
            retry_once: false,
            launch: true,
            dec_source: "auto",
            dec_mode: "default",
            dec_stage: "1-7,CE-5",
            dec_farm: Some(&["CE-5"]),
            run_q: &[("ok", "一切正常")],
            skip_q: &[ArkskSkipItem::T("读数没起来")],
            cfg_q: &[MAIN_CFG],
            desk: "Default",
            sv_txt: "",
        },
        // 7) 二查抛 → WARN "第二次认关检查没做起来：…"，随后按 r2 ok 记 recovered
        ArkskScen {
            name: "arksk-7-check2-throw",
            retry_once: false,
            launch: true,
            dec_source: "auto",
            dec_mode: "default",
            dec_stage: "1-7,CE-5",
            dec_farm: Some(&["CE-5"]),
            run_q: &[("ok", "首轮收尾正常"), ("ok", "剔关轮收尾")],
            skip_q: &[
                ArkskSkipItem::R { skipped: true, plan: &["1-7"], evidence: &["x"] },
                ArkskSkipItem::T("第二次读数炸了"),
            ],
            cfg_q: &[MAIN_CFG, LEAN_CFG],
            desk: "Default",
            sv_txt: "",
        },
        // 8) r1 非 ok → 分流真动词(stalled+retryOnce → 重试) → 重试 ok → 进认关查(没跳) → 收尾
        ArkskScen {
            name: "arksk-8-retry-then-chain",
            retry_once: true,
            launch: true,
            dec_source: "auto",
            dec_mode: "default",
            dec_stage: "1-7,CE-5",
            dec_farm: Some(&["CE-5"]),
            run_q: &[("stalled", "首轮卡住"), ("ok", "重试后正常")],
            skip_q: &[ArkskSkipItem::R { skipped: false, plan: &["1-7"], evidence: &[] }],
            cfg_q: &[MAIN_CFG],
            desk: "Default",
            sv_txt: "",
        },
    ]
}

// ---- R3③ 换头协议的类型面 ----
// 三态各自的"接不接"：Fail=业务错，链体按真身的 try/catch 结构决定接不接（接住的点只
// match 这一支）；Ask=驱动档暂停（冒泡到顶渲染 ask 信封）；Proto=协议/运输级故障（journal
// 坏了、行对不上、夹具缺项）——**任何 catch 都不接**，一路冒到底。
#[derive(Debug)]
enum ArkAbort {
    Fail(String),
    Ask,
    Proto(String),
}

/// `decide(...)?` 这类"旧式返回 Err(String)"的调用点自然转成 Fail（Proto 只能显式构造）。
impl From<String> for ArkAbort {
    fn from(m: String) -> Self {
        ArkAbort::Fail(m)
    }
}

/// 链体输入：驱动档从 ctx 文件来、夹具档从场景合成——两路的形状在这层归一。
struct ArkDInput {
    launch: bool,
    retry_once: bool,
    decision: JVal,
}

/// 链体输出（真身 @{Status;Detail;Duration}）：duration 无 = 真身 error 态没写 Duration 键。
struct ArkDOut {
    status: String,
    detail: String,
    duration: Option<f64>,
}

/// 配置动作的两种腿：主轮（用进程里的原 $Decision 对象）与剔关轮（链体给 stage、executor 照
/// 真身 198-202 现构造 lean 哈希）。载荷长什么样是 executor 的事，链体只递"哪种腿"。
enum ArkCfgKind {
    Main,
    Lean { stage: String },
}

/// Set-MaaDailyConfig 的返回里链体要的两格。
struct ArkCfgOut {
    changes: Vec<String>,
    chains: Vec<String>,
}

/// Invoke-MaaRun 的返回里链体要的三格；duration 无 = 返回对象没带 Duration 键
/// （Invoke-GdStep 原样透传 Body 返回，Duration 是 Invoke-MaaRun 自己算的那枚——common.ps1:718-727 已核）。
struct ArkRunOut {
    status: String,
    detail: String,
    duration: Option<f64>,
}

/// Get-GdMaaFightSkip 的读数：None 支 = 真身 $skip=$null（gui.log 不在/空时读数函数给 $null，
/// `$skip -and $skip.Skipped` 整条短路——R3② 转写漏了这支，这刀补上）。
struct ArkSkipData {
    skipped: bool,
    plan: Vec<String>,
    evidence: Vec<String>,
}

/// 一步的终态（`Invoke-GdStep` 的返回：status/detail/duration；catch 支无 duration）。
struct ArkStepOut {
    status: String,
    detail: String,
    duration: Option<f64>,
}

/// 链体对宿主世界的全部需求（15 动作 + 日志 = 16 方法）。每个动作都是 Result：宿主侧一切皆可
/// 抛，抛了就回 Err——"哪个点接住"是链体的判断（照真身 try/catch 逐点转写），不是宿主的。
/// 无 catch 的调用点直接 `?` 冒泡（等价真身"整函数异常上抛"，换头后落成 fail 信封）。
trait ArkDailyFx {
    fn log(&mut self, level: &str, text: &str);
    /// Get-GdInputDesktopName。
    fn desk(&mut self) -> Result<String, ArkAbort>;
    /// 屏保读数（真身那截 try/catch 全吞的注册表读法；吞的工作照旧住 executor）。
    fn svdetail(&mut self) -> Result<String, ArkAbort>;
    /// Invoke-GdTakeover（names 由 executor 现算：MAA 进程名 + 端游 exe 名）。
    fn takeover(&mut self) -> Result<(), ArkAbort>;
    /// Set-MaaDailyConfig -Apply。
    fn cfg(&mut self, kind: ArkCfgKind) -> Result<ArkCfgOut, ArkAbort>;
    /// Get-GdEntryVerb -Game -Verb。
    fn entryverb(&mut self, game: &str, verb: &str) -> Result<String, ArkAbort>;
    /// Start-GdArkClient。
    fn launch(&mut self) -> Result<bool, ArkAbort>;
    /// Invoke-MaaRun -ExpectedChains。
    fn run(&mut self, chains: &[String]) -> Result<ArkRunOut, ArkAbort>;
    /// Stop-MaaAndGame。
    fn stop(&mut self) -> Result<(), ArkAbort>;
    /// Start-Sleep -Seconds（秒数进参数面；夹具世界不进投影）。
    fn sleep(&mut self, seconds: u32) -> Result<(), ArkAbort>;
    /// Get-GdMaaFightSkip（Err=真身 catch 到的那侧；Ok(None)=$null 支）。
    fn skip(&mut self) -> Result<Option<ArkSkipData>, ArkAbort>;
    /// Set-GdArkFightSkipMark；返回 Date 串（夹具世界是固定桩值）。
    fn mark(&mut self, stages: &[String], reason: &str) -> Result<String, ArkAbort>;
    /// Update-GdArkFarmLedger（空串=没记账）。
    fn ledger(&mut self) -> Result<String, ArkAbort>;
    /// Get-GdMaaStageObservation（可为 null）。
    fn obs(&mut self) -> Result<JVal, ArkAbort>;
    /// Update-GdArkStageObs；None=读数函数没给计数。
    fn obsrec(&mut self, obs: &JVal) -> Result<Option<i64>, ArkAbort>;
    /// Test-GdAdmin 的裸态。短路：仅 not-started 时到。
    fn admin(&mut self) -> Result<bool, ArkAbort>;
}

// ===== B-2d④：r1999 这一路的链体（真身 lib\r1999-daily.ps1 81-167）=====
//
// 与 ark 那一路**刻意不合并**（plan §11.39 ②-1：不为两路真差异做三层抽象）。真差异有五处，
// 每一处都是这一路自己的事实，统一了就成了假话：
//   ① 没有桌面闸（`desk`/`svdetail`）—— M9A 操作的是模拟器里的安卓，会话在不在 Default 都不影响；
//   ② 没有 `launch`（不拉端游）；重试前真身只 `Stop-M9A`，没有"游戏没起来"那条血训；
//   ③ `run` **不带 chains**（真身 `$run9` 那枚闭包就没这个参数）；
//   ④ 重试间隔 **20 秒**（ark 那路是 25），两句数字各照各家真身，不许顺手统一；
//   ⑤ 判定多吃一枚 `rescued`（`decide-daily-retry` 的 r1999 支靠它翻案，ark 支根本不读这格）；
//   另有一整段 ark 没有的：起跑前的**更新闸**与收尾的**审计闸**。
// 八格动作 = updgate/takeover/mumu/cfg/run/sleep/stop/audit，其中**真做只有 run**，其余照旧 ask。

/// 更新闸/审计闸共用的回执三格：子进程退出码 + 挑出来那一句 + 全量输出并串。
/// 真身那两处 try/catch 的形状是同一把（tools\m9a-update.ps1 / tools\1999-event-audit.ps1 都是
/// `-Apply` 起一个 pwsh 子进程、收 2>&1、按正则挑一行、非 0 退出就把全部输出 join ' / '），
/// 所以两格动作共用这一枚结构；**两句挑行的正则与三句文案各不相同**，那些住在链体与 executor 里。
/// 异常不在这三格里（走 `Err(ArkAbort::Fail)`）：`drive_merge_res` 已定死"ok 真不许带 err 键"，
/// 把 PS 异常串塞进 data 会让"执行器抛了"在 journal 里落成一条 data 行，与 ark 侧
/// skip/ledger/obs 三处的 Ok/Fail 分派不同形。这是对 §11.39 ⑤-5「四个数据格」的就地细化。
struct M9aGateOut {
    code: i64,
    line: String,
    joined: String,
}

/// Set-M9AInstance -Apply 的返回里链体要的那一格（ark 侧同款结构还多带 chains 给 run 用；
/// m9a 的 run 不吃 chains ⇒ 这里只有一格，不为了"看起来对称"补一枚死字段）。
struct M9aCfgOut {
    changes: Vec<String>,
}

/// Invoke-M9ARun 的返回里链体要的四格。
/// **不给 `ArkRunOut` 加字段**（§11.39 ②-1 原文）：ark 那一路真身就没有 Rescued 这格，
/// 补上去等于让端游的账行多一个恒 false 的键，journal 形状当场变。
/// duration 与 rescued 都是**条件补**：
///   · duration —— 真身只有 ok 那一路 `$res.Duration = $elapsed`（`m9a.ps1` 的收尾段）；
///   · rescued —— 真身只在"进程都没了但画面兜成了"那一路挂 `$res.Rescued = $true`（`:493`），
///     其余档位这个键**压根不存在**（PS 侧 `[bool]$r.Rescued` 读到 $null=false）。
/// 两格都是"没有就不写"，所以账行与 done 回执的键序是 status → detail → duration? → rescued?。
struct M9aRunOut {
    status: String,
    detail: String,
    duration: Option<f64>,
    rescued: bool,
}

/// r1999 链体的输入。ark 那路 ctx 顶层是 launch/retry_once/decision 三格，这里只有 retry_once 一格：
/// 链体后面**一次都不读 decision**（实例那两枚走 ctx 的 m9aRun 载荷、由 executor 消费，
/// 判定要的四键里也没有 Mode），把 decision 抄进来只会造出一格"账上有、没人读"的死数据。
struct R1999DInput {
    retry_once: bool,
}

/// 链体对宿主世界的全部需求（8 动作 + 日志 = 9 方法）。错阀门与 ark 那一路同一条：
/// 宿主侧一切皆可失败，抛了就回 Err，**哪个点接住是链体的判断**（照真身 try/catch 逐点转写），
/// 无 catch 的调用点直接 `?` 冒泡（等价真身"整函数异常上抛"，换头后落成 fail 信封）。
trait R1999DailyFx {
    fn log(&mut self, level: &str, text: &str);
    /// 更新闸：`& $selfEngine -File tools\m9a-update.ps1 -Apply 2>&1` 那整段（真身 :90-101）。
    /// Ok = 子进程起来了（退出码非 0 也算 Ok，那一档在真身里是 WARN 不是异常）；
    /// Err(Fail) = 真身 catch 到的那句"闸本身抛错"。
    fn updgate(&mut self) -> Result<M9aGateOut, ArkAbort>;
    /// Invoke-GdTakeover（names 由 executor 现算：`(Get-M9APaths -S $S).Name`，ark 那路是两枚）。
    fn takeover(&mut self) -> Result<(), ArkAbort>;
    /// Start-GdMuMu 的裸 bool（"未能确认启动"那句 WARN 住在链体，不在 executor）。
    fn mumu(&mut self) -> Result<bool, ArkAbort>;
    /// Set-M9AInstance -S -Instance -Apply。
    fn cfg(&mut self) -> Result<M9aCfgOut, ArkAbort>;
    /// Invoke-M9ARun -S -Instance —— 本档**唯一真做**的动作，不吃 chains。
    fn run(&mut self) -> Result<M9aRunOut, ArkAbort>;
    /// Stop-M9A（真身调两次：重试前一次、收尾一次）。
    fn stop(&mut self) -> Result<(), ArkAbort>;
    /// Start-Sleep -Seconds（真身重试前那枚 20）。
    fn sleep(&mut self, seconds: u32) -> Result<(), ArkAbort>;
    /// 审计闸：`& $selfEngine -File tools\1999-event-audit.ps1 -Apply 2>&1` 那整段（真身 :155-165）。
    /// 只在 status ∈ {ok, empty-queue} 时链体才会调它。
    fn audit(&mut self) -> Result<M9aGateOut, ArkAbort>;
}

/// 夹具世界（8 档）的 Fx（把 live 门第六路/三侧门的桩语义原样平移进 trait 实现：调用序与入参
/// 进 seq、队列消费、固定返回、admin 恒 false）——链体从真身抽出后，"8 档投影逐字不变"这一半
/// 由它保。投影串里的动作裸名（DESK/CFG/RUN/SKIP/…）都是这一层的历史格式，链体不碰。
/// B-1a 起：mark/obsrec 的读数取真值（今日日期/递增计数），与"新侧进程内真做"的日志逐字对齐；
/// 三侧门的投影过滤（#321）负责把 MARK#/OBSREC 裸段与 ask 侧的对齐。
struct FixtureFx<'a> {
    sc: &'a ArkskScen,
    seq: Vec<String>,
    cfg_i: usize,
    run_i: usize,
    skip_i: usize,
    skip_n: i32,
    obs_n: &'a mut i64,
}

impl<'a> FixtureFx<'a> {
    fn new(sc: &'a ArkskScen, obs_n: &'a mut i64) -> Self {
        FixtureFx { sc, seq: Vec::new(), cfg_i: 0, run_i: 0, skip_i: 0, skip_n: 0, obs_n }
    }
    /// Write-GdLog 桩：`$script:seq += "$Level|$Message"`。
    fn w(&mut self, level: &str, msg: &str) {
        self.seq.push(format!("{}|{}", level, msg));
    }
    /// 动作桩的裸串（TAKEOVER/CFG/RUN/SKIP/… 都不带 Level 前缀）。
    fn raw(&mut self, s: String) {
        self.seq.push(s);
    }
}

impl ArkDailyFx for FixtureFx<'_> {
    fn log(&mut self, level: &str, text: &str) {
        self.w(level, text);
    }
    fn desk(&mut self) -> Result<String, ArkAbort> {
        self.raw("DESK".to_string());
        Ok(self.sc.desk.to_string())
    }
    /// 屏保读数在桩世界里落成夹具字段 sv_txt（真身处是探路 + try/catch 读注册表）。
    fn svdetail(&mut self) -> Result<String, ArkAbort> {
        Ok(self.sc.sv_txt.to_string())
    }
    fn takeover(&mut self) -> Result<(), ArkAbort> {
        // 夹具世界：MAA 名与端游名（Arknights）都是桩值（真身=进程名 + exe 路径去扩展名）。
        self.raw("TAKEOVER#MAA~Arknights#方舟/MAA".to_string());
        Ok(())
    }
    /// 桩：先记 CFG，再消费队列（顺序与 PS 桩逐句一致）。主轮的 source/stage 取夹具字段
    /// （真身=进程里那份 $Decision），剔关轮照真身 198-202 固定 'auto' + 链体现构造的 stage。
    fn cfg(&mut self, kind: ArkCfgKind) -> Result<ArkCfgOut, ArkAbort> {
        let (source, stage) = match kind {
            ArkCfgKind::Main => (self.sc.dec_source.to_string(), self.sc.dec_stage.to_string()),
            ArkCfgKind::Lean { stage } => ("auto".to_string(), stage),
        };
        self.raw(format!("CFG#{}#{}", source, stage));
        let q = self.sc.cfg_q;
        if self.cfg_i >= q.len() {
            // 队列空=夹具缺项（开发期错误），走 Proto —— 不许被链体的 catch 吞成 WARN。
            return Err(ArkAbort::Proto("CFG 夹具队列空了(场景缺项，开发期错误)".to_string()));
        }
        let it = &q[self.cfg_i];
        self.cfg_i += 1;
        Ok(ArkCfgOut {
            changes: it.changes.iter().map(|s| s.to_string()).collect(),
            chains: it.chains.iter().map(|s| s.to_string()).collect(),
        })
    }
    /// 入口参数名桩恒 '-Ack'（8 档不触 event-pending 支；合体工程 Sync 后表给 -AckArk 是本刀外的事）。
    fn entryverb(&mut self, _game: &str, _verb: &str) -> Result<String, ArkAbort> {
        Ok("-Ack".to_string())
    }
    fn launch(&mut self) -> Result<bool, ArkAbort> {
        // Start-GdArkClient 桩恒返回 true —— "窗口没等到"那句 WARN 在 8 档恒不触，形态照转（扩档用）。
        self.raw("LAUNCH".to_string());
        Ok(true)
    }
    fn run(&mut self, chains: &[String]) -> Result<ArkRunOut, ArkAbort> {
        // Invoke-MaaRun 桩：消费 run 队列。
        let q = self.sc.run_q;
        if self.run_i >= q.len() {
            // 队列空=夹具缺项（开发期错误）：Proto 直冒，防被 Invoke-GdStep 的 catch 吞成 error 态。
            return Err(ArkAbort::Proto("RUN 夹具队列空了(场景缺项，开发期错误)".to_string()));
        }
        let (status, detail) = q[self.run_i];
        self.run_i += 1;
        self.raw(format!("RUN#{}#{}", chains.join("+"), status));
        Ok(ArkRunOut { status: status.to_string(), detail: detail.to_string(), duration: None })
    }
    fn stop(&mut self) -> Result<(), ArkAbort> {
        self.raw("STOP".to_string());
        Ok(())
    }
    fn sleep(&mut self, _seconds: u32) -> Result<(), ArkAbort> {
        self.raw("SLEEP".to_string());
        Ok(())
    }
    /// 计数先记，再消费队列；Throw 项=真身 catch 到的那侧（Fail），队列空=夹具缺项（Proto）。
    fn skip(&mut self) -> Result<Option<ArkSkipData>, ArkAbort> {
        self.skip_n += 1;
        self.raw(format!("SKIP#{}", self.skip_n));
        let q = self.sc.skip_q;
        if self.skip_i >= q.len() {
            return Err(ArkAbort::Proto("SKIP 夹具队列空了(场景缺项，开发期错误)".to_string()));
        }
        let it = &q[self.skip_i];
        self.skip_i += 1;
        match it {
            ArkskSkipItem::T(m) => Err(ArkAbort::Fail(m.to_string())),
            ArkskSkipItem::R { skipped, plan, evidence } => Ok(Some(ArkSkipData {
                skipped: *skipped,
                plan: plan.iter().map(|s| s.to_string()).collect(),
                evidence: evidence.iter().map(|s| s.to_string()).collect(),
            })),
        }
    }
    fn mark(&mut self, stages: &[String], reason: &str) -> Result<String, ArkAbort> {
        self.raw(format!("MARK#{}#{}", stages.join("+"), reason));
        // B-1a：桩与"新侧进程内真做"对齐 —— 返回真今日（两侧日志日期同字；投影侧由 #321 过滤 MARK# 段）。
        Ok(gd_local_date())
    }
    fn ledger(&mut self) -> Result<String, ArkAbort> {
        self.raw("LEDGER".to_string());
        Ok("BN1".to_string())
    }
    fn obs(&mut self) -> Result<JVal, ArkAbort> {
        self.raw("OBS".to_string());
        // B-1a：桩载荷升级为全形 7 字段（新侧 obsrec 进程内真写账本读它；固定值照门侧夹具字面）。
        Ok(jv_o(vec![
            ("Found", JVal::B(true)),
            ("Date", jv_s("2026-10-04")),
            ("Stage", jv_s("1-7")),
            ("LastBattle", JVal::B(true)),
            ("SawGuiPlan", JVal::B(false)),
            ("Survivors", JVal::A(vec![jv_s("72"), jv_s("30012")])),
            ("Selected", jv_s("1-7")),
        ]))
    }
    fn obsrec(&mut self, _obs: &JVal) -> Result<Option<i64>, ArkAbort> {
        self.raw("OBSREC".to_string());
        // B-1a：计数器替固定 5 —— 与新侧真账本净计数 1..N 对齐（投影侧由 #321 过滤 OBSREC 裸段）。
        *self.obs_n += 1;
        Ok(Some(*self.obs_n))
    }
    /// Test-GdAdmin 恒 false（与 live 门第六路同桩）。
    fn admin(&mut self) -> Result<bool, ArkAbort> {
        Ok(false)
    }
}

/// 按 Invoke-GdCli 的同一条协议真跑一枚 verb：Body 写 --in 信封、读回 --out 信封、
/// 校验 verb / 退出码 / ok —— 任一环对不上就 Err（不带着半截结论往下跑）。临时文件成败都删。
/// 错误文案里"C# 侧(…)"的句式照 Invoke-GdCli 原话，verb 名参数化后逐字不变；
/// 起不来（exe 不在）这句是本档自己的补充。B-2a 起 --arkskip 与 --maa-run 两档共用这一处。
fn call_panel_verb(panel_cs: &str, verb: &str, body: &JVal) -> Result<JVal, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir();
    let in_path = dir.join(format!("gd-verb-{}-{}.in.json", std::process::id(), stamp));
    let out_path = dir.join(format!("gd-verb-{}-{}.out.json", std::process::id(), stamp));
    let text_body = jv_to_json(body);
    let r = (|| -> Result<JVal, String> {
        std::fs::write(&in_path, text_body.as_bytes()).map_err(|e| format!("Body 信封写不下去：{}", e))?;
        let outp = std::process::Command::new(panel_cs)
            .arg(verb)
            .arg(format!("--in={}", in_path.display()))
            .arg(format!("--out={}", out_path.display()))
            .output()
            .map_err(|e| format!("{} 起不来（{}）：{}", verb, panel_cs, e))?;
        let code = outp.status.code().unwrap_or(-1);
        let text = match std::fs::read_to_string(&out_path) {
            Ok(t) => t,
            Err(_) => return Err(format!("C# 侧({})没吐出结果文件(退出码 {})", verb, code)),
        };
        let env0 = jv_parse(&text).map_err(|e| format!("C# 侧({})信封读不动：{}", verb, e))?;
        if !jv_str(&jv_get(&env0, "verb")).eq_ignore_ascii_case(verb) {
            return Err(format!("C# 侧({})信封不对或缺 verb 字段", verb));
        }
        let env_exit = jv_int(&jv_get(&env0, "exit"), 0);
        if env_exit != code as i64 {
            return Err(format!(
                "C# 侧({})退出码({})与信封 exit({})不一致",
                verb, code, env_exit
            ));
        }
        if !jv_truthy(&jv_get(&env0, "ok")) {
            return Err(jv_str(&jv_get(&env0, "error")));
        }
        Ok(jv_get(&env0, "data"))
    })();
    let _ = std::fs::remove_file(&in_path);
    let _ = std::fs::remove_file(&out_path);
    r
}

/// decide-daily-retry 的那一枚调用点：形状拆成 (Retryable, Logs) 两格，其余全在 call_panel_verb。
fn arksk_call_decide(
    panel_cs: &str,
    status: &str,
    retry_once: bool,
) -> Result<(bool, Vec<(String, String)>), String> {
    let body = jv_o(vec![
        ("game", jv_s("ark")),
        ("status", jv_s(status)),
        ("retryOnce", JVal::B(retry_once)),
    ]);
    let data = call_panel_verb(panel_cs, "decide-daily-retry", &body)?;
    let retryable = jv_truthy(&jv_get(&data, "Retryable"));
    let mut logs = Vec::new();
    for l in jv_list(&jv_get(&data, "Logs")) {
        logs.push((jv_str(&jv_get(l, "Level")), jv_str(&jv_get(l, "Text"))));
    }
    Ok((retryable, logs))
}

/// 夹具 decision（从场景合成）：链体只经 jv_get_ci 读其中三格 —— Mode（event-pending 判）、
/// Stage（剔关切分/认栽标记）、Farm.Stages（补刷关）；键名照 PS 原生对象（$ark.Mode 那套）。
/// dec_farm=None 时**不放 Farm 键**（照真身"计划里没这格"的情形，测试空计划支）。
fn arksk_fixture_decision(sc: &ArkskScen) -> JVal {
    let mut kv: Vec<(String, JVal)> = vec![
        ("Source".to_string(), jv_s(sc.dec_source)),
        ("Mode".to_string(), jv_s(sc.dec_mode)),
        ("Stage".to_string(), jv_s(sc.dec_stage)),
    ];
    if let Some(f) = sc.dec_farm {
        kv.push((
            "Farm".to_string(),
            jv_o(vec![("Stages", JVal::A(f.iter().map(|s| jv_s(s)).collect()))]),
        ));
    }
    JVal::O(kv)
}

/// `Invoke-GdStep -Name -Body { & $run … }` 的等价体：跑体 = 拉端游（按开关）+ 跑 MAA；体里任一步
/// Fail 都被吞成 error 态、**不 rethrow**（Invoke-GdStep 的 catch 口径，common.ps1:718 已核）；
/// 非 ok 打 WARN"结束状态"。`lean` 只决定"窗口没等到"那句的尾部变体（真身 $run/$runLean 闭包
/// 的唯一差别就是这半句 + chains 来源）。Ask/Proto 不接、一路冒到顶。
fn ark_daily_step(
    fx: &mut dyn ArkDailyFx,
    name: &str,
    chains: &[String],
    lean: bool,
    launch: bool,
) -> Result<ArkStepOut, ArkAbort> {
    let body = (|| -> Result<ArkRunOut, ArkAbort> {
        // 拉端游必须在体重：重试前 Stop-MaaAndGame 把端游和 MAA 一起关掉，放外面的话第二次只有
        // MAA 起来、游戏没起来（真身 2026-09-24 06:00 血训注释）。
        if launch {
            if !fx.launch()? {
                let t = if lean {
                    "端游窗口没等到：MAA 大概会报\"未找到窗口\"，仍继续"
                } else {
                    "端游窗口没等到：MAA 大概会报\"未找到窗口\"，仍继续(会按 not-started 早退)"
                };
                fx.log("WARN", t);
            }
        }
        fx.run(chains)
    })();
    let r = match body {
        Ok(r) => r,
        Err(ArkAbort::Fail(m)) => {
            return Ok(ArkStepOut { status: "error".to_string(), detail: format!("{} 异常：{}", name, m), duration: None })
        }
        Err(e) => return Err(e),
    };
    if !r.status.eq_ignore_ascii_case("ok") {
        fx.log("WARN", &format!("{} 结束状态：{} — {}", name, r.status, r.detail));
    }
    Ok(ArkStepOut { status: r.status, detail: r.detail, duration: r.duration })
}

/// 链体：`Start-GdArkDaily` 主体按句转写（真身 lib\ark-daily.ps1 106-255）。宿主差异全在 Fx
/// （夹具记 seq / 驱动走 journal+ask），这里只管"做什么、什么顺序、哪句什么 Level、哪个点接不接
/// 异常"——8 档投影与旧转写逐字节同源是硬门（probe-arkskip-rust-golden 咬的 316 行块）。
fn ark_daily_chain(
    input: &ArkDInput,
    fx: &mut dyn ArkDailyFx,
    decide: &mut dyn FnMut(&str, bool) -> Result<(bool, Vec<(String, String)>), String>,
) -> Result<ArkDOut, ArkAbort> {
    fx.log("STEP", "--- 方舟(端游 + MAA)---");
    // 起跑前置判定（2026-09-26）：桌面不对就早退（读不到桌面名不会走这条），把原因写全。
    let desk = fx.desk()?;
    if !desk.is_empty() && !desk.eq_ignore_ascii_case("Default") {
        let sv_txt = fx.svdetail()?;
        fx.log(
            "WARN",
            &format!("跳过本轮：输入桌面={}(屏保/锁屏已接管会话，鼠标键盘进不来；读不到桌面名不会走这条)", desk),
        );
        if !sv_txt.is_empty() {
            fx.log("WARN", &format!("  屏保读数：{}", sv_txt));
        }
        let mut detail = format!(
            "起跑时会话在 {} 桌面(屏保/锁屏)，直接跳过；长期解法：把 Windows 屏保设为「(无)」——pwsh -File tools\\unarm-screensaver.ps1 -Apply，或 设置→个性化→锁屏界面→屏幕保护程序→(无)；夜间保持解锁也行",
            desk
        );
        if !sv_txt.is_empty() {
            detail.push_str(&format!("。{}", sv_txt));
        }
        return Ok(ArkDOut { status: "not-started".to_string(), detail, duration: Some(0.0) });
    }
    // $ark = $Decision（链体后面读它的 Mode / Stage / Farm.Stages，照 PS 命名不敏感口径）。
    fx.takeover()?;
    let cfg = fx.cfg(ArkCfgKind::Main)?;
    for ch in &cfg.changes {
        fx.log("INFO", &format!("  MAA：{}", ch));
    }
    if jv_str(&jv_get_ci(&input.decision, "Mode")).eq_ignore_ascii_case("event-pending") {
        // 第 1 刀(2026-10-12)改口：event-pending 不再是"跳过理智作战、等人工 -AckArk"，而是
        // "今天拿活动最后一关当探针被 MAA 剪掉 ⇒ 判为还没打通 ⇒ 当日改刷长草但理智照烧"。
        // 那句指向日后不存在的开关的旧话术随之作废(旧串仍留在 arkskip/live 两门的 old 侧基线里，
        // 那是历史真身，不许改 —— 本次有意漂移就登记在那两处)。
        // entryverb 这枚 ask 照旧发：协议名册与两侧投影一字不动（返回值不再进文案，故弃读）。
        let _ack = fx.entryverb("ark", "Ack")?;
        fx.log(
            "WARN",
            "本轮不打活动图(今天试打被 MAA 剪掉，判为还没打通) → 理智照烧、刷长草计划；你打通后下一轮自动切回活动图",
        );
    }
    let mut r = ark_daily_step(fx, "方舟", &cfg.chains, false, input.launch)?;
    // ===== 批次 8：重试分流改问 decide-daily-retry(panel-cs.exe) =====
    // 跳过清单、retryOnce 合取与两句文案住在 C#；链体只回放日志 + 落地"活"(收尾/重跑)。
    let (retryable, logs) = decide(&r.status, input.retry_once)?;
    for (lvl, text) in &logs {
        let level = if lvl.is_empty() || lvl.eq_ignore_ascii_case("INFO") { "INFO" } else { lvl.as_str() };
        fx.log(level, text);
    }
    if retryable {
        fx.stop()?;
        fx.sleep(25)?;
        r = ark_daily_step(fx, "方舟重试", &cfg.chains, false, input.launch)?;
    }
    // ===== 认关止损（非冻结段；判定规则在 C# detect-maa-fight-skip） =====
    if r.status.eq_ignore_ascii_case("ok") {
        let skip1 = match fx.skip() {
            Ok(v) => v,
            Err(ArkAbort::Fail(m)) => {
                fx.log("WARN", &format!("认关检查没做起来：{}(不影响本轮)", m));
                None
            }
            Err(e) => return Err(e),
        };
        if let Some(sd) = skip1 {
            if sd.skipped {
                let farm_stages: Vec<String> =
                    jv_list(&jv_get_ci(&jv_get_ci(&input.decision, "Farm"), "Stages")).into_iter().map(jv_str).collect();
                let plan_txt = if !sd.plan.is_empty() { sd.plan.join(",") } else { "空".to_string() };
                fx.log(
                    "WARN",
                    &format!(
                        "MAA 认库后本轮的关计划实况是[{}]并判「关卡计划内没有当日开放的关卡」(证据：{}) → 整条 Fight 被跳过，这个 ok 是假的",
                        plan_txt,
                        sd.evidence.join(" / ")
                    ),
                );
                let stage_str = jv_str(&jv_get_ci(&input.decision, "Stage"));
                let lean_stages = arksk_lean_stages(&stage_str, &farm_stages);
                let mut give_up = false;
                if !farm_stages.is_empty() && !lean_stages.is_empty() {
                    fx.log(
                        "WARN",
                        &format!(
                            "剔掉补刷关({})后还剩 {} → 收尾后按纯长草计划重试一次(保长草理智)",
                            farm_stages.join(","),
                            lean_stages.join(",")
                        ),
                    );
                    fx.stop()?;
                    fx.sleep(25)?;
                    let cfg2 = fx.cfg(ArkCfgKind::Lean { stage: lean_stages.join(",") })?;
                    for ch in &cfg2.changes {
                        fx.log("INFO", &format!("  MAA(剔关重试)：{}", ch));
                    }
                    r = ark_daily_step(fx, "方舟剔关重试", &cfg2.chains, true, input.launch)?;
                    // PS 口径：catch 只记 WARN、$skip2 留在 $null，随后的 if/elseif 链**照常评估**
                    // —— 二查抛 ∧ r2 ok 时仍落 recovered 支（arksk-7 档实锤）。评估整段只挂 Ok 支
                    // 等于把 catch 当成"跳过整条链"，三侧门 H2 当场抓出。
                    let skip2 = match fx.skip() {
                        Ok(v) => v,
                        Err(ArkAbort::Fail(m)) => {
                            fx.log("WARN", &format!("第二次认关检查没做起来：{}", m));
                            None
                        }
                        Err(e) => return Err(e),
                    };
                    let (skipped2, plan2) = match skip2 {
                        None => (false, Vec::new()),
                        Some(sd2) => (sd2.skipped, sd2.plan),
                    };
                    if skipped2 {
                        give_up = true;
                        let plan2_txt = if !plan2.is_empty() { plan2.join(",") } else { "空".to_string() };
                        fx.log(
                            "WARN",
                            &format!("剔关重试仍被 MAA 判全关(计划实况：{}) → 今日理智关卡确认全关", plan2_txt),
                        );
                    } else if r.status.eq_ignore_ascii_case("ok") {
                        fx.log("OK", "剔关重跑后 Fight 正常打了，长草理智保住 → 今天不记认栽标记");
                    } else {
                        fx.log(
                            "WARN",
                            &format!("剔关重试本轮以 {} 收场，说不死关卡开没开 → 不记认栽标记(明天照常)", r.status),
                        );
                    }
                } else {
                    give_up = true;
                    fx.log("WARN", "本轮没有可剔的补刷关(或剔完计划就空了) → 直接认今日全关");
                }
                if give_up {
                    let stages = arksk_split_commas(&stage_str);
                    let mk_date = fx.mark(&stages, "MAA 判「关卡计划内没有当日开放的关卡」")?;
                    fx.log(
                        "WARN",
                        &format!(
                            "已记今日认栽标记({})：当天后续轮不再前置补刷关；若游戏里确有当日开放的图，可 -FarmArk 专用轮指定开放关补刷(专用轮不吃这条防线)",
                            mk_date
                        ),
                    );
                    let tail_note = "Fight 被 MAA 判全关(认栽，已记当日标记)";
                    if !r.detail.is_empty() {
                        r.detail = format!("{}；{}", r.detail, tail_note);
                    } else {
                        r.detail = tail_note.to_string();
                    }
                }
            }
        }
    }
    if r.status.eq_ignore_ascii_case("not-started") && !fx.admin()? {
        fx.log("ERROR", "方舟这一路要管理员权限：去掉 -NoElevate(或用 run-daily.cmd / 计划任务)重跑即可自动提权");
    }
    fx.stop()?;
    // 掉落记账放在收尾之后（真身注释：MAA 关门前还会补写 gui.log，偏移记在账本里，下轮会补上）。
    // 这段抛任何错都只 WARN，绝不影响本轮结论 —— catch 边界照真身 try/catch。
    let led = (|| -> Result<(), ArkAbort> {
        let bn = fx.ledger()?;
        if !bn.is_empty() {
            fx.log("INFO", &format!("掉落记账：{}", bn));
        }
        Ok(())
    })();
    match led {
        Ok(()) => {}
        Err(ArkAbort::Fail(m)) => fx.log("WARN", &format!("掉落记账没做起来：{}(不影响本轮)", m)),
        Err(e) => return Err(e),
    }
    // B 档观测（#21）：本轮 MAA 关卡开放读数抄进观测账本（只读日志+写账，不改配置、不影响结论）。
    let ob = (|| -> Result<(), ArkAbort> {
        let obs = fx.obs()?;
        let on = fx.obsrec(&obs)?;
        if let Some(n) = on {
            if n != 0 {
                fx.log("OK", &format!("关卡开放观测已记账(账本现 {} 条)", n));
            }
        }
        Ok(())
    })();
    match ob {
        Ok(()) => {}
        Err(ArkAbort::Fail(m)) => fx.log("WARN", &format!("关卡开放观测没做起来：{}(不影响本轮)", m)),
        Err(e) => return Err(e),
    }
    Ok(ArkDOut { status: r.status, detail: r.detail, duration: r.duration })
}

/// m9a 一路的重试分流：真身 lib\r1999-daily.ps1:117-122 那句 `Invoke-GdCli -Verb 'decide-daily-retry'`
/// 的 body 是**四键**（game/status/**rescued**/retryOnce），比 ark 那路多一枚 rescued —— C# 那侧
/// （Orchestration.cs:549-603）只有 r1999 支会读它（`not-started/empty-queue/blocked` 三档靠它翻案），
/// ark 支压根不碰这格。所以两路各写一枚调用体，不共用 arksk_call_decide：共用就要给 ark 的 body
/// 添一枚它不该有的键，判定侧的宽容面会因此变成契约。
/// 键序照真身 [pscustomobject] 的书写序（game→status→rescued→retryOnce）。
fn m9a_call_decide(
    panel_cs: &str,
    status: &str,
    rescued: bool,
    retry_once: bool,
) -> Result<(bool, Vec<(String, String)>), String> {
    let body = jv_o(vec![
        ("game", jv_s("r1999")),
        ("status", jv_s(status)),
        ("rescued", JVal::B(rescued)),
        ("retryOnce", JVal::B(retry_once)),
    ]);
    let data = call_panel_verb(panel_cs, "decide-daily-retry", &body)?;
    let retryable = jv_truthy(&jv_get(&data, "Retryable"));
    let mut logs = Vec::new();
    for l in jv_list(&jv_get(&data, "Logs")) {
        logs.push((jv_str(&jv_get(l, "Level")), jv_str(&jv_get(l, "Text"))));
    }
    Ok((retryable, logs))
}

/// 一步的终态（r1999 那一路）。比 ark 的 `ArkStepOut` 多一枚 rescued —— 真身 `Invoke-GdStep`
/// 原样透传 Body 的返回整块，而 `Invoke-M9ARun` 的返回里有 Rescued 这一格，重试分流要读它。
/// catch 支（error 态）rescued 恒 false：真身那句 catch 返回的 hashtable 只有 Status/Detail 两键。
struct R1999StepOut {
    status: String,
    detail: String,
    duration: Option<f64>,
    rescued: bool,
}

/// `Invoke-GdStep -Name '1999' -Body $run9` 的等价体（真身 :106-107）。
/// 与 ark 那枚 `ark_daily_step` 的差别只有两处，都是这一路自己的事实：
///   · 体重**没有拉端游那半句**（真身 `$run9` 闭包里就没有 Start-GdArkClient，也就没有"窗口没等到"那两句）；
///   · 体只有 `fx.run()` 一句、**不吃 chains**。
/// 相同的是 catch 边界：体内 Fail 吞成 error 态、**不 rethrow**（common.ps1:718-727 那条口径），
/// 非 ok 打 WARN「{名} 结束状态：{status} — {detail}」；Ask/Proto 不接、一路冒到顶。
fn r1999_daily_step(fx: &mut dyn R1999DailyFx, name: &str) -> Result<R1999StepOut, ArkAbort> {
    let body = (|| -> Result<M9aRunOut, ArkAbort> { fx.run() })();
    let r = match body {
        Ok(r) => r,
        Err(ArkAbort::Fail(m)) => {
            return Ok(R1999StepOut {
                status: "error".to_string(),
                detail: format!("{} 异常：{}", name, m),
                duration: None,
                rescued: false,
            })
        }
        Err(e) => return Err(e),
    };
    if !r.status.eq_ignore_ascii_case("ok") {
        fx.log("WARN", &format!("{} 结束状态：{} — {}", name, r.status, r.detail));
    }
    Ok(R1999StepOut { status: r.status, detail: r.detail, duration: r.duration, rescued: r.rescued })
}

/// 链体：`Start-GdR1999Daily` 主体按句转写（真身 lib\r1999-daily.ps1 81-167）。
/// 宿主差异全在 Fx（驱动走 journal+ask、现场动作由 PS 侧照单执行），这里只管
/// "做什么、什么顺序、哪句什么 Level、哪个点接不接异常"。
/// 逐段落纸顺序与真身逐行同源，其中三处**故意不与 ark 那路统一**（数字/句子里有一处统一就当场分歧）：
/// 更新闸与审计闸的两句异常话术由**链体**产、不由执行器产（执行器零自有日志）；
/// 而真身那两处 catch 的异常串（PowerShell 的 `$_.Exception.Message`）与核内 fail 信封的串
/// 本来就不是同一把来源 —— 这一格已登记为"换宿主必不同源"，不进等值判据。
fn r1999_daily_chain(
    input: &R1999DInput,
    fx: &mut dyn R1999DailyFx,
    decide: &mut dyn FnMut(&str, bool, bool) -> Result<(bool, Vec<(String, String)>), String>,
) -> Result<ArkDOut, ArkAbort> {
    fx.log("STEP", "--- 重返未来：1999(MuMu + M9A)---");
    // ===== 更新闸（真身 :87-101）。整段最坏十几分钟，但任何失败只 WARN 不挡当日轮 =====
    // 必须排在起 M9A 之前：更新器见 m9a 进程在跑就拒动（且永不硬杀）。
    // 三态照真身：退出码 0 ⇒ INFO 一句挑出来的行；退出码非 0 ⇒ WARN 带全量输出；抛 ⇒ WARN 带异常串。
    match fx.updgate() {
        Ok(g) => {
            if g.code == 0 {
                fx.log("INFO", &format!("M9A 自动更新闸：{}", g.line));
            } else {
                fx.log(
                    "WARN",
                    &format!("M9A 自动更新未完成(退出码 {})，不挡当日轮：{}", g.code, g.joined),
                );
            }
        }
        Err(ArkAbort::Fail(m)) => {
            fx.log("WARN", &format!("M9A 自动更新闸本身抛错，跳过它照常跑：{}", m));
        }
        Err(e) => return Err(e),
    }
    fx.takeover()?;
    // ===== 起模拟器（真身 :103）：判句在链体，executor 只回裸 bool =====
    if !fx.mumu()? {
        fx.log("WARN", "MuMu 未能确认启动，仍尝试让 M9A 自行处理");
    }
    let cfg = fx.cfg()?;
    for c in &cfg.changes {
        fx.log("INFO", &format!("  M9A：{}", c));
    }
    let mut r = r1999_daily_step(fx, "1999")?;
    // ===== 批次 8：重试分流问 decide-daily-retry(panel-cs)（真身 :108-127）=====
    // 判据(not-started/empty-queue/blocked 只有 Rescued 才翻案)、if/elseif 链的落穿语义
    // 与全部话术都在 C#；链体只回放日志 + 落地"活"(Stop-M9A / sleep 20 / 重跑)。
    let (retryable, logs) = decide(&r.status, r.rescued, input.retry_once)?;
    for (lvl, text) in &logs {
        let level = if lvl.is_empty() || lvl.eq_ignore_ascii_case("INFO") { "INFO" } else { lvl.as_str() };
        fx.log(level, text);
    }
    if retryable {
        fx.stop()?;
        // 20 秒，不是 ark 那一路的 25 —— 真身 :130 写的就是这个数（plan §11.39 ⑤-4：两套话术不许统一）。
        fx.sleep(20)?;
        r = r1999_daily_step(fx, "1999重试")?;
    }
    fx.stop()?;
    // ===== 活动审计闸（真身 :150-165）：只在队列真跑完的形态问 =====
    // 状态名册不许造新词：这一支的条件就是 'ok'/'empty-queue' 两枚（真身 `-in @('ok','empty-queue')`）。
    // 审计是加分项，任何失败只 WARN，不许把当日轮带崩 —— catch 边界照真身。
    if r.status.eq_ignore_ascii_case("ok") || r.status.eq_ignore_ascii_case("empty-queue") {
        match fx.audit() {
            Ok(g) => {
                if g.code == 0 {
                    fx.log("INFO", &format!("1999 活动审计：{}", g.line));
                } else {
                    fx.log(
                        "WARN",
                        &format!("1999 活动审计没审成(退出码 {})，维持现状：{}", g.code, g.joined),
                    );
                }
            }
            Err(ArkAbort::Fail(m)) => {
                fx.log("WARN", &format!("1999 活动审计本身抛错，跳过它照常收尾：{}", m));
            }
            Err(e) => return Err(e),
        }
    }
    // 终态三格（status/detail/duration）与 ark 那一路同形 —— 真身虽然 `return $r` 整块（还带
    // Rescued/Tasks 等格），但框架侧（modules.ps1:419-423）只读这三格，done 信封就多带只会多一处比。
    Ok(ArkDOut { status: r.status, detail: r.detail, duration: r.duration })
}

/// 一档的回放：夹具 Fx + 夹具 decision 走一遍链体，返回投影串（`L=…|S=…|D=…`，与 live 门同形）。
/// `decide` 是重试分流处的注入点：真出口传 arksk_call_decide（真跑 panel-cs.exe），
/// 自检传同语义的替身（自检不许要外部依赖）。`obs_n` 是 obsrec 桩的账本净计数（B-1a 起跨档连续，
/// 与新侧真账本对齐）。Ask 只属于驱动档 —— 夹具世界冒出来就是内部错。
fn arkskip_run_case(
    sc: &ArkskScen,
    decide: &mut dyn FnMut(&str, bool) -> Result<(bool, Vec<(String, String)>), String>,
    obs_n: &mut i64,
) -> Result<String, String> {
    let input = ArkDInput {
        launch: sc.launch,
        retry_once: sc.retry_once,
        decision: arksk_fixture_decision(sc),
    };
    let mut fx = FixtureFx::new(sc, obs_n);
    match ark_daily_chain(&input, &mut fx, decide) {
        Ok(out) => Ok(format!("L={}|S={}|D={}", fx.seq.join("~"), out.status, out.detail)),
        Err(ArkAbort::Fail(m)) => Err(m),
        Err(ArkAbort::Ask) => Err("夹具世界不该冒出 Ask（Ask 只属于驱动档，出现即内部错）".to_string()),
        Err(ArkAbort::Proto(m)) => Err(m),
    }
}

// ==================== 驱动档（R3③：换头重入协议，Start-GdArkDaily 的 Rust 宿主） ====================
// 换头形态：PS 壳起 gdcore **一次一动作一进程** —— 进程从 --journal 重放已办事项回到断点，
// 把下一个要办的动作写成 ask 信封(seq/action/params)退出；壳照单执行完，把结果写 --res 回执，
// 再起下一进程。三件文件：--ctx（壳写一遍：launch/retry_once/decision）、--journal（两端共写：
// init 行 + 记录行 + 可选 done 行，NDJSON）、--res（壳写：单条回执，读后即删）。
//
// journal 是**唯一权威账**：重放时逐行校验"账上动作与链体要点的是不是同一件"（对不上 = journal
// 与现树不同源 → Proto），新区才追加新行。写失败一切转为驱动协议错（rc=1），因为"断点记不住"
// 这件事本身就该当场拒跑，不许静默继续。
//
// 退出码：0（ask/done）/ 2（缺旗标拒跑）/ 1（协议或内部错）。
// role 有两枚（B-2d④ 起）：`--drive-ark-daily` 恒 "R3-drive-ark-daily"（历史字面，arkdrive 等价门钉着它），
// `--drive-m9a-daily` 恒 "B2d-drive-m9a-daily"。两路共用同一套记账与信封模板，差别只在 role/why 与名册。

const DRIVE_WHY: &str = "驱动档是换头专用口：本进程只推链与记账，链体与已搬动作（mark/obsrec/admin）在本进程内跑，其余现场动作由 PS 侧照 ask 的 seq/action 执行后写 --res 回执再重入（缺 --i-drive 直接拒跑）";

/// m9a 那一路的 why（B-2d④）：**不照抄上面那句** —— 那句里"已搬动作（mark/obsrec/admin）"是 ark
/// 链体的事实，1999 这一路八格里没有这三格，真做的只有 run。字面沿用会造出"核认识 1999 专有话术
/// 却说错了"的第二处事实源，所以另立一句、把真做的那一格点名。
const DRIVE_WHY_M9A: &str = "驱动档是换头专用口：本进程只推 1999 的链与记账，链体与 run 真做在本进程内跑，其余七格现场动作由 PS 侧照 ask 的 seq/action 执行后写 --res 回执再重入（缺 --i-drive 直接拒跑）";

/// B-3②-b3 轮骨架的 why：**第三枚 role 才有第三句 why**。这句不许照抄上面任何一句 ——
/// 那两句说的是"链体"，本档管的是**轮**：十段骨架在本进程内逐段走，链体只是它的第九段
/// 各自要调的东西（ark 一路 + r1999 一路挂在一本账上）。
/// ⚠ **名册句不带枚数**（§11.55 ② 定档）：这一档"哪些格直答、哪些格外发"按**族**点名就够，
/// 写成数字就变成第二份事实源 —— 直答面随 b1/b2 的 verb 一颗一颗长，话术每改一次就要重数一遍，
/// 数错的那一句还会被当成判据。链体可直答的那些格由 `round_direct` 一家说了算，
/// `updgate`／`audit` 两格在射程外（真身是再起一个 pwsh 子进程，见 §11.52 ②）。
/// 话术里不许吹"全自动"：外发那几格此刻仍由外侧照 ask 办，本批 `lib\*.ps1` 一行未动（②-d 才转接）；
/// 也不许吹"完全不经 PS"：`updgate`／`audit` 那两格的工具子进程照旧是 pwsh（§11.52 的降级话术）。
const DRIVE_WHY_ROUND: &str = "轮骨架档是换头专用口：本进程逐段走十段轮骨架（选路/环境/提权闸/门表/判定/锁/监督/睡眠屏蔽/逐路链体/收尾汇总通知），链体现场件与本轮的提权读态·通知在本进程内直答，门表·判定·预览·收尾·监督·历史·提权与屏保读数照 ask 的 seq/action 由外侧执行后写 --res 回执再重入（缺 --i-round 直接拒跑）";

/// 七道拒跑闸（顺序即优先级）。七句都点名所缺旗标 —— 自检逐句咬字。
/// 第七闸是 B-2c②「日志侧信道」加上的：链上日志不再攒在内存里等收尾，而是按产生顺序
/// 逐行追加进 --log-sink 指的侧信道文件，由 PS 侧边跑边排空 —— 没有落点就等于整轮日志不落盘，
/// 所以缺这一枚旗标必须当场拒跑，不许退化成"照旧攒着"。
fn drive_refusal(
    i_drive: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
) -> Option<String> {
    if !i_drive {
        return Some("驱动档是换头专用口：缺 --i-drive / --drive-ark-daily 系列参数时直接拒跑（默认档不许启用它）".to_string());
    }
    if journal.is_empty() {
        return Some("驱动档要 --journal 指到会话 journal 文件：重入状态没落点，断点记不住".to_string());
    }
    if ctx.is_empty() {
        return Some("驱动档要 --ctx 指到上下文文件：launch/retry_once/decision 三格没来路".to_string());
    }
    if res.is_empty() {
        return Some("驱动档要 --res 指到回执文件：PS 侧干完一头的落点没给".to_string());
    }
    if panel_cs.is_empty() {
        return Some("驱动档要 --panel-cs 指到判定 exe：重试分流真跑 decide-daily-retry，缺它试不了".to_string());
    }
    if state.is_empty() {
        return Some("驱动档要 --state 指到 state 目录：已搬动作（mark/obsrec/admin）的账要落到生产同一处".to_string());
    }
    if log_sink.is_empty() {
        return Some("驱动档要 --log-sink 指到日志侧信道文件：链上日志按产生顺序逐行落这里，攒在内存里等收尾等于整轮不落盘".to_string());
    }
    None
}

/// m9a 那一路的七道拒跑闸（B-2d④）：**同序、同数量、不同句**（plan §11.39 ⑤）。
/// 不同句不是修辞选择：ark 那七句里点名的是 launch/decision/mark/obsrec/admin 这一路的名册，
/// 1999 这一路一格都不吃它们 —— 共用句子等于让 m9a 的拒跑话术指着一份不存在的名册。
/// 顺序仍是判据（第七闸 log-sink 最末，与 ark 侧同位），自检逐句咬字。
fn drive_m9a_refusal(
    i_drive: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
) -> Option<String> {
    if !i_drive {
        return Some("驱动档是换头专用口：缺 --i-drive / --drive-m9a-daily 系列参数时直接拒跑（默认档不许启用它）".to_string());
    }
    if journal.is_empty() {
        return Some("驱动档要 --journal 指到会话 journal 文件：1999 这一路的断点没落点，重入记不住".to_string());
    }
    if ctx.is_empty() {
        return Some("驱动档要 --ctx 指到上下文文件：retry_once/m9aRun 两格没来路".to_string());
    }
    if res.is_empty() {
        return Some("驱动档要 --res 指到回执文件：八格现场动作（真做的只有 run）干完之后的落点没给".to_string());
    }
    if panel_cs.is_empty() {
        return Some("驱动档要 --panel-cs 指到判定 exe：1999 的重试分流真跑 decide-daily-retry，缺它试不了".to_string());
    }
    if state.is_empty() {
        return Some("驱动档要 --state 指到 state 目录：run 真做的救火账要落到生产同一处".to_string());
    }
    if log_sink.is_empty() {
        return Some("驱动档要 --log-sink 指到日志侧信道文件：1999 链上日志按产生顺序逐行落这里，攒在内存里等收尾等于整轮不落盘".to_string());
    }
    None
}

/// B-3②-b3 轮骨架的拒跑闸：前七闸**同序同句形**（驱动协议没变，变的是这一档叫什么名），
/// 第八闸是本档独有的 `--root`。为什么必须新立一枚旗标：这一档要在位性现问"这棵树上装了哪几路"
/// （判据 = `lib\maa.ps1` / `lib\m9a.ps1` 在不在），而这两条判据**不在任何一枚 ctx verb 的回执里**
/// —— panel-cs 那些 verb 管的是现场动作，不管工程树形状。本机路径只作参数喂进来，
/// 不进 journal、不进 ask 信封、不进日志（§11.52 ④「本机值不进机读面」那条纪律在这里同样成立）。
/// 顺序仍是判据，自检逐句咬字。
fn round_refusal(
    i_round: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
    root: &str,
) -> Option<String> {
    if !i_round {
        return Some("轮骨架档是换头专用口：缺 --i-round / --daily-round 系列参数时直接拒跑（默认档不许启用它）".to_string());
    }
    if journal.is_empty() {
        return Some("轮骨架档要 --journal 指到会话 journal 文件：十段的断点没落点，重入记不住".to_string());
    }
    if ctx.is_empty() {
        return Some("轮骨架档要 --ctx 指到轮上下文文件：ids/gates/opt/force/dryRun/nameField/noticeOwner/tag 与 maaRun/m9aRun 没来路".to_string());
    }
    if res.is_empty() {
        return Some("轮骨架档要 --res 指到回执文件：外发那些段动作干完之后的落点没给".to_string());
    }
    if panel_cs.is_empty() {
        return Some("轮骨架档要 --panel-cs 指到判定 exe：现场件（锁/通知/进程树/兜底）全在这一枚上，缺它一段都走不动".to_string());
    }
    if state.is_empty() {
        return Some("轮骨架档要 --state 指到 state 目录：运行锁与本轮账要落到生产同一处".to_string());
    }
    if log_sink.is_empty() {
        return Some("轮骨架档要 --log-sink 指到日志侧信道文件：十段日志按产生顺序逐行落这里，攒在内存里等收尾等于整轮不落盘".to_string());
    }
    if root.is_empty() {
        return Some("轮骨架档要 --root 指到工程根：这一档要现问「这棵树上装了哪几路」（lib\\maa.ps1 / lib\\m9a.ps1 在不在），判定不在任何一枚 verb 里".to_string());
    }
    None
}

/// 往 journal 尾追一行（NDJSON）。壳与核共用这一把 —— 行格式只此一处。
fn drive_append_row(path: &str, row: &JVal) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("打开 {} 失败：{}", path, e))?;
    f.write_all(format!("{}\n", jv_to_json(row)).as_bytes())
        .map_err(|e| format!("写入 {} 失败：{}", path, e))
}

/// 往日志侧信道尾追一行（NDJSON，`{level,text}`）。B-2c② 的本体：链上日志按**产生顺序**即时落这里，
/// 重放段一行不写（判据在 DriveFx::log 那一处）。行格式只此一处 —— 壳侧照这一行一把解析。
fn drive_sink_row(path: &str, level: &str, text: &str) -> Result<(), String> {
    drive_append_row(
        path,
        &jv_o(vec![("level", jv_s(level)), ("text", jv_s(text))]),
    )
}

/// 读整本 journal：返回（init 行内层, 记录行数组, 末行 done 内层）。行级校验全在这里，
/// 读不过去 = 这轮不许跑（半截文件上继续写只会把账搅浑）。
fn drive_load_journal(text: &str) -> Result<(JVal, Vec<JVal>, Option<JVal>), String> {
    let mut init: Option<JVal> = None;
    let mut rows: Vec<JVal> = Vec::new();
    let mut done: Option<JVal> = None;
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row = jv_parse(line).map_err(|e| format!("第 {} 行不是合法 JSON：{}", i + 1, e))?;
        if done.is_some() {
            return Err(format!("第 {} 行：done 行之后还有内容", i + 1));
        }
        if jv_has(&row, "done") {
            done = Some(row);
            continue;
        }
        if jv_has(&row, "init") {
            if init.is_some() {
                return Err(format!("第 {} 行：init 行出现两次", i + 1));
            }
            init = Some(jv_get(&row, "init"));
            continue;
        }
        if init.is_none() {
            return Err(format!("第 {} 行：记录行出现在 init 之前（半截文件？）", i + 1));
        }
        let seq = jv_int(&jv_get(&row, "seq"), 0);
        if seq != rows.len() as i64 + 1 {
            return Err(format!("第 {} 行：seq={} 不连续（账上已有 {} 行）", i + 1, seq, rows.len()));
        }
        if jv_str(&jv_get(&row, "action")).is_empty() {
            return Err(format!("第 {} 行：action 是空的", i + 1));
        }
        if jv_has(&row, "data") == jv_has(&row, "err") {
            return Err(format!("第 {} 行：data 与 err 必须恰有一个", i + 1));
        }
        rows.push(row);
    }
    match init {
        Some(v) => Ok((v, rows, done)),
        None => Err("没有 init 行（半截文件？）".to_string()),
    }
}

/// 合并一条 --res 回执：None=这条已并过（幂等，忽略）；Some(行)=新行（规范形）。
/// seq 必须恰是账上下一行号 —— 落后是重放、超前是错位，两种都不许静默。
fn drive_merge_res(text: &str, need_seq: i64) -> Result<Option<JVal>, String> {
    let r = jv_parse(text).map_err(|e| format!("res 不是合法 JSON：{}", e))?;
    let seq = jv_int(&jv_get(&r, "seq"), 0);
    if seq < need_seq {
        return Ok(None);
    }
    if seq > need_seq {
        return Err(format!("seq={} 越界（账上下一行应是 {}）", seq, need_seq));
    }
    let action = jv_str(&jv_get(&r, "action"));
    if action.is_empty() {
        return Err("action 是空的".to_string());
    }
    let ok = match jv_get(&r, "ok") {
        JVal::B(b) => b,
        _ => return Err("ok 不是布尔".to_string()),
    };
    if ok {
        if jv_has(&r, "err") {
            return Err("ok 为真却带了 err".to_string());
        }
        let data = jv_get(&r, "data");
        Ok(Some(jv_o(vec![("seq", JVal::I(seq)), ("action", jv_s(&action)), ("data", data)])))
    } else {
        if jv_has(&r, "data") {
            return Err("ok 为假却带了 data".to_string());
        }
        let err = jv_str(&jv_get(&r, "err"));
        if err.is_empty() {
            return Err("ok 为假却没给 err".to_string());
        }
        Ok(Some(jv_o(vec![("seq", JVal::I(seq)), ("action", jv_s(&action)), ("err", jv_s(&err))])))
    }
}

/// 重放一行的去路：Data=办成的结果（日志行也是），Fail=账上记着失败（重放为链体 Fail）。
enum DriveTake {
    Data(JVal),
    Fail(String),
}

/// 驱动侧的 Fx：把链体的 16 个宿主需求翻译成"重放校验 / 新动作执行"。缺口语义在这里：
/// - 重放段动作对不上 = journal 与现树不同源 → io_err（Proto，任何 catch 都不接）；
/// - 新区写 journal 失败 → io_err（同上，断点记不住不许继续）；
/// - 新区动作：已搬集合（mark/obsrec/admin）在本进程内真做并记 journal 成功行；
///   其余动作 = 记 pending 断点 + Err(Ask) 冒泡（链体不 catch Ask —— 它就是本档的暂停信号）。
struct DriveFx<'a> {
    journal: &'a str,
    /// state 目录（已搬动作的账与生产落同一处；write 前不建目录 —— 写不成就是 Fail，照 PS 真身）。
    state_dir: &'a str,
    /// 日志侧信道落点（B-2c②）：新区每产生一条日志就按顺序追加一行，壳侧边跑边排空。
    log_sink: &'a str,
    /// run 真做的现场输入（B-2c③）：ctx 的 `maaRun` 那 13 枚，state_dir 由 `--state` 补上。
    scen: MaaScen,
    /// run 真做的注入点（照 `decide` 的先例）：真出口=`drive_real_run`，自检/门=桩或 `--fixture-run`。
    run_maa: &'a mut dyn FnMut(&MaaScen, &[String]) -> Result<ArkRunOut, String>,
    /// B-2d④：m9a 那一路 run 真做的现场输入（ctx 的 `m9aRun` 那 15 枚，state_dir 由 `--state` 补上）。
    /// 与 `scen` 并列**不是冗余**：两路的 ctx 载荷键名交集为零（一边 maaDir/maaGuiLog，一边
    /// m9aDir/adb/serial），塞进同一枚 MaaScen 会让两本 ctx 各自多出一堆死格 —— 而 ark 侧那本
    /// journal 的 init 行要与 old 代逐字节可比（arkdrive 门的 H0 面），多一格就红一格。
    m9a_scen: M9aScen,
    /// m9a 那一路 run 真做的注入点。ark 那一路传一枚**永不触发**的桩（触发即内部错）：
    /// `ark_daily_chain` 只会调 `run(chains)`，`r1999_daily_chain` 只会调 `run()` —— 两枚 trait
    /// 方法名相同但签名不同，所以编译期就分开了；这一枚桩是**运行期**的那道防线。
    run_m9a: &'a mut dyn FnMut(&M9aScen) -> Result<M9aRunOut, String>,
    rows: Vec<JVal>,
    /// 启动时（load+merge 后）的记录行数：这之后的才是本轮新区。
    replay_end: usize,
    /// 已消费行数（重放推进与新区写入同步推进）。
    consumed: usize,
    /// 本轮已写新行数（seq 基准 = replay_end + written + 1）。
    written: usize,
    /// 新区动作的断点：ask 信封从这里渲染。
    pending: Option<(i64, &'static str, JVal)>,
    /// 驱动协议错。log 没有 Result 通道，只能落这里；下个动作 check_io 冒泡。
    io_err: Option<String>,
    /// B-3②-b3 **第三答手**：链体那些现场格（ark 一路 + m9a 一路）在 panel-cs 里有了直答件之后，
    /// 驱动档不再需要 PS 侧替它办 —— 这一枚闭包就是"核自己答"。三档取值（`ans` 里分派，顺序即判据）：
    /// - 重放段（journal 已有这行）→ `take_act` 还原，**绝不调闭包**（副作用不重做）；
    /// - 新区且闭包答 `Some` → 现场真做、账行落 journal、`Lines` 只进侧信道；
    /// - 新区且闭包答 `None`（**或闭包压根为空**＝`--drive-ark-daily` / `--drive-m9a-daily` 两枚现出口）
    ///   → 退回 `ask`，**两枚现出口逐字节不变**：这一刀改的是"谁来答"，不是"问什么"。
    /// 为什么把"不答"做成闭包的返回值而不是只靠 `Option<闭包>`：哪些格有直答件这件事必须有**一份来源**
    /// （答手自己那张名册），在两枚现出口里它恒为空、在轮骨架里它在场但对**骨架自己外发的那些格**与
    /// **链体射程外那两格**（`updgate`／`audit`，真身是再起 pwsh 子进程）答 `None` ——
    /// 若"不答"只能靠"闭包为空"表达，轮骨架就得为每枚不同 role 各造一枚闭包，名册反而散成两份。
    direct: Option<&'a mut dyn FnMut(&str, &str, &JVal) -> Result<Option<DirectOut>, String>>,
}

/// 第三答手的一格回执：`data` 进 journal（与 PS executor 当年写的 res `data` 同形，投影在答手里做完），
/// `lines` 是 panel-cs 那侧的 `Lines`（**纯字符串数组**：`Chain.cs` 的 `Say(lines, lvl, msg)` 只
/// `lines.Add(msg)`，级别只进子进程自己的 stdout，信封里根本没这枚格）。因此级别在这里无从得知 ——
/// 逐行按 INFO 进侧信道，这条分岔已登记（docs §11.55）；**绝不进 journal**（账上多一行就等于改两侧投影）。
struct DirectOut {
    data: JVal,
    lines: Vec<String>,
}

impl DirectOut {
    fn only(data: JVal) -> DirectOut {
        DirectOut { data, lines: Vec::new() }
    }
    fn with_lines(data: JVal, lines: Vec<String>) -> DirectOut {
        DirectOut { data, lines }
    }
}

impl<'a> DriveFx<'a> {
    fn new(
        journal: &'a str,
        state_dir: &'a str,
        log_sink: &'a str,
        scen: MaaScen,
        run_maa: &'a mut dyn FnMut(&MaaScen, &[String]) -> Result<ArkRunOut, String>,
        m9a_scen: M9aScen,
        run_m9a: &'a mut dyn FnMut(&M9aScen) -> Result<M9aRunOut, String>,
        rows: Vec<JVal>,
        direct: Option<&'a mut dyn FnMut(&str, &str, &JVal) -> Result<Option<DirectOut>, String>>,
    ) -> Self {
        let replay_end = rows.len();
        DriveFx {
            journal,
            state_dir,
            log_sink,
            scen,
            run_maa,
            m9a_scen,
            run_m9a,
            rows,
            replay_end,
            consumed: 0,
            written: 0,
            pending: None,
            io_err: None,
            direct,
        }
    }
    /// 链上日志的唯一实现（B-2d④ 起两路 trait 各有一枚 `log`，body 必须同一处）：
    /// 重放段逐行校验 `level|text`，新区追加 journal 行 + **同序**落侧信道。
    /// 两路日志的记账动作名同为 `"log"`、行格式同为 `{level,text}` —— 这是驱动协议本身，
    /// 不是某一路的话术，所以合并；两路**链体句子**一个字都不在这里（那些住各自的 chain）。
    fn fx_log(&mut self, level: &str, text: &str) {
        // log 没有 Result 通道：驱动协议错一律落 io_err，等动作面 check_io（或收口时）检查。
        if self.io_err.is_some() {
            return;
        }
        if self.replaying() {
            match self.take_act("log") {
                Ok(DriveTake::Data(d)) => {
                    let lv = jv_str(&jv_get(&d, "level"));
                    let tx = jv_str(&jv_get(&d, "text"));
                    if lv != level || tx != text {
                        self.io_err = Some(format!(
                            "journal 对不上：账上 [{}|{}] vs 链体 [{}|{}]（journal 与现树不同源）",
                            lv, tx, level, text
                        ));
                    }
                }
                Ok(DriveTake::Fail(_)) => {
                    self.io_err = Some("journal 对不上：log 行带了 err（journal 与现树不同源）".to_string());
                }
                Err(ArkAbort::Proto(m)) => {
                    self.io_err = Some(m);
                }
                Err(_) => {
                    self.io_err = Some("journal 对不上：取记录行出了非 Proto 错（内部错）".to_string());
                }
            }
        } else {
            let d = jv_o(vec![("level", jv_s(level)), ("text", jv_s(text))]);
            self.append_soft("log", d);
            // 侧信道：新区日志即时落 sink（重放段一行不写 —— 那些壳侧上一轮已经排空过）。
            // 与 journal 同一把 io_err 语义：写不成不当场抛，落 io_err 等下个动作 check_io 冒泡。
            if self.io_err.is_none() {
                if let Err(m) = drive_sink_row(self.log_sink, level, text) {
                    self.io_err = Some(format!("日志侧信道追加失败：{}", m));
                }
            }
        }
    }
    fn replaying(&self) -> bool {
        self.consumed < self.replay_end
    }
    /// 两枚闸动作同形（回执三格 code/line/joined），共用这一把；ask 无参（子进程怎么起是 executor 的事）。
    /// **不是 trait 方法**：`R1999DailyFx` 里只声明 `updgate`/`audit` 两格，本枚住固有 impl ——
    /// trait impl 块里只能出现 trait 声明过的方法，把共用件放这儿是协议层，不是话术层。
    fn gate(&mut self, action: &'static str) -> Result<M9aGateOut, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act(action)? {
                DriveTake::Data(d) => Ok(M9aGateOut {
                    code: jv_int(&jv_get_ci(&d, "code"), 0),
                    line: jv_str(&jv_get_ci(&d, "line")),
                    joined: jv_str(&jv_get_ci(&d, "joined")),
                }),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask(action, JVal::Null)
        }
    }
    fn check_io(&mut self) -> Result<(), ArkAbort> {
        match &self.io_err {
            Some(m) => Err(ArkAbort::Proto(m.clone())),
            None => Ok(()),
        }
    }
    /// 新区：把一行追加进 journal（成功才推进计数；失败落 io_err）。
    fn append_soft(&mut self, action: &str, data: JVal) {
        let seq = self.replay_end + self.written + 1;
        let row = jv_o(vec![("seq", JVal::I(seq as i64)), ("action", jv_s(action)), ("data", data)]);
        match drive_append_row(self.journal, &row) {
            Ok(()) => {
                self.rows.push(row);
                self.written += 1;
                self.consumed += 1;
            }
            Err(m) => {
                self.io_err = Some(format!("journal 追加失败：{}", m));
            }
        }
    }
    /// 新区：把一行**失败**追加进 journal（{seq,action,err}——重放时 take_act 会还原成 Fail）。
    /// 与 append_soft 同账同序，成功/失败只差有没有 data。写失败同样落 io_err。
    fn append_err(&mut self, action: &str, err: &str) {
        let seq = self.replay_end + self.written + 1;
        let row = jv_o(vec![("seq", JVal::I(seq as i64)), ("action", jv_s(action)), ("err", jv_s(err))]);
        match drive_append_row(self.journal, &row) {
            Ok(()) => {
                self.rows.push(row);
                self.written += 1;
                self.consumed += 1;
            }
            Err(m) => {
                self.io_err = Some(format!("journal 追加失败：{}", m));
            }
        }
    }
    /// 重放段取一行：动作名对不上 → Proto；err 形态 → Fail；否则 Data。
    fn take_act(&mut self, expect: &'static str) -> Result<DriveTake, ArkAbort> {
        let row = self.rows.get(self.consumed).cloned().unwrap_or(JVal::Null);
        let act = jv_str(&jv_get(&row, "action"));
        if act != expect {
            return Err(ArkAbort::Proto(format!(
                "journal 第 {} 行对不上：账上动作是 [{}]，链体要点的是 [{}]（journal 与现树不同源）",
                jv_int(&jv_get(&row, "seq"), 0),
                act,
                expect
            )));
        }
        self.consumed += 1;
        if jv_has(&row, "err") {
            Ok(DriveTake::Fail(jv_str(&jv_get(&row, "err"))))
        } else {
            Ok(DriveTake::Data(jv_get(&row, "data")))
        }
    }
    /// 新区动作：记断点 + 暂停。
    fn ask<T>(&mut self, action: &'static str, params: JVal) -> Result<T, ArkAbort> {
        let seq = self.replay_end + self.written + 1;
        self.pending = Some((seq as i64, action, params));
        Err(ArkAbort::Ask)
    }
    /// B-3②-b3 第三答手的唯一入口（链体十六格与轮骨架格共用这一把）。
    /// `role` 必须是**调用它的那枚 trait impl 的字面**（`impl ArkDailyFx` 里恒 "ark"、
    /// `impl R1999DailyFx` 里恒 "r1999"），不是构造期字段 —— 一轮里同一枚 DriveFx 会先后服务
    /// 两路链，把 role 焊在 new 上就只有第一条链拿得到自己的名字。
    /// `decode` 只在三档都成立后跑：重放段喂账上的 data、直答喂答手的 data、`ask` 那档根本不返回。
    /// 所以**同一枚 decode 就是三档的同一把尺**——老驱动档的 replay 形状与新直答的落账形状
    /// 在这里被迫一致，这正是这一刀要的判据（两侧各写一份 decode 迟早漂）。
    fn ans<T, F: FnOnce(&JVal) -> T>(
        &mut self,
        role: &'static str,
        action: &'static str,
        params: JVal,
        decode: F,
    ) -> Result<T, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            return match self.take_act(action)? {
                DriveTake::Data(d) => Ok(decode(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            };
        }
        let out = match self.direct.as_mut() {
            Some(f) => f(role, action, &params),
            None => Ok(None),
        };
        match out {
            // 答手不答这一格（轮骨架外发那十三格）⇒ 与闭包为空走同一句 `ask`：
            // 问法、信封形状、重放依据全部照旧，这一刀改的只是"谁来答"。
            // ⚠ 链体射程外那两格（`updgate`／`audit`）**根本不进这一枚** —— 它们走 `gate`，
            // 那里没有答手；"直答件答不了"与"直答件选择不答"是两件事，名册别混。
            Ok(None) => self.ask(action, params),
            Ok(Some(o)) => {
                // Lines 只进侧信道（不进账）。写侧信道失败落 io_err，与 fx_log 同一把语义。
                for ln in &o.lines {
                    if let Err(m) = drive_sink_row(self.log_sink, "INFO", ln) {
                        self.io_err = Some(format!("日志侧信道追加失败：{}", m));
                    }
                }
                self.check_io()?;
                self.append_soft(action, o.data.clone());
                self.check_io()?;
                Ok(decode(&o.data))
            }
            Err(m) => {
                self.append_err(action, &m);
                self.check_io()?;
                Err(ArkAbort::Fail(m))
            }
        }
    }
}

impl ArkDailyFx for DriveFx<'_> {
    fn log(&mut self, level: &str, text: &str) {
        // B-2d④：实现抽到 `DriveFx::fx_log`（m9a 那一路的 log 也走同一处）——
        // 记账动作名、重放校验、侧信道同序落行，这三件是驱动协议而不是某一路的话术。
        self.fx_log(level, text);
    }
    fn desk(&mut self) -> Result<String, ArkAbort> {
        self.ans("ark", "desk", JVal::Null, |d| jv_str(d))
    }
    fn svdetail(&mut self) -> Result<String, ArkAbort> {
        self.ans("ark", "svdetail", JVal::Null, |d| jv_str(d))
    }
    fn takeover(&mut self) -> Result<(), ArkAbort> {
        self.ans("ark", "takeover", JVal::Null, |_| ())
    }
    fn cfg(&mut self, kind: ArkCfgKind) -> Result<ArkCfgOut, ArkAbort> {
        let params = match &kind {
            ArkCfgKind::Main => jv_o(vec![("kind", jv_s("main"))]),
            ArkCfgKind::Lean { stage } => jv_o(vec![("kind", jv_s("lean")), ("stage", jv_s(stage))]),
        };
        self.ans("ark", "cfg", params, |d| ArkCfgOut {
            changes: jv_list(&jv_get_ci(d, "changes")).into_iter().map(jv_str).collect(),
            chains: jv_list(&jv_get_ci(d, "chains")).into_iter().map(jv_str).collect(),
        })
    }
    fn entryverb(&mut self, game: &str, verb: &str) -> Result<String, ArkAbort> {
        self.ans("ark", "entryverb", jv_o(vec![("game", jv_s(game)), ("verb", jv_s(verb))]), |d| jv_str(d))
    }
    fn launch(&mut self) -> Result<bool, ArkAbort> {
        self.ans("ark", "launch", JVal::Null, |d| jv_truthy(d))
    }
    fn run(&mut self, chains: &[String]) -> Result<ArkRunOut, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("run")? {
                DriveTake::Data(d) => {
                    let dur = jv_get_ci(&d, "duration");
                    Ok(ArkRunOut {
                        status: jv_str(&jv_get_ci(&d, "status")),
                        detail: jv_str(&jv_get_ci(&d, "detail")),
                        duration: if matches!(dur, JVal::Null) { None } else { Some(jv_to_d(&dur)) },
                    })
                }
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            // 已搬动作（B-2c③）：run 真做，不再 ask。chains 是链体每次现给的期望链（cfg 回执里的
            // ExpectedChains），不进 ctx —— ctx 给的是"在哪儿跑、跑多久算卡"那 13 枚现场参数。
            // 账行形状与 ask 时代壳写的 res data 逐字同形（status→detail→duration，无 duration 则键不出现），
            // 所以 old 代与 new 代的 journal 在这一格仍可比；两侧 err 文本不同源（一侧 PS 异常串、
            // 一侧核内消息），已登记为"换宿主必不同源"那类格，不进等值判据。
            let scen = self.scen.clone();
            let ch = chains.to_vec();
            match (self.run_maa)(&scen, &ch) {
                Ok(o) => {
                    let mut kv: Vec<(&str, JVal)> =
                        vec![("status", jv_s(&o.status)), ("detail", jv_s(&o.detail))];
                    if let Some(d) = o.duration {
                        kv.push(("duration", JVal::F(d)));
                    }
                    self.append_soft("run", jv_o(kv));
                    self.check_io()?;
                    Ok(o)
                }
                Err(m) => {
                    self.append_err("run", &m);
                    self.check_io()?;
                    Err(ArkAbort::Fail(m))
                }
            }
        }
    }
    fn stop(&mut self) -> Result<(), ArkAbort> {
        self.ans("ark", "stop", JVal::Null, |_| ())
    }
    fn sleep(&mut self, seconds: u32) -> Result<(), ArkAbort> {
        self.ans("ark", "sleep", JVal::I(seconds as i64), |_| ())
    }
    fn skip(&mut self) -> Result<Option<ArkSkipData>, ArkAbort> {
        self.ans("ark", "skip", JVal::Null, |d| {
            if matches!(d, JVal::Null) {
                return None;
            }
            Some(ArkSkipData {
                skipped: jv_truthy(&jv_get_ci(d, "skipped")),
                plan: jv_list(&jv_get_ci(d, "plan")).into_iter().map(jv_str).collect(),
                evidence: jv_list(&jv_get_ci(d, "evidence")).into_iter().map(jv_str).collect(),
            })
        })
    }
    fn mark(&mut self, stages: &[String], reason: &str) -> Result<String, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("mark")? {
                DriveTake::Data(d) => Ok(jv_str(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            // 已搬动作（B-1a）：进程内真做，不再 ask —— 账行照写（重放段与 ask 时代逐字节等价）。
            match drive_mark(self.state_dir, stages, reason) {
                Ok(date) => {
                    self.append_soft("mark", jv_s(&date));
                    self.check_io()?;
                    Ok(date)
                }
                Err(m) => {
                    self.append_err("mark", &m);
                    self.check_io()?;
                    Err(ArkAbort::Fail(m))
                }
            }
        }
    }
    fn ledger(&mut self) -> Result<String, ArkAbort> {
        self.ans("ark", "ledger", JVal::Null, |d| jv_str(d))
    }
    fn obs(&mut self) -> Result<JVal, ArkAbort> {
        self.ans("ark", "obs", JVal::Null, |d| d.clone())
    }
    fn obsrec(&mut self, obs: &JVal) -> Result<Option<i64>, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("obsrec")? {
                DriveTake::Data(d) => {
                    if matches!(d, JVal::Null) {
                        return Ok(None);
                    }
                    Ok(Some(jv_int(&d, 0)))
                }
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            // 已搬动作（B-1a）：观测账本进程内写。
            match drive_obsrec(self.state_dir, obs) {
                Ok(n) => {
                    let d = match n {
                        Some(v) => JVal::I(v),
                        None => JVal::Null,
                    };
                    self.append_soft("obsrec", d);
                    self.check_io()?;
                    Ok(n)
                }
                Err(m) => {
                    self.append_err("obsrec", &m);
                    self.check_io()?;
                    Err(ArkAbort::Fail(m))
                }
            }
        }
    }
    fn admin(&mut self) -> Result<bool, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("admin")? {
                DriveTake::Data(d) => Ok(jv_truthy(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            // 已搬动作（B-1a）：当前令牌进程内查（fail-closed，绝不抛）。
            let b = drive_is_admin();
            self.append_soft("admin", JVal::B(b));
            self.check_io()?;
            Ok(b)
        }
    }
}

/// 驱动侧的 m9a 那一路 Fx（B-2d④）：与上面 `impl ArkDailyFx for DriveFx` 共用同一套记账机制
/// （`fx_log` / `take_act` / `append_soft` / `append_err` / `ask`），**八格里真做只有 run**。
/// 动作名就是 journal 里的 `action` 字面，也是 PS 侧 executor 白名单里的那八枚 —— 两侧必须逐字同名，
/// 任一侧漏改都会在白名单处当场抛（ark 侧 B-2c③ 那条"双删即回归抓器"的先例）。
impl R1999DailyFx for DriveFx<'_> {
    fn log(&mut self, level: &str, text: &str) {
        self.fx_log(level, text);
    }
    fn updgate(&mut self) -> Result<M9aGateOut, ArkAbort> {
        self.gate("updgate")
    }
    fn audit(&mut self) -> Result<M9aGateOut, ArkAbort> {
        self.gate("audit")
    }
    fn takeover(&mut self) -> Result<(), ArkAbort> {
        self.ans("r1999", "takeover", JVal::Null, |_| ())
    }
    fn mumu(&mut self) -> Result<bool, ArkAbort> {
        self.ans("r1999", "mumu", JVal::Null, |d| jv_truthy(d))
    }
    fn cfg(&mut self) -> Result<M9aCfgOut, ArkAbort> {
        self.ans("r1999", "cfg", JVal::Null, |d| M9aCfgOut {
            changes: jv_list(&jv_get_ci(d, "changes")).into_iter().map(jv_str).collect(),
        })
    }
    fn stop(&mut self) -> Result<(), ArkAbort> {
        self.ans("r1999", "stop", JVal::Null, |_| ())
    }
    fn sleep(&mut self, seconds: u32) -> Result<(), ArkAbort> {
        self.ans("r1999", "sleep", JVal::I(seconds as i64), |_| ())
    }
    fn run(&mut self) -> Result<M9aRunOut, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("run")? {
                DriveTake::Data(d) => {
                    let dur = jv_get_ci(&d, "duration");
                    Ok(M9aRunOut {
                        status: jv_str(&jv_get_ci(&d, "status")),
                        detail: jv_str(&jv_get_ci(&d, "detail")),
                        duration: if matches!(dur, JVal::Null) { None } else { Some(jv_to_d(&dur)) },
                        rescued: jv_truthy(&jv_get_ci(&d, "rescued")),
                    })
                }
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            // 本档唯一的真做动作：Invoke-M9ARun 那整段循环在核内跑（m9a_run + RealHost），
            // 现场 15 枚参数随 ctx 的 m9aRun 交进去。账行键序 status→detail→duration?→rescued?：
            // 两枚条件补各照各家真身（无 duration=非 ok 档、无 rescued=没兜成），**没有就不写这个键**，
            // 于是这一格在门上与 PS 侧 legacy 当年写的 res data 逐字节可比。
            // 两侧 err 文本不同源（一侧 PS 异常串、一侧核内消息），属"换宿主必不同源"那类格，不进等值判据。
            let scen = self.m9a_scen.clone();
            match (self.run_m9a)(&scen) {
                Ok(o) => {
                    let mut kv: Vec<(&str, JVal)> = vec![("status", jv_s(&o.status)), ("detail", jv_s(&o.detail))];
                    if let Some(d) = o.duration {
                        kv.push(("duration", JVal::F(d)));
                    }
                    if o.rescued {
                        kv.push(("rescued", JVal::B(true)));
                    }
                    self.append_soft("run", jv_o(kv));
                    self.check_io()?;
                    Ok(o)
                }
                Err(m) => {
                    self.append_err("run", &m);
                    self.check_io()?;
                    Err(ArkAbort::Fail(m))
                }
            }
        }
    }
}

/// 已搬动作 mark（B-1a）：写 state\ark-fight-skip-mark.json，返回日期串。
/// 文件字节 = PS 真身 Set-GdArkFightSkipMark 经 pwsh 7 ConvertTo-Json 的输出（形状口径见 ps_to_json_pretty）。
/// 不建目录：写不成 → Err（照真身：Write-GdJson 抛 → 链体侧 WARN）。
fn drive_mark(state_dir: &str, stages: &[String], reason: &str) -> Result<String, String> {
    let date = gd_local_date();
    let doc = jv_o(vec![
        ("Date", jv_s(&date)),
        ("Stages", JVal::A(stages.iter().map(|s| jv_s(s)).collect())),
        ("Reason", jv_s(reason)),
    ]);
    let path = std::path::Path::new(state_dir).join("ark-fight-skip-mark.json");
    std::fs::write(&path, ps_to_json_pretty(&doc).as_bytes())
        .map_err(|e| format!("写 {} 失败：{}", path.display(), e))?;
    Ok(date)
}

/// 已搬动作 obsrec（B-1a）：观测账本进程内写，返回账本条数（Found 假 → None，不落文件）。
/// 语义逐句照 PS 真身 Update-GdArkStageObs（lib\ark-farm.ps1）：
/// - 读账：文件不在/读不出/解析坏 → 空账；Records 缺/null → [null]（@($null) 单元素占位，实测语义：
///   探针 json3 把 @($null) 序列化成 [null]）；空数组摊平为空；标量按单个元素。
/// - Dow：日期解不出 → Err（照真身 Get-Date 抛）；周日 0 → 7。
/// - Survivors：管道语义 —— 缺/null → [""]（$null 过管道恰一次，探针 obsrec-d 实测）；数组逐项 [string]。
/// - 滑窗：>60 条留尾 60。写不成 → Err。
fn drive_obsrec(state_dir: &str, obs: &JVal) -> Result<Option<i64>, String> {
    if !jv_truthy(&jv_get_ci(obs, "Found")) {
        return Ok(None);
    }
    let path = std::path::Path::new(state_dir).join("ark-stage-weekday-obs.json");
    let mut records: Vec<JVal> = Vec::new();
    if path.exists() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let text = text.trim_start_matches('\u{feff}');
            if let Ok(doc) = jv_parse(text) {
                records = match jv_get(&doc, "Records") {
                    JVal::A(a) => a,
                    other => vec![other],
                };
            }
        }
    }
    let date = jv_str(&jv_get_ci(obs, "Date"));
    let dow0 = gd_dow_from_date(&date).ok_or_else(|| format!("日期解析不了：{}", date))?;
    let dow = if dow0 == 0 { 7 } else { dow0 };
    let survivors: Vec<JVal> = match jv_get_ci(obs, "Survivors") {
        JVal::A(a) => a.iter().map(|v| jv_s(&jv_str(v))).collect(),
        JVal::Null => vec![jv_s("")],
        other => vec![jv_s(&jv_str(&other))],
    };
    records.push(jv_o(vec![
        ("Date", jv_s(&date)),
        ("Dow", JVal::I(dow as i64)),
        ("Stage", jv_s(&jv_str(&jv_get_ci(obs, "Stage")))),
        ("LastBattle", JVal::B(jv_truthy(&jv_get_ci(obs, "LastBattle")))),
        ("SawGuiPlan", JVal::B(jv_truthy(&jv_get_ci(obs, "SawGuiPlan")))),
        ("Survivors", JVal::A(survivors)),
        ("Selected", jv_s(&jv_str(&jv_get_ci(obs, "Selected")))),
    ]));
    if records.len() > 60 {
        let cut = records.len() - 60;
        records.drain(0..cut);
    }
    let doc = jv_o(vec![("Records", JVal::A(records.clone()))]);
    std::fs::write(&path, ps_to_json_pretty(&doc).as_bytes())
        .map_err(|e| format!("写 {} 失败：{}", path.display(), e))?;
    Ok(Some(records.len() as i64))
}

/// 已搬动作 admin（B-1a）：当前有效令牌是否 BUILTIN\Administrators（S-1-5-32-544）。
/// 照 PS 真身 Test-GdAdmin 语义：任何一步失败 → false（fail-closed），绝不抛。
fn drive_is_admin() -> bool {
    unsafe {
        let authority = SidIdentifierAuthority { value: [0, 0, 0, 0, 0, 5] };
        let mut sid: *mut c_void = std::ptr::null_mut();
        if AllocateAndInitializeSid(&authority, 2, 32, 544, 0, 0, 0, 0, 0, 0, &mut sid) == 0 {
            return false;
        }
        let mut is_member: i32 = 0;
        let ok = CheckTokenMembership(std::ptr::null_mut(), sid, &mut is_member);
        FreeSid(sid);
        ok != 0 && is_member != 0
    }
}

/// done 行的内层 → 出口 result（status/detail 恒在；duration 有才放）。
fn drive_done_result(d: &JVal) -> JVal {
    let status = jv_str(&jv_get_ci(d, "status"));
    let detail = jv_str(&jv_get_ci(d, "detail"));
    let mut kv: Vec<(&str, JVal)> = vec![("status", jv_s(&status)), ("detail", jv_s(&detail))];
    let dur = jv_get_ci(d, "duration");
    if !matches!(dur, JVal::Null) {
        kv.push(("duration", JVal::F(jv_to_d(&dur))));
    }
    jv_o(kv)
}

/// B-2c② 起日志改走 --log-sink 侧信道，信封 `logs` **留键不删键**：键还在、恒空数组。
/// 判据是「换通道不换形状」——`probe-arkdrive` 等价门的 `C-logs` 面照旧逐字节比，
/// 而壳侧不再等收尾才拿到日志（链上一有日志就落盘、边跑边排空）。
///
/// 两路驱动档各自的信封身份（B-2d④）：role 与 why **成对**，一条链体一套话术。
/// ark 那一路沿用历史字面 `R3-drive-ark-daily`（arkdrive 等价门的 role 校验、老壳侧那两枚
/// 校验都钉着它，改一个字就是形状变）；m9a 那一路是新立 role，**不复用**旧名 —— 两路的
/// ctx/journal 名册不同源，名字共用会让"这一本账是谁的"当场问不出来。
#[derive(Clone, Copy)]
struct DriveRole {
    role: &'static str,
    why: &'static str,
}
const DRIVE_ROLE_ARK: DriveRole = DriveRole { role: "R3-drive-ark-daily", why: DRIVE_WHY };
const DRIVE_ROLE_M9A: DriveRole = DriveRole { role: "B2d-drive-m9a-daily", why: DRIVE_WHY_M9A };

/// B-3②-b3：轮骨架这一档的 role。**新字面 = 新取值域** —— 三枚 role 各自对应一份不同的
/// journal 名册（轮骨架走十段、ark 链体与 m9a 链体各走自己那一本），把它们挤进同一枚字面，
/// "这一本账是谁的"就当场问不出来。这也是本批升 `R7.0 → R8.0` 的依据：
/// ask 信封的 `role` 是出口形状的一部分，出现了历史两枚都不认识的新取值。
const DRIVE_ROLE_ROUND: DriveRole = DriveRole { role: "B3b-round-daily", why: DRIVE_WHY_ROUND };

/// ask 信封的内层：链体停在哪个动作上（seq/action/params），壳照单执行后写 res 回执再重入。
/// 三枚模板里 role/why 是唯一的差别，记账机制与键序两路完全同源 —— 所以这里只有一份字面。
fn drive_env_ask(rl: DriveRole, seq: i64, action: &str, params: &JVal) -> String {
    format!(
        "{{\"ok\":true,\"why\":{},\"contract\":{},\"role\":{},\"kind\":\"ask\",\"seq\":{},\"action\":{},\"params\":{},\"logs\":[]}}",
        jstr(rl.why),
        jstr(CONTRACT),
        jstr(rl.role),
        seq,
        jstr(action),
        jv_to_json(params)
    )
}

/// done 信封的内层：链体整条走完（含 not-started 早退等一切终态），result 就是真身返回的
/// @{Status;Detail;Duration?}。
fn drive_env_done(rl: DriveRole, result: &JVal) -> String {
    format!(
        "{{\"ok\":true,\"why\":{},\"contract\":{},\"role\":{},\"kind\":\"done\",\"result\":{},\"logs\":[]}}",
        jstr(rl.why),
        jstr(CONTRACT),
        jstr(rl.role),
        jv_to_json(result)
    )
}

/// fail 信封的内层：链体 Fail 与驱动协议错共用（后者 error 里带"驱动协议坏了："前缀）。
fn drive_env_fail(rl: DriveRole, error: &str) -> String {
    format!(
        "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":{},\"kind\":\"fail\",\"error\":{},\"logs\":[]}}",
        jstr(rl.why),
        jstr(CONTRACT),
        jstr(rl.role),
        jstr(error)
    )
}

/// 下面三枚是 ark 那一路的薄包装：`drive_ark_daily_envelope` 里那 15 处调用点一字不动，
/// 也不出现"两套信封模板"。m9a 那一路直接调内层并喂 `DRIVE_ROLE_M9A`。
fn render_drive_ask(seq: i64, action: &str, params: &JVal) -> String {
    drive_env_ask(DRIVE_ROLE_ARK, seq, action, params)
}
fn render_drive_done(result: &JVal) -> String {
    drive_env_done(DRIVE_ROLE_ARK, result)
}
fn render_drive_fail(error: &str) -> String {
    drive_env_fail(DRIVE_ROLE_ARK, error)
}

/// 驱动档一轮的信封装配（纯函数；`run_drive_ark_daily` 与 `--selftest` 共用同一处）。
/// 一轮 = 从 journal 重放回断点 → 再走一步链体 → 落 ask / done / fail 三种收场之一。
/// `decide` = 重试分流真跑 decide-daily-retry 的注入点（生产=调 panel-cs；自检=闭包假身，不依赖外部件）。
/// `run_maa` = run 真做的注入点（B-2c③，同一条先例）：生产=`drive_real_run`、自检=桩、门=`--fixture-run` 夹具。
fn drive_ark_daily_envelope(
    i_drive: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
    decide: &mut dyn FnMut(&str, bool) -> Result<(bool, Vec<(String, String)>), String>,
    run_maa: &mut dyn FnMut(&MaaScen, &[String]) -> Result<ArkRunOut, String>,
) -> (i32, String) {
    if let Some(m) = drive_refusal(i_drive, journal, ctx, res, panel_cs, state, log_sink) {
        return (2, render_drive_fail(&m));
    }
    // 侧信道是**一本会话一个文件**（与 journal 同生命周期）：每轮只追加、不重开，
    // 壳侧按字节偏移增量排空 —— 所以这里绝不截断（截断会把上一轮已报过的行抹掉）。
    // ---- ctx：壳写一遍的三格（launch/retry_once/decision） ----
    let ctx_text = match std::fs::read_to_string(ctx) {
        Ok(t) => t,
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：ctx 读不动：{}", e))),
    };
    let ctx_val = match jv_parse(&ctx_text) {
        Ok(v) => v,
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：ctx 不是合法 JSON：{}", e))),
    };
    let launch = match jv_get_ci(&ctx_val, "launch") {
        JVal::B(b) => b,
        _ => return (1, render_drive_fail("驱动协议坏了：ctx 的 launch/retry_once 必须是布尔")),
    };
    let retry_once = match jv_get_ci(&ctx_val, "retry_once") {
        JVal::B(b) => b,
        _ => return (1, render_drive_fail("驱动协议坏了：ctx 的 launch/retry_once 必须是布尔")),
    };
    let decision = jv_get_ci(&ctx_val, "decision");
    if !matches!(decision, JVal::O(_)) {
        return (1, render_drive_fail("驱动协议坏了：ctx 的 decision 必须是对象"));
    }
    // ---- ctx 的 maaRun 子对象（B-2c③ run 真做的现场载荷）----
    // 解析器与剧本档**同一把**（maarun_parse）：13 枚键名与 config\settings.json 逐字同名，立第二套
    // 字面就等于两处口径可以各漂一半。缺 maaRun 不在这里报错（老 ctx 照样能走到 run 之前）；
    // 到 run 真做那一步由 drive_real_run 点名缺哪一格 —— 这样失败落进 journal 的 err 行，可追。
    let mut scen = match maarun_parse(&jv_get_ci(&ctx_val, "maaRun")) {
        Ok(s) => s,
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：ctx 的 maaRun 读不动：{}", e))),
    };
    // state_dir 不在 ctx 里（已有 --state 第六闸）：run 那本救火账与已搬三枚的账落同一处。
    scen.state_dir = state.to_string();
    // ---- journal：账本在手就重放，不在就现起一本 ----
    let mut rows: Vec<JVal>;
    match std::fs::read_to_string(journal) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let init_row = jv_o(vec![("init", ctx_val.clone())]);
            if let Err(m) = drive_append_row(journal, &init_row) {
                return (1, render_drive_fail(&format!("驱动协议坏了：journal 建不起来：{}", m)));
            }
            let _ = std::fs::remove_file(res);
            rows = Vec::new();
        }
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：journal 读不动：{}", e))),
        Ok(text) => {
            let (init, r, done) = match drive_load_journal(&text) {
                Ok(t) => t,
                Err(m) => return (1, render_drive_fail(&format!("驱动协议坏了：journal 读不动：{}", m))),
            };
            if jv_to_json(&init) != jv_to_json(&ctx_val) {
                return (
                    1,
                    render_drive_fail("驱动协议坏了：journal 的 init 与 ctx 不是同一份（换了 ctx 就得起新 journal）"),
                );
            }
            if let Some(drow) = done {
                let _ = std::fs::remove_file(res);
                return (0, render_drive_done(&drive_done_result(&jv_get(&drow, "done"))));
            }
            rows = r;
        }
    }
    // ---- res：上一轮壳的回执（读后即删；≤账上下一行的 seq 是幂等重放，忽略） ----
    match std::fs::read_to_string(res) {
        Ok(text) => {
            let _ = std::fs::remove_file(res);
            let need = rows.len() as i64 + 1;
            match drive_merge_res(&text, need) {
                Ok(Some(row)) => {
                    if let Err(m) = drive_append_row(journal, &row) {
                        return (1, render_drive_fail(&format!("驱动协议坏了：journal 追加失败：{}", m)));
                    }
                    rows.push(row);
                }
                Ok(None) => {}
                Err(m) => return (1, render_drive_fail(&format!("驱动协议坏了：res 对不上：{}", m))),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：res 读不动：{}", e))),
    }
    // ---- 一轮：重放到断点，之后再走一步 ----
    let input = ArkDInput { launch, retry_once, decision };
    // m9a 那两枚字段在这一路是结构性桩（与 m9a envelope 里那枚 ark 桩对称）：ark 链体调不到 run_m9a，
    // 调到了就是分派写错 —— Err 那句当场进 journal 的 err 行，不会被当成"run 办成了"。
    let mut m9a_stub = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
        Err("内部错：ark 链体不许调 m9a 那一路的 run 闭包（DriveFx 分派写错了）".to_string())
    };
    // B-3②-b3 护栏「两枚现出口逐字节不变」：这一路**不给**第三答手（`direct` 为空），
    // `ans` 在新区直接退回 `ask` ⇒ `--drive-ark-daily` 的 ask 信封、journal、退出码一律如旧。
    let no_direct: Option<&mut dyn FnMut(&str, &str, &JVal) -> Result<Option<DirectOut>, String>> = None;
    let mut fx = DriveFx::new(
        journal,
        state,
        log_sink,
        scen,
        &mut *run_maa,
        M9aScen::default(),
        &mut m9a_stub,
        rows,
        no_direct,
    );
    let r = ark_daily_chain(&input, &mut fx, decide);
    match r {
        Ok(o) => {
            if let Some(m) = fx.io_err.take() {
                return (1, render_drive_fail(&format!("驱动协议坏了：{}", m)));
            }
            let mut kv: Vec<(&str, JVal)> = vec![("status", jv_s(&o.status)), ("detail", jv_s(&o.detail))];
            if let Some(d) = o.duration {
                kv.push(("duration", JVal::F(d)));
            }
            let result = jv_o(kv);
            let done_row = jv_o(vec![("done", result.clone())]);
            if let Err(m) = drive_append_row(journal, &done_row) {
                return (1, render_drive_fail(&format!("驱动协议坏了：journal 追加失败：{}", m)));
            }
            (0, render_drive_done(&result))
        }
        Err(ArkAbort::Ask) => {
            if let Some(m) = fx.io_err.take() {
                return (1, render_drive_fail(&format!("驱动协议坏了：{}", m)));
            }
            match fx.pending.take() {
                Some((seq, action, params)) => (0, render_drive_ask(seq, action, &params)),
                None => (1, render_drive_fail("驱动协议坏了：Ask 出档却没记断点（内部错）")),
            }
        }
        Err(ArkAbort::Fail(m)) => (1, render_drive_fail(&m)),
        Err(ArkAbort::Proto(m)) => (1, render_drive_fail(&format!("驱动协议坏了：{}", m))),
    }
}

/// 驱动档那一档的出口（`--drive-ark-daily`）。
/// `fixture_run` 非空＝run 真做换成夹具（门/自检专用旗标 `--fixture-run`，形状
/// `{status,detail,duration?,logs:[{level,text}]}`）：**生产协议形状一字不动**，换的只是这枚闭包
/// 背后做什么。夹具的 logs 逐行落进同一本侧信道 —— 生产里这些行由 maa_run 自己写，落点与顺序一致，
/// 所以壳侧的流式排空（轮询、级别、顺序）在门里就能真拍一次，不必等生产 06:00。
/// 夹具读不动/不是合法 JSON 就当场 fail 信封：给了旗标却喂错形状，比不喂更容易被当成"跑过了"。
/// 三条分支（拒跑 / 真做 / 夹具）**收在同一处 emit**（照装配回放档那条旧例）：这一档的出口只有一处是判据 ——
/// 多开一处，`--out` 就在其中一条分支上是空头承诺，而数这个的计数门第一个红在自己身上。
fn run_drive_ark_daily(
    i_drive: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
    fixture_run: &str,
    out: &Option<String>,
) -> i32 {
    let mut dec = |st: &str, ro: bool| arksk_call_decide(panel_cs, st, ro);
    let rc: i32;
    let json: String;
    // 七道拒跑闸提到**读夹具之前**：缺 --i-drive 一律 rc=2 落拒跑句，绝对优先于"喂进来的夹具对不对"。
    // 生产壳永远不带 --fixture-run，若先读夹具，一次参数漏写会被夹具错误掩盖成 rc=1 的"协议坏了"，
    // 而拒跑句是给人的、必须第一眼就是它。闸本体仍在 envelope 顶部（自检会话共用同一处），
    // 同一把纯函数同输入同输出，这里只是把它的结果提前用掉。
    if let Some(m) = drive_refusal(i_drive, journal, ctx, res, panel_cs, state, log_sink) {
        rc = 2;
        json = render_drive_fail(&m);
    } else if fixture_run.is_empty() {
        let mut run = |sc: &MaaScen, ch: &[String]| drive_real_run(panel_cs, log_sink, sc, ch);
        let pair = drive_ark_daily_envelope(i_drive, journal, ctx, res, panel_cs, state, log_sink, &mut dec, &mut run);
        rc = pair.0;
        json = pair.1;
    } else {
        let loaded = match std::fs::read_to_string(fixture_run) {
            Ok(t) => jv_parse(&t).map_err(|e| format!("--fixture-run 的夹具不是合法 JSON：{}", e)),
            Err(e) => Err(format!("--fixture-run 的夹具读不动：{}", e)),
        };
        match loaded {
            Err(m) => {
                rc = 1;
                json = render_drive_fail(&format!("驱动协议坏了：{}", m));
            }
            Ok(fxv) => {
                let f_status = jv_str(&jv_get_ci(&fxv, "status"));
                let f_detail = jv_str(&jv_get_ci(&fxv, "detail"));
                let f_dur = {
                    let d = jv_get_ci(&fxv, "duration");
                    if matches!(d, JVal::Null) {
                        None
                    } else {
                        Some(jv_to_d(&d))
                    }
                };
                let f_logs: Vec<(String, String)> = jv_list(&jv_get_ci(&fxv, "logs"))
                    .into_iter()
                    .map(|v| (jv_str(&jv_get_ci(&v, "level")), jv_str(&jv_get_ci(&v, "text"))))
                    .collect();
                let sink = log_sink.to_string();
                let mut run = move |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
                    for (lv, tx) in &f_logs {
                        let _ = drive_sink_row(&sink, lv, tx);
                    }
                    Ok(ArkRunOut { status: f_status.clone(), detail: f_detail.clone(), duration: f_dur })
                };
                let pair =
                    drive_ark_daily_envelope(i_drive, journal, ctx, res, panel_cs, state, log_sink, &mut dec, &mut run);
                rc = pair.0;
                json = pair.1;
            }
        }
    }
    emit(&json, out);
    rc
}

/// m9a 那一路的驱动档信封装配（B-2d④；纯函数，`run_drive_m9a_daily` 与 `--selftest` 第 23 组共用同一处）。
/// 骨架与上面那枚**逐段同形**（拒跑 → ctx → journal 重放 → res 合并 → 走一步 → ask/done/fail），
/// 因为骨架是驱动协议；差的四格全是两路各自的真身事实，不许顺手统一：
/// - 拒跑闸用 `drive_m9a_refusal`（同序、同数量、不同句）；
/// - ctx 只有两格：顶层 `retry_once` ＋ `m9aRun`（15 枚名册，由 `m9a_scen_core` 逐枚点名取键）。
///   **没有 launch、也没有 decision** —— 1999 这一路不拉端游，当日档位也不现编
///   （链体一次都不读 decision，所以名册里没有它的位置）；
/// - role/why 用 `DRIVE_ROLE_M9A`；
/// - run 闭包只吃 scen（`Invoke-M9ARun` 没有 `-Chains`），所以 `DriveFx` 里 ark 那两枚字段在这一路是
///   **结构性桩**：`MaaScen::default()` ＋ 一枚当场 Err 的闭包。它存在只因为 `DriveFx` 是一枚 struct
///   （两路共用同一套记账），不是一枚按路分的对象池；真被跑到就是分派写错了，Err 那句就是抓器。
fn drive_m9a_daily_envelope(
    i_drive: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
    decide: &mut dyn FnMut(&str, bool, bool) -> Result<(bool, Vec<(String, String)>), String>,
    run_m9a: &mut dyn FnMut(&M9aScen) -> Result<M9aRunOut, String>,
) -> (i32, String) {
    let fail = |m: &str| drive_env_fail(DRIVE_ROLE_M9A, m);
    if let Some(m) = drive_m9a_refusal(i_drive, journal, ctx, res, panel_cs, state, log_sink) {
        return (2, fail(&m));
    }
    // ---- ctx：壳写一遍的两格 ----
    let ctx_text = match std::fs::read_to_string(ctx) {
        Ok(t) => t,
        Err(e) => return (1, fail(&format!("驱动协议坏了：ctx 读不动：{}", e))),
    };
    let ctx_val = match jv_parse(&ctx_text) {
        Ok(v) => v,
        Err(e) => return (1, fail(&format!("驱动协议坏了：ctx 不是合法 JSON：{}", e))),
    };
    let retry_once = match jv_get_ci(&ctx_val, "retry_once") {
        JVal::B(b) => b,
        _ => return (1, fail("驱动协议坏了：ctx 的 retry_once 必须是布尔")),
    };
    // 名册那 15 枚在这一处进核（与 --m9a-run 的夹具共用 `m9a_scen_core`，键名字面只有一份）。
    // m9aDir/adb 两闸在这里就会咬：run 是这一路的核心动作，载荷残缺不许走到 ask 之后再说。
    let mut scen = match m9a_scen_core(&jv_get_ci(&ctx_val, "m9aRun")) {
        Ok(s) => s,
        Err(e) => return (1, fail(&format!("驱动协议坏了：ctx 的 m9aRun 读不动：{}", e))),
    };
    // 名册外那五格按真身给：state_dir 来自 --state（第六闸），game/target 是真身 m9a.ps1 里写死的
    // 两个字母面，两枚可用性是装载期事实（lib\r1999-module.ps1:27 只带 vision-rescue.ps1）。
    scen.state_dir = state.to_string();
    scen.game = "r1999".to_string();
    scen.target = "mumu".to_string();
    scen.rescue_available = true;
    scen.grab_available = false;
    // ---- journal：账本在手就重放，不在就现起一本 ----
    let mut rows: Vec<JVal>;
    match std::fs::read_to_string(journal) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let init_row = jv_o(vec![("init", ctx_val.clone())]);
            if let Err(m) = drive_append_row(journal, &init_row) {
                return (1, fail(&format!("驱动协议坏了：journal 建不起来：{}", m)));
            }
            let _ = std::fs::remove_file(res);
            rows = Vec::new();
        }
        Err(e) => return (1, fail(&format!("驱动协议坏了：journal 读不动：{}", e))),
        Ok(text) => {
            let (init, r, done) = match drive_load_journal(&text) {
                Ok(t) => t,
                Err(m) => return (1, fail(&format!("驱动协议坏了：journal 读不动：{}", m))),
            };
            if jv_to_json(&init) != jv_to_json(&ctx_val) {
                return (
                    1,
                    fail("驱动协议坏了：journal 的 init 与 ctx 不是同一份（换了 ctx 就得起新 journal）"),
                );
            }
            if let Some(drow) = done {
                let _ = std::fs::remove_file(res);
                return (0, drive_env_done(DRIVE_ROLE_M9A, &drive_done_result(&jv_get(&drow, "done"))));
            }
            rows = r;
        }
    }
    // ---- res：上一轮壳的回执（读后即删；≤账上下一行的 seq 是幂等重放，忽略） ----
    match std::fs::read_to_string(res) {
        Ok(text) => {
            let _ = std::fs::remove_file(res);
            let need = rows.len() as i64 + 1;
            match drive_merge_res(&text, need) {
                Ok(Some(row)) => {
                    if let Err(m) = drive_append_row(journal, &row) {
                        return (1, fail(&format!("驱动协议坏了：journal 追加失败：{}", m)));
                    }
                    rows.push(row);
                }
                Ok(None) => {}
                Err(m) => return (1, fail(&format!("驱动协议坏了：res 对不上：{}", m))),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return (1, fail(&format!("驱动协议坏了：res 读不动：{}", e))),
    }
    // ---- 一轮：重放到断点，之后再走一步 ----
    let input = R1999DInput { retry_once };
    let mut ark_stub = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
        Err("内部错：m9a 链体不许调 ark 那一路的 run 闭包（DriveFx 分派写错了）".to_string())
    };
    // 同上：`--drive-m9a-daily` 也不给第三答手，两枚现出口的形状一字不动。
    let no_direct: Option<&mut dyn FnMut(&str, &str, &JVal) -> Result<Option<DirectOut>, String>> = None;
    let mut fx = DriveFx::new(
        journal,
        state,
        log_sink,
        MaaScen::default(),
        &mut ark_stub,
        scen,
        run_m9a,
        rows,
        no_direct,
    );
    let r = r1999_daily_chain(&input, &mut fx, decide);
    match r {
        Ok(o) => {
            if let Some(m) = fx.io_err.take() {
                return (1, fail(&format!("驱动协议坏了：{}", m)));
            }
            let mut kv: Vec<(&str, JVal)> = vec![("status", jv_s(&o.status)), ("detail", jv_s(&o.detail))];
            if let Some(d) = o.duration {
                kv.push(("duration", JVal::F(d)));
            }
            let result = jv_o(kv);
            let done_row = jv_o(vec![("done", result.clone())]);
            if let Err(m) = drive_append_row(journal, &done_row) {
                return (1, fail(&format!("驱动协议坏了：journal 追加失败：{}", m)));
            }
            (0, drive_env_done(DRIVE_ROLE_M9A, &result))
        }
        Err(ArkAbort::Ask) => {
            if let Some(m) = fx.io_err.take() {
                return (1, fail(&format!("驱动协议坏了：{}", m)));
            }
            match fx.pending.take() {
                Some((seq, action, params)) => (0, drive_env_ask(DRIVE_ROLE_M9A, seq, action, &params)),
                None => (1, fail("驱动协议坏了：Ask 出档却没记断点（内部错）")),
            }
        }
        Err(ArkAbort::Fail(m)) => (1, fail(&m)),
        Err(ArkAbort::Proto(m)) => (1, fail(&format!("驱动协议坏了：{}", m))),
    }
}

/// m9a 那一路驱动档的出口（`--drive-m9a-daily`）。三条分支（拒跑 / 真做 / 夹具）**收在同一处 emit**，
/// 照 ark 那枚出口的旧例（多开一处，`--out` 就在其中一条分支上是空头承诺）。
/// 夹具复用 ark 侧那枚 `--fixture-run`：形状同 `{status,detail,duration?,logs:[{level,text}]}`，
/// m9a 只多认一枚可选 `rescued`（ark 链体不读它，喂了也不影响那一路）。
/// 复用而不是新立一枚旗标，是为了让 ⑤ 的门不必再改一次协议 —— 协议形状再变就是第三次升号。
fn run_drive_m9a_daily(
    i_drive: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
    fixture_run: &str,
    out: &Option<String>,
) -> i32 {
    let mut dec = |st: &str, rs: bool, ro: bool| m9a_call_decide(panel_cs, st, rs, ro);
    let rc: i32;
    let json: String;
    // 拒跑闸同样提到读夹具之前，理由与 ark 侧那条一字不差（拒跑句是给人的，必须第一眼就是它）。
    if let Some(m) = drive_m9a_refusal(i_drive, journal, ctx, res, panel_cs, state, log_sink) {
        rc = 2;
        json = drive_env_fail(DRIVE_ROLE_M9A, &m);
    } else if fixture_run.is_empty() {
        let mut run = |sc: &M9aScen| drive_real_m9a_run(panel_cs, log_sink, sc);
        let pair = drive_m9a_daily_envelope(i_drive, journal, ctx, res, panel_cs, state, log_sink, &mut dec, &mut run);
        rc = pair.0;
        json = pair.1;
    } else {
        let loaded = match std::fs::read_to_string(fixture_run) {
            Ok(t) => jv_parse(&t).map_err(|e| format!("--fixture-run 的夹具不是合法 JSON：{}", e)),
            Err(e) => Err(format!("--fixture-run 的夹具读不动：{}", e)),
        };
        match loaded {
            Err(m) => {
                rc = 1;
                json = drive_env_fail(DRIVE_ROLE_M9A, &format!("驱动协议坏了：{}", m));
            }
            Ok(fxv) => {
                let f_status = jv_str(&jv_get_ci(&fxv, "status"));
                let f_detail = jv_str(&jv_get_ci(&fxv, "detail"));
                let f_dur = {
                    let d = jv_get_ci(&fxv, "duration");
                    if matches!(d, JVal::Null) {
                        None
                    } else {
                        Some(jv_to_d(&d))
                    }
                };
                let f_rescued = jv_truthy(&jv_get_ci(&fxv, "rescued"));
                let f_logs: Vec<(String, String)> = jv_list(&jv_get_ci(&fxv, "logs"))
                    .into_iter()
                    .map(|v| (jv_str(&jv_get_ci(&v, "level")), jv_str(&jv_get_ci(&v, "text"))))
                    .collect();
                let sink = log_sink.to_string();
                let mut run = move |_sc: &M9aScen| -> Result<M9aRunOut, String> {
                    for (lv, tx) in &f_logs {
                        let _ = drive_sink_row(&sink, lv, tx);
                    }
                    Ok(M9aRunOut {
                        status: f_status.clone(),
                        detail: f_detail.clone(),
                        duration: f_dur,
                        rescued: f_rescued,
                    })
                };
                let pair =
                    drive_m9a_daily_envelope(i_drive, journal, ctx, res, panel_cs, state, log_sink, &mut dec, &mut run);
                rc = pair.0;
                json = pair.1;
            }
        }
    }
    emit(&json, out);
    rc
}

// ------------------------------------------------------------------ B-3②-b3 轮骨架：清单数据面
//
// 这一整块是 `lib\ark-module.ps1` / `lib\r1999-module.ps1` 那两份**清单**里能被纯数据表达的格子
// 的复刻（十段骨架要读的只有这些：短名、要不要管理员、三句提示、环境探测行、入口 verb 名册）。
// ⚠ 双源登记（§11.52 ④ 第⑤条同族）：清单改这些字段必须同批改这里，否则宿主版读到的是旧话术。
// 为什么不做成"从盘上现解析清单"：PS 清单的真身是 `[pscustomobject]` + 七枚 `scriptblock` 钩子，
// 钩子搬不动（§11.50 ④-4 已定档），剩下这几格是纯数据 —— 抄进码面比"在 Rust 里解析 PS"诚实。
// 清单的**钩子**该干什么，本块一概不管：Resolve/Preview/Ack/ShopState/Run/Stop 那六枚仍按 ask
// 交给外侧执行（见 `daily_round_chain` 的 action 名册）。

/// 清单 `Hooks.EnvProbes` 返回的那本 `[ordered]@{ 标签 = 路径 }` 的一行，折成三格纯数据：
/// - `rel_key` 为空 ⇒ 这一行就是 `paths.<base_key>` 本身（真身写 `$S.paths.maaDir`）；
/// - `rel_key` 非空 ⇒ 这一行是 `Join-GdPath paths.<base_key> @(paths.<rel_key>)`，走 `gd_join_path`。
/// `label` 是话术的一部分（"缺失：MAA 配置 → …"这一行玩家见过），不许换成英文键名。
#[derive(Clone, Copy)]
struct GdEnvRow {
    label: &'static str,
    base_key: &'static str,
    rel_key: &'static str,
}

/// 清单 `EntryVerbs.Merged` 的一行（`Ack` → `-AckArk`）。
/// 这张表是 `entryverb` 那格直答的**唯一来源**：§11.50 ④-4 抓过的坑就是"回落成短名 `-Ack` 了事"——
/// 真身 `Get-GdEntryVerb` 先查名册、查不到才回落 `'-' + $Verb`，所以宿主版必须带着名册＋在位性现问。
#[derive(Clone, Copy)]
struct GdEntryVerbRow {
    verb: &'static str,
    flag: &'static str,
}

/// 一份模块清单里十段骨架要读的那些格。
/// `stop_act` 是本批**新立**的一格静态动作名（`"stop:ark"` 等，不是清单里的字面）：
/// 骨架的收尾段按模块分派"停哪一路"，而 `DriveFx` 的断点格是 `&'static str`（`&'a mut dyn …` 让它对 `'a`
/// 不变、reborrow 不缩短生命周期），把动作名焊在静态清单上正好同时满足"分派表只有一份"与借用检查。
/// ⚠ 曾经配了对的 `run_act` 已删（§11.55 ③ 判定）：两路的 run 在 B-2c③／B-2d② 就变成**核内真做**，
/// 骨架再也不会把"跑哪一路"外发 —— 留着它就是一份没人读的第二个事实源。
/// `manifest_file` 判的是"这一路注册没注册"（真身 = `lib\*-module.ps1` 扫不扫得到），
/// `assistant_lib` 判的是"这一路装没装"（`Get-GdModules -AvailableOnly` / `Test-GdModuleInstalled`），
/// `orchestration_lib` 判的是"这棵树把几路当游戏在管"（`Test-GdMergedEntry` 的判据）—— 三枚**各自独立**，
/// 合成一枚就把"注册／装载／合体"三种状态糊成一档了。
/// ⚠ 下面五枚 `*_act` 是 b3 落码时**新立的静态动作名**，不是清单里的字面，也不是可有可无的装饰：
/// `DriveFx` 的断点格与 `ask`／`take_act` 的 `action` 形参是 `&'static str`（`DriveFx<'a>` 里的
/// `&'a mut dyn FnMut<…>` 让它对 `'a` 不变、reborrow 也缩短不了生命周期），于是**骨架里不可能用
/// `format!` 现造动作名** —— 想在第十段"按模块分派"又不能动态拼串，唯一诚实的去处就是把名字焊进静态清单。
/// 先例是本结构原本的 `stop_act`；这五枚与它同一档，两枚模块各写一份字面 ⇒ 名册仍是闭合的、可现扫的。
/// `probe_act` 只有 ark 有（`round-probe:ark` = 活动图每轮自证，真身在 `lib\ark-module.ps1` 的 Run 钩子里），
/// r1999 那一路**这一格根本不存在**（清单的 Run 钩子没这一段），所以它置空串、骨架按空串跳过；
/// 空串不是"名字待定"，是"这一路没这格"的显式读数。
/// ⚠ 同一模块的门表两枚（Ack / ShopDone / ShopReset）共用 `round-gate:<id>`：靠 journal 里的
/// **位置**＋`params` 里那枚 `verb` 区分。这是合法的 —— `params` 进 ask 信封、不进重放判据
/// （`take_act` 只逐行比 `action`），所以重复的 action 名不会让重放认错账。
#[derive(Clone, Copy)]
struct GdModule {
    id: &'static str,
    short_name: &'static str,
    assistant_lib: &'static str,
    orchestration_lib: &'static str,
    manifest_file: &'static str,
    requires_admin: bool,
    elevation_flag: &'static str,
    admin_why: &'static str,
    admin_hint: &'static str,
    admin_ok_note: &'static str,
    env_rows: &'static [GdEnvRow],
    entry_verbs: &'static [GdEntryVerbRow],
    stop_act: &'static str,
    resolve_act: &'static str,
    preview_act: &'static str,
    gate_act: &'static str,
    history_act: &'static str,
    probe_act: &'static str,
}

const GD_ENV_ARK: &[GdEnvRow] = &[
    GdEnvRow { label: "MAA 目录", base_key: "maaDir", rel_key: "" },
    GdEnvRow { label: "MAA 配置", base_key: "maaDir", rel_key: "maaConfig" },
    GdEnvRow { label: "方舟端游", base_key: "arknightsExe", rel_key: "" },
];

const GD_ENV_R1999: &[GdEnvRow] = &[
    GdEnvRow { label: "M9A 目录", base_key: "m9aDir", rel_key: "" },
    GdEnvRow { label: "M9A appsettings", base_key: "m9aDir", rel_key: "m9aAppSettings" },
    GdEnvRow { label: "MuMu Manager", base_key: "mumuDir", rel_key: "mumuManager" },
];

const GD_VERBS_ARK: &[GdEntryVerbRow] = &[
    GdEntryVerbRow { verb: "Ack", flag: "-AckArk" },
    GdEntryVerbRow { verb: "ShopDone", flag: "-ArkShopDone" },
    GdEntryVerbRow { verb: "ShopReset", flag: "-ArkShopReset" },
    GdEntryVerbRow { verb: "Farm", flag: "-FarmArk" },
];

const GD_VERBS_R1999: &[GdEntryVerbRow] = &[
    GdEntryVerbRow { verb: "Ack", flag: "-Ack1999" },
    GdEntryVerbRow { verb: "ShopDone", flag: "-RShopDone" },
    GdEntryVerbRow { verb: "ShopReset", flag: "-RShopReset" },
];

/// ark 那一路的清单。`RequiresAdmin=true` 且 `ElevationFlag='arkRequiresElevation'`：
/// 真身那句 `$S.runtime.arkRequiresElevation -ne $false` 已由 `resolve-round-context` 的
/// `Elevation` 折成真值（`Round.FlagEnabled`），核内不再抄第二把 PowerShell 真值尺。
const GD_MODULE_ARK: GdModule = GdModule {
    id: "ark",
    short_name: "方舟",
    assistant_lib: "maa.ps1",
    orchestration_lib: "ark-daily.ps1",
    manifest_file: "ark-module.ps1",
    requires_admin: true,
    elevation_flag: "arkRequiresElevation",
    admin_why: "端游要管理员权限(MAA 未提权时连不上客户端)",
    admin_hint: "MAA 可能连不上端游",
    admin_ok_note: "端游/MAA 需要",
    env_rows: GD_ENV_ARK,
    entry_verbs: GD_VERBS_ARK,
    stop_act: "stop:ark",
    resolve_act: "round-resolve:ark",
    preview_act: "round-preview:ark",
    gate_act: "round-gate:ark",
    history_act: "round-history:ark",
    probe_act: "round-probe:ark",
};

/// r1999 那一路的清单。三句 Admin 话术是**空串**（原样怪癖：字段齐但没话要说，`Assert` 认这个形状），
/// `Where-Object { $_ }` 那一步把它们滤掉 ⇒ 拼出来的 `$why/$hint/$okNote` 是空串，话术里不出现"（）"。
/// `ElevationFlag` 抄的是清单原值（`arkRequiresElevation`），但本路 `RequiresAdmin=false`，框架读不到它。
const GD_MODULE_R1999: GdModule = GdModule {
    id: "r1999",
    short_name: "1999",
    assistant_lib: "m9a.ps1",
    orchestration_lib: "r1999-daily.ps1",
    manifest_file: "r1999-module.ps1",
    requires_admin: false,
    elevation_flag: "arkRequiresElevation",
    admin_why: "",
    admin_hint: "",
    admin_ok_note: "",
    env_rows: GD_ENV_R1999,
    entry_verbs: GD_VERBS_R1999,
    stop_act: "stop:r1999",
    resolve_act: "round-resolve:r1999",
    preview_act: "round-preview:r1999",
    gate_act: "round-gate:r1999",
    history_act: "round-history:r1999",
    // 空串＝**这一路没有活动图自证那一格**（真身 `lib\r1999-module.ps1` 的 Run 钩子没这段），
    // 骨架读到空串就跳过；不是"名字待定"。r1999 侧的同类判据在 1999-event-audit 那一路，
    // 而它挂在 `audit` 那格（链体里、走 `gate`），与轮骨架的第十段无关。
    probe_act: "",
};

/// 注册表（真身 `$global:GdModules` 是 `[ordered]`，顺序 = `lib\*-module.ps1` 文件名序 = ark 在前）。
/// 顺序是判据：汇总行、`-Games` 那串、`$modeLine` 都按它排。
const GD_MODULES: &[GdModule] = &[GD_MODULE_ARK, GD_MODULE_R1999];

/// `Get-GdModule -Id`：认不出来返 None，由调用方复刻真身那句 throw。
fn gd_module_by_id(id: &str) -> Option<&'static GdModule> {
    GD_MODULES.iter().find(|m| m.id.eq_ignore_ascii_case(id))
}

/// 环境探测那七枚键之外，骨架还要两枚**取名用**的键：
/// `takeover` 直答要喂 `takeover-processes` 的 `names`，而那两个名字在真身里是
/// `[IO.Path]::GetFileNameWithoutExtension($S.paths.maaExe / .arknightsExe / .m9aExe)` 现算的
/// （ark-daily.ps1:134、r1999-daily.ps1:116）—— 不是清单里的字面，只能从 paths 取名。
/// ⚠ 这两枚只进 `resolve-round-context` 的 `pathKeys`（点名取键、逐枚），不参与环境检查：
/// 真身的环境探测只有 `EnvProbes` 那三行，多催一项就是把"缺什么"那本账改了形状。
const ROUND_NAME_PATH_KEYS: &[&str] = &["maaExe", "m9aExe"];

/// 本轮要问 ctx verb 的 pathKeys：两路 EnvProbes 涉及的全部键（按清单顺序、去重）＋两枚取名键。
fn round_path_keys() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let push = |k: &'static str, out: &mut Vec<&'static str>| {
        if !out.contains(&k) {
            out.push(k);
        }
    };
    for m in GD_MODULES {
        for r in m.env_rows {
            push(r.base_key, &mut out);
            if !r.rel_key.is_empty() {
                push(r.rel_key, &mut out);
            }
        }
    }
    for k in ROUND_NAME_PATH_KEYS {
        push(k, &mut out);
    }
    out
}

/// 本轮要问 ctx verb 的 runtimeKeys（逐枚点名，**整块 runtime 绝不进核**——visionApiKey 住在那一块里）。
/// 每枚都有消费点，一枚不落：
/// - `blockSleep`／`keepScreenOn` ⇒ 段 9 的睡眠屏蔽（核内 `SetThreadExecutionState`，见 `round_no_sleep`）
/// - `retryOnce` ⇒ 两路 `ArkDInput.retry_once` / `R1999DInput.retry_once`
/// ⚠ 两枚**不在这本名册上**的键，各自因为一把尺不同而被挪走（b3 落码前的取键修正，§11.55 ③）：
/// - `launchArkFromHelper` ⇒ 真身是 `$S.runtime.launchArkFromHelper -ne $false`（**缺键算开**，
///   `lib\ark-daily.ps1:330` 实证），与本名册的**真值判**（缺键算关）方向相反。挪到 `elevFlags`，
///   由 verb 侧 `Round.FlagEnabled` 折成真值 —— 留在 runtimeKeys 会把"没配的一轮"从"照样拉起端游"
///   读成"不拉起"，那是判定翻案不是取数。
/// - `closeEmulatorAfter` ⇒ 唯一消费点在 r1999 的收尾钩子，而收尾段按 ② 的名册走 **ask**
///   （`Stop-GdMuMu` 现场件不在 B-3 射程：真身走 MuMuManager 命令行），核内没有读它的那一格 ⇒ 移除。
///   留着它只会造出"账上取了、没人读"的死格 —— 与 `R1999DInput.decision` 那格同一类，那格已被注释判死。
const ROUND_RUNTIME_KEYS: &[&str] = &["blockSleep", "keepScreenOn", "retryOnce"];

/// 本轮要问 ctx verb 的 elevFlags（`-ne $false` 那一把尺，缺键算**开**）。两来源合一：
/// 1. 两枚清单声明的 `ElevationFlag` —— 真身 `$S.runtime.<flag> -ne $false` 提权闸门读的就是它。
///    ⚠ 两路的清单**写的是同一枚字面**（`arkRequiresElevation`；r1999 那路 `RequiresAdmin=false`
///    所以框架读不到它，但字段照抄清单原值）⇒ 名册必须**去重**，喂两枚同名键只是把同一格算两遍。
/// 2. `launchArkFromHelper` —— 不在清单里，但真身用的是**同一把尺**（`lib\ark-daily.ps1:330`），
///    所以搭这趟车由 verb 侧折成真值，核内不抄第二把 PowerShell 真值语义（§11.52 ⑤）。
fn round_elev_flags() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for m in GD_MODULES {
        if !out.contains(&m.elevation_flag) {
            out.push(m.elevation_flag);
        }
    }
    out.push("launchArkFromHelper");
    out
}

/// `resolve-round-context` 回执的骨架侧投影（②-a 那枚 verb 的既有形状，本批**只读不加键**）。
/// ⚠ 只挑这几格进结构体：`Paths` 的值与 `Absent` 的串里带本机绝对路径，
/// 它们只用于现场判在不在和写那行"缺失：…"日志，**绝不进 journal、不进 ask 信封参数**
/// （`ask` 的 params 只带 id/verb/opt 这类符号，路径由被问的一方自己现取）。
struct RoundEnv {
    runtime: JVal,
    elevation: JVal,
    paths: JVal,
    now_text: String,
}

impl Clone for RoundEnv {
    fn clone(&self) -> RoundEnv {
        RoundEnv {
            runtime: self.runtime.clone(),
            elevation: self.elevation.clone(),
            paths: self.paths.clone(),
            now_text: self.now_text.clone(),
        }
    }
}

impl RoundEnv {
    /// `if ($S.runtime.<k>)` / `[bool]$S.runtime.<k>` 那一把尺：真值判（缺键算假）。
    fn flag(&self, key: &str) -> bool {
        jv_truthy(&jv_get_ci(&self.runtime, key))
    }
    /// 提权闸门那把尺：`-ne $false`（缺键算**开**）。折算已在 C# 侧 `Round.FlagEnabled` 做完，
    /// 核内只读布尔 —— 这正是 §11.52 ⑤ 说的"别在 Rust 侧再抄一遍 PowerShell 的真值语义"。
    fn elev_on(&self, flag: &str) -> bool {
        jv_truthy(&jv_get_ci(&self.elevation, flag))
    }
    fn path(&self, key: &str) -> String {
        jv_str(&jv_get_ci(&self.paths, key))
    }
    /// EnvProbes 那一行的实际路径（`Join-GdPath` 的空 base 语义由 `gd_join_path` 保证：没填 ⇒ 空串）。
    fn row_path(&self, row: &GdEnvRow) -> String {
        let base = self.path(row.base_key);
        if row.rel_key.is_empty() {
            return base;
        }
        let rel = self.path(row.rel_key);
        gd_join_path(&base, &[rel.as_str()])
    }
    /// `Get-MaaPaths.Name` / `Get-M9APaths.Name` 那一步：exe 名去扩展名。
    fn proc_name(&self, exe_key: &str) -> String {
        let p = self.path(exe_key);
        if p.is_empty() {
            String::new()
        } else {
            file_stem(&p)
        }
    }
}

/// 起轮前唯一那一发 ctx 取数（②-a 的 `resolve-round-context`，既有形状一字不动）。
/// 拒跑闸已经保证 `panel_cs` 非空；这一枚 verb 起不来就是 Fail 信封 —— 骨架没有"猜一份 settings"的退路，
/// 猜出来的路径会让环境检查把"没装"读成"装了"。
fn round_env_fetch(
    call: &mut dyn FnMut(&str, &JVal) -> Result<JVal, String>,
) -> Result<RoundEnv, String> {
    let body = jv_o(vec![
        ("runtimeKeys", JVal::A(ROUND_RUNTIME_KEYS.iter().map(|k| jv_s(k)).collect())),
        ("elevFlags", JVal::A(round_elev_flags().into_iter().map(|k| jv_s(k)).collect())),
        ("pathKeys", JVal::A(round_path_keys().into_iter().map(|k| jv_s(k)).collect())),
    ]);
    let d = call("resolve-round-context", &body)?;
    // ⚠ 回执里另有 `IsAdmin`／`NowMs`／`Dow` 三格，骨架**一格都不接**（与上面 RoundEnv 只剩四枚字段同源）：
    // - `IsAdmin` ⇒ 提权闸门读的是"这一轮要不要问人"，不是"起轮那一刻是不是管理员"。骨架走账行
    //   （`fx.ans("round", ROUND_ADMIN_ACT, …)` → 直答件现读 `drive_is_admin()`），因为写进 ctx 快照的活读
    //   会在重放时把"当时没提权"翻成"现在也没提权"——那是判定翻案不是取数。
    // - `NowMs`／`Dow` ⇒ 骨架只在那几行日志里用 `NowText`（串），没有第二处算術消费点；
    //   留两枚死格只会造出"账上取了、没人读"。
    Ok(RoundEnv {
        runtime: jv_get_ci(&d, "Runtime"),
        elevation: jv_get_ci(&d, "Elevation"),
        paths: jv_get_ci(&d, "Paths"),
        now_text: jv_str(&jv_get_ci(&d, "NowText")),
    })
}

/// `--daily-round` 的 ctx（壳写一遍、journal 的 init 行原样存它 ⇒ 换 ctx 必须起新 journal）。
/// 十枚对应 `Invoke-GdDailyRound` 的十枚参数：`Ids Gates Opt Force DryRun SkipCleanup NoElevate
/// NameField NoticeOwner Tag`。刻意**不带**的两枚：`S`（settings 由 ctx verb 现取，整块不许进核）、
/// `ElevateScript`/`BoundParams`（提权那发重跑的是**入口自己的**路径，核不知道也不该知道 ⇒ 走 ask）。
/// `log_leaf` 是**第十一枚、且不是参数**：真身那句"(详情见 logs\$leaf)"有**两处**消费点 ——
/// 判定段 catch 里 `Stop-GdBeforeRun "$($m.ShortName) 这一路今天判定不出来：…(详情见 logs\$leaf)"`、
/// 以及收尾那句 `$noticeText += "`n(详情见 logs\$leaf)"`。`$leaf` 取的是
/// `[IO.Path]::GetFileName($global:GD.LogFile)`，而日志落点归**壳**管（核只往 --log-sink 追行、
/// 不知道自己那本账在生产里叫什么名）。所以由壳把 leaf 一起写进 ctx。⚠ 它缺省是空串 ⇒
/// 那两处少掉括号段，**这是登记在册的形状差**（不是漏写）：leaf 没来路时不许瞎猜一个。
/// `maaRun`／`m9aRun` 是**第十二、十三枚、也不是参数**：段 9 那两路的 run 真做要它们，而
/// `Invoke-GdDailyRound` 从来不需要这两格 —— 它把整块 `$S` 交给链体，链体自己从 `$S` 里取。
/// 换成头以后核只拿到点名过的格，所以 run 载荷必须由**壳**照 `--drive-ark-daily`／`--drive-m9a-daily`
/// 那两档的同一把尺写进 ctx（`maarun_parse` / `m9a_scen_core` 共用，键名与 settings 逐字同名）；
/// 缺失不在这里报错（与那两档同规矩，走到 run 才炸"不替生产猜默认路径"）。
/// `log_path` 是**第十四枚、也不是参数**：段 7 的运行锁登记里有 `log` 一格，真身取的是
/// `$global:GD.LogFile`（本轮日志的**全路径**），而 `start-run-lock` 的 `log` 缺省回落的是
/// **那枚一次性 verb 子进程自己的** `GdLog.LogFile` —— 换头以后那是另一个进程的日志文件，
/// 拿它冒充轮日志比空串更坏（running.json 里会指着一本没人写的账）。所以由壳把全路径一起写进 ctx。
/// ⚠ 它缺省也是空串：登记出去的 log 就是空，这是**在册形状差**，与 `log_leaf` 同一档处置。
/// 两枚都由壳给、核不猜：核不知道自己那本账在生产里叫什么名（只往 --log-sink 追行）。
/// `pid` 是**第十五枚、也不是参数**：段 6 的 `start-run-lock` 与段 9 收尾的 `stop-run-lock` 都把
/// `pid` 判成**必填**（`Round.cs:633`／`:678` 那两处 `GdRuleException`），理由是"登记成活锁的那个 PID
/// 只能是调用方那一轮，不能是这枚一次性 verb 的子进程"。所以这一枚**必须由壳写进 ctx**，
/// ⚠ **绝不允许回落到 `std::process::id()`** —— 那是 gdcore 自己的进程号，而驱动协议是一 ask 一进程，
/// 用它登记等于把"下一轮才活着的 PID"写成活锁，撤锁那一发又按同一枚号去比：账面自洽、现场全错。
/// 缺它 ⇒ `drive_round_daily_envelope` 在解析 ctx 之后**当场 Fail**（不是走到段 6 才炸），
/// 因为这是"壳忘了给"这一类协议错，而不是现场动作失败。
struct RoundCtx {
    ids: Vec<String>,
    gates: Vec<(String, String)>,
    opt: JVal,
    force: bool,
    dry_run: bool,
    skip_cleanup: bool,
    no_elevate: bool,
    name_field: String,
    notice_owner: String,
    tag: String,
    log_leaf: String,
    log_path: String,
    /// 壳那一轮的真实进程号（`start-run-lock`／`stop-run-lock` 的 `pid` 唯一来路）。
    /// 解析期只保证"给得到、且是个正数"；语义上的"必须是调用方那一轮"由壳负责（②-d 转接时同规矩）。
    pid: i64,
    /// 段 9 那两路 run 真做的现场载荷（ark 一路 / m9a 一路各一枚子对象）。
    /// **存原样 JVal、不在这里解析**：解析器是 `maarun_parse` / `m9a_scen_core`，与 `--drive-ark-daily`
    /// ／`--drive-m9a-daily` / 两枚回放档**共用同一把**（先例见 5640 与 5865）—— 在这里解析会把
    /// "读不动"这一档从 envelope 挪进 ctx 解析器，两处拒跑话术就分家了。
    /// ⚠ 这一格的来路只有一条：**由壳写进 ctx**。不许改用 `RoundEnv.paths` 现拼 —— 那份 paths 是
    /// `resolve-round-context` 的投影名册（逐枚点名、只够环境检查与三句日志用），拿它拼 MaaScen
    /// 等于"替生产猜默认路径"，`drive_real_run` 的注释明确禁这条（13 枚里的 runtime 六格根本不在那份名册上）。
    maa_run: JVal,
    m9a_run: JVal,
}

fn round_ctx_parse(v: &JVal) -> Result<RoundCtx, String> {
    if !matches!(v, JVal::O(_)) {
        return Err("轮骨架档的 ctx 必须是对象".to_string());
    }
    let mut ids: Vec<String> = Vec::new();
    match jv_get_ci(v, "ids") {
        JVal::Null => {}
        JVal::A(a) => {
            for it in &a {
                ids.push(jv_str(it));
            }
        }
        other => ids.push(jv_str(&other)),
    }
    let mut gates: Vec<(String, String)> = Vec::new();
    for g in jv_list(&jv_get_ci(v, "gates")) {
        if matches!(g, JVal::Null) {
            continue; // 真身 `$Gates | Where-Object { $_ }`：空项静默滤掉，不算错
        }
        gates.push((jv_str(&jv_get_ci(g, "id")), jv_str(&jv_get_ci(g, "verb"))));
    }
    let opt = jv_get_ci(v, "opt");
    if !matches!(opt, JVal::O(_)) && !matches!(opt, JVal::Null) {
        return Err("轮骨架档的 ctx 里 opt 必须是对象".to_string());
    }
    let num_str = |key: &str, def: &str| -> String {
        let raw = jv_get_ci(v, key);
        if matches!(raw, JVal::Null) {
            def.to_string()
        } else {
            jv_str(&raw)
        }
    };
    let name_field = num_str("nameField", "Id");
    if !name_field.eq_ignore_ascii_case("Id") && !name_field.eq_ignore_ascii_case("ShortName") {
        return Err(format!("轮骨架档的 ctx 里 nameField 只认 Id/ShortName，收到的是 {}", name_field));
    }
    // pid：**必填、且不许回落**。`std::process::id()` 在这里是禁手（理由见 RoundCtx 的头注：
    // 驱动协议一 ask 一进程，核自己的 PID 撤锁那一发早就不是它了）。
    // 缺失／非正数一律 Err ⇒ 由 envelope 转成 fail 信封（rc=1），与上面 nameField 那一档同路。
    let pid_raw = jv_get_ci(v, "pid");
    if matches!(pid_raw, JVal::Null) {
        return Err("轮骨架档的 ctx 里 pid 必须由壳写明（撤锁只撤自己登记的那一份，核不许拿自己的进程号冒充）".to_string());
    }
    let pid = match &pid_raw {
        JVal::I(n) => *n,
        JVal::F(_) | JVal::S(_) => jv_int(&pid_raw, 0),
        other => {
            return Err(format!(
                "轮骨架档的 ctx 里 pid 必须是进程号，收到的是 {}",
                jv_to_json(other)
            ))
        }
    };
    if pid <= 0 {
        return Err(format!("轮骨架档的 ctx 里 pid 必须是正整数，收到的是 {}", pid));
    }
    Ok(RoundCtx {
        ids,
        gates,
        opt,
        force: jv_truthy(&jv_get_ci(v, "force")),
        dry_run: jv_truthy(&jv_get_ci(v, "dryRun")),
        skip_cleanup: jv_truthy(&jv_get_ci(v, "skipCleanup")),
        no_elevate: jv_truthy(&jv_get_ci(v, "noElevate")),
        name_field,
        notice_owner: num_str("noticeOwner", "每日"),
        tag: num_str("tag", "manual"),
        log_leaf: num_str("logLeaf", ""),
        log_path: num_str("logPath", ""),
        pid,
        maa_run: jv_get_ci(v, "maaRun"),
        m9a_run: jv_get_ci(v, "m9aRun"),
    })
}

/// `Select-GdModules`（lib\modules.ps1:179）的复刻。**三句 throw 各是不同档**，不许合并：
/// 树上一路都没有／命令行点了没注册的那一路／点了但没装的那一路，玩家看到的三句话完全不一样。
/// 判据全在 `--root` 现问的三个文件上：`lib\<id>-module.ps1`（注册）、`lib\<AssistantLib>`（装了）、
/// `lib\<OrchestrationLib>`（合体判据，`entryverb` 那格另读）。
/// 返回 Err 的两句是骨架里**唯一**在进十段之前就发生的失败 ⇒ 调用方一律按 Fail 信封出（rc=1），
/// 真身那边它们是 terminating error，由入口的 catch 收尾，形状本来就不是"退出码 2"。
/// `Select-GdModules` 里"认不出来的 id 当场报错"那一句（throw 原文逐字）。
/// 括号里那格名册：真身列现装注册表的 id（= 文件名序），核内列**这棵树上清单在位**的那几枚 ——
/// 同一把尺换了事实源，只在清单被裁坏时文案才分岔（登记在册）。
fn round_no_module_msg(id: &str, registered: &[&'static GdModule]) -> String {
    let names: Vec<&str> = registered.iter().map(|m| m.id).collect();
    format!(
        "没有这个模块：{}(这棵树上有：{})。要跑某一款就先把它的 <id>-module.ps1 放进 lib\\",
        id,
        names.join(" / ")
    )
}

fn select_gd_modules(ctx: &RoundCtx, root: &str) -> Result<Vec<&'static GdModule>, String> {
    let lib_dir = std::path::Path::new(root).join("lib");
    let has = |f: &str| lib_dir.join(f).exists();
    let registered: Vec<&'static GdModule> = GD_MODULES.iter().filter(|m| has(m.manifest_file)).collect();
    let avail: Vec<&'static GdModule> = registered.iter().cloned().filter(|m| has(m.assistant_lib)).collect();
    if avail.is_empty() {
        return Err("这棵树上没有可加载的游戏模块(lib\\*-module.ps1 一个都没有)，也没有任何一路的助手库 —— 检查工程是否完整".to_string());
    }
    let want: Vec<String> = ctx
        .ids
        .iter()
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all"))
        .map(|s| s.to_string())
        .collect();
    if want.is_empty() {
        return Ok(avail);
    }
    let mut picked: Vec<&'static GdModule> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for id in &want {
        match gd_module_by_id(id) {
            None => return Err(round_no_module_msg(id, &registered)),
            Some(m) => {
                // 真身这一档查的是**现装注册表**（`<id>-module.ps1` 被 dot-source 进 `$global:GdModules`
                // 才算注册），查不到当场 throw。核内没有注册表，等价判据只能落在清单文件在不在位上 ——
                // §11.55 ④ 补的就是这一档：原来只查助手库，"清单没进树但助手库在"那一路被照跑，
                // 而真身是报错退出（"认不出来的 id 不许变成静默少跑一款"那句立项理由）。
                if !has(m.manifest_file) {
                    return Err(round_no_module_msg(id, &registered));
                }
                if !has(m.assistant_lib) {
                    skipped.push(format!("{} 已注册但这一路没装(lib\\{} 不在)，本轮跳过它", id, m.assistant_lib));
                    continue;
                }
                picked.push(m);
            }
        }
    }
    if picked.is_empty() {
        return Err(format!("选中的模块一个都没装：{}", want.join(" / ")));
    }
    // 那一句 WARN 必须进日志：跳过一款是玩家要看见的事，而它排在环境检查之前（真身就是这个序）。
    Ok(picked)
}

/// 选路那一步的 WARN 行（与 `select_gd_modules` 分开发，因为骨架的日志走 fx_log/侧信道、
/// 而选路函数只管形状）：返回"已注册但没装"的那几句，条序 = 命令行 ids 的序。
fn select_gd_module_warns(ctx: &RoundCtx, root: &str) -> Vec<String> {
    let lib_dir = std::path::Path::new(root).join("lib");
    let want: Vec<String> = ctx
        .ids
        .iter()
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all"))
        .map(|s| s.to_string())
        .collect();
    let mut out = Vec::new();
    for id in want {
        if let Some(m) = gd_module_by_id(&id) {
            if !lib_dir.join(m.manifest_file).exists() {
                continue; // 没注册那一档是 throw，不是 WARN
            }
            if !lib_dir.join(m.assistant_lib).exists() {
                out.push(format!("{} 已注册但这一路没装(lib\\{} 不在)，本轮跳过它", id, m.assistant_lib));
            }
        }
    }
    out
}

/// `Test-GdMergedEntry`（lib\decisions-common.ps1）的宿主等价判据：
/// "这棵树上把几路当一款游戏在管" —— 有注册表时数 `OrchestrationLib` 在位的枚数 >1，
/// 注册表没装载时回落到"两份编排文件都在"。这里两档都在：清单在场，故按枚数算。
fn round_is_merged_entry(root: &str) -> bool {
    let lib_dir = std::path::Path::new(root).join("lib");
    GD_MODULES
        .iter()
        .filter(|m| lib_dir.join(m.manifest_file).exists())
        .filter(|m| lib_dir.join(m.orchestration_lib).exists())
        .count()
        > 1
}

/// `Get-GdEntryVerb -Game <id> -Verb <verb>`（lib\decisions-common.ps1:147）的等价：
/// 查内建名册 → 查不到回落 `'-' + $Verb` 这种短名。**回落那一档必须留着**（合体树上没注册的那一枚
/// 就是靠它拿到短名），但名册在场时不许越过它去回短名 —— §11.50 ④-4 抓的就是"写成短名了事"。
/// `Sync-GdEntryVerbs` 的覆盖层（入口运行时按命令行参数再盖一层）仍在射程外，已登记。
fn round_entry_verb(game: &str, verb: &str, root: &str) -> String {
    let merged = round_is_merged_entry(root);
    if merged {
        if let Some(m) = gd_module_by_id(game) {
            let lib_dir = std::path::Path::new(root).join("lib");
            if lib_dir.join(m.manifest_file).exists() && lib_dir.join(m.orchestration_lib).exists() {
                if let Some(row) = m.entry_verbs.iter().find(|r| r.verb.eq_ignore_ascii_case(verb)) {
                    return row.flag.to_string();
                }
            }
        }
    }
    format!("-{}", verb)
}

/// 环境检查那一段（modules.ps1:242-255）的读数：就绪条数 + 缺失行（行文本与真身逐字同形）。
/// 第二枚返回值非空 ⇒ 骨架写 ERROR 后 `return 2`（**不是报错**、也不抛：真身就是"环境不完整"退 2）。
/// ⚠ 这里读到的是 `Test-Path`，与 `resolve-round-context` 的 `Ready` 同一把尺（填了且真存在），
/// 但那一格按**单键**算、这一处按**拼接后的整行**算 —— 'MAA 配置' 的 Ready 在 verb 侧比的是相对段，
/// 拿它当环境检查的读数会假绿，所以这半段留在核内自己走一遍。
fn round_env_check(mods: &[&'static GdModule], env: &RoundEnv) -> (usize, Vec<String>) {
    let mut ok = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for m in mods {
        for row in m.env_rows {
            let p = env.row_path(row);
            if !p.is_empty() && std::path::Path::new(&p).exists() {
                ok += 1;
            } else {
                let shown = if p.is_empty() { "(没填)".to_string() } else { p };
                bad.push(format!("{} → {}", row.label, shown));
            }
        }
    }
    (ok, bad)
}

// ---------------------------------------------------------------- B-3②-b3 轮骨架：动作名册＋第三答手
//
// 十段骨架每往下走一段，凡"要问现场"的那一格都成一枚动作（与链体那 18 枚同一套记账、同一个 `ans`）。
// 名字分两族：
//   · `round-*`／`stop:<id>`／`round-gate:<id>`… = **骨架自有的格**，role 恒 `"round"`；
//   · 链体那十六格（desk／cfg／run／skip…）由两枚 trait 自己发，role 是 `"ark"`／`"r1999"`。
// **哪一格直答、哪一格外发，只由 `round_answer` 这张表说了算**（§11.52 ③ 采纳的乙档）。骨架这边一律
// `fx.ans(...)`，闭包答 `Some` 就现场真做并落账、答 `None` 就退回 `ask` —— 所以骨架里不许出现
// "这格能不能直答"的 if：写了就是第二份事实源，而名册会随 b1/b2 的 verb 一颗一颗长。
//
// ⚠ 与 §11.52 ② 那份甲/乙/丙归属表的**一处实测改口**（本批落码时才看得见）：甲档那两枚里
// `svdetail` 直答不了。`resolve-screensaver-verdict` 吃的是 `cfg`（= PS 侧 `Get-GdScreensaverConfig`
// 对 `HKCU\Control Panel\Desktop` 的**活读**，见 lib\common.ps1:392 那枚转发壳），而注册表读数既不在
// 这枚 verb 的射程、也没有任何一枚 verb 供它 —— b3 的题面是"核内骨架＋答手"、不含新立 verb（那是 b2 的
// 射程），所以 `svdetail` 与 `updgate`／`audit` 同批留外发。`DRIVE_WHY_ROUND` 那句"屏保读数照 ask"
// 写的正是这一格（当初按甲档判它可直答，落码时按现场改口，登记见 docs §11.55）。

/// 骨架自有格的动作名（journal 的 `action` 字面、ask 信封的 `action`、直答表的键 —— 三处同名只此一份）。
const ROUND_SELECT_ACT: &str = "round-select";
const ROUND_ENV_ACT: &str = "round-env";
const ROUND_ADMIN_ACT: &str = "round-admin";
const ROUND_ELEVATE_ACT: &str = "round-elevate";
const ROUND_SCREENSAVER_ACT: &str = "round-screensaver";
const ROUND_NOSLEEP_ACT: &str = "round-nosleep";
const ROUND_NOSLEEP_OFF_ACT: &str = "round-nosleep-off";
const ROUND_LOCK_ACT: &str = "round-lock";
const ROUND_UNLOCK_ACT: &str = "round-unlock";
const ROUND_NOTICE_ACT: &str = "round-notice";
const ROUND_SUPERVISOR_START_ACT: &str = "round-supervisor-start";
const ROUND_SUPERVISOR_STOP_ACT: &str = "round-supervisor-stop";

/// 轮骨架的现场注入点。三枚都是**生产／自检共用同一把**的那条形（先例：`decide` 与 `run` 两枚注入点）：
/// - `call` = 起 panel-cs 子进程真跑一枚 verb（自检给假身 ⇒ 一次子进程都不起，也不碰任何现场）；
/// - `nap` = 核内 sleep（链体重试间隔那 25/20 秒；自检只记秒数，否则一轮自检真等四十秒）；
/// - `es` = `SetThreadExecutionState`（自检给假身 ⇒ **绝不为自检去动本机的睡眠屏蔽**；
///   这一枚在生产里是核内真做，因为它是"对本进程有效"的句柄语义，外发给壳就等于没设）。
/// ⚠ 在位性现问（`lib\*-module.ps1` 在不在）**不走注入点**：它就是用 `std::fs` 现问，自检去建临时树
/// 喂给 `--root`。给假身等于把"这棵树上装了哪几路"这半段判据从被测物里摘出去 —— 那正是要钉的那件事。
struct RoundHooks<'a> {
    call: &'a mut dyn FnMut(&str, &JVal) -> Result<JVal, String>,
    nap: &'a mut dyn FnMut(u32),
    es: &'a mut dyn FnMut(u32) -> u32,
}

/// 答手要读的两枚 out-of-band 现场值里的那一枚（另一枚 `env` 由信封在起步时就取好、按值捕进闭包）：
/// ark 本轮的判定对象。为什么要走旁路而不是走 `params`：`cfg` 那格的 params 是**链体**给的
/// （`ArkCfgKind::Main`／`Lean{stage}`），而链体是 B-2c③ 冻结的那件 —— 让它多带一枚 decision
/// 就等于改 `--drive-ark-daily` 的 ask 信封形状，那一刀本批改不得（§11.52 ④ 随行定档第 1 条）。
/// 所以骨架在第五段拿到判定后写进这一格，第九段的 `cfg` 直答从这里读 —— 只在**新区**被读：
/// 重放段那一格也是从账上还原的，压根不进闭包 ⇒ 旁路不污染重放的确定性。
/// ⚠ 读写都只经 `RefCell`，且骨架侧一次 borrow 就完事（不许握着 borrow 去调 `ans`，那会撞运行时借位）。
fn round_ark_decision_slot() -> std::rc::Rc<std::cell::RefCell<JVal>> {
    std::rc::Rc::new(std::cell::RefCell::new(JVal::Null))
}

/// 把第五段的判定对象按 PS executor 那两枚动词要的键名折出来。
/// ark 的 `cfg` 直答有两腿（照 lib\ark-daily.ps1:441-450 那两档逐字）：
/// - 主轮 = 把 decision 整块交给 verb（PS 那侧就是 `-Decision $Decision`）；
/// - 剔关轮 = PS 现场**另构造**一枚 lean 哈希（八键，`Stage` 取 params 给的、没有 `Farm` 键），
///   verb 侧于是读到 `Farm`=缺 ⇒ `farmActive=false`／`farmMode=''` —— 这是真身的形状，
///   不许"顺手把 Farm 也带上"（带上就等于给剔关轮补了一份它本来没有的补刷档位）。
fn round_lean_decision(dec: &JVal, stage: &str) -> JVal {
    jv_o(vec![
        ("Source", jv_s("auto")),
        ("Mode", jv_get_ci(dec, "Mode")),
        ("Stage", jv_s(stage)),
        ("ClearSanity", jv_get_ci(dec, "ClearSanity")),
        ("EventKey", jv_get_ci(dec, "EventKey")),
        ("EventTip", jv_get_ci(dec, "EventTip")),
        ("StageHint", jv_get_ci(dec, "StageHint")),
    ])
}

/// **第三答手的名册**（本批唯一一处"谁来答"的判据）。返回 `Ok(None)` = 这一格照旧外发 ask。
/// 每一格直答的三件事都是既有的：verb 名是 b1/b2/②-a 落下的那一枚、body 键按枚点名、
/// 落账的 `data` 形状**与 PS executor 当年写进 res 的那一枚逐字同形**（两侧账行才可对拍）。
fn round_answer(
    hooks: &mut RoundHooks,
    env: &RoundEnv,
    root: &str,
    state: &str,
    m9a_instance: &JVal,
    ark_dec: &std::cell::RefCell<JVal>,
    role: &str,
    action: &str,
    params: &JVal,
) -> Result<Option<DirectOut>, String> {
    let out: DirectOut = match (role, action) {
        // ==================== 骨架自有格 ====================
        ("round", ROUND_SELECT_ACT) => {
            // 在位性现问＋纯规则：不 verb、不子进程。Err 那两档（树上一路都没有／点了没注册的）
            // 由 `ans` 落成 err 行再冒泡，正对真身那句 terminating error（不是"退出码 2"那一档）。
            let want: Vec<String> = jv_list(&jv_get_ci(params, "ids")).into_iter().map(jv_str).collect();
            let probe = RoundCtx {
                ids: want,
                ..round_ctx_empty()
            };
            let mods = select_gd_modules(&probe, root)?;
            let warns: Vec<JVal> = select_gd_module_warns(&probe, root).into_iter().map(|w| jv_s(&w)).collect();
            // 那几句 WARN **进账**（不带路径、纯符号）：真身排在环境检查之前，走的是 WARN 档的
            // Write-GdLog ⇒ 由骨架逐条 fx.log，不许塞进 DirectOut.lines（那条道只有 INFO，且重放不再补）。
            DirectOut::only(jv_o(vec![
                ("Ids", JVal::A(mods.iter().map(|m| jv_s(m.id)).collect())),
                ("Warns", JVal::A(warns)),
            ]))
        }
        ("round", ROUND_ENV_ACT) => {
            // 数据面在场（env 由信封起步时取），这里只做"这一行在不在"的那把 Test-Path。
            // ⚠ 本机路径分岔（§11.55 登记）：真身那句是 `缺失：<label> → <绝对路径>`，而**话术不许改**
            //   是本工程最硬的一条纪律（改文案 = golden 双红），比"journal 里不出现本机路径"更硬 ——
            //   两害相权取了话术：这一格是全批唯一一处让本机路径进账的地方。判据不受影响：
            //   控制流只看 `Bad` 的条数，路径只出现在给人看的那一句里；等值门把这一格喂夹具。
            let ids: Vec<String> = jv_list(&jv_get_ci(params, "mods")).into_iter().map(jv_str).collect();
            let mut mods: Vec<&'static GdModule> = Vec::new();
            for id in &ids {
                if let Some(m) = gd_module_by_id(id) {
                    mods.push(m);
                }
            }
            let (ok, bad) = round_env_check(&mods, env);
            DirectOut::only(jv_o(vec![
                ("Ok", JVal::I(ok as i64)),
                ("Bad", JVal::A(bad.into_iter().map(|b| jv_s(&b)).collect())),
            ]))
        }
        ("round", ROUND_ADMIN_ACT) => {
            // 提权**读态**：当前令牌的成员关系是进程内事实（B-1a 已搬），外发才是怪事。
            DirectOut::only(JVal::B(drive_is_admin()))
        }
        ("round", ROUND_LOCK_ACT) => {
            let d = (hooks.call)(
                "start-run-lock",
                &jv_o(vec![
                    ("pid", JVal::I(jv_int(params, 0))),
                    ("games", jv_get_ci(params, "games")),
                    ("tag", jv_get_ci(params, "tag")),
                    ("log", jv_get_ci(params, "log")),
                ]),
            )?;
            // 落账只留两枚布尔：verb 回执里的 `Log` 是绝对路径（`Round.cs:669`），`Started` 是墙钟串
            // —— 两者都在账上没人读，而路径那枚一记账就破了"本机值不进机读面"。
            DirectOut::with_lines(
                jv_o(vec![
                    ("Wrote", jv_get_ci(&d, "Wrote")),
                    ("PrevAlive", jv_get_ci(&d, "PrevAlive")),
                ]),
                jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect(),
            )
        }
        ("round", ROUND_UNLOCK_ACT) => {
            let d = (hooks.call)(
                "stop-run-lock",
                &jv_o(vec![("pid", JVal::I(jv_int(params, 0)))]),
            )?;
            DirectOut::only(jv_o(vec![
                ("Had", jv_get_ci(&d, "Had")),
                ("Removed", jv_get_ci(&d, "Removed")),
            ]))
        }
        ("round", ROUND_NOTICE_ACT) => {
            let d = (hooks.call)(
                "send-round-notice",
                &jv_o(vec![
                    ("owner", jv_get_ci(params, "owner")),
                    ("allOk", jv_get_ci(params, "allOk")),
                    ("text", jv_get_ci(params, "text")),
                    ("level", jv_get_ci(params, "level")),
                ]),
            )?;
            // 标题**不在核内拼**：`Round.NoticeTitle` 是那句话在 C# 侧的唯一来源（②-a 的 ⑦-1 定档），
            // 回执里的 Title 就是发出去的那一句 ⇒ 宿主版与 PS 版不许各写一份标题话术。
            DirectOut::with_lines(
                jv_o(vec![
                    ("Title", jv_get_ci(&d, "Title")),
                    ("Level", jv_get_ci(&d, "Level")),
                    ("File", jv_get_ci(&d, "File")),
                    ("Webhook", jv_get_ci(&d, "Webhook")),
                    ("Toast", jv_get_ci(&d, "Toast")),
                ]),
                jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect(),
            )
        }
        ("round", ROUND_NOSLEEP_ACT) => {
            // 段 8 的睡眠屏蔽：**本进程**设、本进程查（这就是它不做成 verb 的理由，见 extern 那段注释）。
            // 位型照 Enter-GdNoSleep:475-477 那三行：CONTINUOUS|SYSTEM_REQUIRED（keepScreenOn 再或上 DISPLAY）。
            let keep = jv_truthy(&jv_get_ci(params, "keepScreenOn"));
            let mut flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED;
            if keep {
                flags |= ES_DISPLAY_REQUIRED;
            }
            let set = (hooks.es)(flags);
            // 同进程自证（§11.50 ①-8 欠的那一发）：查询形态回的是**当前位型**，本轮设的位在不在里面当场见分晓。
            let query = (hooks.es)(ES_CONTINUOUS | ES_QUERY_ONLY);
            let system_seen = query & ES_SYSTEM_REQUIRED != 0;
            let display_seen = !keep || query & ES_DISPLAY_REQUIRED != 0;
            DirectOut::only(jv_o(vec![
                ("Set", JVal::B(set != 0)),
                ("Query", JVal::I(query as i64)),
                ("SystemSeen", JVal::B(system_seen)),
                ("DisplaySeen", JVal::B(display_seen)),
            ]))
        }
        ("round", ROUND_NOSLEEP_OFF_ACT) => {
            // Exit-GdNoSleep 那一句：真身只在"曾经设过"时才撤（$global:GdSleepHandle），
            // 核内没有那枚全局句柄 ⇒ 撤的判据改成"这一段本轮真设过"（骨架只在设过之后才发这一格）。
            let set = (hooks.es)(ES_CONTINUOUS);
            DirectOut::only(JVal::B(set != 0))
        }
        // ==================== 链体格：ark 那一路 ====================
        ("ark", "desk") => {
            // 甲档：不喂 handle ⇒ Live.cs:244 那一路只现读桌面名、一个窗口调用都不碰（"没读"与"读到 false"
            // 在这条话术里不是一回事，所以这里只取 `Desktop` 那一格，别的都不动）。
            let d = (hooks.call)("ark-desktop-state", &JVal::O(Vec::new()))?;
            DirectOut::only(jv_get_ci(&d, "Desktop"))
        }
        ("ark", "takeover") => {
            // 丙档：两枚名字都是**现算**出来的 exe 名（`(Get-MaaPaths -S $S).Name` + 端游名，
            // lib\ark-daily.ps1:415 那两枚），而取名的来路是 ctx 的 pathKeys（只两枚取名键，不动环境判据）。
            let mut names: Vec<JVal> = Vec::new();
            for k in ["maaExe", "arknightsExe"] {
                names.push(jv_s(&env.proc_name(k)));
            }
            let d = (hooks.call)(
                "takeover-processes",
                &jv_o(vec![("names", JVal::A(names)), ("label", jv_s("方舟/MAA"))]),
            )?;
            // PS 那侧 `$data = $true`（Invoke-GdTakeover 不返东西，executor 补的一枚）⇒ 账行同形。
            let mut lines: Vec<String> = Vec::new();
            lines.extend(jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str));
            DirectOut::with_lines(JVal::B(true), lines)
        }
        ("ark", "cfg") => {
            let dec = ark_dec.borrow().clone();
            if matches!(dec, JVal::Null) {
                return Err("内部错：ark 的 cfg 直答没拿到本轮判定（第五段没落账就到第九段？）".to_string());
            }
            let kind = jv_str(&jv_get_ci(params, "kind"));
            let body_dec = if kind == "lean" {
                round_lean_decision(&dec, &jv_str(&jv_get_ci(params, "stage")))
            } else {
                dec
            };
            let d = (hooks.call)(
                "apply-maa-daily-config",
                &jv_o(vec![("decision", body_dec), ("apply", JVal::B(true))]),
            )?;
            let lines: Vec<String> = jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect();
            // 账行两枚小写键 = PS executor 那枚 `[pscustomobject]@{changes=…;chains=…}` 逐字同名。
            DirectOut::with_lines(
                jv_o(vec![
                    ("changes", JVal::A(jv_list(&jv_get_ci(&d, "Changes")).into_iter().cloned().collect())),
                    ("chains", JVal::A(jv_list(&jv_get_ci(&d, "ExpectedChains")).into_iter().cloned().collect())),
                ]),
                lines,
            )
        }
        ("ark", "entryverb") => {
            // 乙档：**内建名册＋在位性现问**（§11.50 ④-4 抓过"写成短名了事"）—— 不起子进程，
            // 但必须照 `Get-GdEntryVerb` 那两档（查名册 → 回落 `'-' + $Verb`），回落那一档留着。
            let game = jv_str(&jv_get_ci(params, "game"));
            let verb = jv_str(&jv_get_ci(params, "verb"));
            DirectOut::only(jv_s(&round_entry_verb(&game, &verb, root)))
        }
        ("ark", "launch") => {
            // 丙档：Start-GdArkClient 整枚（路径闸→等窗口→暖身→更新页按画面确认）在 Chain.cs:743；
            // 判据一枚不在这儿重写（`Live.cs:1120` 那条注释钉的是它）。
            let d = (hooks.call)("start-ark-client", &JVal::O(Vec::new()))?;
            let lines: Vec<String> = jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect();
            DirectOut::with_lines(jv_get_ci(&d, "Started"), lines)
        }
        ("ark", "stop") => {
            let d = (hooks.call)("stop-maa-and-game", &JVal::O(Vec::new()))?;
            let lines: Vec<String> = jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect();
            DirectOut::with_lines(JVal::B(true), lines)
        }
        ("ark", "sleep") => {
            let secs = jv_int(params, 0) as u32;
            (hooks.nap)(secs);
            DirectOut::only(JVal::B(true))
        }
        ("ark", "skip") => {
            // 丙档里"读日志尾"那半：判定早在场（detect-maa-fight-skip），缺的只是现场读盘。
            // Found=false ⇒ **账上落 null**（= PS 那侧 `$sv = $null` 那一档），不许补一个空对象。
            let d = (hooks.call)("read-maa-fight-skip", &JVal::O(Vec::new()))?;
            if !jv_truthy(&jv_get_ci(&d, "Found")) {
                DirectOut::only(JVal::Null)
            } else {
                DirectOut::only(jv_o(vec![
                    ("skipped", jv_get_ci(&d, "Skipped")),
                    ("plan", jv_get_ci(&d, "Plan")),
                    ("evidence", jv_get_ci(&d, "Evidence")),
                ]))
            }
        }
        ("ark", "ledger") => {
            // stateDir 必须给：verb 缺省回落的是**它自己那次 GdEnv 定位**的 state，而驱动协议要求
            // "已搬动作的账落到生产同一处"（--state 第六闸的同一条理由）。
            let d = (hooks.call)(
                "update-ark-farm-ledger",
                &jv_o(vec![("stateDir", jv_s(state))]),
            )?;
            let lines: Vec<String> = jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect();
            DirectOut::with_lines(jv_get_ci(&d, "Summary"), lines)
        }
        ("ark", "obs") => {
            let mut body: Vec<(&str, JVal)> = Vec::new();
            if !env.now_text.is_empty() {
                body.push(("nowText", jv_s(&env.now_text)));
            }
            let d = (hooks.call)("read-maa-stage-observation", &jv_o(body))?;
            if !jv_truthy(&jv_get_ci(&d, "Found")) {
                // 两本日志都没读到 = 旧身返 $null 那一档 ⇒ 账上落 null（与 executor 当年逐字同形）。
                return Ok(Some(DirectOut::only(JVal::Null)));
            }
            // 观测七键**按 verb 的原键名摆**（PascalCase）：这一枚的 data 之后由 `obsrec` 那格真做吃掉，
            // 而 obsrec 的取键是按 PS 对象属性名写的 —— 在这里改成小写就是把跨格协议拆了。
            // verb 多出来的 `LogsRead`/`Reason` 不落账：旧身那块对象里根本没有这两格。
            DirectOut::only(jv_o(vec![
                ("Found", jv_get_ci(&d, "Found")),
                ("Date", jv_get_ci(&d, "Date")),
                ("Stage", jv_get_ci(&d, "Stage")),
                ("LastBattle", jv_get_ci(&d, "LastBattle")),
                ("SawGuiPlan", jv_get_ci(&d, "SawGuiPlan")),
                ("Survivors", jv_get_ci(&d, "Survivors")),
                ("Selected", jv_get_ci(&d, "Selected")),
            ]))
        }
        // ==================== 链体格：r1999 那一路 ====================
        ("r1999", "takeover") => {
            let d = (hooks.call)(
                "takeover-processes",
                &jv_o(vec![
                    ("names", JVal::A(vec![jv_s(&env.proc_name("m9aExe"))])),
                    ("label", jv_s("M9A")),
                ]),
            )?;
            let lines: Vec<String> = jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect();
            DirectOut::with_lines(JVal::B(true), lines)
        }
        ("r1999", "mumu") => {
            let d = (hooks.call)("start-mumu", &JVal::O(Vec::new()))?;
            let lines: Vec<String> = jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect();
            DirectOut::with_lines(jv_get_ci(&d, "Started"), lines)
        }
        ("r1999", "cfg") => {
            // instance 那两枚来自 ctx 的 m9aRun（**由壳写进 ctx**，与 --drive-m9a-daily 同一把尺）：
            // verb 读的是 `Id`/`Name`（PascalCase，照 Chain.cs:1058 那两枚 Gd.Get），核内不改键名。
            let d = (hooks.call)(
                "apply-m9a-instance",
                &jv_o(vec![
                    (
                        "instance",
                        jv_o(vec![
                            ("Id", jv_get_ci(m9a_instance, "id")),
                            ("Name", jv_get_ci(m9a_instance, "name")),
                        ]),
                    ),
                    ("apply", JVal::B(true)),
                ]),
            )?;
            let lines: Vec<String> = jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect();
            DirectOut::with_lines(
                jv_o(vec![(
                    "changes",
                    JVal::A(jv_list(&jv_get_ci(&d, "Changes")).into_iter().cloned().collect()),
                )]),
                lines,
            )
        }
        ("r1999", "stop") => {
            let d = (hooks.call)("stop-m9a", &JVal::O(Vec::new()))?;
            let lines: Vec<String> = jv_list(&jv_get_ci(&d, "Lines")).into_iter().map(jv_str).collect();
            DirectOut::with_lines(JVal::B(true), lines)
        }
        ("r1999", "sleep") => {
            let secs = jv_int(params, 0) as u32;
            (hooks.nap)(secs);
            DirectOut::only(JVal::B(true))
        }
        // ==================== 其余一律外发 ====================
        // 名单：门表（round-gate:*）／判定（round-resolve:*）／预览（round-preview:*）／
        // 收尾钩子（stop:*）／监督两枚／历史（round-history:*）／提权重启（round-elevate）／屏保那格，
        // 加上链体里 `svdetail`（本批改口）与 `updgate`／`audit`（那两枚走 `gate`、根本不进这里）。
        // **这里不写任何判据**：外发那一族的真身动作在 PS 侧，②-d 转接时才谈"要不要挪进来"。
        _ => return Ok(None),
    };
    Ok(Some(out))
}

/// 只给 `round-select` 那一步搭一枚"只有 ids 的" RoundCtx：选路函数签名要整个 RoundCtx，
/// 但它一辈子只读 `.ids`。用默认值搭其余十二枚比新立一枚小函数诚实（少一处判据来源）。
fn round_ctx_empty() -> RoundCtx {
    RoundCtx {
        ids: Vec::new(),
        gates: Vec::new(),
        opt: JVal::Null,
        force: false,
        dry_run: false,
        skip_cleanup: false,
        no_elevate: false,
        name_field: "Id".to_string(),
        notice_owner: String::new(),
        tag: String::new(),
        log_leaf: String::new(),
        log_path: String::new(),
        pid: 0,
        maa_run: JVal::Null,
        m9a_run: JVal::Null,
    }
}

/// 十段走完（或早退那几档之一）的读数。
/// `code` 就是真身 `Invoke-GdDailyRound` `return` 的那个退出码（0 全成 / 1 部分成 / 2 有失败或环境不完整，
/// 提权那一档是**子轮的码**）；`reached_summary` 分"走没走到第十段"，因为早退那三档既没有汇总也没有通知。
/// 驱动协议的 rc 与它**不是一件事**（done/ask 恒 0、拒跑 2、fail 1），所以这一枚只能从 done 信封的
/// `result.code` 读 —— ②-d 转接时入口按它 exit。
struct RoundFlow {
    code: i64,
    reached_summary: bool,
    all_ok: bool,
    summary: String,
}

/// 早退那几档共用的构造（没汇总、没通知）。
fn round_flow_early(code: i64) -> RoundFlow {
    RoundFlow { code, reached_summary: false, all_ok: false, summary: String::new() }
}

/// 真身那句 `"(详情见 logs\$leaf)"`：两处消费点（判定段的 Stop-GdBeforeRun、通知那句尾巴）共用这一把。
/// `leaf` 由**壳**写进 ctx（核只往 --log-sink 追行、不知道自己那本账在生产里叫什么名）；
/// 空串 ⇒ 整段括号不出现，这是在册形状差（见 `RoundCtx` 的 `log_leaf` 头注），不许瞎猜一个文件名。
fn round_log_tail(leaf: &str) -> String {
    if leaf.is_empty() {
        String::new()
    } else {
        format!("(详情见 logs\\{})", leaf)
    }
}

/// `($arr | Where-Object { $_ }) -join '；'`：清单里那三句 Admin 话术的拼法（r1999 那一路是三枚空串 ⇒
/// 拼出空串 ⇒ 话术里不出现"（）"，这是原样怪癖，不许"顺手补个默认值"）。
fn round_join_cn(vals: &[&'static str], sep: &str) -> String {
    let kept: Vec<&str> = vals.iter().cloned().filter(|s| !s.is_empty()).collect();
    kept.join(sep)
}

/// .NET `Math.Round(x, 0)` 的舍入档：**向偶数舍**（`MidpointRounding.ToEven` 是默认值）。
/// `f64::round` 是"半程向上"，直接抄会把 22.5 舍成 23，与真身实测的 22 分岔。
fn gd_round_half_to_even(x: f64) -> f64 {
    let f = x.floor();
    let d = x - f;
    if d > 0.5 {
        f + 1.0
    } else if d < 0.5 {
        f
    } else if (f / 2.0).fract() == 0.0 {
        f
    } else {
        f + 1.0
    }
}

/// 汇总与历史那一行的分钟数（真身 `"$([math]::Round($r.Duration / 60, 1)) 分钟"`）。
/// 两件事必须一起复刻，实测在 `--selftest` 24k 那一段（十六枚时长逐个对拍 PS 现测读数）：
/// ① 先放大到"分钟×10"再向偶数舍 ⇒ 135 秒是 2.2 分钟、75 秒是 1.2 分钟、45 秒是 0.8 分钟；
/// ② 舍完走 double 的最短可往返表示 ⇒ 60 秒是「1 分钟」而不是「1.0 分钟」，1 秒是「0 分钟」。
///    `{:.1}` 给不出 ②，`f64::round` 给不出 ①，两处都不是"顺手统一格式"。
fn round_minutes_display(secs: f64) -> String {
    let scaled = gd_round_half_to_even(secs / 6.0);
    format!(" {} 分钟", scaled / 10.0)
}

/// 回执里 `Available` 那一格的读法 —— 真身 `Get-Command … -ErrorAction SilentlyContinue` 那把守卫
/// 换头以后的替身。**键缺席按"在"处理**：外侧答手没有义务为"命令在场"多写一格，而真身里"命令在但
/// 起挂了"那一档本来就走 catch 出 WARN；显式 false 才是"这棵树上压根没这枚命令"⇒ 整段一句不吭。
fn round_answer_available(d: &JVal) -> bool {
    match jv_get_ci(d, "Available") {
        JVal::Null => true,
        v => jv_truthy(&v),
    }
}

/// **十段轮骨架**（真身 = `lib\modules.ps1` 的 `Invoke-GdDailyRound`，逐行复刻）。
/// 段的划分与顺序照 `DRIVE_WHY_ROUND` 那十枚名：选路／环境／提权闸／门表／判定／锁／监督／睡眠屏蔽／
/// 逐路链体／收尾汇总通知。dry-run 预览挂在判定之后（真身就在那个位置 return 0）。
///
/// 三件事压在下面每一行上，改之前先读：
/// 1. **判据一行不许新写**。这一段只做"什么时候问、问完拿那一句写日志、按读数走哪条分支"；
///    判定本身（decide-ark／farm／门表）全在 `round-resolve:*` 那枚 ask 的另一头。
/// 2. **谁来答由 `round_answer` 一家说了算**（§11.52 ③ 的乙档）。这里一律 `fx.ans(...)`，
///    不许出现"这一格能不能直答"的 if —— 那是第二份事实源。
/// 3. **ask/res 协议不许删**（§11.52 ④ 随行定档第 1 条）。外发那几格照旧出 ask 信封、照旧靠 seq 重放，
///    幂等与回退通道都在这条道上；第三答手是"增"不是"换"。
///
/// 真身那段 `try { 逐路 Run } finally { 收尾 }` 在重入协议里的对应物（登记在册，别当成漏写）：
/// 收尾四步排在链体之后、且每一步自己是一枚账格 ⇒ 某一步 ask 时进程退出，**重入会逐行重放到这里再把
/// 剩下那几步做掉**，语义与 finally 等价（锁的 PID 判活本来就是为了"进程半路没了"那种残留）。
/// 真身那个 catch 只接异常，所以这里也只接 `ArkAbort::Fail`；`Ask`／`Proto` 一律冒泡。
fn daily_round_chain(
    rc: &RoundCtx,
    env: &RoundEnv,
    ark_dec: &std::cell::RefCell<JVal>,
    fx: &mut DriveFx,
    ark_decide: &mut dyn FnMut(&str, bool) -> Result<(bool, Vec<(String, String)>), String>,
    m9a_decide: &mut dyn FnMut(&str, bool, bool) -> Result<(bool, Vec<(String, String)>), String>,
) -> Result<RoundFlow, ArkAbort> {
    // ---------- 段 1 选路 ----------
    let sel = fx.ans(
        "round",
        ROUND_SELECT_ACT,
        jv_o(vec![("ids", JVal::A(rc.ids.iter().map(|s| jv_s(s)).collect()))]),
        |d| d.clone(),
    )?;
    for w in jv_list(&jv_get_ci(&sel, "Warns")) {
        // 真身这几句是 WARN、且排在环境检查之前 —— 顺序就是判据。
        fx.fx_log("WARN", &jv_str(&w));
    }
    let mut mods: Vec<&'static GdModule> = Vec::new();
    for id in jv_list(&jv_get_ci(&sel, "Ids")) {
        if let Some(m) = gd_module_by_id(&jv_str(&id)) {
            mods.push(m);
        }
    }

    // ---------- 段 2 环境 ----------
    let ev = fx.ans(
        "round",
        ROUND_ENV_ACT,
        jv_o(vec![("mods", JVal::A(mods.iter().map(|m| jv_s(m.id)).collect()))]),
        |d| d.clone(),
    )?;
    fx.fx_log("INFO", &format!("环境检查：{} 项就绪", jv_int(&jv_get_ci(&ev, "Ok"), 0)));
    let bad_rows = jv_get_ci(&ev, "Bad");
    let bad = jv_list(&bad_rows);
    for b in &bad {
        fx.fx_log("ERROR", &format!("缺失：{}", jv_str(b)));
    }
    if !bad.is_empty() {
        fx.fx_log("ERROR", "环境不完整：见上面缺失项(可跑 setup.ps1 或改 config\\settings.json 的路径)");
        return Ok(round_flow_early(2));
    }

    // ---------- 段 3 提权闸门 ----------
    // `$mods | Where-Object { $_.RequiresAdmin -and ($S.runtime.$($_.ElevationFlag) -ne $false) }`：
    // 那把"缺键算开"的尺**不在核内抄**（§11.52 ⑤），由 resolve-round-context 的 Elevation 折成真值。
    let need_admin: Vec<&&'static GdModule> = mods
        .iter()
        .filter(|m| m.requires_admin && env.elev_on(m.elevation_flag))
        .collect();
    if !need_admin.is_empty() {
        let who = need_admin.iter().map(|m| m.short_name).collect::<Vec<_>>().join("/");
        let whys: Vec<&'static str> = need_admin.iter().map(|m| m.admin_why).collect();
        let hints: Vec<&'static str> = need_admin.iter().map(|m| m.admin_hint).collect();
        let notes: Vec<&'static str> = need_admin.iter().map(|m| m.admin_ok_note).collect();
        let why = round_join_cn(&whys, "；");
        let hint = round_join_cn(&hints, "；");
        let ok_note = round_join_cn(&notes, "；");
        let is_admin = fx.ans("round", ROUND_ADMIN_ACT, JVal::Null, |d| jv_truthy(d))?;
        if is_admin {
            let extra = if ok_note.is_empty() { String::new() } else { format!("({})", ok_note) };
            fx.fx_log("INFO", &format!("管理员权限已具备{}", extra));
        } else if rc.no_elevate {
            let head = if hint.is_empty() { String::new() } else { format!("{}，", hint) };
            fx.fx_log("WARN", &format!("非管理员且指定了 -NoElevate：{}{}这一路会偏脆", head, who));
        } else if rc.dry_run {
            fx.fx_log("WARN", &format!("非管理员：dry-run 无所谓，真正跑{}时会自动提权(弹一次 UAC)", who));
        } else {
            fx.fx_log("WARN", &format!("{}{} → 以管理员身份重启本轮，请在 UAC 弹窗点「是」", who, why));
            // 入口路径/命令行那半是**外侧**的事（本机路径不进信封，Start-GdElevatedSelf 重跑的是入口自己），
            // 所以这一格无参外发；回执只要求一格 `Code` = 提权子轮的退出码。
            let code = fx.ans(
                "round",
                ROUND_ELEVATE_ACT,
                JVal::Null,
                |d| jv_int(&jv_get_ci(d, "Code"), 0),
            )?;
            fx.fx_log(
                if code == 0 { "OK" } else { "WARN" },
                &format!("提权子轮结束，退出码 {}(它的日志同样写在 logs\\，不在本文件里)", code),
            );
            return Ok(round_flow_early(code));
        }
    }

    // ---------- 段 4 两道手动门 ----------
    for (id, verb) in &rc.gates {
        let m = match gd_module_by_id(id) {
            Some(m) => *m,
            // 真身 Set-GdModuleState 那句 throw 逐字（装载期事实，不是本轮的"退出码 2"那一档）。
            None => return Err(ArkAbort::Fail(format!("没有这个模块：{}", id))),
        };
        fx.ans(
            "round",
            m.gate_act,
            jv_o(vec![("id", jv_s(id)), ("verb", jv_s(verb))]),
            |_| (),
        )?;
    }
    if !rc.gates.is_empty() {
        if rc.dry_run {
            fx.fx_log("OK", "[dry-run] 仅记录确认/领空状态，结束");
            return Ok(round_flow_early(0));
        }
        fx.fx_log("INFO", "状态已更新；继续执行本轮日常");
    }

    // ---------- 段 5 判定 ----------
    // 判定失败=今天不知道该跑什么，硬跑只会乱点 ⇒ 真身在这里 Stop-GdBeforeRun（写一句 ERROR、退 2），
    // 不是一屏 "Exception: …\modules.ps1:298"。所以这一格**自己接住 err**（Fail 那一档），
    // Ask/Proto 照旧冒泡 —— "哪个点接住"是骨架的判断，不是宿主的（trait 头注那条纪律）。
    let mut plan: Vec<(&'static GdModule, JVal)> = Vec::new();
    for m in mods.iter() {
        let got = fx.ans(
            "round",
            m.resolve_act,
            jv_o(vec![("opt", rc.opt.clone()), ("force", JVal::B(rc.force))]),
            |d| d.clone(),
        );
        let dec = match got {
            Ok(v) => v,
            Err(ArkAbort::Fail(msg)) => {
                let line = format!(
                    "{} 这一路今天判定不出来：{}{}",
                    m.short_name,
                    msg,
                    round_log_tail(&rc.log_leaf)
                );
                fx.fx_log("ERROR", &format!("这一轮没跑：{}", line));
                return Ok(round_flow_early(2));
            }
            Err(other) => return Err(other),
        };
        // ark 那一路的判定对象同时写进旁路一格：段 9 的 `cfg` 直答要用它（理由见
        // `round_ark_decision_slot` 的头注）。重放段也走这一行 ⇒ 旁路与账上永远同源。
        if m.id == "ark" {
            ark_dec.replace(dec.clone());
        }
        plan.push((*m, dec));
    }

    if rc.dry_run {
        fx.fx_log("STEP", "[dry-run] 预览将要下发的配置，不启动任何程序");
        for (m, dec) in &plan {
            fx.ans("round", m.preview_act, jv_o(vec![("decision", dec.clone())]), |_| ())?;
        }
        fx.fx_log("OK", "[dry-run] 完成");
        return Ok(round_flow_early(0));
    }

    // ---------- 段 6 登记活体 ----------
    // 真身把这一行排在 dry-run 之后是刻意的：预演不占助手、不占端游。
    let games = if rc.ids.is_empty() { "all".to_string() } else { rc.ids.join("+") };
    let lock_params = jv_o(vec![
        ("pid", JVal::I(rc.pid)),
        ("games", jv_s(&games)),
        ("tag", jv_s(&rc.tag)),
        ("log", jv_s(&rc.log_path)),
    ]);
    fx.ans("round", ROUND_LOCK_ACT, lock_params, |_| ())?;

    // ---------- 段 7 监督 agent ----------
    // 真身用 `Get-Command` 守卫（自检的假环境不点这一枚），且起不来只少一份旁观、绝不影响主流程。
    // 换头以后这一格外发，于是"命令在不在"变成回执里那一枚 `Available`：**没这枚命令那一档是真身的一句
    // 都不吭**（守卫不成立 ⇒ 整段跳过，连"没起来"那句 WARN 都不该有）。骨架自己不许决定要不要监督，
    // 那是把旁观层的开关搬进编排层。`Available` 这一格缺席时按"在"处理 —— 外侧答手没义务凭空多给一格，
    // 而真身里"命令在但起挂了"那一档本来就是走 catch 出 WARN。
    let sup = fx.ans(
        "round",
        ROUND_SUPERVISOR_START_ACT,
        jv_o(vec![("games", jv_s(&games))]),
        |d| d.clone(),
    );
    match sup {
        Ok(d) => {
            if round_answer_available(&d) {
                let e = jv_str(&jv_get_ci(&d, "Err"));
                if !e.is_empty() {
                    fx.fx_log("WARN", &format!("监督 agent 没起来：{}(这一轮没人监督，主流程不受影响)", e));
                }
            }
        }
        Err(ArkAbort::Fail(msg)) => {
            fx.fx_log("WARN", &format!("监督 agent 没起来：{}(这一轮没人监督，主流程不受影响)", msg));
        }
        Err(other) => return Err(other),
    }

    // ---------- 段 8 睡眠屏蔽 ----------
    let mut nosleep_on = false;
    if env.flag("blockSleep") {
        let keep = env.flag("keepScreenOn");
        let es = fx.ans(
            "round",
            ROUND_NOSLEEP_ACT,
            jv_o(vec![("keepScreenOn", JVal::B(keep))]),
            |d| d.clone(),
        );
        match es {
            Ok(d) => {
                nosleep_on = true;
                // 真身这一句在 ES 调用之后、屏保那一段之前，逐字。
                fx.fx_log("INFO", "已临时禁止系统睡眠(跑完自动恢复)");
                // 自证那一发的读数（§11.50 ①-8 欠的那笔）：查到的位型没含本轮设的位 ⇒ 当场补一行。
                // 这句**不是真身的话**：真身不知道设成没设成，宿主版既然测得到就说测得到的 ——
                // 测不到的那几档照口径记 unobserved，绝不许反过来吹"屏蔽已生效"。
                if !jv_truthy(&jv_get_ci(&d, "Set"))
                    || !jv_truthy(&jv_get_ci(&d, "SystemSeen"))
                    || !jv_truthy(&jv_get_ci(&d, "DisplaySeen"))
                {
                    fx.fx_log(
                        "WARN",
                        &format!(
                            "睡眠屏蔽自证没看到本轮设的位(查询回 {}，本轮要设 system{})—— 这一轮的屏蔽按未证实处理",
                            jv_int(&jv_get_ci(&d, "Query"), 0),
                            if keep { " + display" } else { "" }
                        ),
                    );
                }
                if keep {
                    // 屏保那一格（读 SPI + 关闭 + 清旧机制那本 state 文件）留外发：核不越界去抢
                    // user32 那枚（extern 块的头注写了依据）。四档话术在骨架，判据在外侧。
                    // ⚠ 这一格的 Fail **不许冒泡**：真身整段 `Enter-GdNoSleep` 只有一个 try/catch，
                    // 屏保那几句全在 try 内 ⇒ 拿不到读数只留一句「禁止睡眠失败：…」，本轮照跑
                    // （§11.55 ③ 抓出来的真红之一：写成 `?` 等于把"屏保读不动"升级成本轮崩）。
                    let sv = fx.ans("round", ROUND_SCREENSAVER_ACT, JVal::Null, |d| d.clone());
                    let sv = match sv {
                        Ok(d) => Some(d),
                        Err(ArkAbort::Fail(msg)) => {
                            fx.fx_log("WARN", &format!("禁止睡眠失败：{}", msg));
                            None // 真身 catch 到句尾：四档一句都不打，但本轮照跑、收尾照旧撤屏蔽
                        }
                        Err(other) => return Err(other),
                    };
                    if let Some(sv) = sv {
                        if !jv_truthy(&jv_get_ci(&sv, "Read")) {
                            fx.fx_log("WARN", "读屏保状态失败，本轮不动屏保");
                        } else if jv_truthy(&jv_get_ci(&sv, "WasActive")) {
                            if jv_truthy(&jv_get_ci(&sv, "Disabled")) {
                                fx.fx_log("INFO", "屏保已关闭(策略：只关不复，收尾不再恢复)");
                            } else {
                                fx.fx_log(
                                    "WARN",
                                    "关闭屏保失败：会话可能已被屏保桌面接管(方舟会因此早退)；长期解法见 status.ps1 的「屏保」那一行(解除命令 tools\\unarm-screensaver.ps1 -Apply)",
                                );
                            }
                        } else {
                            fx.fx_log("INFO", "屏保本来就是关闭状态(稳态)");
                        }
                    }
                }
            }
            Err(ArkAbort::Fail(msg)) => {
                // 真身那一整段在 try 里：屏蔽失败只 WARN，绝不挡本轮。
                fx.fx_log("WARN", &format!("禁止睡眠失败：{}", msg));
            }
            Err(other) => return Err(other),
        }
    }

    // ---------- 段 9 逐路链体 ----------
    let mut results: Vec<(&'static GdModule, ArkDOut)> = Vec::new();
    let mut fatal: Option<String> = None;
    let mut idx = 0;
    while idx < plan.len() {
        let (m, dec) = (plan[idx].0, plan[idx].1.clone());
        idx += 1;
        let got: Result<ArkDOut, ArkAbort> = if m.id == "ark" {
            let input = ArkDInput {
                launch: env.elev_on("launchArkFromHelper"),
                retry_once: env.flag("retryOnce"),
                decision: dec.clone(),
            };
            ark_daily_chain(&input, &mut *fx, ark_decide)
        } else {
            let input = R1999DInput { retry_once: env.flag("retryOnce") };
            r1999_daily_chain(&input, &mut *fx, m9a_decide)
        };
        match got {
            Ok(o) => {
                // 清单 Run 钩子里那半段"每轮自证活动图"（lib\ark-module.ps1:59-76）：
                // 只在这一路真跑到 MAA 关门前(ok)才采，抛任何错都只 WARN。r1999 那一路没这格（空串）。
                if !m.probe_act.is_empty() && o.status == "ok" {
                    let p = fx.ans(
                        "round",
                        m.probe_act,
                        jv_o(vec![("decision", dec.clone())]),
                        |_| (),
                    );
                    match p {
                        Ok(()) => {}
                        Err(ArkAbort::Fail(msg)) => {
                            fx.fx_log("WARN", &format!("活动图自证没做起来：{}(不影响本轮)", msg));
                        }
                        Err(other) => return Err(other),
                    }
                }
                results.push((m, o));
            }
            Err(ArkAbort::Fail(msg)) => {
                fatal = Some(msg.clone());
                fx.fx_log("ERROR", &format!("本轮异常中断：{}", msg));
                break;
            }
            Err(other) => return Err(other),
        }
    }

    // ---------- 段 10 之前那步 finally：收尾 ----------
    // 监督先停、停在各路 Stop 钩子之前（真身那段注释的理由：收尾会杀助手，那是我们自己动手，
    // 监督若还在岗会把"助手进程不见了"当真出事发一条假警报）。
    let sup_stop = fx.ans("round", ROUND_SUPERVISOR_STOP_ACT, JVal::Null, |d| d.clone());
    match sup_stop {
        Ok(d) => {
            if round_answer_available(&d) {
                let e = jv_str(&jv_get_ci(&d, "Err"));
                if !e.is_empty() {
                    fx.fx_log("WARN", &format!("监督 agent 没停干净：{}", e));
                }
            }
        }
        Err(ArkAbort::Fail(msg)) => {
            fx.fx_log("WARN", &format!("监督 agent 没停干净：{}", msg));
        }
        Err(other) => return Err(other),
    }
    if !rc.skip_cleanup {
        fx.fx_log("STEP", "--- 收尾 ---");
        for (m, _) in &plan {
            let s = fx.ans("round", m.stop_act, JVal::Null, |_| ());
            if let Err(ArkAbort::Fail(msg)) = s {
                fx.fx_log("WARN", &format!("收尾这一步没做成：{}", msg));
            } else if let Err(other) = s {
                return Err(other);
            }
        }
    }
    if nosleep_on {
        // Exit-GdNoSleep：真身整枚是 catch{} ⇒ 撤失败一句都不吭（撤不成也只是下一班照旧，不该挡本轮）。
        let off = fx.ans("round", ROUND_NOSLEEP_OFF_ACT, JVal::Null, |_| ());
        if let Err(ArkAbort::Fail(_)) = off {
        } else if let Err(other) = off {
            return Err(other);
        }
    }
    // Stop-GdRunLock 同样不吭：撤锁失败绝不能把收尾这一路带崩。
    let un = fx.ans("round", ROUND_UNLOCK_ACT, jv_o(vec![("pid", JVal::I(rc.pid))]), |_| ());
    if let Err(ArkAbort::Fail(_)) = un {
    } else if let Err(other) = un {
        return Err(other);
    }

    // ---------- 段 10 汇总／历史／通知 ----------
    let mut lines: Vec<String> = Vec::new();
    let mut all_ok = fatal.is_none();
    for (i, (m, r)) in results.iter().enumerate() {
        if r.status != "ok" {
            all_ok = false;
        }
        let mins = match r.duration {
            Some(d) if d != 0.0 => round_minutes_display(d),
            _ => String::new(),
        };
        let dec = &plan[i].1;
        let mode = if matches!(dec, JVal::Null) {
            "skip".to_string()
        } else {
            jv_str(&jv_get_ci(dec, "Mode"))
        };
        let shown = if rc.name_field.eq_ignore_ascii_case("ShortName") {
            m.short_name
        } else {
            m.id
        };
        lines.push(format!("{}：{}{} — {}", shown, r.status, mins, r.detail));
        // 历史行按模块 id 落账（history.csv 里是 id，不是短名）；这一格外发 = "历史照 ask"那一族。
        fx.ans(
            "round",
            m.history_act,
            jv_o(vec![
                ("id", jv_s(m.id)),
                ("status", jv_s(&r.status)),
                ("mode", jv_s(&mode)),
                ("detail", jv_s(&r.detail)),
                (
                    "durationSec",
                    match r.duration {
                        Some(d) => JVal::F(d),
                        None => JVal::Null,
                    },
                ),
            ]),
            |_| (),
        )?;
    }
    let mut summary = lines.join("\n");
    if let Some(f) = &fatal {
        all_ok = false;
        summary = format!("本轮异常中断：{}\n{}", f, summary);
    }
    fx.fx_log(
        if all_ok { "OK" } else { "WARN" },
        &format!("=== 本轮结束：{} ===", if all_ok { "全部完成" } else { "存在未完成项" }),
    );
    // 真身这里还有一句 `Write-Host $summary`：宿主版的 stdout 是信封兜底通道，机读只走 --out，
    // 所以这一句不复刻（登记在册）。汇总文本随 done 信封的 result.summary 交给外侧。
    let mode_line = plan
        .iter()
        .map(|(m, dec)| {
            let mode = if matches!(dec, JVal::Null) {
                "skip".to_string()
            } else {
                jv_str(&jv_get_ci(dec, "Mode"))
            };
            format!("{}：{}", m.short_name, mode)
        })
        .collect::<Vec<_>>()
        .join(" / ");
    let mut notice_text = format!("{}\n{}", mode_line, summary);
    if !all_ok {
        let tail = round_log_tail(&rc.log_leaf);
        if !tail.is_empty() {
            notice_text.push_str(&format!("\n{}", tail));
        }
    }
    fx.ans(
        "round",
        ROUND_NOTICE_ACT,
        jv_o(vec![
            ("owner", jv_s(&rc.notice_owner)),
            ("allOk", JVal::B(all_ok)),
            ("text", jv_s(&notice_text)),
            ("level", jv_s(if all_ok { "OK" } else { "WARN" })),
        ]),
        |_| (),
    )?;

    // 真身那三档码：全成 0；有结果且至少一枚 ok ⇒ 1（部分成）；其余 2。
    let ok_n = results.iter().filter(|(_, r)| r.status == "ok").count();
    let code = if all_ok {
        0
    } else if !results.is_empty() && ok_n > 0 {
        1
    } else {
        2
    };
    Ok(RoundFlow { code, reached_summary: true, all_ok, summary })
}

/// `--daily-round` 一轮的信封装配（纯函数；`run_drive_round_daily` 与 `--selftest` 共用这一处）。
/// 前七闸同序同句形、第八闸 `--root` ⇒ `round_refusal`；起步顺序照两枚现出口那一把：
/// 拒跑 → 读 ctx → 解 ctx（含 pid 那枚必填）→ 解两路 run 载荷 → **取本轮现场读数** → journal 重放 → res 合并 → 走一步。
/// ⚠ env 取数排在**journal 之前**、且不落任何一格账：它是"这轮问话要用哪几枚名字"的来路，
///   与 ctx 同一档（读不到就 fail 信封，绝不退化成"猜一份 settings"）。
/// 三枚注入点（`ark_decide`／`m9a_decide`／两枚 run）与两枚现出口共用同一批真身实现，
/// 自检喂假身 —— 这是"自检不许要求机器现场状态"那条老规矩在这座门上的落点。
fn drive_round_daily_envelope(
    i_round: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
    root: &str,
    hooks: &mut RoundHooks,
    ark_decide: &mut dyn FnMut(&str, bool) -> Result<(bool, Vec<(String, String)>), String>,
    m9a_decide: &mut dyn FnMut(&str, bool, bool) -> Result<(bool, Vec<(String, String)>), String>,
    run_maa: &mut dyn FnMut(&MaaScen, &[String]) -> Result<ArkRunOut, String>,
    run_m9a: &mut dyn FnMut(&M9aScen) -> Result<M9aRunOut, String>,
) -> (i32, String) {
    let fail = |m: &str| drive_env_fail(DRIVE_ROLE_ROUND, m);
    if let Some(m) = round_refusal(i_round, journal, ctx, res, panel_cs, state, log_sink, root) {
        return (2, fail(&m));
    }
    // ---- ctx ----
    let ctx_text = match std::fs::read_to_string(ctx) {
        Ok(t) => t,
        Err(e) => return (1, fail(&format!("驱动协议坏了：ctx 读不动：{}", e))),
    };
    let ctx_val = match jv_parse(&ctx_text) {
        Ok(v) => v,
        Err(e) => return (1, fail(&format!("驱动协议坏了：ctx 不是合法 JSON：{}", e))),
    };
    let rc = match round_ctx_parse(&ctx_val) {
        Ok(c) => c,
        Err(e) => return (1, fail(&format!("驱动协议坏了：{}", e))),
    };
    // ---- 两路 run 真做的现场载荷（与两枚现出口**共用同一把解析器**，键名只有一份）----
    // ⚠ 与两枚现出口的一处**故意不同**：那两档各自只推一路，载荷残缺就该起步即拒；这一档是**合体轮**，
    //   `--daily-round` 完全可能只跑其中一路（另一路在这棵树上根本没装，壳也就没处去写那份载荷）。
    //   在这里 strict 会把"没装 1999 的机器"读成"轮骨架档跑不了" —— 而真身那边那一路压根不进 plan。
    //   所以取**宽松**：读不动就用默认体，等真跑到那一路的 run 时由 drive_real_run 点名缺哪一格
    //   （失败落进 journal 的 err 行，可追 —— 与两枚现出口 B-2c③ 那条"缺 maaRun 不在这里报错"同一条规矩）。
    let mut scen = maarun_parse(&rc.maa_run).unwrap_or_else(|_| MaaScen::default());
    scen.state_dir = state.to_string();
    let mut m9a_scen = m9a_scen_core(&rc.m9a_run).unwrap_or_else(|_| M9aScen::default());
    // 名册外那五格照 --drive-m9a-daily 同一把（game/target 是清单写死的两枚字面，两枚可用性是装载期事实）。
    m9a_scen.state_dir = state.to_string();
    m9a_scen.game = "r1999".to_string();
    m9a_scen.target = "mumu".to_string();
    m9a_scen.rescue_available = true;
    m9a_scen.grab_available = false;
    // ---- 本轮现场读数（runtime/elevation/paths/nowText，逐枚点名）----
    let env = match round_env_fetch(&mut *hooks.call) {
        Ok(e) => e,
        Err(e) => return (1, fail(&format!("驱动协议坏了：本轮现场读数取不到：{}", e))),
    };
    // ---- journal ----
    let mut rows: Vec<JVal>;
    match std::fs::read_to_string(journal) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let init_row = jv_o(vec![("init", ctx_val.clone())]);
            if let Err(m) = drive_append_row(journal, &init_row) {
                return (1, fail(&format!("驱动协议坏了：journal 建不起来：{}", m)));
            }
            let _ = std::fs::remove_file(res);
            rows = Vec::new();
        }
        Err(e) => return (1, fail(&format!("驱动协议坏了：journal 读不动：{}", e))),
        Ok(text) => {
            let (init, r, done) = match drive_load_journal(&text) {
                Ok(t) => t,
                Err(m) => return (1, fail(&format!("驱动协议坏了：journal 读不动：{}", m))),
            };
            if jv_to_json(&init) != jv_to_json(&ctx_val) {
                return (
                    1,
                    fail("驱动协议坏了：journal 的 init 与 ctx 不是同一份（换了 ctx 就得起新 journal）"),
                );
            }
            if let Some(drow) = done {
                let _ = std::fs::remove_file(res);
                // ⚠ 这一档**不套** `drive_done_result`：那枚投影认的是链体那三格（status/detail/duration），
                // 而轮骨架的 done 装的是**本轮的账**（code/allOk/summary）。在这里折一遍就等于把
                // "这一轮跑成几分"读成"链体跑成了什么" —— 直接照账上那一格原样交出去。
                return (0, drive_env_done(DRIVE_ROLE_ROUND, &jv_get(&drow, "done")));
            }
            rows = r;
        }
    }
    // ---- res ----
    match std::fs::read_to_string(res) {
        Ok(text) => {
            let _ = std::fs::remove_file(res);
            let need = rows.len() as i64 + 1;
            match drive_merge_res(&text, need) {
                Ok(Some(row)) => {
                    if let Err(m) = drive_append_row(journal, &row) {
                        return (1, fail(&format!("驱动协议坏了：journal 追加失败：{}", m)));
                    }
                    rows.push(row);
                }
                Ok(None) => {}
                Err(m) => return (1, fail(&format!("驱动协议坏了：res 对不上：{}", m))),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return (1, fail(&format!("驱动协议坏了：res 读不动：{}", e))),
    }
    // ---- 一轮：三枚注入点由**调用方**给（生产那三枚在 run_drive_round_daily 里装，自检给假身）----
    let ark_dec = round_ark_decision_slot();
    let m9a_instance = jv_get_ci(&rc.m9a_run, "instance");
    let mut answer = |role: &str, action: &str, params: &JVal| -> Result<Option<DirectOut>, String> {
        round_answer(&mut *hooks, &env, root, state, &m9a_instance, &ark_dec, role, action, params)
    };
    let mut fx = DriveFx::new(
        journal,
        state,
        log_sink,
        scen,
        &mut *run_maa,
        m9a_scen,
        &mut *run_m9a,
        rows,
        Some(&mut answer),
    );
    match daily_round_chain(&rc, &env, &ark_dec, &mut fx, ark_decide, m9a_decide) {
        Ok(flow) => {
            if let Some(m) = fx.io_err.take() {
                return (1, fail(&format!("驱动协议坏了：{}", m)));
            }
            // done 的 result：本轮退出码是**外侧要的那一枚**（入口按它 exit），
            // allOk/summary 给面板与②-d 的话术用；两者都从已落账的读数来，不新立判据。
            let result = jv_o(vec![
                ("code", JVal::I(flow.code)),
                ("allOk", JVal::B(flow.all_ok)),
                ("reachedSummary", JVal::B(flow.reached_summary)),
                ("summary", jv_s(&flow.summary)),
            ]);
            let done_row = jv_o(vec![("done", result.clone())]);
            if let Err(m) = drive_append_row(journal, &done_row) {
                return (1, fail(&format!("驱动协议坏了：journal 追加失败：{}", m)));
            }
            (0, drive_env_done(DRIVE_ROLE_ROUND, &result))
        }
        Err(ArkAbort::Ask) => {
            if let Some(m) = fx.io_err.take() {
                return (1, fail(&format!("驱动协议坏了：{}", m)));
            }
            match fx.pending.take() {
                Some((seq, action, params)) => (0, drive_env_ask(DRIVE_ROLE_ROUND, seq, action, &params)),
                None => (1, fail("驱动协议坏了：Ask 出档却没记断点（内部错）")),
            }
        }
        Err(ArkAbort::Fail(m)) => (1, fail(&m)),
        Err(ArkAbort::Proto(m)) => (1, fail(&format!("驱动协议坏了：{}", m))),
    }
}

/// 轮骨架档的出口（`--daily-round`）。分支收在同一处 emit（照两枚现出口那条旧例：多开一处，
/// `--out` 就在其中一条分支上是空头承诺）。
/// `--fixture-run` 在这里喂给**两路**的 run（ark 吃 chains、m9a 不吃，但夹具形状一样）：
/// 自检与门不许为了跑一轮骨架去真起 MAA/M9A —— 那一枚旗标的语义在两枚现出口里已经定死，
/// 这里复用而不是新立旗标（协议形状再变一次就是再升一次号）。
fn run_drive_round_daily(
    i_round: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    state: &str,
    log_sink: &str,
    root: &str,
    fixture_run: &str,
    out: &Option<String>,
) -> i32 {
    let mut dec = |st: &str, ro: bool| arksk_call_decide(panel_cs, st, ro);
    let mut dec9 = |st: &str, rs: bool, ro: bool| m9a_call_decide(panel_cs, st, rs, ro);
    // 生产三枚注入点（verb 真起子进程／真睡／真设睡眠屏蔽）—— 与 envelope 的 hooks 参数同一处装配。
    let mut call = |v: &str, b: &JVal| call_panel_verb(panel_cs, v, b);
    let mut nap = |secs: u32| std::thread::sleep(std::time::Duration::from_secs(secs as u64));
    let mut es = |flags: u32| unsafe { SetThreadExecutionState(flags) };
    let mut hooks = RoundHooks { call: &mut call, nap: &mut nap, es: &mut es };
    let rc: i32;
    let json: String;
    // 拒跑闸提到读夹具之前（两枚现出口同一条例外：缺 --i-round 一律 rc=2 落拒跑句，
    // 绝对优先于"喂进来的夹具对不对"—— 拒跑句是给人的，必须第一眼就是它）。
    if let Some(m) = round_refusal(i_round, journal, ctx, res, panel_cs, state, log_sink, root) {
        rc = 2;
        json = drive_env_fail(DRIVE_ROLE_ROUND, &m);
    } else if fixture_run.is_empty() {
        let mut run = |sc: &MaaScen, ch: &[String]| drive_real_run(panel_cs, log_sink, sc, ch);
        let mut run9 = |sc: &M9aScen| drive_real_m9a_run(panel_cs, log_sink, sc);
        let pair = drive_round_daily_envelope(
            i_round, journal, ctx, res, panel_cs, state, log_sink, root, &mut hooks, &mut dec, &mut dec9, &mut run,
            &mut run9,
        );
        rc = pair.0;
        json = pair.1;
    } else {
        let loaded = match std::fs::read_to_string(fixture_run) {
            Ok(t) => jv_parse(&t).map_err(|e| format!("--fixture-run 的夹具不是合法 JSON：{}", e)),
            Err(e) => Err(format!("--fixture-run 的夹具读不动：{}", e)),
        };
        match loaded {
            Err(m) => {
                rc = 1;
                json = drive_env_fail(DRIVE_ROLE_ROUND, &format!("驱动协议坏了：{}", m));
            }
            Ok(fxv) => {
                let f_status = jv_str(&jv_get_ci(&fxv, "status"));
                let f_detail = jv_str(&jv_get_ci(&fxv, "detail"));
                let f_dur = {
                    let d = jv_get_ci(&fxv, "duration");
                    if matches!(d, JVal::Null) {
                        None
                    } else {
                        Some(jv_to_d(&d))
                    }
                };
                let f_rescued = jv_truthy(&jv_get_ci(&fxv, "rescued"));
                let f_logs: Vec<(String, String)> = jv_list(&jv_get_ci(&fxv, "logs"))
                    .into_iter()
                    .map(|v| (jv_str(&jv_get_ci(v, "level")), jv_str(&jv_get_ci(v, "text"))))
                    .collect();
                let sink = log_sink.to_string();
                let sink2 = log_sink.to_string();
                let logs9 = f_logs.clone();
                let mut run = {
                    let s = f_status.clone();
                    let d = f_detail.clone();
                    move |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
                        for (lv, tx) in &f_logs {
                            let _ = drive_sink_row(&sink, lv, tx);
                        }
                        Ok(ArkRunOut { status: s.clone(), detail: d.clone(), duration: f_dur })
                    }
                };
                let mut run9 = {
                    let s = f_status.clone();
                    let d = f_detail.clone();
                    move |_sc: &M9aScen| -> Result<M9aRunOut, String> {
                        for (lv, tx) in &logs9 {
                            let _ = drive_sink_row(&sink2, lv, tx);
                        }
                        Ok(M9aRunOut {
                            status: s.clone(),
                            detail: d.clone(),
                            duration: f_dur,
                            rescued: f_rescued,
                        })
                    }
                };
                let pair = drive_round_daily_envelope(
                    i_round, journal, ctx, res, panel_cs, state, log_sink, root, &mut hooks, &mut dec, &mut dec9, 
                    &mut run, &mut run9,
                );
                rc = pair.0;
                json = pair.1;
            }
        }
    }
    emit(&json, out);
    rc
}

/// B-1a 自检配套（仅供 --selftest）：迷你驱动器 —— 照生产壳 Invoke-GdArkDailyRust 的口径起轮：
/// 一轮一进程 → 读信封 → ask 就替它办（responder 出回执）并写 res → 再起一轮，直到 done/fail。
/// B-2c② 起日志不再从信封取，改从 --log-sink 侧信道取（会话开始前先截空，跑完读全量）——
/// 壳侧是边跑边排空，这里没有并发的壳，末次读一次即可，行序与产生序天然一致。
/// 返回（ask 动作序, 新面日志 "级别|文本", 终 kind, done 的 result（无则 Null）, 错一句话）。
fn drive_selftest_session(
    journal: &str,
    ctx: &str,
    res: &str,
    state: &str,
    log_sink: &str,
    responder: &mut dyn FnMut(&str, &JVal) -> Result<Option<JVal>, String>,
    run_maa: &mut dyn FnMut(&MaaScen, &[String]) -> Result<ArkRunOut, String>,
) -> (Vec<String>, Vec<String>, String, JVal, Option<String>) {
    let mut asked: Vec<String> = Vec::new();
    let mut logs: Vec<String> = Vec::new();
    let mut kind = String::new();
    let mut result = JVal::Null;
    let mut err: Option<String> = None;
    let _ = std::fs::write(log_sink, "");
    // 分流替身：自检不真跑 panel-cs（重试分流本身不在本组射程 —— 十连往返那组已钉"坏 exe 当场断"）。
    let mut dec =
        |_st: &str, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> { Ok((false, Vec::new())) };
    for _ in 0..64 {
        let (rc, env) =
            drive_ark_daily_envelope(true, journal, ctx, res, "unused-panel-cs", state, log_sink, &mut dec, &mut *run_maa);
        let ev = match jv_parse(&env) {
            Ok(v) => v,
            Err(e) => {
                err = Some(format!("信封不是合法 JSON：{}", e));
                break;
            }
        };
        kind = jv_str(&jv_get(&ev, "kind"));
        if kind == "done" {
            if rc != 0 {
                err = Some(format!("done 信封却非 0 码：{}", rc));
                break;
            }
            result = jv_get(&ev, "result");
            break;
        }
        if kind == "fail" {
            err = Some(jv_str(&jv_get(&ev, "error")));
            break;
        }
        if kind != "ask" {
            err = Some(format!("没见过的 kind：[{}]", kind));
            break;
        }
        if rc != 0 {
            err = Some(format!("ask 信封却非 0 码：{}", rc));
            break;
        }
        let act = jv_str(&jv_get(&ev, "action"));
        let seq = jv_int(&jv_get(&ev, "seq"), 0);
        let prm = jv_get(&ev, "params");
        asked.push(act.clone());
        let row = match responder(&act, &prm) {
            Ok(Some(d)) => {
                jv_o(vec![("seq", JVal::I(seq)), ("action", jv_s(&act)), ("ok", JVal::B(true)), ("data", d)])
            }
            Ok(None) => jv_o(vec![("seq", JVal::I(seq)), ("action", jv_s(&act)), ("ok", JVal::B(true))]),
            Err(m) => {
                jv_o(vec![("seq", JVal::I(seq)), ("action", jv_s(&act)), ("ok", JVal::B(false)), ("err", jv_s(&m))])
            }
        };
        if let Err(e) = std::fs::write(res, jv_to_json(&row)) {
            err = Some(format!("res 写不动：{}", e));
            break;
        }
    }
    drive_sink_drain(log_sink, &mut logs, &mut err);
    if err.is_none() && kind != "done" {
        err = Some("会话没推到 done（迷你驱动器轮次打完了）".to_string());
    }
    (asked, logs, kind, result, err)
}

/// 侧信道末次排空（--selftest 专用）：整本按行读回 "级别|文本"。
/// 读不动或有半截行 = 把错话塞进 last_err，让自检组当场红 —— 不许拿空日志假装跑过。
fn drive_sink_drain(log_sink: &str, logs: &mut Vec<String>, last_err: &mut Option<String>) {
    let text = match std::fs::read_to_string(log_sink) {
        Ok(t) => t,
        Err(e) => {
            if last_err.is_none() {
                *last_err = Some(format!("日志侧信道读不动：{}", e));
            }
            return;
        }
    };
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match jv_parse(line) {
            Ok(v) => logs.push(format!(
                "{}|{}",
                jv_str(&jv_get_ci(&v, "level")),
                jv_str(&jv_get_ci(&v, "text"))
            )),
            Err(e) => {
                if last_err.is_none() {
                    *last_err = Some(format!("日志侧信道第 {} 行不是合法 JSON：{}", i + 1, e));
                }
            }
        }
    }
}

/// B-1a 自检配套：迷你驱动器的回执桩（会话 A/B 共用）。
/// 桩面覆盖链体全序（desk…obs）—— 已搬动作（mark/obsrec/admin）不该出现在这里，出现即分派没接上。
/// B-2c③ 起 `run` 也从这里删了：真做之后壳不再 ask run，桩面留着这号就等于给"分派漏改"留了后门，
/// 让 `other => Err` 抓回归才是这一刀要的判据（run 的终态改由会话自带的 run 闭包注入）。
fn drive_selftest_respond(act: &str, _prm: &JVal) -> Result<Option<JVal>, String> {
    match act {
        "desk" => Ok(Some(jv_s("Default"))),
        "takeover" => Ok(None),
        "cfg" => Ok(Some(jv_o(vec![
            ("changes", JVal::A(vec![jv_s("chg-main")])),
            ("chains", JVal::A(vec![jv_s("Fight"), jv_s("Shop")])),
        ]))),
        "launch" => Ok(Some(JVal::B(true))),
        "skip" => Ok(Some(jv_o(vec![
            ("skipped", JVal::B(true)),
            ("plan", JVal::A(vec![jv_s("CE-5")])),
            ("evidence", JVal::A(vec![jv_s("x")])),
        ]))),
        "stop" => Ok(None),
        "ledger" => Ok(Some(jv_s("BN1"))),
        "obs" => Ok(Some(jv_o(vec![
            ("Found", JVal::B(true)),
            ("Date", jv_s("2026-10-04")),
            ("Stage", jv_s("1-7")),
            ("LastBattle", JVal::B(true)),
            ("SawGuiPlan", JVal::B(false)),
            ("Survivors", JVal::A(vec![jv_s("72"), jv_s("30012")])),
            ("Selected", jv_s("1-7")),
        ]))),
        other => Err(format!("迷你驱动器没有这号动作的桩：{}", other)),
    }
}

/// B-2d④ 自检配套（仅供 --selftest）：m9a 那一路的迷你驱动器。
/// 起轮口径与上面 `drive_selftest_session` **逐段同形**（一轮一"进程"→读信封→ask 就替它办→写 res→再起），
/// 因为那是驱动协议；三处差别全是这一路自己的事实：
/// - 走 `drive_m9a_daily_envelope`（role/why 两枚、ctx 只有 retry_once/m9aRun 两格）；
/// - run 闭包只吃 scen（真身 `Invoke-M9ARun` 签名里没有 `-Chains`）；
/// - `decide` 在这里是**注入的替身**而不是坏路径真跑：这一组要钉的是重试分流前后的动作序
///   （stop / sleep 20 / 再 run 一次），拿坏 exe 路径当替身只会让会话在第一次分流就 fail 收场。
fn drive_m9a_selftest_session(
    journal: &str,
    ctx: &str,
    res: &str,
    state: &str,
    log_sink: &str,
    responder: &mut dyn FnMut(&str, &JVal) -> Result<Option<JVal>, String>,
    run_m9a: &mut dyn FnMut(&M9aScen) -> Result<M9aRunOut, String>,
    decide: &mut dyn FnMut(&str, bool, bool) -> Result<(bool, Vec<(String, String)>), String>,
) -> (Vec<String>, Vec<String>, String, JVal, Option<String>) {
    let mut asked: Vec<String> = Vec::new();
    let mut logs: Vec<String> = Vec::new();
    let mut kind = String::new();
    let mut result = JVal::Null;
    let mut err: Option<String> = None;
    let _ = std::fs::write(log_sink, "");
    for _ in 0..64 {
        let (rc, env) = drive_m9a_daily_envelope(
            true,
            journal,
            ctx,
            res,
            "unused-panel-cs",
            state,
            log_sink,
            &mut *decide,
            &mut *run_m9a,
        );
        let ev = match jv_parse(&env) {
            Ok(v) => v,
            Err(e) => {
                err = Some(format!("信封不是合法 JSON：{}", e));
                break;
            }
        };
        kind = jv_str(&jv_get(&ev, "kind"));
        if kind == "done" {
            if rc != 0 {
                err = Some(format!("done 信封却非 0 码：{}", rc));
                break;
            }
            result = jv_get(&ev, "result");
            break;
        }
        if kind == "fail" {
            err = Some(jv_str(&jv_get(&ev, "error")));
            break;
        }
        if kind != "ask" {
            err = Some(format!("没见过的 kind：[{}]", kind));
            break;
        }
        if rc != 0 {
            err = Some(format!("ask 信封却非 0 码：{}", rc));
            break;
        }
        let act = jv_str(&jv_get(&ev, "action"));
        let seq = jv_int(&jv_get(&ev, "seq"), 0);
        let prm = jv_get(&ev, "params");
        asked.push(act.clone());
        let row = match responder(&act, &prm) {
            Ok(Some(d)) => {
                jv_o(vec![("seq", JVal::I(seq)), ("action", jv_s(&act)), ("ok", JVal::B(true)), ("data", d)])
            }
            Ok(None) => jv_o(vec![("seq", JVal::I(seq)), ("action", jv_s(&act)), ("ok", JVal::B(true))]),
            Err(m) => {
                jv_o(vec![("seq", JVal::I(seq)), ("action", jv_s(&act)), ("ok", JVal::B(false)), ("err", jv_s(&m))])
            }
        };
        if let Err(e) = std::fs::write(res, jv_to_json(&row)) {
            err = Some(format!("res 写不动：{}", e));
            break;
        }
    }
    drive_sink_drain(log_sink, &mut logs, &mut err);
    if err.is_none() && kind != "done" {
        err = Some("会话没推到 done（迷你驱动器轮次打完了）".to_string());
    }
    (asked, logs, kind, result, err)
}

/// B-2d④ 自检配套：m9a 那一路迷你驱动器的回执桩。八格里这里只有**七格** ——
/// 名册里没有 `run`（这一路唯一真做的动作，真做之后壳不再 ask run），出现即分派漏改；
/// 与 ark 侧 B-2c③ 那条「白名单与 case 双删，双删本身就是回归抓器」同一条判据。
/// 两枚闸的回执是三格 {code,line,joined}（不是ark 那一路的任何形状）。
fn drive_m9a_selftest_respond(act: &str, _prm: &JVal) -> Result<Option<JVal>, String> {
    match act {
        "updgate" => Ok(Some(jv_o(vec![
            ("code", JVal::I(0)),
            ("line", jv_s("已是最新，无需更新")),
            ("joined", jv_s("已是最新，无需更新")),
        ]))),
        "takeover" => Ok(None),
        "mumu" => Ok(Some(JVal::B(true))),
        "cfg" => Ok(Some(jv_o(vec![("changes", JVal::A(vec![jv_s("实例：1999")]))]))),
        "stop" => Ok(None),
        "sleep" => Ok(None),
        "audit" => Ok(Some(jv_o(vec![
            ("code", JVal::I(0)),
            ("line", jv_s("无待审活动")),
            ("joined", jv_s("无待审活动")),
        ]))),
        other => Err(format!("m9a 迷你驱动器没有这号动作的桩：{}", other)),
    }
}

/// `--arkskip` 的信封装配（纯函数；`run_arkskip` 与对拍门共用同一处 —— 门拿到的就是真出口）。
fn render_arkskip_json(items: &[(String, String)]) -> String {
    let cases: Vec<String> = items
        .iter()
        .map(|(name, data)| format!("{{\"name\":{},\"data\":{}}}", jstr(name), jstr(data)))
        .collect();
    format!(
        "{{\"ok\":true,\"why\":\"只吃内联夹具的 ark 认关止损链装配回放（8 档）；这不是对方行为的读数\",\"contract\":{},\"role\":\"R3-arkskip\",\"fixture\":\"arkskip\",\"count\":{},\"cases\":[{}]}}",
        jstr(CONTRACT),
        cases.len(),
        cases.join(",")
    )
}

/// `--arkskip` 那一档：同前几档的出口纪律 —— 只吃内联夹具、不加载任何 DLL、不碰任何现场。
/// **唯一外部调用** = `--panel-cs` 指的那枚判定 exe（装配真身在这一步就是真跑 decide-daily-retry）；
/// 缺它 / 跑挂 / 信封对不上 → ok:false 信封 + 非 0 码。8 档全过 → 0。
fn run_arkskip(panel_cs: &str, out: &Option<String>) -> i32 {
    let mut rc = 0;
    let json = if panel_cs.is_empty() {
        rc = 2;
        format!(
            "{{\"ok\":false,\"why\":\"装配回放档要 --panel-cs 指到判定 exe：这一档的装配真身本就真跑 decide-daily-retry，缺它试不了\",\"contract\":{},\"role\":\"R3-arkskip\"}}",
            jstr(CONTRACT)
        )
    } else {
        let scens = arkskip_scenarios();
        let mut items: Vec<(String, String)> = Vec::new();
        let mut bad: Option<String> = None;
        // B-1a：obsrec 桩的账本净计数跨档连续（与"新侧真账本净计数"同口径；三侧门只比投影）。
        let mut obs_n: i64 = 0;
        for sc in &scens {
            let mut dec = |st: &str, ro: bool| arksk_call_decide(panel_cs, st, ro);
            match arkskip_run_case(sc, &mut dec, &mut obs_n) {
                Ok(proj) => items.push((sc.name.to_string(), proj)),
                Err(e) => {
                    bad = Some(format!("装配回放档第 {} 档没跑完：{}", sc.name, e));
                    break;
                }
            }
        }
        match bad {
            Some(e) => {
                rc = 1;
                format!(
                    "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"R3-arkskip\"}}",
                    jstr(&e),
                    jstr(CONTRACT)
                )
            }
            None => render_arkskip_json(&items),
        }
    };
    emit(&json, out);
    rc
}

// ==================== B-2a：运行面宿主（Invoke-MaaRun 的循环骨架进核） ====================
//
// 这一层一条判据都不写：十档梯子、话术整句、兜底预算扣数、RecordTag 错位一格全都住在
// tools\panel-cs\RunLoops.cs(verb：decide-maa-run-tick)，本层只做「采集 → 喂 verb → 回灌状态 →
// 落地 Directives」，与 lib\maa.ps1:220-620（批次 7/9 冻结的现壳真身）逐行对照。
// 对拍门 tools\rustcore\probe-arkrun-rust-golden.ps1 比的是**可见行为序列**（日志/睡眠/起进程/
// 读文件/verb 调用序/返回值），不是内部变量 —— 两侧真跑同一枚 panel-cs，判据本身不在射程里。
//
// B-2a 刻意不落的两块（都不是"漏"，是登记过的批次切分，见 plan §11.32）：
//   1) 点击环境探测（maa.ps1:251-261）：两朵探测函数在两侧的世界里都不在场（old 侧桩浴不加载
//      那份 lib，new 侧宿主没有这条采集），逐字复刻一段永远不进的分支等于搬死代码；
//   2) 三枚 effect 执行块（stuck/drain/quiet）：两枚可用性恒 false ⇒ C# 不挂 effect，执行块
//      按构造不可达。真身那句"effect 没落到执行块，本 tick 作废"照抄保留在下面。
// 两块的原话是「随 B-2b 落」—— 就地补记（B-2b③ 实际只落了一块，另一块至今不落核，如实分说）：
//   · 第 2 块「effect 三执行块 + 真兜底链」：B-2b③ 已落。三枚执行块照 maa.ps1:502-596 逐字进核，
//     兜底链住在 vision_rescue()（七道闸只消费、话术全由 verb 整句给出），七枚现场 verb 一律
//     经 verb 通道外调 panel-cs ⇒ key 不进核：它只落子进程环境变量，那条通道整个在 panel-cs，
//     Rust 侧连取值都不做（见 plan §11.34 ④-1）。
//     回执仍然当场 push 进 effx、带 effectsResolved 重入 C#，同进程同 tick 内闭环、不进 journal。
//   · 第 1 块「点击环境探测」：**仍不进核**，且不是漏 —— §11.34 ② 的"进 Rust"清单里没有这一格。
//     现场量那半已由 ark-desktop-state 落到 panel-cs（verb 就位），核内没有调用点 ⇒ 那一档采集
//     在 Rust 侧仍是空白；接线时机与归属跟着 B-2c 的生产面一起定，别当成已经搬完。

/// 运行面对宿主世界的全部需求（十一件），每一件对应 lib\maa.ps1 或 lib\m9a.ps1 里的一行现场动作。
/// 两个宿主：`ScriptHost`=剧本夹具（B-2a 的对拍面，虚拟钟 + 事件时间线 + 投影），
/// `RealHost`=生产现场（B-2c 接线时才启用；本刀不接生产，`run` 仍走 ask）。
///
/// **B-2d② 把十件扩成十一件**（plan §11.39 ③-2 登记的两处破面，逐条给由）：
///   · `start_process` 多带一截 `args` —— m9a 必带 `-i/-a/-q/-f`，ark 一路从不带参数；
///   · 新增 `day_stamp` —— M9A 的日志名吃本地日历日（`log-yyyyMMdd.log`，每轮重算），
///     Rust 零依赖不自加日期库 ⇒ 真身走 GetLocalTime，剧本走夹具喂的日期表。
/// 那半句"key 不进核 ⇒ 没有新的 env 通道"照旧成立，本刀没碰。
trait MaaHost {
    /// Write-GdLog。level 归一：真身裸调用（不带 -Level）在两侧都落成显式 INFO。
    fn log(&mut self, level: &str, text: &str);
    /// Get-Date（秒）。剧本世界是整数虚拟钟、只在睡眠时走；真身世界是 Unix epoch 秒。
    fn clock(&mut self) -> f64;
    /// Start-Sleep -Seconds。剧本世界里这一步同时推钟并落该时刻的事件。
    fn sleep(&mut self, seconds: u32);
    /// `Test-Path` + `(Get-Item).Length` 合成一件：None = 文件不在（真身那两行总是成对出现）。
    fn file_len(&mut self, path: &str) -> Option<u64>;
    /// Get-GdFileTail（common.ps1:1033-1048）：返回（新增文本, 新偏移）。口径含"偏移越界 → 归零全读"。
    fn file_tail(&mut self, path: &str, offset: u64) -> (String, u64);
    /// Test-GdProcessRunning（Get-Process -Name：不分大小写、名字不带扩展名）。
    fn proc_running(&mut self, name: &str) -> bool;
    /// Start-Process -FilePath [-ArgumentList …] -WorkingDirectory。真身不 try/catch：起不来就整函数上抛。
    /// `args` 空 = ark 那一路的裸启动（投影逐字保持 `R|{exe}|{dir}`，ark 侧 27 档基线因此一字不动）；
    /// m9a 必带 `-i/-a/-q/-f` ⇒ 非空时投影追加一截，真身走 `Command::args`。
    fn start_process(&mut self, exe: &str, dir: &str, args: &[String]) -> Result<(), String>;
    /// `Get-Date -Format 'yyyyMMdd'` —— **m9a 独有**的一件：日志文件名每轮重算，跨 00:00 就换文件，
    /// 所以偏移必须按路径 keyed（ark 侧是两枚固定 offset 变量，形状不同）。
    /// 剧本侧由夹具喂的日期表按虚拟钟查表，投影只在**值变化时**落一条 `D|{stamp}`。
    fn day_stamp(&mut self) -> String;
    /// Invoke-GdCli：唯一转发通道，verb 的 data 原样交回（本层不解释其内容）。
    fn verb(&mut self, name: &str, body: &JVal) -> Result<JVal, String>;
    /// `Get-Command Invoke-GdVisionRescue`。B-2a 恒 false；B-2b③ 起转真，但**真身那句问不出**
    /// —— 树上有没有这枚函数属装载期事实，不属判据：剧本侧由 `rescueAvailable` 喂（门里由夹具说，
    /// 不许核自己猜），生产侧按 game 给常量（ark/m9a 都带 vision-rescue.ps1 ⇒ 恒 true）。
    fn rescue_available(&self) -> bool;
    /// `Get-Command Invoke-GdArkForegroundGrab`。同上，但**按场景分**：抢前台那枚住 vision-ark.ps1，
    /// 只有端游一路的 Libs 带着它 ⇒ ark 可 true，m9a 那一路结构上就是 false（剧本侧喂 `grabAvailable`）。
    fn grab_available(&self) -> bool;
}

/// 一条剧本事件：`at`=发生的钟点（秒，睡眠推进到该值时落地，同值按剧本先后），
/// `kind`=append|grow|truncate|up|down，`target`=gui|asst（文件）或进程名，`text`/`bytes` 按 kind 取用。
#[derive(Clone, Default)]
struct MaaEv {
    at: f64,
    kind: &'static str,
    target: String,
    text: String,
    bytes: i64,
}

/// 一档剧本 = 一次 Invoke-MaaRun 的全部现场输入（lib\maa.ps1 那边读 $S/$P/期望链路/时钟环境，
/// 这里读这一份）。两侧（old 桩浴 / new 内核）吃同一份 JSON，投影才可比。
#[derive(Clone, Default)]
struct MaaScen {
    name: String,
    dry_run: bool,
    expected: Vec<String>,
    maa_dir: String,
    maa_exe: String,
    maa_config: String,
    gui_log: String,
    asst_log: String,
    ark_exe: String,
    start_min: i64,
    timeout_min: i64,
    stall_min: i64,
    idle_sec: i64,
    abort_stall: bool,
    vision_max: i64,
    benign: Vec<String>,
    seed_gui: Option<String>,
    seed_asst: Option<String>,
    procs: Vec<(String, bool)>,
    events: Vec<MaaEv>,
    // ===== B-2b：兜底编排要吃的六格 =====
    // game/target 是 `Add-GdRescueRecord -Game/-Target` 与靶子名（真身 maa.ps1 五处调用点写死 'ark'）；
    // 两枚 *_available 取代 B-2a 的恒 false —— 剧本给什么就用什么，`Get-Command` 那两问的真身在
    // lib\ark-module.ps1 / lib\r1999-module.ps1 的 Libs 清单里（ark 两枚都在场；r1999 只有 rescue 在场，
    // 抢前台那枚住 lib\vision-ark.ps1，只有 ark 一路点它）。
    game: String,
    target: String,
    rescue_available: bool,
    grab_available: bool,
    // state_dir = `$global:GD.State`：rescue.json 与 state\rescue\ 那本账、那堆截图的落点。
    state_dir: String,
    // live = 现场 verb 的夹具队列（按名 FIFO）：有夹具就用夹具、没夹具就真跑 panel-cs。
    // 门(④)只喂七枚现场 verb 的夹具 ⇒ 判定与纯规则仍真跑；自检可以连纯规则一起喂 ⇒ 全链不碰 panel-cs。
    live: Vec<(String, JVal)>,
}

/// `Get-MaaPaths`(lib\maa.ps1:46-58) 的复刻：六格照真身一个不少 —— Config 那格循环本身用不上，
/// 但真身 maa.ps1:228 的 dry-run 话术里写的是 `$P.Exe`(不是 `$($P.Exe)`)，PS 会把整个 $P 渲染成
/// `@{Dir=…; Exe=…; Config=…; GuiLog=…; AsstLog=…; Name=…}` 再补一个字面 `.Exe`，
/// 于是那一行日志里六格全都要露面。缺 Config 就没法逐字复刻这句原话，故按契约照算。
struct MaaPaths {
    dir: String,
    exe: String,
    config: String,
    gui_log: String,
    asst_log: String,
    name: String,
}

impl MaaPaths {
    fn of(sc: &MaaScen) -> MaaPaths {
        // asst.log 缺省 'debug\asst.log'（老配置里可能没有这一项，它只是线索不是判据）；
        // Name = maaExe 去扩展名。两条都是真身原话，Join-GdPath 的口径见 gd_join_path。
        let asst_rel = if sc.asst_log.is_empty() {
            "debug\\asst.log".to_string()
        } else {
            sc.asst_log.clone()
        };
        MaaPaths {
            dir: sc.maa_dir.clone(),
            exe: gd_join_path(&sc.maa_dir, &[&sc.maa_exe]),
            config: gd_join_path(&sc.maa_dir, &[&sc.maa_config]),
            gui_log: gd_join_path(&sc.maa_dir, &[&sc.gui_log]),
            asst_log: gd_join_path(&sc.maa_dir, &[&asst_rel]),
            name: file_stem(&sc.maa_exe),
        }
    }

    /// PowerShell 把 `[pscustomobject]` 当字符串用时的渲染（属性顺序 = 真身字面量顺序，分隔符 `; `）。
    /// 不是随便挑的写法：maa.ps1:228 那一行日志吃的就是这个形状，门(new vs rust)逐字符比它。
    fn ps_to_string(&self) -> String {
        format!(
            "@{{Dir={}; Exe={}; Config={}; GuiLog={}; AsstLog={}; Name={}}}",
            self.dir, self.exe, self.config, self.gui_log, self.asst_log, self.name
        )
    }
}

/// `Join-GdPath`(common.ps1:294-315) 的原身支复刻：空 base → 空串；空段跳过；逐段 Join-Path。
fn gd_join_path(base: &str, parts: &[&str]) -> String {
    if base.is_empty() {
        return String::new();
    }
    let mut p = base.to_string();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        p = ps_join_path(&p, part);
    }
    p
}

/// .NET `Path::Combine` 在本门射程内的两支持：第二段是 rooted（盘符或 UNC）就整段替换，
/// 否则用单个反斜杠接上。夹具只用反斜杠相对段与绝对段，两种都在这里，不走三分支。
fn ps_join_path(base: &str, rel: &str) -> String {
    if is_rooted_path(rel) {
        return rel.to_string();
    }
    let bytes: Vec<char> = base.chars().collect();
    if let Some(&last) = bytes.last() {
        if last == '\\' || last == '/' {
            return format!("{}{}", base, rel);
        }
    }
    format!("{}\\{}", base, rel)
}

fn is_rooted_path(p: &str) -> bool {
    let c: Vec<char> = p.chars().collect();
    if c.is_empty() {
        return false;
    }
    if c[0] == '\\' || c[0] == '/' {
        return true;
    }
    c.len() >= 2 && c[1] == ':' && (c[0].is_ascii_alphabetic())
}

/// `[System.IO.Path]::GetFileNameWithoutExtension`：取最后一段路径，再去掉最后一个点之后的部分。
fn file_stem(p: &str) -> String {
    let cut = p.rfind(['\\', '/']).map(|i| i + 1).unwrap_or(0);
    let last = &p[cut..];
    match last.rfind('.') {
        Some(d) if d > 0 => last[..d].to_string(),
        _ => last.to_string(),
    }
}

/// `[System.IO.Path]::GetFileName`（投影里给 TAIL 事件打标签用）。
fn file_name(p: &str) -> String {
    let cut = p.rfind(['\\', '/']).map(|i| i + 1).unwrap_or(0);
    p[cut..].to_string()
}

/// Invoke-MaaRun 的返回：Status/Detail 恒有，Duration 只在挂钟收尾那一路有，
/// Errors 在 dry-run 那一路根本没有这个键（真身 `@{ Status='dryrun'; Detail='' }`）。
struct MaaRet {
    status: String,
    detail: String,
    duration: Option<f64>,
    errors: Option<Vec<String>>,
}

impl MaaRet {
    /// 投影末行：`Z|status|detail|dur|errors`。没给的键分别落成 "-"，空数组落成 ""（两侧同一口径）。
    fn proj_line(&self) -> String {
        let dur = match self.duration {
            None => "-".to_string(),
            Some(d) => format!("{}", d),
        };
        let errs = match &self.errors {
            None => "-".to_string(),
            Some(v) => v.join("\u{1e}"),
        };
        format!("Z|{}|{}|{}|{}", self.status, self.detail, dur, errs)
    }
}

/// m9a 一路的剧本事件表把 target 收窄到 `"m9alog"` 一枚（见 `m9arun_parse`），所以这两格在 m9a 档里
/// 是空串：m9a 的日志只有一本，名字还每轮变，装不进 `MaaPaths` 那两枚固定路径的形状。
#[derive(Clone, Default)]
struct M9aScen {
    name: String,
    dry_run: bool,
    // ===== ctx 名册：逐枚点名 15 枚（paths 3 ＋ conn 2 ＋ instance 2 ＋ package 1 ＋ runtime 7）=====
    // 硬约束（plan §11.39 ②-3，与 B-2c① 那条同款）：**绝不把 `$S.runtime` 整块序列化** ——
    // `visionApiKey` 与下面这六枚 runtime 键住同一个块，整块序列化就等于把 key 送进核、送进落盘。
    // paths 3（真身 `Get-M9APaths` 的六格里循环只吃这四样：Dir/Exe/LogDir/Name，Name 由 Exe 去扩展名算出）
    m9a_dir: String,
    m9a_exe: String,
    m9a_log_dir: String,
    // conn 2（真身 `Get-M9AConnection` 从实例 JSON 里解出来的两枚；解 JSON 那一步留在 ctx 侧，
    // 因为它吃的是 `$Instance.Path` 指向的对方文件，不属判据）
    adb: String,
    serial: String,
    // instance 2 ＋ package 1
    instance_id: String,
    instance_name: String,
    package: String,
    // runtime 7
    mode: String, // m9aInstanceArg（'id'|'name'，缺键按真身调用的默认走 'id'）
    force_start: bool, // m9aForceStart
    /// m9aCliArgs 的**三态**：`None` = settings 里这枚键压根不存在 ⇒ 真身 `Get-GdM9ACliEnabled`
    /// 那句 `if ($null -eq …) { return $true }` 判"开"。写成 `bool` 就把"没填"和"填了 false"两档
    /// 塌成一档，而这两档在真身里走的是完全不同的路（前者带 -i/-a/-q/-f，后者裸启动＋"在跑就 throw"）。
    cli: Option<bool>,
    vision_max: i64, // visionMaxRescue（m9a 送的是**原始值**，不像 ark 那样先本地镜像一份预算）
    start_min: i64, // r1999StartTimeoutMinutes
    timeout_min: i64, // r1999TimeoutMinutes
    stall_min: i64, // stallTimeoutMinutes
    // ===== 兜底链三格 ＋ 两枚可用性（真身 m9a.ps1 写死 -Game 'r1999' -Target 'mumu'）=====
    game: String,
    target: String,
    state_dir: String,
    rescue_available: bool,
    grab_available: bool, // m9a 一路结构上就是 false（抢前台那枚只有端游点）
    // ===== 夹具面 =====
    /// 本地日期读数表 `(at, yyyyMMdd)`，升序；M9A 的日志名每轮重算 ⇒ 这张表要能跨 00:00，
    /// 门里"跨 00:00 的一轮"那一格就靠它（plan §11.39 ⑥-2，arkrun 的 27 档复用不了）。
    days: Vec<(f64, String)>,
    /// 起点日志内容：给 Some(整写，空串也算"文件在、零字节")／没给就删（"日志压根不在场"单独一档）。
    seed: Option<String>,
    procs: Vec<(String, bool)>,
    events: Vec<MaaEv>,
    live: Vec<(String, JVal)>,
}

/// `Get-M9APaths`(m9a.ps1:43-53) 里循环用得上的四格。
/// 真身那六格的 AppSettings/FrameLog 两格**不复刻**：ma 循环一次都不读它们，
/// 而 m9a 的 dry-run 话术写的是 `$($P.Exe)`（正确插值，不是 maa.ps1:228 那种整对象渲染），
/// 所以这边不需要 `ps_to_string` —— 两格补上来只是死字段。
struct M9aPaths {
    dir: String,
    exe: String,
    log_dir: String,
    name: String,
}

impl M9aPaths {
    fn of(sc: &M9aScen) -> M9aPaths {
        M9aPaths {
            dir: sc.m9a_dir.clone(),
            exe: gd_join_path(&sc.m9a_dir, &[&sc.m9a_exe]),
            log_dir: gd_join_path(&sc.m9a_dir, &[&sc.m9a_log_dir]),
            name: file_stem(&sc.m9a_exe),
        }
    }
}

/// Invoke-M9ARun 的返回：Status/Detail 恒有；Duration 只有 ok 那一路带（真身 `$res.Duration = $elapsed`
/// 那一支）、Rescued 只在"进程都没了但画面兜成了"那一路才挂（`:493` 的语义）、
/// Tasks/FailedTasks 是判因的话术料（`FailedTasks` 这一格在真身里**只有 blocked 才存在**，
/// 因为 `Resolve-M9AStatus:315` 那句"信封有这个键才带"）。
/// Args 是 dry-run 那一路唯一多出来的键（`:342`），其余档位真身压根不带 ⇒ 这里恒空数组。
#[derive(Default)]
struct M9aRet {
    status: String,
    detail: String,
    duration: Option<f64>,
    rescued: bool,
    tasks: Vec<String>,
    failed_tasks: Option<Vec<String>>,
    args: Vec<String>,
}

impl M9aRet {
    /// 投影末行：`Z|status|detail|dur|rescued|tasks|failed|args`。
    /// 缺键落 "-"（与 ark 的 `Z|` 同口径）、数组内用单元分隔符，只有 args 用空格
    /// —— 因为真身那句 `-join ' '` 就是空格，这一格在门上还要跟 `R|` 行对照着读。
    fn proj_line(&self) -> String {
        let dur = match self.duration {
            None => "-".to_string(),
            Some(d) => format!("{}", d),
        };
        let failed = match &self.failed_tasks {
            None => "-".to_string(),
            Some(v) => v.join("\u{1e}"),
        };
        format!(
            "Z|{}|{}|{}|{}|{}|{}|{}",
            self.status,
            self.detail,
            dur,
            if self.rescued { 1 } else { 0 },
            self.tasks.join("\u{1e}"),
            failed,
            self.args.join(" ")
        )
    }
}

/// completed 的容器：PS 侧是大小写不敏感的 hashtable（重复键**保留首个拼写**、覆盖值），
/// 键序不是不变量 ⇒ 投影前一律 Ordinal 排序（与 RunLoops.cs:403 同口径；链名只可能是 [A-Za-z]+，
/// 夹具面不混大小写是门纪律，见 probe-live-golden 头注那条同源规矩）。
fn cmap_set(m: &mut Vec<(String, i64)>, k: &str, v: i64) {
    for (ek, ev) in m.iter_mut() {
        if ek.eq_ignore_ascii_case(k) {
            *ev = v;
            return;
        }
    }
    m.push((k.to_string(), v));
}

fn cmap_to_jv(m: &[(String, i64)]) -> JVal {
    JVal::O(m.iter().map(|(k, v)| (k.clone(), JVal::I(*v))).collect())
}

fn cmap_keys(m: &[(String, i64)]) -> Vec<String> {
    let mut ks: Vec<String> = m.iter().map(|(k, _)| k.clone()).collect();
    ks.sort();
    ks
}

fn vstr_list(v: &JVal) -> Vec<String> {
    jv_list(v).iter().map(|e| jv_str(e)).collect()
}

/// `[string]::IsNullOrWhiteSpace`（Get-MaaAsstStuckState/Hint 的壳内早退判据，maa.ps1:117-138）。
fn is_null_or_white(s: &str) -> bool {
    s.trim().is_empty()
}

/// `Wait-GdCondition`(common.ps1:1017-1031) 的复刻：先测后睡、`now < deadline` 才继续，
/// 超时那句 WARN 的整句与参数面（秒数是 [int] 原样）逐字对齐。
fn wait_condition<H: MaaHost, F: FnMut(&mut H) -> bool>(
    h: &mut H,
    timeout_seconds: i64,
    poll_seconds: u32,
    what: &str,
    mut test: F,
) -> bool {
    let deadline = h.clock() + timeout_seconds as f64;
    while h.clock() < deadline {
        if test(h) {
            return true;
        }
        h.sleep(poll_seconds);
    }
    h.log(
        "WARN",
        &format!("等待超时({}s)：{}", timeout_seconds, what),
    );
    false
}

/// 把一帧 verb 回的 Logs 落进宿主：真身那两句"有 Level 且不是 INFO 就带 -Level，否则裸调"
/// 归一化成同一个显式 INFO（maa.ps1:317-320 / 467-470 同款）。
fn replay_logs<H: MaaHost>(h: &mut H, logs: &JVal) {
    for l in jv_list(logs) {
        let lvl = jv_str(&jv_get(l, "Level"));
        let text = jv_str(&jv_get(l, "Text"));
        if !lvl.is_empty() && lvl != "INFO" {
            h.log(&lvl, &text);
        } else {
            h.log("INFO", &text);
        }
    }
}

// ==================== B-2b：兜底编排进核（lib\vision-rescue.ps1 的 Invoke-GdVisionRescue）====================
// 分工照 plan §11.34 ②：Rust 这层只搬"七道闸的消费顺序 + 候选循环 + 三种结论话术 + 两本账的落盘"，
// 凡要动这台机器的（拍窗口、点客户区、抢前台、起 Python 问模型、读 key/管理员/解释器）一律经 verb 外调。
// key 因此进不了核：它只在 panel-cs 侧进子进程环境变量（Live.cs 的 RunChild），核里一个字节都摸不到，
// 也不经命令行、不进请求文件、不进日志（§11.34 里"必答"的那一条 —— 本刀为 m9a 扩的那两件
// 与 key 无关，那条判据照旧成立：key 只进 panel-cs 子进程的 EnvironmentVariables）。

/// `tag -like 'stuck*'` 这类"前缀通配"：PS 的 -like 不分大小写。三种 tag 前缀（stuck/drain/stall）
/// 加 grab 那枚空 tag 用得上，别处的 -like 形状（带 [] ?）在本射程里不存在。
/// 字节级比较不返 UTF-8 边界 panic（tag 只可能是 C# 拼的 ASCII 前缀 + 数字）。
fn tag_like(tag: &str, prefix: &str) -> bool {
    let t = tag.as_bytes();
    let p = prefix.as_bytes();
    t.len() >= p.len() && t[..p.len()].eq_ignore_ascii_case(p)
}

/// `kind -eq 'grab'`：PS 的 -eq 比字符串不分大小写（C# 出的 kind 恒小写，这里同形而不是裸 ==）。
fn kind_eq(kind: &str, want: &str) -> bool {
    kind.eq_ignore_ascii_case(want)
}

/// 真身 maa.ps1:230-231 的 `$expectedCount`：大小写不敏感去重、重复键留首个拼写、值恒 1。
/// （容器语义与 completed 同一把尺：`$expectedCount[$c] = 1` 落在 PS hashtable 上。）
fn expected_dedup(chains: &[String]) -> Vec<(String, i64)> {
    let mut m: Vec<(String, i64)> = Vec::new();
    for c in chains {
        cmap_set(&mut m, c, 1);
    }
    m
}

fn cmap_get(m: &[(String, i64)], k: &str) -> Option<i64> {
    m.iter()
        .find(|(ek, _)| ek.eq_ignore_ascii_case(k))
        .map(|(_, ev)| *ev)
}

/// maa.ps1:411 的 `$doneCount`：每条期望链最多算 1（`[Math]::Min([int]$completed[$k], 1)`，键不在场按 0）。
fn done_count(expected: &[(String, i64)], completed: &[(String, i64)]) -> i64 {
    expected
        .iter()
        .filter(|(k, _)| cmap_get(completed, k).unwrap_or(0) >= 1)
        .count() as i64
}

/// maa.ps1:412 的 `$allDone`（期望链数为 0 时恒假：没有期望链就谈不上"跑齐"）。
/// 只在 drain 档执行块的现场复核守卫里用（判据本体在 RunLoops.cs:398，两处同口径）。
fn all_done(expected: &[(String, i64)], completed: &[(String, i64)]) -> bool {
    !expected.is_empty() && done_count(expected, completed) >= expected.len() as i64
}

/// 兜底链的**环境依赖**三格（B-2d② 从 `MaaScen` 里抽出来的一层，plan §11.39 ②-4 那条"配套抽取"）。
/// 为什么现在抽：`vision_rescue`/记账那六枚函数原本吃整份 `&MaaScen`，实际只读这三格
/// （`target` 给六闸与截图文件名、`state_dir` 给两本账的落点、`game` 给账本的 `-Game`）。
/// m9a 那一路的循环宿主有自己的场景结构体（`M9aScen`，字段集与 ark 完全不同），
/// 不抽这一层就得让 m9a 造一份假 `MaaScen` 或者把六枚函数各写两遍 —— 两条都比抽薄更糟。
#[derive(Clone, Default)]
struct RescueEnv {
    game: String,
    target: String,
    state_dir: String,
    // ===== B-2d③：mumu 那一路的两枚现场量（ark 路恒空串 ⇒ ark 行为零影响）=====
    // 为什么是**传进来**而不是核里去算：算端点那段是「三源两正则 ＋ 两次 native」
    // （`Get-GdMuMuAdbEndpoint:20-73`），plan §11.39 ②-3 明令不许搬进核，本刀照字面不搬。
    // 事实源在 ctx 的 `conn` 两格（`m9arun_parse:7971-7972`，真身 `Get-M9AConnection` 现取），
    // 到这儿已经是一枚**绝对路径**与一枚**序列号**，核只转达、不判定。
    adb: String,
    serial: String,
}

impl RescueEnv {
    /// ark 一路：三格直接投影自 `MaaScen`（真身那三处写死的 'ark' 已在 `maarun_parse` 的默认支里）。
    /// adb/serial 给空串：端游那一路压根没有 adb，`Live.cs` 的 ark 支也读不到这两枚键。
    fn of_maa(sc: &MaaScen) -> RescueEnv {
        RescueEnv {
            game: sc.game.clone(),
            target: sc.target.clone(),
            state_dir: sc.state_dir.clone(),
            adb: String::new(),
            serial: String::new(),
        }
    }
    /// m9a 一路：同一格三枚，缺省是真身写死的 `'r1999'`／`'mumu'`（`m9a.ps1:393/396/463/471`）。
    /// adb/serial 走 `conn` 那两格 ⇒ 与循环开头 `adb-connect`／每轮 `adb-game-pid` 用的是**同一份**，
    /// 不是"兜底时另算一次端点"（真身那边 Shot/Click/Alive 各自现查，本刀定案见 plan §11.42 ①-12）。
    fn of_m9a(sc: &M9aScen) -> RescueEnv {
        RescueEnv {
            game: sc.game.clone(),
            target: sc.target.clone(),
            state_dir: sc.state_dir.clone(),
            adb: sc.adb.clone(),
            serial: sc.serial.clone(),
        }
    }
}

/// `Get-GdRescueShotDir`(vision-rescue.ps1:64) = `<GD.State>\rescue`。
fn rescue_shot_dir(env: &RescueEnv) -> String {
    gd_join_path(&env.state_dir, &["rescue"])
}

/// `Get-GdRescueLedgerPath`(:411) = `<GD.State>\rescue.json`。
fn rescue_ledger_path(env: &RescueEnv) -> String {
    gd_join_path(&env.state_dir, &["rescue.json"])
}

/// `Clear-GdVisionFiles`(:399-407) 的"留哪几枚"那半，抽成纯函数：mtime 在自检里造不出可信次序，
/// 判据只能单独钉。同刻并列按输入先后（Rust sort_by 稳定；PS 的 Sort-Object 对同键不保证次序，
/// 这一格两侧都不作判据 —— 门④的账本/截图比对口径因此不碰文件名顺序）。
fn pick_stale(items: Vec<(String, u64)>, keep: usize) -> Vec<String> {
    let mut v = items;
    v.sort_by(|a, b| b.1.cmp(&a.1));
    if v.len() > keep {
        v.split_off(keep).into_iter().map(|(p, _)| p).collect()
    } else {
        Vec::new()
    }
}

/// 真跑那半：列目录、只收 *.png（`-Filter '*.png'` 在不分大小写的文件系统上等价于 endswith ".png"）、
/// 删掉 pick_stale 给的那批。删除失败逐枚吞掉（真身 -ErrorAction SilentlyContinue 同形）；
/// 目录不在场就当没事（真身那句 `if (-not (Test-Path ...)) { return }`）。
fn trim_shot_dir(dir: &str, keep: usize) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    let mut items: Vec<(String, u64)> = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.to_lowercase().ends_with(".png") {
            continue;
        }
        let secs = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        items.push((entry.path().to_string_lossy().to_string(), secs));
    }
    for p in pick_stale(items, keep) {
        let _ = std::fs::remove_file(&p);
    }
}

/// `$ledger.events = @($ledger.events) + @{…}` 再 `-gt $Keep → Select -Last $Keep`（:450-461）。
/// 也抽成纯函数给自检钉"第 61 条挤掉第 1 条"这一格。
fn ledger_push(events: Vec<JVal>, entry: JVal, keep: i64) -> Vec<JVal> {
    let mut v = events;
    v.push(entry);
    if keep > 0 && v.len() > keep as usize {
        v.split_off(v.len() - keep as usize)
    } else {
        v
    }
}

/// `Invoke-GdVisionRescue` 的返回形状（真身：`@{Tried;Done;Detail}`）。
struct VisionRc {
    tried: bool,
    done: bool,
    detail: String,
}

/// `New-GdRescueSkip`(:66-70)："这次没救成，原因是……"的统一出口 = 一次点击都没发出去。
fn rescue_skip<D: Into<String>>(why: D) -> VisionRc {
    VisionRc {
        tried: false,
        done: false,
        detail: why.into(),
    }
}

/// 兜底总入口的复刻（vision-rescue.ps1:305-397）。七道闸的顺序不许调换（真身头注原话），
/// 前六道合成一次 `resolve-rescue-context` 外调（闸序在那边逐条落着），第七道"截图"与黑屏闸在
/// `ark-window-shot`／`mumu-screen-shot`（B-2d③ 起两路同形状十五格，核侧消费点一字不区分）。
/// **Err = 真身没罩 try 的那些调用**（截图/点位换算/点击/比签名），
/// 它们会一路冒到调用方的 catch ⇒ 回执 `{kind:'rescue',exception:true,message}`；
/// 唯独"问模型"这一处真身自己罩了 catch，所以它的 verb 错在本地降级成一句 skip（见下面那条注释）。
fn vision_rescue<H: MaaHost>(
    h: &mut H,
    env: &RescueEnv,
    tag: &str,
    context: &str,
) -> Result<VisionRc, String> {
    // 闸 1..6（后端在场 / 开关 / 管理员 / key / Python / 对象在场）。skip 话术全部出自 verb，
    // 编排侧一个字不改 —— 关开关那句必须带 settings 的键名，t13 按那几个字断言"关了确实一次都不试"。
    // mumu 那一路的"对象在场"也是**现场量**，但它由 C# 那一侧现查（`MumuAlive`：devices→补 connect），
    // 与 ark 那一路"按进程名现枚举"同形 ⇒ 核在这里**不多问一枚 verb**，只把端点两键当入参喂进去。
    // ⚠ 10-12 实测改口（原 §11.42 ①-12／⑥-1 定的是"核先调 adb-connect 再把 Ok 当 alive 覆盖格喂进来"）：
    // 那条把 alive 变成 ctx 的**入参** ⇒ 核只能排在 ctx 之前，而真身的闸序是"先拿靶子(=这一问)、过前五闸、
    // 第六闸才碰 adb" ⇒ m9arun 门 H2 当场红 8 处（投影多一枚 `V|adb-connect` 且顺序正反），
    // 且"没配 key／开关关着"那一档核会白起两趟真身根本不起的 adb native。 ⇒ 现查移回 C#，闸序随真身。
    // 端点三源两正则仍绝不进核（§11.39 ②-3）：这里只把 conn 那两枚**已经算好的**键转喂 verb。
    // ark 那一路一个键都不多喂（`alive`/`adb`/`serial` 三格全缺 ⇒ C# 自走它的进程现查，与 B-2b③ 起逐字相同）。
    let is_mumu_target = kind_eq(&env.target, "mumu");
    let mut ctx_body = vec![("target", jv_s(&env.target))];
    if is_mumu_target {
        ctx_body.push(("adb", jv_s(&env.adb)));
        ctx_body.push(("serial", jv_s(&env.serial)));
    }
    let ctx = h.verb("resolve-rescue-context", &jv_o(ctx_body))?;
    if !jv_truthy(&jv_get(&ctx, "CanAsk")) {
        return Ok(rescue_skip(jv_str(&jv_get(&ctx, "Skip"))));
    }
    let kind = jv_str(&jv_get(&ctx, "Kind"));
    let name = jv_str(&jv_get(&ctx, "Name"));
    // 靶子 kinds 与现场 verb 的对应：ark 两枚随 B-2b③、mumu 两枚随 B-2d③ **都已在核**。
    // 下面那句 `other` 桩已删净（§11.42 ①-1：删桩即回归抓器 —— 只改一侧、另一侧的夹具还认 mumu 就当场红）。
    // 走不到 catch-all：resolve-rescue-context 对认不出的 kind 第一道闸就回 CanAsk=false（`known` 判据），
    // 真到这儿说明夹具撒了谎，宁可当场 Err 让上面落成 exception 回执，也不编一句真身没有的话术。
    let (shot_verb, click_verb) = match kind.as_str() {
        "ark" => ("ark-window-shot", "ark-window-click"),
        "mumu" => ("mumu-screen-shot", "mumu-screen-click"),
        other => return Err(format!("vision_rescue：靶子自报了现场 verb 之外的 kind：{}", other)),
    };
    // 两路口径不同只在这三格：mumu 的 shot/click 要带 adb/serial（回执十五格/两格已在 C# 同形状补齐），
    // ark 的 body 逐字不动。投影安全由构造保证——`ScriptHost::verb` 只投影 `V|{name}` 一行
    // （§11.42 ①-10），多喂键不可能把 ark 侧那 27 档 1218 步基线带跑。
    let is_mumu = kind_eq(&kind, "mumu");

    let dir = rescue_shot_dir(env);
    let suffix = gd_shot_suffix();
    // 闸 7：先拍到东西才判得了黑屏。文件名 `<kind>-<tag>-<MMdd-HHmmss>-before.png` 是编排侧的现场量，
    // verb 只管往这个路径落图（Live.cs 那句"路径属现场量，由编排侧拼"就是对着这里写的）。
    let before_path = gd_join_path(&dir, &[&format!("{}-{}-{}-before.png", kind, tag, suffix)]);
    let mut before_body = vec![("path", jv_s(&before_path)), ("name", jv_s(&name))];
    if is_mumu {
        before_body.push(("adb", jv_s(&env.adb)));
        before_body.push(("serial", jv_s(&env.serial)));
    }
    let before = h.verb(shot_verb, &jv_o(before_body))?;
    if !jv_truthy(&jv_get(&before, "Ok")) {
        // verb 的 Skip 已是"截图失败：<Why>"整句；异常路径上 Why 自带前缀 ⇒ 双前缀那处原样怪癖在这复刻。
        return Ok(rescue_skip(jv_str(&jv_get(&before, "Skip"))));
    }
    if !jv_truthy(&jv_get(&before, "CanContinue")) {
        // 真身这一道写作 `[int]$before.AvgLum -lt 8`；判据与那句"截图几乎是全黑(平均亮度 N)"随 B-2b②
        // 落进 ark-window-shot 的 CanContinue/Skip 两格，这里照字面消费，不再回核里重拼一遍。
        return Ok(rescue_skip(jv_str(&jv_get(&before, "Skip"))));
    }
    h.log("WARN", &jv_str(&jv_get(&before, "LogLine")));

    // 问模型：scene/userText 取自靶子（ark 一路两格都是空串 = 用 C# 那两份默认文案，逐字不许变；
    // mumu 一路两格是 `Live.cs` 的 1999 话术 ⇒ 核只原样转达、一个都不认识，plan §11.42 ①-11 那条硬约束），
    // context 是"为什么要问这一句"，截 200 字的规矩在 Live.cs AskUserText 里。
    // 真身 try/catch 罩的是整次问模型（含转发壳自己起不来），所以 verb 的 Err 也归"模型调用失败"那一句 ——
    // 这是三处 catch 里唯一把 verb 错吃掉的一处，照抄。
    let ask = match h.verb(
        "vision-ask-model",
        &jv_o(vec![
            ("imagePath", jv_get(&before, "Path")),
            ("scene", jv_get(&ctx, "Scene")),
            ("userText", jv_get(&ctx, "UserText")),
            ("context", jv_s(context)),
        ]),
    ) {
        Ok(d) => d,
        Err(e) => return Ok(rescue_skip(format!("模型调用失败：{}", e))),
    };
    // 真身的 Invoke-GdVisionAsk 对 raw.Ok=false 是 throw（消息就是 Error），罩它的 catch 拼成下面这句。
    if !jv_truthy(&jv_get(&ask, "Ok")) {
        return Ok(rescue_skip(format!(
            "模型调用失败：{}",
            jv_str(&jv_get(&ask, "Error"))
        )));
    }
    let v = jv_get(&ask, "Verdict");
    // 空文本 verb 直接给 Null；文本有但抠不出 JSON 时 verb 仍回一个 Found=false 的对象
    // —— 真身那边 Get-GdVisionVerdictFromText 两种都塌成 $null，所以 Found 也归"没按格式回 JSON"。
    if matches!(&v, JVal::Null) || !jv_truthy(&jv_get(&v, "Found")) {
        return Ok(rescue_skip("模型没按格式回 JSON，不拿它的话当依据"));
    }
    let what = jv_str(&jv_get(&v, "What"));
    if !jv_truthy(&jv_get(&v, "Blocking")) {
        return Ok(rescue_skip(format!("模型认为没有弹窗挡路({})", what)));
    }
    // 换算 + 越界拒绝都在 vision-click-point（C# 那一层连"右下角最后一个像素也算越界"都钉过）；
    // 这里先把落不到画面里的剔掉：宁可"一个可用点位都没有"直接放弃，也不要在真点击时才发现点在标题栏上。
    let mut points: Vec<JVal> = Vec::new();
    for c in jv_list(&jv_get(&v, "Clicks")) {
        let nx = jv_get(c, "X");
        let ny = jv_get(c, "Y");
        // 转发壳的壳内早退（vision-rescue.ps1:149）：两个坐标有一个没给就不必外调。
        if matches!(&nx, JVal::Null) || matches!(&ny, JVal::Null) {
            continue;
        }
        let p = h.verb(
            "vision-click-point",
            &jv_o(vec![
                ("nx", nx),
                ("ny", ny),
                ("imgW", jv_get(&before, "ImgW")),
                ("imgH", jv_get(&before, "ImgH")),
                ("clientLeft", jv_get(&before, "ClientLeft")),
                ("clientTop", jv_get(&before, "ClientTop")),
                ("clientW", jv_get(&before, "ClientW")),
                ("clientH", jv_get(&before, "ClientH")),
                ("max", JVal::I(1000)),
                ("why", jv_get(c, "Why")),
            ]),
        )?;
        if jv_truthy(&jv_get(&p, "Found")) {
            points.push(p);
        }
    }
    if points.is_empty() {
        // 那句坐标吃的是 verdict 对象上的 X/Y（= 第一个候选点位），不是换算后的客户区坐标；没点位时是空串。
        return Ok(rescue_skip(format!(
            "模型说挡路的是{}，但它给的坐标({},{})落不到画面里 → 不点",
            what,
            jv_str(&jv_get(&v, "X")),
            jv_str(&jv_get(&v, "Y"))
        )));
    }

    // 按把握从高到低逐个试，谁点完画面真变了就算救活。三次机会都来自同一次问价：
    // 不额外花钱、不额外等模型、也不会"换个答案把上一个还回去"。
    let mut tries: Vec<String> = Vec::new();
    for pt in points.iter() {
        let cx = jv_str(&jv_get(pt, "ClientX"));
        let cy = jv_str(&jv_get(pt, "ClientY"));
        // 预告句：真身打在点击**之前**（vision-rescue.ps1:373），而 verb 回的那个 LogLine 是在点击之后算的
        // ⇒ 想保住"W| 排在 V| 前"的投影次序，这句只能在核里本地拼（与 Live.cs 那份字面逐字相同，
        //   H2 的逐字符等值就是这句双源的事实守卫）。B-2b 的第四处偏差，登记 plan §11.35。
        h.log(
            "WARN",
            &format!(
                "视觉兜底：{} → 点{} {},{}({})",
                what,
                name,
                cx,
                cy,
                jv_str(&jv_get(pt, "Why"))
            ),
        );
        let mut click_body = vec![
            ("clientX", jv_get(pt, "ClientX")),
            ("clientY", jv_get(pt, "ClientY")),
            ("name", jv_s(&name)),
            ("what", jv_get(&v, "What")),
            ("why", jv_get(pt, "Why")),
        ];
        if is_mumu {
            click_body.push(("adb", jv_s(&env.adb)));
            click_body.push(("serial", jv_s(&env.serial)));
        }
        let click = h.verb(click_verb, &jv_o(click_body))?;
        let mode = jv_str(&jv_get(&click, "Mode"));
        h.sleep(6);
        let after_path = gd_join_path(
            &dir,
            &[&format!("{}-{}-{}-after{}.png", kind, tag, suffix, tries.len() + 1)],
        );
        let mut after_body = vec![("path", jv_s(&after_path)), ("name", jv_s(&name))];
        if is_mumu {
            after_body.push(("adb", jv_s(&env.adb)));
            after_body.push(("serial", jv_s(&env.serial)));
        }
        let after = h.verb(shot_verb, &jv_o(after_body))?;
        if !jv_truthy(&jv_get(&after, "Ok")) {
            trim_shot_dir(&dir, 24);
            // 这一句用的是 after.Why（不是 Skip）⇒ 补拍失败只有一层前缀，与首拍那两处不成对称，原样照抄。
            return Ok(rescue_skip(format!(
                "点完补拍失败：{}({})，无法确认是否生效",
                jv_str(&jv_get(&after, "Why")),
                mode
            )));
        }
        let chg = h.verb(
            "shot-changed",
            &jv_o(vec![
                ("beforeSig", jv_get(&before, "Sig")),
                ("afterSig", jv_get(&after, "Sig")),
                ("minCells", JVal::I(2)),
                ("minDelta", JVal::I(3)),
            ]),
        )?;
        let chg_detail = jv_str(&jv_get(&chg, "Detail"));
        tries.push(format!("点位 {},{}({})→{}", cx, cy, mode, chg_detail));
        let last = tries[tries.len() - 1].clone();
        if !jv_truthy(&jv_get(&chg, "Known")) {
            trim_shot_dir(&dir, 24);
            return Ok(VisionRc {
                tried: true,
                done: false,
                detail: format!(
                    "按模型说的把{}点了一下({})，但{} → 不算救活",
                    what, last, chg_detail
                ),
            });
        }
        if jv_truthy(&jv_get(&chg, "Changed")) {
            trim_shot_dir(&dir, 24);
            let head = if tries.len() > 1 {
                format!("试了 {} 个点位，", tries.len())
            } else {
                String::new()
            };
            return Ok(VisionRc {
                tried: true,
                done: true,
                detail: format!("已按模型判断关掉{}：{}{}", what, head, last),
            });
        }
    }
    trim_shot_dir(&dir, 24);
    Ok(VisionRc {
        tried: true,
        done: false,
        detail: format!(
            "按模型说的把{}点了 {} 个候选点位({})，画面都没动 → 这一路走不通",
            what,
            tries.len(),
            tries.join("；")
        ),
    })
}

/// grab 回执（真身 maa.ps1:504-509，只有 stuck 档有这一支）。
/// 抢前台抛异常**不上抛**：真身那个 try/catch 把它塌成一条 `{kind:'grab',exception:true,message}` 回执，
/// 下一圈由 C# 从回执的 message 拼成 grabNote（RunLoops.cs:182「抢前台抛异常：…」）——
/// 于是"抢前台失败"只会改这一轮的措辞，不会弄挂整轮。verb 空参即可：进程名默认由 C# 从 settings 读，
/// 与真身 `Invoke-GdArkForegroundGrab -S $S` 同一事实源。
fn do_grab<H: MaaHost>(h: &mut H, effx: &mut Vec<JVal>) {
    match h.verb("ark-grab-foreground", &jv_o(Vec::new())) {
        Ok(d) => effx.push(jv_o(vec![
            ("kind", jv_s("grab")),
            ("detail", jv_s(&jv_str(&jv_get(&d, "Detail")))),
        ])),
        Err(msg) => effx.push(jv_o(vec![
            ("kind", jv_s("grab")),
            ("exception", JVal::B(true)),
            ("message", jv_s(&msg)),
        ])),
    }
}

/// 三档执行块里那段 rescue 调用（真身在 maa.ps1:511-521 / 546-555 / 578-586 抄了三遍，语义同一份；
/// m9a 那一路的真身在 m9a.ps1:459-466，算术同一份）。
/// `ack_kind` = 回执上那条 kind 的字面：**ark 侧叫 `rescue`、m9a 侧叫 `ask-screen`**（两套词各归各处，
/// 由各自的 C# 按 kind 分派 ⇒ 不许"顺手统一成一个"，plan §11.39 ⑤-3 那条同款纪律）。
/// 预算的还原/再扣照真身原样：`$rescueLeft = [int]$st.rescueLeft + 1` **无条件先做**，
/// 于是"树上没有兜底函数"那一支会把预算加回去一格 —— 原样怪癖，不顺手修（§11.34 ⑦）。
/// quiet 档真身连 -Context 都不给（默认空串）；这里统一读 effect 的 Context，因为 C# 给 stall 那档
/// 挂的 Context 恒是 ""（RunLoops.cs:317），两条路在输入面上就是同一个值。
fn do_rescue<H: MaaHost>(
    h: &mut H,
    env: &RescueEnv,
    ack_kind: &str,
    e: &JVal,
    state: &JVal,
    effx: &mut Vec<JVal>,
    rescue_left: &mut i64,
) {
    let mut rl = jv_int(&jv_get(state, "rescueLeft"), 0) + 1;
    if rl > 0 && h.rescue_available() {
        rl -= 1;
        let tag = jv_str(&jv_get(e, "Tag"));
        let context = jv_str(&jv_get(e, "Context"));
        match vision_rescue(h, env, &tag, &context) {
            Ok(rc) => effx.push(jv_o(vec![
                ("kind", jv_s(ack_kind)),
                ("tried", JVal::B(rc.tried)),
                ("done", JVal::B(rc.done)),
                ("detail", jv_s(&rc.detail)),
            ])),
            Err(msg) => effx.push(jv_o(vec![
                ("kind", jv_s(ack_kind)),
                ("exception", JVal::B(true)),
                ("message", jv_s(&msg)),
            ])),
        }
    }
    *rescue_left = rl;
}

/// record 回执（真身 maa.ps1:522-528 / 557-563 / 588-594）。
/// 那句 `$rescues += $rc.Detail` 的影子账本**不搬**：它只喂下一 tick 开头的 $detail 镜像，
/// 而 $detail 自批次 9 起没有任何活口吃它（结论话术的事实源是 C# 的 rescueNotes）——
/// 搬过来就是死代码，与上面 maa_run 头部那条"230-231/410-417 不搬"同一条理由。
/// 记一笔账失败不能弄挂日常：真身对盘的两处 try/catch 都在本函数里，唯独 verb 那句不在
/// ⇒ verb 错一路往上抛（= 整轮失败），照抄。
#[allow(clippy::too_many_arguments)]
fn do_record<H: MaaHost>(
    h: &mut H,
    env: &RescueEnv,
    e: &JVal,
    effx: &mut Vec<JVal>,
) -> Result<(), String> {
    let tried = jv_truthy(&jv_get(e, "Tried"));
    let done = jv_truthy(&jv_get(e, "Done"));
    add_rescue_record(
        h,
        env,
        &jv_str(&jv_get(e, "Tag")),
        &jv_str(&jv_get(e, "Detail")),
        tried,
        done,
    )?;
    effx.push(jv_o(vec![("kind", jv_s("record"))]));
    Ok(())
}

/// `Add-GdRescueRecord`(vision-rescue.ps1:413-466) 的复刻：读-改-写 `state\rescue.json`，
/// 七键一条、Keep 60 滑窗、UTF-8 无 BOM 的 ConvertTo-Json 形状（复用 B-1a 那枚 ps_to_json_pretty）。
/// 两处"写坏了别弄挂日常"的 WARN 整句照抄；时间戳走 GetLocalTime 自由函数（不经 MaaHost，
/// 因此剧本世界里它是真钟而不是虚拟钟 —— 门④比这本账按语义比，不比文件名/时间戳字面）。
fn add_rescue_record<H: MaaHost>(
    h: &mut H,
    env: &RescueEnv,
    tag: &str,
    detail: &str,
    tried: bool,
    done: bool,
) -> Result<(), String> {
    let path = rescue_ledger_path(env);
    let mut events: Vec<JVal> = Vec::new();
    // "第一次救火"恰恰就是文件不存在那次，所以入口先用 Test-Path 守一次（真身那句注释的理由仍在，
    // 而且它还被 t16 钉着）。读不动 → WARN 按空账继续。
    if std::fs::metadata(&path).is_ok() {
        match std::fs::read_to_string(&path) {
            Ok(text) => match jv_parse(&text) {
                Ok(old) => {
                    for ev in jv_list(&jv_get(&old, "events")) {
                        events.push(ev.clone());
                    }
                }
                Err(e) => h.log(
                    "WARN",
                    &format!("救火账本旧内容读不动(按空账继续)：{}", e),
                ),
            },
            Err(e) => h.log(
                "WARN",
                &format!("救火账本旧内容读不动(按空账继续)：{}", e),
            ),
        }
    }
    let e = h.verb(
        "rescue-entry",
        &jv_o(vec![
            ("time", jv_s(&gd_ledger_time())),
            ("game", jv_s(&env.game)),
            ("target", jv_s(&env.target)),
            // 真身这里传 [string]$Result.Target —— 靶子自报名；effect 上没这一格，恒空串（verb 的回退支用不上）。
            ("fallbackTarget", jv_s("")),
            ("tag", jv_s(tag)),
            ("tried", JVal::B(tried)),
            ("done", JVal::B(done)),
            ("detail", jv_s(detail)),
        ]),
    )?;
    events = ledger_push(
        events,
        jv_o(vec![
            ("time", jv_get(&e, "Time")),
            ("game", jv_get(&e, "Game")),
            ("target", jv_get(&e, "Target")),
            ("tag", jv_get(&e, "Tag")),
            ("tried", jv_get(&e, "Tried")),
            ("done", jv_get(&e, "Done")),
            ("detail", jv_get(&e, "Detail")),
        ]),
        60,
    );
    let ledger = jv_o(vec![
        ("version", JVal::I(1)),
        ("updated", jv_s(&gd_ledger_time())),
        ("events", JVal::A(events)),
    ]);
    if let Err(msg) = write_at(&path, ps_to_json_pretty(&ledger).as_bytes(), false) {
        h.log("WARN", &format!("救火账本没写进去：{}", msg));
    }
    Ok(())
}

/// `Invoke-MaaRun` 的循环骨架（lib\maa.ps1:220-620）逐行进核。
fn maa_run<H: MaaHost>(h: &mut H, sc: &MaaScen) -> Result<MaaRet, String> {
    let p = MaaPaths::of(sc);
    // 兜底链的环境三格：整轮建一次（B-2d② 抽出的那层，真身在 maa.ps1 里是三处调用点各写一遍同样的参数）。
    let env = RescueEnv::of_maa(sc);
    // 228：DryRun 只打一行 STEP 就回（Detail 是空串、Errors 这个键根本不存在）。
    // 话术里那串 `@{Dir=…}.Exe` 不是笔误复刻、是**真身就这么印**：maa.ps1 写的是 `$P.Exe`，
    // PS 按 `$P` 整体插值 + 字面 `.Exe` 处理 ⇒ 打印的是对象渲染而不是 exe 路径。
    // 搬家批不改文案(改了就与 old 侧钉的 251bd3c 分岔)，这处显示缺陷已登记待修(见 docs 的 B-2a 实录)。
    if sc.dry_run {
        h.log(
            "STEP",
            &format!(
                "[dry-run] 将启动 {}.Exe(自动开始队列：{})",
                p.ps_to_string(),
                sc.expected.join(", ")
            ),
        );
        return Ok(MaaRet {
            status: "dryrun".to_string(),
            detail: String::new(),
            duration: None,
            errors: None,
        });
    }

    // 230-231 的 `$expectedCount`：真身把它建在**循环之外**（每条期望链恒记 1、大小写不敏感去重），
    // 所以它的条数在一轮里是不变量 —— 这边照样只建一次。410-412 的 $doneCount/$allDone 每 tick 现算
    // （读数本 tick 冻结），413-417 的 $chainText/$detail 镜像段仍不搬：它只喂下一 tick 开头的
    // $detail 影子，而那份影子自批次 7 起没有任何活口吃它（结论话术的事实源在 C#）。
    let expected = expected_dedup(&sc.expected);
    let mut offset: u64 = h.file_len(&p.gui_log).unwrap_or(0);
    let mut asst_offset: u64 = 0;
    let mut asst_hint = String::new();
    let mut rescue_left = sc.vision_max;
    if rescue_left <= 0 {
        rescue_left = 2;
    }
    if !p.asst_log.is_empty() {
        if let Some(l) = h.file_len(&p.asst_log) {
            asst_offset = l;
        }
    }
    let started = h.clock();
    h.log("STEP", &format!("启动 MAA：{}", p.exe));
    // ark 一路从不带参数（真身 maa.ps1 那句裸 Start-Process）⇒ 传空切片，投影保持 `R|exe|dir` 两截。
    h.start_process(&p.exe, &p.dir, &[])?;
    if !wait_condition(h, 90, 3, "MAA 进程启动", |hh| hh.proc_running(&p.name)) {
        return Ok(MaaRet {
            status: "error".to_string(),
            detail: "MAA 未能启动(进程未出现)".to_string(),
            duration: None,
            errors: None,
        });
    }

    let mut last_growth = h.clock();
    let mut completed: Vec<(String, i64)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut chan_clicks: i64 = 0;
    let mut stuck_task = String::new();
    let mut stuck_exec: i64 = 0;
    let mut chan_grab_at: Option<f64> = None;
    let mut benign_seen: Vec<String> = Vec::new();
    let mut markers: i64 = 0;
    let mut log_chars: i64 = 0;
    let mut finished = false;
    let mut drained = false;
    let mut stall_warn = String::new();
    let start_timeout = sc.start_min * 60;
    let timeout = sc.timeout_min * 60;
    let stall = sc.stall_min * 60;
    let idle_confirm = sc.idle_sec;
    let ark_name = file_stem(&sc.ark_exe);
    let mut seen_game = h.proc_running(&ark_name);

    loop {
        h.sleep(10);
        if !seen_game && h.proc_running(&ark_name) {
            seen_game = true;
            h.log("INFO", &format!("端游客户端已被拉起来({})", ark_name));
        }
        let (ttext, toff) = h.file_tail(&p.gui_log, offset);
        offset = toff;
        if !ttext.is_empty() {
            last_growth = h.clock();
            // 分类规则在 C#(parse-maa-gui-lines)，六档回灌也在 C#(apply-maa-gui-events)；
            // 这里只做真身那三件事：喂 → 按档重印日志 → 拿回执重建本地量。
            let benign_jv = JVal::A(sc.benign.iter().map(|s| jv_s(s)).collect());
            let g = h.verb(
                "parse-maa-gui-lines",
                &jv_o(vec![("text", jv_s(&ttext)), ("benign", benign_jv)]),
            )?;
            markers += jv_int(&jv_get(&g, "Markers"), 0);
            log_chars += jv_int(&jv_get(&g, "LogChars"), 0);
            let events = jv_get(&g, "Events");
            let ge = h.verb(
                "apply-maa-gui-events",
                &jv_o(vec![
                    ("events", events),
                    (
                        "state",
                        jv_o(vec![
                            ("completed", cmap_to_jv(&completed)),
                            ("errors", JVal::A(errors.iter().map(|s| jv_s(s)).collect())),
                            (
                                "benignPatterns",
                                JVal::A(benign_seen.iter().map(|s| jv_s(s)).collect()),
                            ),
                            ("finished", JVal::B(finished)),
                            ("drained", JVal::B(drained)),
                            ("stallWarn", jv_s(&stall_warn)),
                        ]),
                    ),
                ]),
            )?;
            replay_logs(h, &jv_get(&ge, "Logs"));
            let st = jv_get(&ge, "State");
            completed.clear();
            if let JVal::O(cm) = jv_get(&st, "completed") {
                for (k, v) in cm.iter() {
                    cmap_set(&mut completed, k, jv_int(v, 0));
                }
            }
            errors = vstr_list(&jv_get(&st, "errors"));
            benign_seen = Vec::new();
            for bp in jv_list(&jv_get(&st, "benignPatterns")) {
                let k = jv_str(bp);
                if !benign_seen.iter().any(|e| e.eq_ignore_ascii_case(&k)) {
                    benign_seen.push(k);
                }
            }
            finished = jv_truthy(&jv_get(&st, "finished"));
            drained = jv_truthy(&jv_get(&st, "drained"));
            stall_warn = jv_str(&jv_get(&st, "stallWarn"));
        }

        // asst.log 只取线索、不参与"日志有没有在动"；整段 try/catch，读不动就 asstOffset=0。
        if !p.asst_log.is_empty() {
            let r: Result<(), String> = (|| -> Result<(), String> {
                let len = match h.file_len(&p.asst_log) {
                    None => return Ok(()),
                    Some(l) => l,
                };
                // 4MB 闸用有符号差：被原地清空/轮转后 len-offset 是负数，落进"跳末尾"那一支。
                if (len as i64 - asst_offset as i64) > 4_194_304 {
                    asst_offset = len;
                } else if len > asst_offset {
                    let (at, ao) = h.file_tail(&p.asst_log, asst_offset);
                    asst_offset = ao;
                    if !at.is_empty() {
                        // 两枚转发壳的"空文本壳内早退"（别为空串起进程）照 maa.ps1:117-138 复刻。
                        let white = is_null_or_white(&at);
                        let (task, exec, clicks) = if white {
                            (String::new(), 0i64, 0i64)
                        } else {
                            let sv = h.verb("parse-asst-state", &jv_o(vec![("text", jv_s(&at))]))?;
                            (
                                jv_str(&jv_get(&sv, "Task")),
                                jv_int(&jv_get(&sv, "Exec"), 0),
                                jv_int(&jv_get(&sv, "Clicks"), 0),
                            )
                        };
                        chan_clicks += clicks;
                        if !task.is_empty() {
                            stuck_task = task;
                            stuck_exec = exec;
                        }
                        let hint = if white {
                            String::new()
                        } else {
                            let hv = h.verb("parse-asst-hint", &jv_o(vec![("text", jv_s(&at))]))?;
                            jv_str(&jv_get(&hv, "Hint"))
                        };
                        if !hint.is_empty() {
                            asst_hint = hint;
                        }
                    }
                } else {
                    asst_offset = len;
                }
                Ok(())
            })();
            if r.is_err() {
                asst_offset = 0;
            }
        }

        let elapsed = h.clock() - started;
        let quiet = h.clock() - last_growth;
        // 410-412 的现场镜像：$doneCount/$allDone 每 tick 现算一次，整个迷你循环里读的是同一个值
        //（真身把它们算在迷你循环之外 —— "读数本 tick 冻结"就是这条时序）。
        let all_done_flag = all_done(&expected, &completed);
        if finished {
            break;
        }

        // ===== 迷你循环：同一 tick 内「喂 verb → 执行 effect → 带回执重入」=====
        // $st/$effx 两个跨圈中继照真身(maa.ps1:431-433)留在**外层 tick 之内**：真身每 tick 从 $null 起步，
        // state 不跨 tick —— 别把它挪到外层循环外面，挪了就改了语义。
        let mut state: JVal = JVal::Null;
        let mut effx: Vec<JVal> = Vec::new(); // 回执累加器(maa.ps1:432 的 $effx = @())
        let mut tail_now = false;
        loop {
            // 真身建 Body 的求值序：now(含 grabAgo 取钟) → live(探活 → 两枚 Get-Command) → ...
            let grab_ago = chan_grab_at.map(|t| h.clock() - t);
            // 三次现场采集先落本地：借用检查只认这个 —— 真身里它们是 Body 里的三个子表达式，
            // 但同一 tick 内没有别的东西读它们，先算后拼不改语义（探活/两枚可用性各一次）。
            let maa_running = h.proc_running(&p.name);
            let rescue_avail = h.rescue_available();
            let grab_avail = h.grab_available();
            let d = h.verb(
                "decide-maa-run-tick",
                &jv_o(vec![
                    (
                        "now",
                        jv_o(vec![
                            ("elapsedSec", JVal::F(elapsed)),
                            ("quietSec", JVal::F(quiet)),
                            ("grabAgoSec", grab_ago.map_or(JVal::Null, JVal::F)),
                        ]),
                    ),
                    (
                        "live",
                        jv_o(vec![
                            ("maaRunning", JVal::B(maa_running)),
                            ("rescueAvailable", JVal::B(rescue_avail)),
                            ("grabAvailable", JVal::B(grab_avail)),
                        ]),
                    ),
                    (
                        "flags",
                        jv_o(vec![
                            ("seenGame", JVal::B(seen_game)),
                            ("markers", JVal::I(markers)),
                            ("logChars", JVal::I(log_chars)),
                            ("finished", JVal::B(finished)),
                            ("drained", JVal::B(drained)),
                            ("stallWarn", jv_s(&stall_warn)),
                            ("completed", cmap_to_jv(&completed)),
                            ("errors", JVal::A(errors.iter().map(|s| jv_s(s)).collect())),
                            ("expectedChains", JVal::A(sc.expected.iter().map(|s| jv_s(s)).collect())),
                            ("asstHint", jv_s(&asst_hint)),
                            ("stuckTask", jv_s(&stuck_task)),
                            ("stuckExec", JVal::I(stuck_exec)),
                            ("chanClicks", JVal::I(chan_clicks)),
                        ]),
                    ),
                    (
                        "cfg",
                        jv_o(vec![
                            ("startTimeoutSec", JVal::I(start_timeout)),
                            ("timeoutSec", JVal::I(timeout)),
                            ("stallSec", JVal::I(stall)),
                            ("idleConfirmSec", JVal::I(idle_confirm)),
                            ("startTimeoutMin", JVal::I(sc.start_min)),
                            ("timeoutMin", JVal::I(sc.timeout_min)),
                            ("abortOnMaaStallWarning", JVal::B(sc.abort_stall)),
                            ("guiLogPath", jv_s(&p.gui_log)),
                            ("rescueMaxRaw", JVal::I(rescue_left)),
                        ]),
                    ),
                    ("state", state.clone()),
                    ("effectsResolved", JVal::A(effx.clone())),
                ]),
            )?;
            state = jv_get(&d, "State");
            // Directives：计时重置的下令时机在 C#（连"树上没有抢前台的函数"那一档也要打点）。
            let dirs = jv_get(&d, "Directives");
            if jv_truthy(&jv_get(&dirs, "GrabNow")) {
                chan_grab_at = Some(h.clock());
            }
            if jv_truthy(&jv_get(&dirs, "ResetLastGrowth")) {
                last_growth = h.clock();
            }
            if jv_truthy(&jv_get(&dirs, "ClearStallWarn")) {
                stall_warn = String::new();
            }
            replay_logs(h, &jv_get(&d, "Logs"));

            let effs = jv_get(&d, "Effects");
            let ef = jv_list(&effs);
            if ef.is_empty() {
                if jv_str(&jv_get(&d, "Action")) == "return" {
                    // 真身是五支 switch 各写一遍同名键（unknown-format/not-started/timeout/stalled/
                    // 其余落 error）；档位字面串照 maa.ps1:479-485。
                    let st = jv_str(&jv_get(&d, "Status"));
                    let status = match st.as_str() {
                        "unknown-format" | "not-started" | "timeout" | "stalled" => st,
                        _ => "error".to_string(),
                    };
                    return Ok(MaaRet {
                        status,
                        detail: jv_str(&jv_get(&d, "Detail")),
                        duration: None,
                        errors: Some(vstr_list(&jv_get(&d, "Errors"))),
                    });
                }
                tail_now = jv_str(&jv_get(&d, "Action")) == "tail";
                break;
            }
            let e = &ef[0];
            let kind = jv_str(&jv_get(e, "Kind"));
            let tag = jv_str(&jv_get(e, "Tag"));
            let mut did = false;

            // ===== stuck 档执行块(maa.ps1:495-530) =====
            // 原判据(exec≥60 闸 + 链路 0 完成闸、抢前台后 60 秒仍在原地整句早退、"抢完前台当场问一次画面"
            // 的早触发)全部在 RunLoops.cs 的 Ladder 第 4 档。下面这层 if 是真身那句"现场复核"守卫
            // (读数本 tick 冻结 ⇒ C# 挂了这档的 effect 它就成立)，照抄不并删：test-ark 按这段里的
            // 调用点记"三笔账"、按 Grab→Rescue→Record 的落地顺序钉锚，三档的先后次序就是判据。
            if kind_eq(&kind, "grab") || tag_like(&tag, "stuck") {
                if stuck_exec >= 60 && completed.is_empty() && !finished {
                    if kind_eq(&kind, "grab") {
                        do_grab(h, &mut effx);
                        did = true;
                    }
                    if kind_eq(&kind, "rescue") {
                        do_rescue(h, &env, "rescue", e, &state, &mut effx, &mut rescue_left);
                        did = true;
                    }
                    if kind_eq(&kind, "record") {
                        do_record(h, &env, e, &mut effx)?;
                        did = true;
                    }
                }
            }
            // ===== drain 档执行块(maa.ps1:531-565) =====
            // 2026-09-25 07:53 那轮的教训：07:56 就宣布"跑完但有错误"、6 条链路 0 完成，老代码却傻等到
            // "静默 22 分钟"才反应。这一路排在静默判定之前(顺序错了等于没改)，只做两件被证据撑住的事：
            // 当场兜底一次 + 立刻早退(早退那句已在 C# 的 DrainSentence)。这一段不写任何进程起停代码 ——
            // 关端游重开 + 重起 MAA 是外层那一次重试的事。守卫 `$drained -and -not $allDone` 照抄。
            if tag_like(&tag, "drain") {
                if drained && !all_done_flag {
                    if kind_eq(&kind, "rescue") {
                        do_rescue(h, &env, "rescue", e, &state, &mut effx, &mut rescue_left);
                        did = true;
                    }
                    if kind_eq(&kind, "record") {
                        do_record(h, &env, e, &mut effx)?;
                        did = true;
                    }
                }
            }
            // ===== quiet/stall 档执行块(maa.ps1:567-596) =====
            // 旧代码先给视觉兜底一次机会再判 stalled(2026-09-24 中秋弹窗：人眼一看就该把挡路窗口关掉，
            // 模板匹配自己永远走不出来)。兜成 → 停滞计时从头算、也不让 stallWarn 下一轮再把它提前判死，
            // 旧代码那一句原地 continue 由"带回执重入 + Directives(ResetLastGrowth/ClearStallWarn)"等价替换；
            // 兜败或没有兜底 → 同一 tick 直接 stalled。这一档的调用 tag 在旧代码里就叫 stall(与档位名
            // quiet 不同词)，照抄。`-not $did` 打头 ⇒ 前三档谁先接手后面就不再看，顺序即判据。
            if !did && tag_like(&tag, "stall") {
                if quiet > stall as f64 {
                    if kind_eq(&kind, "rescue") {
                        do_rescue(h, &env, "rescue", e, &state, &mut effx, &mut rescue_left);
                        did = true;
                    }
                    if kind_eq(&kind, "record") {
                        do_record(h, &env, e, &mut effx)?;
                        did = true;
                    }
                }
            }
            if !did {
                // 判据挂了 effect 可哪个执行块都没接手(读数本 tick 冻结，理论上到不了这里)：
                // 宁可作废这一 tick，也不能拿着没回执的挂起点空转 —— 下一 tick 的新读数会重新定档。
                // 整句照抄 maa.ps1:600（真身是 `$kind` 与 `$([string]$e.Tag)`，这边就是那两个本地串）。
                h.log(
                    "WARN",
                    &format!(
                        "MAA tick：effect(kind={}，tag={})没有落到执行块，本 tick 作废(下一轮按新读数重判)",
                        kind, tag
                    ),
                );
                break;
            }
        }
        if tail_now {
            break;
        }
    }

    // 挂钟收尾（maa.ps1:612-616）：$dur 与排序链表是现场量，算好喂 finish-maa-run。
    let dur = h.clock() - started;
    let chain_text = cmap_keys(&completed).join(", ");
    let fin = h.verb(
        "finish-maa-run",
        &jv_o(vec![("durSec", JVal::F(dur)), ("chainText", jv_s(&chain_text))]),
    )?;
    for l in jv_list(&jv_get(&fin, "Logs")) {
        h.log(&jv_str(&jv_get(l, "Level")), &jv_str(&jv_get(l, "Text")));
    }
    Ok(MaaRet {
        status: jv_str(&jv_get(&fin, "Status")),
        detail: jv_str(&jv_get(&fin, "Detail")),
        duration: Some(jv_to_d(&jv_get(&fin, "Duration"))),
        errors: Some(errors.clone()),
    })
}

// ---- ScriptHost：剧本世界 ----

/// 剧本宿主：虚拟钟（只在睡眠时走）＋ 真文件（与 old 侧桩浴读写的是同一批临时文件）
/// ＋ 真 verb 外调（判据不在射程里的前提）。每一步都往 `proj` 里落一条事件，投影即门的面。
struct ScriptHost {
    panel_cs: String,
    paths: MaaPaths,
    procs: Vec<(String, bool)>,
    events: Vec<MaaEv>,
    next_ev: usize,
    clock: f64,
    proj: Vec<String>,
    // B-2b：两枚可用性由剧本喂（真身是 `Get-Command` 问出来的，树上有没有那枚函数属现场事实，
    // 门里就得由夹具说）；`live` 是七枚现场 verb 的夹具队列，按名 FIFO。
    rescue_avail: bool,
    grab_avail: bool,
    live: Vec<(String, JVal)>,
    // B-2d②：本地日期读数按剧本喂的表查（`(at, stamp)` 升序，`at` 之前一律用首档）。
    // ark 一路不填这张表也不调 `day_stamp` ⇒ 四格全空、投影零条 `D|`，27 档基线一字不动。
    days: Vec<(f64, String)>,
    cur_day: String,
    /// 已经落过投影的那枚日期值：`D|` 只在**值变化时**落一条（日期是"每轮重算的同一个读数"，
    /// 逐轮重复落只会把门变成"数行数"，换不了文件的读数没有信息量）。空串当"还没落过"，
    /// 因为真日期恒为 8 位、不可能等于空串。
    day_shown: String,
    /// m9a 那一路的日志目录（`log-yyyyMMdd.log` 的父目录）；ark 一路为空。
    m9a_log_dir: String,
    /// B-2d③：每次外调的**入参**按先后各留一份，只给自检钉「body 按 kind 分派」那一格用
    /// （mumu 那三枚要带 `adb`/`serial`、ark 那三枚一个键都不许多）。
    /// 它**不进 `proj`** —— 等值门的比对面一字未动，ark 侧 27 档 1218 步基线因此不可能被这一格带跑
    /// （§11.42 ①-10 那句"投影安全由构造保证"的同族）。
    bodies: Vec<(String, JVal)>,
}

/// 按虚拟钟查日期表：取 `at <= clock` 的最后一档，钟还没走到首档时用首档（表空返回空串，
/// 由调用侧的形状门拒跑 —— 见 `m9arun_parse` 那条"剧本没给 dates"）。
fn day_lookup(days: &[(f64, String)], clock: f64) -> String {
    let mut out = String::new();
    for (at, stamp) in days {
        if *at > clock {
            break;
        }
        out = stamp.clone();
    }
    if out.is_empty() {
        if let Some((_, stamp)) = days.first() {
            out = stamp.clone();
        }
    }
    out
}

impl ScriptHost {
    fn new(panel_cs: &str, sc: &MaaScen) -> ScriptHost {
        ScriptHost {
            panel_cs: panel_cs.to_string(),
            paths: MaaPaths::of(sc),
            procs: sc.procs.clone(),
            events: sc.events.clone(),
            next_ev: 0,
            clock: 0.0,
            proj: Vec::new(),
            rescue_avail: sc.rescue_available,
            grab_avail: sc.grab_available,
            live: sc.live.clone(),
            days: Vec::new(),
            cur_day: String::new(),
            day_shown: String::new(),
            m9a_log_dir: String::new(),
            bodies: Vec::new(),
        }
    }

    /// m9a 一路的剧本宿主：ark 那两枚固定路径（gui/asst）在这里**没有落点**，
    /// 日志名每轮由 `days` 表查出来 ⇒ `m9a_log_dir` 填上、`paths` 留空。
    /// `file_of` 因此只对 `"m9alog"` 给路径，而那枚 target 之外的事件由 `m9arun_parse` 当场拒跑
    /// （不是静默丢弃：靶子名写错会让一整档跑成"什么都没发生"，那种假绿比红更糟）。
    fn new_m9a(panel_cs: &str, sc: &M9aScen) -> ScriptHost {
        ScriptHost {
            panel_cs: panel_cs.to_string(),
            paths: MaaPaths::of(&MaaScen::default()),
            procs: sc.procs.clone(),
            events: sc.events.clone(),
            next_ev: 0,
            clock: 0.0,
            proj: Vec::new(),
            rescue_avail: sc.rescue_available,
            grab_avail: sc.grab_available,
            live: sc.live.clone(),
            days: sc.days.clone(),
            cur_day: day_lookup(&sc.days, 0.0),
            day_shown: String::new(),
            m9a_log_dir: M9aPaths::of(sc).log_dir,
            bodies: Vec::new(),
        }
    }

    /// 自检用：同名 verb 的**最后**一枚入参（兜底链里首拍与补拍同名，判据要看最后一次的分派）。
    /// 它只读 `bodies`，不进 `proj` ⇒ 等值门的比对面一字未动。
    fn body_of(&self, name: &str) -> Option<&JVal> {
        self.bodies
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, b)| b)
    }

    /// 睡眠推钟并把 `at <= 新钟` 的事件按剧本先后落完（同一 at 的先后 = 数组先后，两侧一致）。
    /// B-2d② 起顺序是「**先置钟 → 刷日期 → 落事件**」：真身每轮是 `Start-Sleep` 之后才现算
    /// `log-<今天>.log` 这个路径，所以"睡醒那一刻的日期"才是事件内容该落进去的文件；
    /// 原先"落完事件再置钟"的写法在 ark 一路等价（它不读日期、两枚路径是常量），故 27 档基线不动。
    fn advance(&mut self, seconds: u32) {
        let until = self.clock + seconds as f64;
        self.clock = until;
        self.cur_day = day_lookup(&self.days, until);
        while self.next_ev < self.events.len() && self.events[self.next_ev].at <= until {
            let ev = self.events[self.next_ev].clone();
            self.next_ev += 1;
            self.apply_ev(&ev);
        }
    }

    fn file_of(&self, target: &str) -> Option<String> {
        match target {
            "gui" => Some(self.paths.gui_log.clone()),
            "asst" => Some(self.paths.asst_log.clone()),
            // m9a 一路的日志名吃本地日历日，日期由夹具表给 ⇒ 文件名每轮可能换一枚。
            // 目录为空（ark 一路）时给 None：那一档事件在本剧本里没有落点，照旧静默丢弃。
            "m9alog" => {
                if self.m9a_log_dir.is_empty() || self.cur_day.is_empty() {
                    return None;
                }
                Some(m9a_daily_path(&self.m9a_log_dir, &self.cur_day))
            }
            _ => None,
        }
    }

    fn apply_ev(&mut self, ev: &MaaEv) {
        match ev.kind {
            "up" | "down" => {
                let on = ev.kind == "up";
                let mut hit = false;
                for (n, v) in self.procs.iter_mut() {
                    if n.eq_ignore_ascii_case(&ev.target) {
                        *v = on;
                        hit = true;
                    }
                }
                if !hit {
                    self.procs.push((ev.target.clone(), on));
                }
            }
            "append" | "grow" | "truncate" => {
                let path = match self.file_of(&ev.target) {
                    Some(p) => p,
                    None => return,
                };
                let bytes: Vec<u8> = if ev.kind == "grow" {
                    vec![b'A'; ev.bytes.max(0) as usize]
                } else if ev.kind == "append" {
                    ev.text.as_bytes().to_vec()
                } else {
                    Vec::new()
                };
                let _ = write_at(&path, &bytes, ev.kind == "append");
            }
            _ => {}
        }
    }
}

/// 落盘一件：append=true 追加、否则整写（等价 old 侧的 AppendAllText / WriteAllText，UTF-8 无 BOM）。
fn write_at(path: &str, bytes: &[u8], append: bool) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if append {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("写不进 {}：{}", path, e))?;
        f.write_all(bytes).map_err(|e| format!("写不进 {}：{}", path, e))
    } else {
        std::fs::write(path, bytes).map_err(|e| format!("写不进 {}：{}", path, e))
    }
}

impl MaaHost for ScriptHost {
    fn log(&mut self, level: &str, text: &str) {
        self.proj.push(format!("W|{}|{}", level, text));
    }
    fn clock(&mut self) -> f64 {
        self.clock
    }
    fn sleep(&mut self, seconds: u32) {
        self.proj.push(format!("S|{}", seconds));
        self.advance(seconds);
    }
    fn file_len(&mut self, path: &str) -> Option<u64> {
        std::fs::metadata(path).ok().map(|m| m.len())
    }
    fn file_tail(&mut self, path: &str, offset: u64) -> (String, u64) {
        let (text, after) = real_file_tail(path, offset);
        self.proj.push(format!(
            "T|{}|{}|{}|{}",
            file_name(path),
            offset,
            after,
            text.chars().count()
        ));
        (text, after)
    }
    fn proc_running(&mut self, name: &str) -> bool {
        let on = self
            .procs
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
            .unwrap_or(false);
        self.proj.push(format!("P|{}|{}", name, if on { 1 } else { 0 }));
        on
    }
    fn start_process(&mut self, exe: &str, dir: &str, args: &[String]) -> Result<(), String> {
        // args 为空 ⇒ 逐字 `R|{exe}|{dir}`（ark 一路的裸启动，27 档 1218 步基线因此一字不动）；
        // 非空 ⇒ 追加一截，参数间用单元分隔符（数组在投影里的一贯记法，不为这一格另立形状）。
        if args.is_empty() {
            self.proj.push(format!("R|{}|{}", exe, dir));
        } else {
            self.proj.push(format!(
                "R|{}|{}|{}",
                exe,
                dir,
                args.join("\u{1e}")
            ));
        }
        Ok(())
    }
    fn day_stamp(&mut self) -> String {
        self.cur_day = day_lookup(&self.days, self.clock);
        if self.day_shown != self.cur_day {
            self.proj.push(format!("D|{}", self.cur_day));
            self.day_shown = self.cur_day.clone();
        }
        self.cur_day.clone()
    }
    fn verb(&mut self, name: &str, body: &JVal) -> Result<JVal, String> {
        // 夹具命中与真跑外调在投影上同形（都记一条 `V|名`）：门④要的是"这一路问了哪几枚 verb、
        // 按什么顺序"，而不是"答案是夹具给的还是 C# 给的"。判定与纯规则仍一律真跑 —— 夹具只喂
        // 七枚现场 verb（拍窗/点窗/抢前台/问模型/六闸上下文/窗口在场/桌面状态）。
        if let Some(i) = self.live.iter().position(|(n, _)| n == name) {
            self.proj.push(format!("V|{}", name));
            self.bodies.push((name.to_string(), body.clone()));
            let (_, data) = self.live.remove(i);
            return Ok(data);
        }
        self.proj.push(format!("V|{}", name));
        self.bodies.push((name.to_string(), body.clone()));
        call_panel_verb(&self.panel_cs, name, body)
    }
    fn rescue_available(&self) -> bool {
        self.rescue_avail
    }
    fn grab_available(&self) -> bool {
        self.grab_avail
    }
}

// ---- B-2d②：m9a 一路的采集面与循环骨架（lib\m9a.ps1:126-528 逐段进核）----

/// `Get-M9ADailyLog`(m9a.ps1:126-130) 的复刻：`<LogDir>\log-<yyyyMMdd>.log`，**每轮重算**（跨 00:00 就换文件）。
/// 真身那句是裸 `Join-Path` 而不是宽容的 `Join-GdPath` —— LogDir 为空时真身当场抛，
/// 所以 `m9arun_parse` 把 `paths.m9aDir` 做成拒跑闸，运行期这一格不可能为空（少这一闸就要多复刻一句 PS 绑定异常）。
fn m9a_daily_path(log_dir: &str, day: &str) -> String {
    gd_join_path(log_dir, &[&format!("log-{}.log", day)])
}

/// PS hashtable 的**键存在性**（真身两句都吃这个：`Register-M9AEvents` 的 `ContainsKey`、
/// `Resolve-M9AStatus:315` 的 `$null -ne $d.PSObject.Properties['FailedTasks']`）。
/// 不能用 `jv_get(...) == Null` 代替 —— 那会把"键在、值是 null"与"键压根不在"塌成一档，
/// 而 C# 那侧 `queueCount` 的 `qc != null` 判据恰恰区分它们（Processes.cs:488）。
fn jvo_has(v: &JVal, key: &str) -> bool {
    if let JVal::O(o) = v {
        return o.iter().any(|(k, _)| k == key);
    }
    false
}

/// PS hashtable 的原位赋值：键在则保位置换值，不在则追加。
/// 键序**不是**判据（user 记忆 pwsh-hashtable-keyorder-not-an-invariant：换进程或拷贝就可能重排），
/// 保位置只是让这份 state 在投影之外也读得顺。
fn jvo_set(st: &mut JVal, key: &str, val: JVal) {
    if let JVal::O(o) = st {
        for (k, v) in o.iter_mut() {
            if k == key {
                *v = val;
                return;
            }
        }
        o.push((key.to_string(), val));
    }
}

/// `Register-M9AEvents:187-188` 那份名单，十二枚逐字照抄（`lastLog` 刻意不在内 —— 挂钟量不进信封）。
/// 这一序 = `$state` hashtable 字面量(m9a.ps1:370-375)去掉 `lastLog` 后的余下十二格：两处同源，
/// 改一处就要改另一处（ark 侧那枚 `completed` 容器同理，见 cmap_*）。
const M9A_STATE_KEYS: [&str; 12] = [
    "tasks",
    "chainStarted",
    "chainDone",
    "connectPhase",
    "failed",
    "queueCount",
    "startRequested",
    "eventCount",
    "failedTasks",
    "resCurrent",
    "resLatest",
    "connectFail",
];

/// keyed offsets 三件（`$Offsets`/`$PreOffsets` 是 hashtable，ark 侧是两枚固定变量 —— 形状不同源）：
/// ensure 只在"这一枚路径第一次出现"时建格，起点取 PreOffsets 里那条或 0。
fn keyed_offset_ensure(v: &mut Vec<(String, u64)>, k: &str, pre: &[(String, u64)]) {
    if !v.iter().any(|(ek, _)| ek == k) {
        let start = pre.iter().find(|(ek, _)| ek == k).map(|(_, ev)| *ev).unwrap_or(0);
        v.push((k.to_string(), start));
    }
}

fn keyed_offset_get(v: &[(String, u64)], k: &str) -> u64 {
    v.iter().find(|(ek, _)| ek == k).map(|(_, ev)| *ev).unwrap_or(0)
}

fn keyed_offset_set(v: &mut Vec<(String, u64)>, k: &str, val: u64) {
    for (ek, ev) in v.iter_mut() {
        if ek == k {
            *ev = val;
            return;
        }
    }
    v.push((k.to_string(), val));
}

/// `Read-M9ALogEvents`(m9a.ps1:146-172) 的复刻：日志路径每轮现算，偏移按路径 keyed。
/// 返回 `(日志路径, 事件数组)`；事件形状是真身那两键 hashtable（`Kind`/`Text`），不是信封原物。
fn m9a_read_log<H: MaaHost>(
    h: &mut H,
    p: &M9aPaths,
    inst_id: &str,
    offsets: &mut Vec<(String, u64)>,
    pre: &[(String, u64)],
) -> Result<(String, Vec<JVal>), String> {
    let day = h.day_stamp();
    let log = m9a_daily_path(&p.log_dir, &day);
    let mut events: Vec<JVal> = Vec::new();
    // `Test-GdPathReady`(common.ps1:287-292) 先挡：不在场就**直接回空事件、并且不给 offsets 建格**。
    // 次序要紧：文件后来才出现时，起点仍按 PreOffsets（或 0）算，而不是按"这一轮读过的 0"续着走。
    if log.is_empty() || h.file_len(&log).is_none() {
        return Ok((log, events));
    }
    keyed_offset_ensure(offsets, &log, pre);
    let off = keyed_offset_get(offsets, &log);
    let (text, after) = h.file_tail(&log, off);
    keyed_offset_set(offsets, &log, after);
    // 「本轮没有新字节就干脆不起进程」是真身那句 `if ($tail.Text.Trim())`（与旧实现同形的省钱点）。
    if text.trim().is_empty() {
        return Ok((log, events));
    }
    let d = h.verb(
        "parse-m9a-lines",
        &jv_o(vec![("text", jv_s(&text)), ("instanceId", jv_s(inst_id))]),
    )?;
    for e in jv_list(&jv_get(&d, "Events")) {
        events.push(jv_o(vec![
            ("Kind", jv_s(&jv_str(&jv_get(e, "Kind")))),
            ("Text", jv_s(&jv_str(&jv_get(e, "Text")))),
        ]));
    }
    Ok((log, events))
}

/// `Register-M9AEvents`(m9a.ps1:174-198) 的复刻：0 条事件连 verb 都不问 →
/// 摘名单十二键（只摘真在的）→ 整批一次转发 → 按档重印日志 → 回执 State 全键回写 →
/// `Directives.TouchLastLog` 置挂钟（剧本世界里那枚挂钟就是虚拟钟）。
/// 三个在案 PS 语义坑（`@($null)` 占位 / `$null -eq @()` 塌缩 / `-ne` 不分大小写）全在 C# 逐字复刻，
/// 这一层只搬运，不解释。
fn m9a_register_events<H: MaaHost>(
    h: &mut H,
    state: &mut JVal,
    events: &[JVal],
    last_log: &mut f64,
) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    let mut pairs: Vec<(String, JVal)> = Vec::new();
    for k in M9A_STATE_KEYS.iter() {
        if jvo_has(state, k) {
            pairs.push(((*k).to_string(), jv_get(state, k)));
        }
    }
    let d = h.verb(
        "apply-m9a-events",
        &JVal::O(vec![
            ("events".to_string(), JVal::A(events.to_vec())),
            ("state".to_string(), JVal::O(pairs)),
        ]),
    )?;
    replay_logs(h, &jv_get(&d, "Logs"));
    if let JVal::O(props) = jv_get(&d, "State") {
        for (k, v) in props.iter() {
            jvo_set(state, k, v.clone());
        }
    }
    if jv_truthy(&jv_get(&jv_get(&d, "Directives"), "TouchLastLog")) {
        *last_log = h.clock();
    }
    Ok(())
}

/// `Resolve-M9AStatus`(m9a.ps1:282-317) 的复刻：十键 body 顺序照真身；`Found:false` = 还在跑 → `None`
/// （主循环那记 `continue` 的落点）；`FailedTasks` 按**键存在**取值（只有 blocked 那一路 C# 才带这枚键）。
/// 真身那句"拷一份、摘掉 lastLog"在核内没有对应物：这里的 lastLog 从来不进映射，
/// 另存成 `last_log` 本地量（ark 侧同一处置，见 `maa_run` 的 `last_growth`）—— 是形状不同，不是漏搬。
fn m9a_resolve_status<H: MaaHost>(
    h: &mut H,
    state: &JVal,
    elapsed: f64,
    last_log: f64,
    sc: &M9aScen,
    process_gone: bool,
    log_name: &str,
) -> Result<Option<M9aRet>, String> {
    let mut pairs: Vec<(String, JVal)> = Vec::new();
    if let JVal::O(o) = state {
        for (k, v) in o.iter() {
            pairs.push((k.clone(), v.clone()));
        }
    }
    // 静默时距＝真身的 (Get-Date) - $state.lastLog；先取局部是因为 h.verb 与 h.clock 不能同表达式并存。
    let quiet_seconds = h.clock() - last_log;
    let d = h.verb(
        "resolve-m9a-status",
        &jv_o(vec![
            ("state", JVal::O(pairs)),
            ("elapsed", JVal::F(elapsed)),
            ("quietSeconds", JVal::F(quiet_seconds)),
            ("processGone", JVal::B(process_gone)),
            ("logName", jv_s(log_name)),
            ("instanceName", jv_s(&sc.instance_name)),
            ("instanceId", jv_s(&sc.instance_id)),
            ("startTimeoutMinutes", JVal::I(sc.start_min)),
            ("timeoutMinutes", JVal::I(sc.timeout_min)),
            ("stallTimeoutMinutes", JVal::I(sc.stall_min)),
        ]),
    )?;
    if !jv_truthy(&jv_get(&d, "Found")) {
        return Ok(None);
    }
    let mut r = M9aRet {
        status: jv_str(&jv_get(&d, "Status")),
        detail: jv_str(&jv_get(&d, "Detail")),
        tasks: vstr_list(&jv_get(&d, "Tasks")),
        ..Default::default()
    };
    if jvo_has(&d, "FailedTasks") {
        r.failed_tasks = Some(vstr_list(&jv_get(&d, "FailedTasks")));
    }
    Ok(Some(r))
}

/// `Invoke-M9ARun` 的循环骨架（lib\m9a.ps1:319-528）逐段进核。
/// 与 ark 一路（`maa_run`）是同一段形状：装配启动参数 → 启动等待 → 每 10 秒采集 → 喂判定 verb →
/// 同一 tick 内执行 effect 并带回执重入。三处不同是**真身本来就不同**，不许"顺手统一"（§11.39 ⑤）：
///   · 偏移按**日志路径 keyed**（跨 00:00 换文件、新文件从 0 起算），不是 ark 那两枚固定变量；
///   · `cfg.rescueMaxRaw` 送**原始** visionMaxRescue（ark 送本地预算镜像），两边各自在 C# clamp `<=0→2`；
///   · 兜底回执的 kind 叫 `ask-screen`（ark 叫 `rescue`），记账 Tag 与调用 Tag **同数**（ark 差一格）。
/// 外层没有退出条件这点也是真身原样：出结论全靠 `resolve-m9a-status` 的档位，而三枚超时钟一定会撞上。
fn m9a_run<H: MaaHost>(h: &mut H, sc: &M9aScen) -> Result<M9aRet, String> {
    let p = M9aPaths::of(sc);
    let env = RescueEnv::of_m9a(sc);
    // :333 Get-GdM9ACliEnabled 的三态：settings 里这枚**键不存在**（None）= 开。
    let cli = sc.cli.unwrap_or(true);
    // :334-337 Get-M9ALaunchArgs：五键 body 顺序照真身（cliArgs/mode/id/name/forceStart）。
    // ⚠ mode 的"没填"是真身递进去的**空串**，不是 `Get-M9ALaunchArgs` 那个 param 默认 'id' ——
    // 真句是 `-Mode ([string]$S.runtime.m9aInstanceArg)`，显式传了就不走默认；C# 本体只认 "name"、
    // 其余一律按 id 取值（Processes.cs:392），所以空串与 'id' 在**结果**上同形，但喂进去的字节必须是空串。
    let d = h.verb(
        "m9a-launch-args",
        &jv_o(vec![
            ("cliArgs", JVal::B(cli)),
            ("mode", jv_s(&sc.mode)),
            ("id", jv_s(&sc.instance_id)),
            ("name", jv_s(&sc.instance_name)),
            ("forceStart", JVal::B(sc.force_start)),
        ]),
    )?;
    let launch_args = vstr_list(&jv_get(&d, "Args"));

    // :339-343 DryRun 一行 STEP 就回，返回表里多带 Args 一格（其余档位真身根本没有这一键）。
    if sc.dry_run {
        let shown = if launch_args.is_empty() {
            "(不带参数：老路子，靠 M9A 自己的自动开始)".to_string()
        } else {
            launch_args.join(" ")
        };
        h.log(
            "STEP",
            &format!(
                "[dry-run] 将启动 {} {}，ADB {}，以 logs\\log-*.log 判定完成",
                p.exe, shown, sc.serial
            ),
        );
        return Ok(M9aRet {
            status: "dryrun".to_string(),
            detail: String::new(),
            args: launch_args,
            ..Default::default()
        });
    }

    // :344-347 两枚"已在运行"分支。真身是两句各问一次 `Test-GdProcessRunning` ⇒ 这边也各问一次
    // （投影因此有两条 `P|`，合成一条就是把两句判据并成一句）。第二句是真 throw：上抛，不返回结果表。
    if cli && h.proc_running(&p.name) {
        h.log(
            "WARN",
            "M9A 已在运行：命令行会被转发给正在跑的实例，按 -i/-f 重新开始任务",
        );
    }
    if !cli && h.proc_running(&p.name) {
        return Err(
            "M9A 已在运行，无法保证按新实例开始(请先关闭，或把 runtime.m9aCliArgs 打开)".to_string()
        );
    }

    // :349-352 ADB 连接段。`Connect-GdAdb`(mumu.ps1:61-69) 的第一句是 `Test-Path` 挡 adb 不在场，
    // 那是纯现场检查 ⇒ 在核内做（那句 throw 的整字面照抄）；connect + devices 两步是真碰设备 ⇒ verb。
    // panel-cs 真身已随 **B-2d③-a** 落（`Live.AdbConnect`），生产接线也已随 **B-2d④** 的链体批落地：
    // `--drive-m9a-daily` 的 run 真做走 `drive_real_m9a_run` ⇒ RealHost，§11.42 ⑦-2 那笔欠账已结清。
    // 等价门里这一枚仍由夹具喂回执（`--fixture-run`），为的是让"对拍"不被现场设备拖住。
    h.log("STEP", &format!("ADB 连接 {}", sc.serial));
    if h.file_len(&sc.adb).is_none() {
        return Err(format!("adb 不存在：{}", sc.adb));
    }
    let ac = h.verb(
        "adb-connect",
        &jv_o(vec![("adb", jv_s(&sc.adb)), ("serial", jv_s(&sc.serial))]),
    )?;
    if !jv_truthy(&jv_get(&ac, "Ok")) {
        h.log("WARN", "adb devices 未列出该序列号，仍继续(M9A 会自行重连)");
    }

    // :354-358 先记今天日志现有长度：只认启动之后的新增内容，别把上次运行的"完成"读成本轮的。
    let started = h.clock();
    let mut pre_offsets: Vec<(String, u64)> = Vec::new();
    let today_start = h.day_stamp();
    let today = m9a_daily_path(&p.log_dir, &today_start);
    if let Some(l) = h.file_len(&today) {
        pre_offsets.push((today.clone(), l));
    }

    // :359 这句的**句尾空格**是真身原样：模板里那枚空格在 `$(if …)` 之外，参数为空时展开成空串，
    // 于是那句日志以空格收尾。门 old 侧逐字比 ⇒ 不许 trim（与 §11.39 ⑤-2 那族"原样怪癖"同处置）。
    h.log(
        "STEP",
        &format!(
            "启动 M9A：{} {}",
            p.exe,
            if launch_args.is_empty() {
                String::new()
            } else {
                launch_args.join(" ")
            }
        ),
    );
    // :360-364 两支 Start-Process 在真身是为了 `-ArgumentList` 给不给；这边一枚方法两支都吃
    // （args 为空 ⇒ `ScriptHost` 投影保持两截 `R|exe|dir`，与 ark 那一路同款保基线写法）。
    h.start_process(&p.exe, &p.dir, &launch_args)?;
    // :365-367 先测后睡的那 120/3 秒；超时是真身那句 return（不是 throw）。
    if !wait_condition(h, 120, 3, "M9A 进程启动", |hh| hh.proc_running(&p.name)) {
        return Ok(M9aRet {
            status: "error".to_string(),
            detail: "M9A 未能启动(进程未出现)".to_string(),
            ..Default::default()
        });
    }

    // :369-377 三份跨拍状态：keyed 偏移表（**空表起步**，与 pre_offsets 分开）、十三键 $state、
    // seenGame/gone 两枚现场量。lastLog 那格在真身是 DateTime ⇒ 不进映射，另存 last_log。
    let mut offsets: Vec<(String, u64)> = Vec::new();
    let mut state = jv_o(vec![
        ("tasks", JVal::A(Vec::new())),
        ("chainStarted", JVal::B(false)),
        ("chainDone", JVal::B(false)),
        ("connectPhase", jv_s("none")),
        ("failed", JVal::Null),
        ("queueCount", JVal::Null),
        ("startRequested", JVal::B(false)),
        ("eventCount", JVal::I(0)),
        ("failedTasks", JVal::A(Vec::new())),
        ("resCurrent", JVal::Null),
        ("resLatest", JVal::Null),
        ("connectFail", JVal::Null),
    ]);
    let mut last_log = h.clock();
    let mut seen_game = false;
    let mut gone: i64 = 0;
    // :401 $stM9a 建在 while **之外**：C# 那份小状态跨拍保命（phase/预算/Notes），一次调用一份。
    let mut st_m9a: JVal = JVal::Null;
    // 预算本地镜像：m9a 真身**没有**这一格（预算只活在 C# 的 state 里，body 送的是原始值）。
    // 建它只为喂 `do_rescue` 的出参 —— 那个出参每拍都被 state 重算盖掉，不参与任何判据。
    let mut rescue_left = sc.vision_max;
    if rescue_left <= 0 {
        rescue_left = 2;
    }

    loop {
        // :403 每轮先睡 10 秒（真身这句在 while 的第一行，一轮都不许省）。
        h.sleep(10);
        // :404-406 采集 + 整批一次回灌（批次 9 的省钱点：一轮一个进程，绝不每事件一个）。
        let (log_path, events) = m9a_read_log(h, &p, &sc.instance_id, &mut offsets, &pre_offsets)?;
        m9a_register_events(h, &mut state, &events, &mut last_log)?;
        // :407 `Get-Gd1999Pid`(mumu.ps1:71-78)：Found:false = 游戏没在跑（旧语义的 $null）。
        // 这句不挡 adb 在场与否 —— 真身也是直接起 adb。panel-cs 真身已随 **B-2d③-a** 落（`Live.AdbGamePid`），
        // 生产接线已随 **B-2d④** 落地（run 真做走 RealHost）；等价门里这一枚仍由夹具喂，理由同上。
        let gp = h.verb(
            "adb-game-pid",
            &jv_o(vec![
                ("adb", jv_s(&sc.adb)),
                ("serial", jv_s(&sc.serial)),
                ("package", jv_s(&sc.package)),
            ]),
        )?;
        if jv_truthy(&jv_get(&gp, "Found")) {
            seen_game = true;
        }
        // :409-413 elapsed 在 adb 之后现算；进程连续两轮不在场才算真退出（自更新重启会闪一下）。
        let elapsed = h.clock() - started;
        if !h.proc_running(&p.name) {
            gone += 1;
        } else {
            gone = 0;
        }
        let mut res = match m9a_resolve_status(
            h,
            &state,
            elapsed,
            last_log,
            sc,
            gone >= 2,
            &file_name(&log_path),
        )? {
            Some(r) => r,
            None => continue,
        };

        // ===== 迷你循环：同一 tick 内「喂 verb → 执行 effect → 带回执重入」（真身 m9a.ps1:422-480）=====
        // $effx 建在迷你循环之内、$stM9a 建在它之外 —— 照真身：预算/Notes 要跨拍，"本拍已办什么"不跨拍。
        let mut effx: Vec<JVal> = Vec::new();
        loop {
            let rescue_avail = h.rescue_available();
            let d = h.verb(
                "decide-m9a-run-tick",
                &jv_o(vec![
                    ("now", jv_o(vec![("elapsedSec", JVal::F(elapsed))])),
                    (
                        "live",
                        jv_o(vec![
                            ("seenGame", JVal::B(seen_game)),
                            ("rescueAvailable", JVal::B(rescue_avail)),
                        ]),
                    ),
                    (
                        "res",
                        jv_o(vec![
                            ("status", jv_s(&res.status)),
                            ("detail", jv_s(&res.detail)),
                            ("tasksCount", JVal::I(res.tasks.len() as i64)),
                        ]),
                    ),
                    ("cfg", jv_o(vec![("rescueMaxRaw", JVal::I(sc.vision_max))])),
                    ("state", st_m9a.clone()),
                    ("effectsResolved", JVal::A(effx.clone())),
                ]),
            )?;
            st_m9a = jv_get(&d, "State");
            // Directives：停滞解除那一拍把停滞计时归零 —— 真身 `$state.lastLog = Get-Date` 的落地处。
            if jv_truthy(&jv_get(&jv_get(&d, "Directives"), "ResetLastLog")) {
                last_log = h.clock();
            }
            replay_logs(h, &jv_get(&d, "Logs"));

            let effs = jv_get(&d, "Effects");
            let ef = jv_list(&effs);
            if ef.is_empty() {
                if jv_str(&jv_get(&d, "Action")) == "return" {
                    // 完成整句/进程后缀/兜底拼串的全集已由 C# 拼好（上面 Logs 已回放）；
                    // Rescued 只在真兜成时有意义，Duration 只有 ok 档带（按**键存在**取，同 FailedTasks 那口径）。
                    res.detail = jv_str(&jv_get(&d, "Detail"));
                    if jv_truthy(&jv_get(&d, "Rescued")) {
                        res.rescued = true;
                    }
                    if jvo_has(&d, "Duration") {
                        res.duration = Some(jv_to_d(&jv_get(&d, "Duration")));
                    }
                    return Ok(res);
                }
                // loop 且无待办 effect = 兜成功了、这一轮的结论撤回继续等（真身那记原地 continue）。
                break;
            }
            let e = ef[0].clone();
            let kind = jv_str(&jv_get(&e, "Kind"));
            let mut did = false;
            if kind_eq(&kind, "ask-screen") {
                // 真兜底：Tag/Context 由 C# 挂回（预算在挂 effect 那拍已扣）；异常回执走 exception 键，
                // C# 续判时合成旧闭包同款"兜底抛异常：…"结论。ack_kind 用 `ask-screen` 这套词。
                do_rescue(h, &env, "ask-screen", &e, &st_m9a, &mut effx, &mut rescue_left);
                did = true;
            }
            if !did && kind_eq(&kind, "record") {
                do_record(h, &env, &e, &mut effx)?;
                did = true;
            }
            if !did {
                // 判据挂了 effect 可哪个执行块都没接手：宁可作废这一拍，也不能拿着没回执的挂起点空转。
                // 整句照抄 m9a.ps1:478（`$kind` 与 `$([string]$e.Tag)` 就是下面那两个本地串）。
                h.log(
                    "WARN",
                    &format!(
                        "M9A tick：effect(kind={}，tag={})没有落到执行块，本 tick 作废(下一轮按新读数重判)",
                        kind,
                        jv_str(&jv_get(&e, "Tag"))
                    ),
                );
                break;
            }
        }
    }
}

// ---- RealHost：生产现场（B-2c③ 起真接线：`run` 真做就是从这里进 maa_run） ----

struct RealHost {
    panel_cs: String,
    /// 日志落点＝与链体日志**同一本**侧信道（B-2c② 第七闸给的那枚路径）。
    /// §11.36 ②-1 那句"核内所有 log 产生点"的另一半就是这里：run 内部行不再走 stderr，
    /// 它与链体行同序落进同一文件，壳侧一次排空就照原级别回放整条链 —— 通道只有一条。
    log_sink: String,
    // 两枚可用性在真身世界不是问盘问出来的，而是问"这一路的模块清单里有没有那枚函数"：
    // lib\ark-module.ps1:31 的 Libs 带 vision-rescue.ps1 + vision-ark.ps1 ⇒ 两枚都在场；
    // lib\r1999-module.ps1:27 的 Libs 只带 vision-rescue.ps1（抢前台那枚是端游专用的，住 vision-ark.ps1）
    // ⇒ rescue 在场、grab 不在场。这是**装载期就定了的事实**，不随现场变，所以按 game 给常量，
    // 不去现跑 `Get-Command`（问了也是同一个答案，还要多引一条"当前模块加载了哪些 lib"的现场依赖）。
    rescue_avail: bool,
    grab_avail: bool,
}

impl RealHost {
    /// 默认按 ark 一路建（生产接线到目前为止只有端游这条线）。
    fn new(panel_cs: &str, log_sink: &str) -> RealHost {
        RealHost::for_game(panel_cs, "ark", log_sink)
    }
    /// B-2d 给 m9a 那一路时用它：rescue 照旧在场，grab 按清单不在场 ⇒ 兜底链会走 C# 那句
    /// "这棵树上没有抢前台的函数，只能等停滞判定"，而不是去调一枚根本不该调的现场 verb。
    fn for_game(panel_cs: &str, game: &str, log_sink: &str) -> RealHost {
        let ark = game.eq_ignore_ascii_case("ark");
        RealHost {
            panel_cs: panel_cs.to_string(),
            log_sink: log_sink.to_string(),
            rescue_avail: true,
            grab_avail: ark,
        }
    }
}

/// `Get-GdFileTail` 的现场口径：不在 → ('',0)；offset 越界 → 归零全读；偏移 = 读时的文件总长。
fn real_file_tail(path: &str, offset: u64) -> (String, u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (String::new(), 0);
    };
    let len = meta.len();
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (String::new(), 0),
    };
    let start = if offset > len { 0 } else { offset };
    if std::io::Seek::seek(&mut f, std::io::SeekFrom::Start(start)).is_err() {
        return (String::new(), len);
    }
    let mut buf = Vec::new();
    if std::io::Read::read_to_end(&mut f, &mut buf).is_err() {
        return (String::new(), len);
    }
    (String::from_utf8_lossy(&buf).to_string(), len)
}

/// 进程名（不带扩展名）在不在场：Get-Process -Name 的口径。读不出来按"不在"（调用方据此走
/// "进程没起来"那一档，与真身 SilentlyContinue 给假同形）。
fn real_proc_running(name: &str) -> bool {
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap.is_null() || snap as isize == INVALID_HANDLE_ISIZE {
        return false;
    }
    let mut entry: ProcessEntry32W = unsafe { std::mem::zeroed() };
    entry.dw_size = PROCESSENTRY32W_DWSIZE;
    let mut found = false;
    let mut ok = unsafe { Process32FirstW(snap, &mut entry) } != 0;
    while ok {
        let raw = String::from_utf16_lossy(&entry.sz_exe_file);
        let raw = raw.trim_end_matches('\0').to_string();
        if file_stem(&raw).eq_ignore_ascii_case(name) {
            found = true;
            break;
        }
        ok = unsafe { Process32NextW(snap, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snap) };
    found
}

impl MaaHost for RealHost {
    fn log(&mut self, level: &str, text: &str) {
        // B-2c③ 结清 B-2a 那笔留言：run 内部行的落点就是链体同一本侧信道，通道只有一条。
        // log 的签名不带 Result（trait 定的），所以这里不另开第二条错路：写不成先吞掉，同一枚
        // sink 的毛病由链体下一次 fx.log 的 io_err 咬住并冒泡 —— 真做段与新区段共用同一把错阀。
        let _ = drive_sink_row(&self.log_sink, level, text);
    }
    fn clock(&mut self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
    fn sleep(&mut self, seconds: u32) {
        std::thread::sleep(std::time::Duration::from_secs(seconds as u64));
    }
    fn file_len(&mut self, path: &str) -> Option<u64> {
        std::fs::metadata(path).ok().map(|m| m.len())
    }
    fn file_tail(&mut self, path: &str, offset: u64) -> (String, u64) {
        real_file_tail(path, offset)
    }
    fn proc_running(&mut self, name: &str) -> bool {
        real_proc_running(name)
    }
    fn start_process(&mut self, exe: &str, dir: &str, args: &[String]) -> Result<(), String> {
        let mut cmd = std::process::Command::new(exe);
        cmd.current_dir(dir);
        // 三枚 stdio 一律指空设备。默认是 Inherit，被起的进程于是**继承了本核的 stdout 管道写端**：
        // 核退出后，壳侧 Start-GdNativeStreamed 拿到 ExitCode 之后还要过收 stdout 那一行，而那根管道
        // 唯一剩下的写端在对方进程手里 —— 它起的是常驻 GUI，跑完任务照常在，于是 EOF 永不来，
        // 整轮卡在收尾之前（10-13 现场定因，读数与逐轮对照见 docs\b2e-eventstream-feasibility.md §6）。
        // 这一路本来就不读被起进程的任何输出（返回值只有成/败），null 既是最小修法，也是唯一
        // 不新增读流、不改跨进程出口形状的修法 —— 两枚契约号因此都不必动。
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        // Start-Process -ArgumentList 的现场等价物：非空才带（ark 一路裸启动，参数面一个字节都不进）。
        if !args.is_empty() {
            cmd.args(args);
        }
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| format!("Start-Process {} 失败：{}", exe, e))
    }
    fn day_stamp(&mut self) -> String {
        gd_day_stamp()
    }
    fn verb(&mut self, name: &str, body: &JVal) -> Result<JVal, String> {
        call_panel_verb(&self.panel_cs, name, body)
    }
    fn rescue_available(&self) -> bool {
        self.rescue_avail
    }
    fn grab_available(&self) -> bool {
        self.grab_avail
    }
}

// ---- B-2c③：`run` 真做的生产入口（链体要 run 时从这里进 maa_run，不再 ask 给壳） ----

/// 生产侧那枚 run 闭包的真身：ctx 的 `maaRun` 载荷（已解析成 MaaScen）＋链体现给的 chains → maa_run。
/// 两枚 paths 缺一就当场 Err：往空目录去起 MAA 只会造出一条没法归因的现场错，不如在这里点名
/// "载荷缺哪一格"（这句话进 journal 的 err 行，链体照旧把它吞成 status=error 那档）。
/// `dry_run` 在这里按死=false —— 生产三处调用点（ark-daily.ps1:157/221/408）都不给 `-DryRun`，
/// ctx 里也没有这一格；给它留口子等于给生产载荷开一扇"什么都不做也算 ok"的门。
fn drive_real_run(
    panel_cs: &str,
    log_sink: &str,
    sc: &MaaScen,
    chains: &[String],
) -> Result<ArkRunOut, String> {
    if sc.maa_dir.is_empty() || sc.maa_exe.is_empty() {
        return Err(
            "ctx 没给 maaRun 的 paths(maaDir/maaExe)：run 真做只吃 ctx 载荷，不替生产猜默认路径".to_string(),
        );
    }
    let mut sc2 = sc.clone();
    // expected 不是配置，是链体每次现给的期望链（cfg 回执里的 ExpectedChains）——覆盖 ctx 那格。
    sc2.expected = chains.to_vec();
    sc2.dry_run = false;
    let mut h = RealHost::for_game(panel_cs, &sc2.game, log_sink);
    let r = maa_run(&mut h, &sc2)?;
    // MaaRet 的 errors 键不进链体（PS 侧的 $rd 只有 status/detail/duration 三格，同一条口径）。
    Ok(ArkRunOut { status: r.status, detail: r.detail, duration: r.duration })
}

/// 生产侧那枚 m9a run 闭包的真身（B-2d④）：ctx 的 `m9aRun` 载荷（已解析成 `M9aScen`）→ `m9a_run`。
/// 这一枚落地的同时，B-2d② 挂着的两笔"RealHost 编而不接"留言一并结清（§11.42 ⑦-2）。
/// 与上面那枚同规矩、两处不同源，都来自真身的形状而不是风格：
/// 1) **没有 chains 参数** —— `Invoke-M9ARun` 的签名里没有 `-Chains`，m9a 的期望链根本不由链体现给
///    （cfg 回执只有 changes 数组），所以本闭包只吃 scen；
/// 2) **rescued 要带出去** —— 真身 `Start-GdR1999Daily` 把它喂给 `decide-daily-retry`（ark 那一路
///    压根不吃这格），所以 `M9aRunOut` 比 `ArkRunOut` 多这一枚；tasks/failedTasks/args 三格仍不带，
///    它们是 `Z|` 投影面的话术料，链体不读（与 ark 侧 errors 不同源那条同款）。
/// 空路径照 ark 那枚的规矩当场点名拒做：往空目录去起 M9A 只会造出一条没法归因的现场错。
/// `dry_run` 按死=false —— 生产那三处调用点都不给 `-DryRun`，ctx 里也没有这一格。
fn drive_real_m9a_run(
    panel_cs: &str,
    log_sink: &str,
    sc: &M9aScen,
) -> Result<M9aRunOut, String> {
    if sc.m9a_dir.is_empty() || sc.m9a_exe.is_empty() {
        return Err(
            "ctx 没给 m9aRun 的 paths(m9aDir/m9aExe)：run 真做只吃 ctx 载荷，不替生产猜默认路径".to_string(),
        );
    }
    let mut sc2 = sc.clone();
    sc2.dry_run = false;
    // game 是名册外的常量（真身 m9a.ps1 三处写死 'r1999'），由 envelope 填好再进来；
    // for_game 按它给两枚可用性：r1999 ⇒ rescue 在场、grab 不在场（lib\r1999-module.ps1:27 的清单事实）。
    let mut h = RealHost::for_game(panel_cs, &sc2.game, log_sink);
    let r = m9a_run(&mut h, &sc2)?;
    Ok(M9aRunOut { status: r.status, detail: r.detail, duration: r.duration, rescued: r.rescued })
}

// ---- 剧本 JSON 的读取与 --maa-run 出口 ----

/// 一档剧本的读法：缺键一律按"没填"取值（0 / 空串 / 空数组），不替夹具猜默认值。
/// 唯一的例外是 B-2b 的 game/target 两格：它们的"没填"就是真身写死的那两个 'ark'（见下面的注释）。
fn maarun_parse(v: &JVal) -> Result<MaaScen, String> {
    let mut sc = MaaScen::default();
    sc.name = jv_str(&jv_get(v, "name"));
    sc.dry_run = jv_truthy(&jv_get(v, "dryRun"));
    sc.expected = vstr_list(&jv_get(v, "expectedChains"));
    let paths = jv_get(v, "paths");
    sc.maa_dir = jv_str(&jv_get(&paths, "maaDir"));
    sc.maa_exe = jv_str(&jv_get(&paths, "maaExe"));
    sc.maa_config = jv_str(&jv_get(&paths, "maaConfig"));
    sc.gui_log = jv_str(&jv_get(&paths, "maaGuiLog"));
    sc.asst_log = jv_str(&jv_get(&paths, "maaAsstLog"));
    sc.ark_exe = jv_str(&jv_get(&paths, "arknightsExe"));
    let rt = jv_get(v, "runtime");
    sc.start_min = jv_int(&jv_get(&rt, "arkStartTimeoutMinutes"), 0);
    sc.timeout_min = jv_int(&jv_get(&rt, "arkTimeoutMinutes"), 0);
    sc.stall_min = jv_int(&jv_get(&rt, "stallTimeoutMinutes"), 0);
    sc.idle_sec = jv_int(&jv_get(&rt, "idleConfirmSeconds"), 0);
    sc.abort_stall = jv_truthy(&jv_get(&rt, "abortOnMaaStallWarning"));
    sc.vision_max = jv_int(&jv_get(&rt, "visionMaxRescue"), 0);
    sc.benign = vstr_list(&jv_get(&rt, "benignMaaErrors"));
    let seed_gui = jv_get(v, "seedGui");
    if !matches!(seed_gui, JVal::Null) {
        sc.seed_gui = Some(jv_str(&seed_gui));
    }
    let seed_asst = jv_get(v, "seedAsst");
    if !matches!(seed_asst, JVal::Null) {
        sc.seed_asst = Some(jv_str(&seed_asst));
    }
    for pr in jv_list(&jv_get(v, "procs")) {
        sc.procs.push((jv_str(&jv_get(pr, "name")), jv_truthy(&jv_get(pr, "up"))));
    }
    // B-2b 六格。game/target 的缺键默认是**真身写死的值**（maa.ps1:525/560/591 三处 -Game 'ark' -Target 'ark'），
    // 不是替夹具猜；其余四格缺键就是"没填"（false / 空串 / 空队列）。
    sc.game = {
        let g = jv_str(&jv_get(v, "game"));
        if g.is_empty() {
            "ark".to_string()
        } else {
            g
        }
    };
    sc.target = {
        let t = jv_str(&jv_get(v, "target"));
        if t.is_empty() {
            sc.game.clone()
        } else {
            t
        }
    };
    sc.rescue_available = jv_truthy(&jv_get(v, "rescueAvailable"));
    sc.grab_available = jv_truthy(&jv_get(v, "grabAvailable"));
    sc.state_dir = jv_str(&jv_get(v, "stateDir"));
    for lv in jv_list(&jv_get(v, "live")) {
        sc.live.push((jv_str(&jv_get(lv, "verb")), jv_get(lv, "data")));
    }
    for ev in jv_list(&jv_get(v, "events")) {
        let kind = jv_str(&jv_get(&ev, "kind"));
        if !matches!(kind.as_str(), "append" | "grow" | "truncate" | "up" | "down") {
            return Err(format!("剧本里认不出的事件 kind：{}（只有 append/grow/truncate/up/down）", kind));
        }
        let target = {
            let t = jv_str(&jv_get(&ev, "target"));
            if t.is_empty() {
                jv_str(&jv_get(&ev, "proc"))
            } else {
                t
            }
        };
        sc.events.push(MaaEv {
            at: jv_to_d(&jv_get(&ev, "at")),
            kind: match kind.as_str() {
                "append" => "append",
                "grow" => "grow",
                "truncate" => "truncate",
                "up" => "up",
                _ => "down",
            },
            target,
            text: jv_str(&jv_get(&ev, "text")),
            bytes: jv_int(&jv_get(&ev, "bytes"), 0),
        });
    }
    Ok(sc)
}

/// 每档开跑前把两份日志摆回剧本起点：给了 seed 就整写（空串也算"文件在、零字节"），
/// 没给就删掉（"日志压根不在场"是单独一档）。old 侧桩浴做同一件事，两侧起点才相同。
/// 落点用 MaaPaths 解出来的那两条（asst 缺省 debug\asst.log 也一起吃到）。
fn maarun_seed(sc: &MaaScen) -> Result<(), String> {
    let p = MaaPaths::of(sc);
    for (path, seed) in [(&p.gui_log, &sc.seed_gui), (&p.asst_log, &sc.seed_asst)] {
        match seed {
            None => {
                let _ = std::fs::remove_file(path);
            }
            Some(s) => write_at(path, s.as_bytes(), false)?,
        }
    }
    Ok(())
}

fn render_maarun_json(items: &[(String, String)]) -> String {
    let cases: Vec<String> = items
        .iter()
        .map(|(name, data)| format!("{{\"name\":{},\"data\":{}}}", jstr(name), jstr(data)))
        .collect();
    format!(
        "{{\"ok\":true,\"why\":\"运行面宿主的剧本回放：循环骨架在内核里跑，判据照旧真跑 panel-cs；这不是对方行为的读数\",\"contract\":{},\"role\":\"B2-maa-run\",\"fixture\":\"maa-run\",\"count\":{},\"cases\":[{}]}}",
        jstr(CONTRACT),
        cases.len(),
        cases.join(",")
    )
}

/// `--maa-run --script <剧本文件> --panel-cs <判定 exe>`：一文件多档（JSON 数组），逐档回放，
/// 出口 = 每档一行投影（`cases[{name,data}]`，与 --arkskip 同形）。
/// 与前面五档同理落在两必填**之前**返回；缺 --script / 缺 --panel-cs 自己拒跑（不是被必填拦死）。
fn run_maa_run(script_path: &str, panel_cs: &str, out: &Option<String>) -> i32 {
    let missing = if script_path.is_empty() {
        Some("--script：剧本文件（JSON 数组，一档一项）没给")
    } else if panel_cs.is_empty() {
        Some("--panel-cs：循环骨架每 tick 真跑 decide-maa-run-tick，缺判定 exe 这一档跑不了")
    } else {
        None
    };
    if let Some(m) = missing {
        emit(
            &format!(
                "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"B2-maa-run\"}}",
                jstr(&format!("运行面宿主回放档拒跑：{} —— 缺它这一档没开", m)),
                jstr(CONTRACT)
            ),
            out,
        );
        return 2;
    }
    let text = match std::fs::read_to_string(script_path) {
        Ok(t) => t,
        Err(e) => {
            emit(
                &format!(
                    "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"B2-maa-run\"}}",
                    jstr(&format!("剧本读不动（{}）：{}", script_path, e)),
                    jstr(CONTRACT)
                ),
                out,
            );
            return 1;
        }
    };
    let root = match jv_parse(&text) {
        Ok(v) => v,
        Err(e) => {
            emit(
                &format!(
                    "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"B2-maa-run\"}}",
                    jstr(&format!("剧本不是合法 JSON：{}", e)),
                    jstr(CONTRACT)
                ),
                out,
            );
            return 1;
        }
    };
    let mut items: Vec<(String, String)> = Vec::new();
    let mut bad: Option<String> = None;
    for raw in jv_list(&root) {
        let sc = match maarun_parse(raw) {
            Ok(s) => s,
            Err(e) => {
                bad = Some(format!("剧本第 {} 档读不动：{}", items.len() + 1, e));
                break;
            }
        };
        if let Err(e) = maarun_seed(&sc) {
            bad = Some(format!("剧本第 {} 档的日志摆不回起点：{}", items.len() + 1, e));
            break;
        }
        let mut host = ScriptHost::new(panel_cs, &sc);
        match maa_run(&mut host, &sc) {
            Ok(r) => {
                let mut proj = host.proj.clone();
                proj.push(r.proj_line());
                items.push((sc.name.clone(), proj.join("\u{1f}")));
            }
            Err(e) => {
                bad = Some(format!("剧本第 {} 档没跑完：{}", sc.name, e));
                break;
            }
        }
    }
    let json = match bad {
        Some(e) => {
            emit(
                &format!(
                    "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"B2-maa-run\"}}",
                    jstr(&e),
                    jstr(CONTRACT)
                ),
                out,
            );
            return 1;
        }
        None => render_maarun_json(&items),
    };
    emit(&json, out);
    0
}

// ---- --m9a-run 出口（B-2d②：m9a 那一路的循环骨架回放档）----

/// ctx 名册那 15 枚的**唯一出处**（B-2d④）：paths 3 ＋ conn 2 ＋ instance 2 ＋ package 1 ＋ runtime 7。
/// 剧本档（`--m9a-run` 的夹具）与生产档（`--drive-m9a-daily` 的 `ctx.m9aRun`）共用这一把：
/// 立第二套键名字面就等于两处口径可以各漂一半（B-2c③ 那条 `maarun_parse` 先例同款）。
/// 硬约束照 plan §11.39 ⑥-1 的字面：这 15 枚**逐枚点名取键**，绝不允许把 `$S.runtime` 整块序列化，
/// 所以 `visionApiKey` 与名册那七枚 runtime 键虽住同一个块，落到这里只有这七枚。
/// 两枚起步拒跑闸也在这里（它们吃的全是名册内的格）；第三枚 days 闸留在外层 ——
/// 日期读数表只有夹具要（生产走 `gd_day_stamp` 现取挂钟），放这里会让生产 ctx 当场拒跑。
/// **三闸相对次序 m9aDir → adb → days 是门的判据，不许在两层的任何一侧重排。**
fn m9a_scen_core(v: &JVal) -> Result<M9aScen, String> {
    let mut sc = M9aScen::default();
    let paths = jv_get(v, "paths");
    sc.m9a_dir = jv_str(&jv_get(&paths, "m9aDir"));
    sc.m9a_exe = jv_str(&jv_get(&paths, "m9aExe"));
    sc.m9a_log_dir = jv_str(&jv_get(&paths, "m9aLogDir"));
    // 真身 `Get-M9ADailyLog:129` 那句是裸 `Join-Path $P.LogDir ...`（不是宽容的 Join-GdPath），
    // LogDir 为空当场抛绑定异常。核里没有"绑定异常"这一档，所以把它前移成起步拒跑：
    // 宁可剧本改一行，也不要一核内"看起来跑完了、其实一次文件都没读"的假绿。
    if sc.m9a_dir.is_empty() {
        return Err("paths.m9aDir 没给：日志目录与 exe 落点都从它拼，真身那句裸 Join-Path 会当场抛".to_string());
    }
    let conn = jv_get(v, "conn");
    sc.adb = jv_str(&jv_get(&conn, "adb"));
    sc.serial = jv_str(&jv_get(&conn, "serial"));
    // adb 路径为空时真身那句 `Connect-GdAdb` 的 `throw "adb 不存在："` 会在起循环**之前**炸掉，
    // 也就是说这一档在现场根本走不到读日志那一步；与其让内核产出一行 E| 假装跑过，不如拒跑。
    if sc.adb.is_empty() {
        return Err("conn.adb 没给：真身在这一步就 throw（adb 不存在），这一档连循环都进不去".to_string());
    }
    let inst = jv_get(v, "instance");
    sc.instance_id = jv_str(&jv_get(&inst, "id"));
    sc.instance_name = jv_str(&jv_get(&inst, "name"));
    sc.package = jv_str(&jv_get(v, "package"));
    let rt = jv_get(v, "runtime");
    // mode 缺键落成**空串**而不是 'id'：真身 `:335` 那句是 `[string]$S.runtime.m9aInstanceArg`
    // （显式转换，不是 param 默认值），C# 那侧只认 "name"、其余一律按 id —— 结果同形，字节要照原样。
    sc.mode = jv_str(&jv_get(&rt, "m9aInstanceArg"));
    sc.force_start = jv_truthy(&jv_get(&rt, "m9aForceStart"));
    // m9aCliArgs 是**三态**（plan §11.39 ⑤-1 点名的那枚地雷）：键不存在 = 真身
    // `Get-GdM9ACliEnabled` 的"没写"档 = **开**命令行入口。用 `jv_truthy` 收会把"没填"和
    // "填了 false"塌成一档，而这两档在真身里走完全不同的路（前者带 -i/-a/-q/-f，后者裸启动＋"在跑就 throw"）。
    let cli_raw = jv_get(&rt, "m9aCliArgs");
    if !matches!(cli_raw, JVal::Null) {
        sc.cli = Some(jv_truthy(&cli_raw));
    }
    sc.vision_max = jv_int(&jv_get(&rt, "visionMaxRescue"), 0);
    sc.start_min = jv_int(&jv_get(&rt, "r1999StartTimeoutMinutes"), 0);
    sc.timeout_min = jv_int(&jv_get(&rt, "r1999TimeoutMinutes"), 0);
    sc.stall_min = jv_int(&jv_get(&rt, "stallTimeoutMinutes"), 0);
    Ok(sc)
}

/// m9a 一档剧本的读法，与 `maarun_parse` 同规矩：缺键一律按"没填"取值，不替夹具猜默认值。
/// 名册那 15 枚与 m9aDir/adb 两闸走 `m9a_scen_core`（生产 ctx 与夹具同一把），其余全是夹具面：
/// 名字/干跑档/game/target/stateDir/两枚可用性/日期读数表/种子/进程表/事件表/live 队列。
/// 一处例外：game/target 的"没填"＝真身写死的那两个字母面（m9a.ps1 三处 `-Game 'r1999' -Target 'mumu'`）。
/// 拒跑闸三枚（都不是判据，是"这档剧本压根不能开"）：m9aDir / adb / days。
fn m9arun_parse(v: &JVal) -> Result<M9aScen, String> {
    let mut sc = m9a_scen_core(v)?;
    sc.name = jv_str(&jv_get(v, "name"));
    sc.dry_run = jv_truthy(&jv_get(v, "dryRun"));
    sc.game = {
        let g = jv_str(&jv_get(v, "game"));
        if g.is_empty() {
            "r1999".to_string()
        } else {
            g
        }
    };
    sc.target = {
        let t = jv_str(&jv_get(v, "target"));
        if t.is_empty() {
            "mumu".to_string()
        } else {
            t
        }
    };
    sc.rescue_available = jv_truthy(&jv_get(v, "rescueAvailable"));
    sc.grab_available = jv_truthy(&jv_get(v, "grabAvailable"));
    sc.state_dir = jv_str(&jv_get(v, "stateDir"));
    for dv in jv_list(&jv_get(v, "days")) {
        sc.days.push((jv_to_d(&jv_get(&dv, "at")), jv_str(&jv_get(&dv, "stamp"))));
    }
    // 日期表为空 ⇒ `day_stamp` 恒给空串 ⇒ 日志文件名恒为 `log-.log`：这档读的是**一个真身永远不会读的文件名**。
    // 这一闸是"跨 00:00"那格的前提（plan §11.39 ⑥-2）：日期由夹具喂，而不是核里现取挂钟。
    if sc.days.is_empty() {
        return Err("days 日期读数表没给：日志名 log-yyyyMMdd.log 每轮重算，没表就等于恒读 log-.log".to_string());
    }
    let seed = jv_get(v, "seed");
    if !matches!(seed, JVal::Null) {
        sc.seed = Some(jv_str(&seed));
    }
    for pr in jv_list(&jv_get(v, "procs")) {
        sc.procs.push((jv_str(&jv_get(pr, "name")), jv_truthy(&jv_get(pr, "up"))));
    }
    for ev in jv_list(&jv_get(v, "events")) {
        let kind = jv_str(&jv_get(&ev, "kind"));
        if !matches!(kind.as_str(), "append" | "grow" | "truncate" | "up" | "down") {
            return Err(format!("剧本里认不出的事件 kind：{}（只有 append/grow/truncate/up/down）", kind));
        }
        let target = {
            let t = jv_str(&jv_get(&ev, "target"));
            if t.is_empty() {
                jv_str(&jv_get(&ev, "proc"))
            } else {
                t
            }
        };
        // m9a 一路只有一本日志，而且文件名每轮换 ⇒ 事件的文件靶子收窄到 `m9alog` 一枚。
        // 写错的靶子名必须**当场拒跑**：`file_of` 认不出的事件是静默丢弃（ark 那两枚固定路径的形状），
        // 拼错一个字母就会让一整档跑成"日志一条没进、判定全按空读数走"的假绿。
        if matches!(kind.as_str(), "append" | "grow" | "truncate") && target != "m9alog" {
            return Err(format!(
                "剧本第 {} 条事件的文件靶子是 {}：m9a 一路只有 m9alog 一枚（gui/asst 是 ark 那两本固定日志）",
                sc.events.len() + 1,
                target
            ));
        }
        sc.events.push(MaaEv {
            at: jv_to_d(&jv_get(&ev, "at")),
            kind: match kind.as_str() {
                "append" => "append",
                "grow" => "grow",
                "truncate" => "truncate",
                "up" => "up",
                _ => "down",
            },
            target,
            text: jv_str(&jv_get(&ev, "text")),
            bytes: jv_int(&jv_get(&ev, "bytes"), 0),
        });
    }
    for lv in jv_list(&jv_get(v, "live")) {
        sc.live.push((jv_str(&jv_get(lv, "verb")), jv_get(lv, "data")));
    }
    Ok(sc)
}

/// 开跑前先把日志目录里**所有** `log-*.log` 清掉，再按剧本摆起点(给了 seed 就整写，空串也算"文件在、零字节"；
/// 没给就删掉 —— "日志压根不在场"是单独一档，真身 `Test-GdPathReady` 那一支)。
/// 落点用 `days` 表在钟 0 处查出的那枚日期 —— 与 old 侧桩浴摆的是同一个文件，两侧起点才相同。
/// ⚠ 这一步"全清"是 m9a 一路独有的形状，ark 那两路没有：m9a 的文件名**带日期**(days 表按钟查)，
/// 一档可能写出两三本，而 seed 只指得住钟 0 那一本；ark 的 gui.log/asst.log 是两枚固定名、seed 逐枚覆盖，
/// 天然不会串档。不清就真串过：三侧共用同一个沙盒、rust 侧先跑，它在 `log-<次日>.log` 里留下的 22 字节
/// 被随后的 PS 侧原样读到 ⇒ PS 比核内**提前一整轮**读到完成行(63s 出 ok、核内 73s)，
/// 看着像"两侧循环差一轮"，其实是夹具没归零 —— 跨天档(m9as17)是本门唯一咬得住这一格的读数。
fn m9arun_seed(sc: &M9aScen) -> Result<(), String> {
    let p = M9aPaths::of(sc);
    if let Ok(rd) = std::fs::read_dir(&p.log_dir) {
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if name.starts_with("log-") && name.ends_with(".log") {
                let _ = std::fs::remove_file(ent.path());
            }
        }
    }
    let path = m9a_daily_path(&p.log_dir, &day_lookup(&sc.days, 0.0));
    match &sc.seed {
        None => {
            let _ = std::fs::remove_file(&path);
        }
        Some(s) => write_at(&path, s.as_bytes(), false)?,
    }
    Ok(())
}

fn render_m9arun_json(items: &[(String, String)]) -> String {
    let cases: Vec<String> = items
        .iter()
        .map(|(name, data)| format!("{{\"name\":{},\"data\":{}}}", jstr(name), jstr(data)))
        .collect();
    format!(
        "{{\"ok\":true,\"why\":\"m9a 运行面的剧本回放：循环骨架在内核里跑，判据照旧真跑 panel-cs；这不是对方行为的读数\",\"contract\":{},\"role\":\"B2-m9a-run\",\"fixture\":\"m9a-run\",\"count\":{},\"cases\":[{}]}}",
        jstr(CONTRACT),
        cases.len(),
        cases.join(",")
    )
}

/// `--m9a-run --script <剧本文件> --panel-cs <判定 exe>`：与 `--maa-run` 同形（一文件多档，逐档回放）。
/// 两处**故意**不同，都记在 plan §11.39：
/// 1) 出口面：m9a 那一路的返回值里 `Duration/Rescued/FailedTasks` 是有料的（ark 侧那三格走的是另一形状），
///    所以末行是 `M9aRet::proj_line()` 的七段而不是 `MaaRet` 的四段。
/// 2) 单档 `Err` 不杀整批：真身 `:347` 那句 `throw`（命令行入口关了、M9A 又在跑）本身就是**一档的正常结论**，
///    把它做成"整批中止"会让门里那一档连投影都看不到。故 Err 落成该档末行 `E|{msg}` 后继续跑下一档；
///    剧本读不动/摆不回起点仍然整批中止（那是夹具自己的形状错，不属场景）。
fn run_m9a_run(script_path: &str, panel_cs: &str, out: &Option<String>) -> i32 {
    let missing = if script_path.is_empty() {
        Some("--script：剧本文件（JSON 数组，一档一项）没给")
    } else if panel_cs.is_empty() {
        Some("--panel-cs：循环骨架每 tick 真跑 decide-m9a-run-tick，缺判定 exe 这一档跑不了")
    } else {
        None
    };
    if let Some(m) = missing {
        emit(
            &format!(
                "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"B2-m9a-run\"}}",
                jstr(&format!("m9a 运行面回放档拒跑：{} —— 缺它这一档没开", m)),
                jstr(CONTRACT)
            ),
            out,
        );
        return 2;
    }
    let text = match std::fs::read_to_string(script_path) {
        Ok(t) => t,
        Err(e) => {
            emit(
                &format!(
                    "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"B2-m9a-run\"}}",
                    jstr(&format!("剧本读不动（{}）：{}", script_path, e)),
                    jstr(CONTRACT)
                ),
                out,
            );
            return 1;
        }
    };
    let root = match jv_parse(&text) {
        Ok(v) => v,
        Err(e) => {
            emit(
                &format!(
                    "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"B2-m9a-run\"}}",
                    jstr(&format!("剧本不是合法 JSON：{}", e)),
                    jstr(CONTRACT)
                ),
                out,
            );
            return 1;
        }
    };
    let mut items: Vec<(String, String)> = Vec::new();
    let mut bad: Option<String> = None;
    for raw in jv_list(&root) {
        let sc = match m9arun_parse(raw) {
            Ok(s) => s,
            Err(e) => {
                bad = Some(format!("剧本第 {} 档读不动：{}", items.len() + 1, e));
                break;
            }
        };
        if let Err(e) = m9arun_seed(&sc) {
            bad = Some(format!("剧本第 {} 档的日志摆不回起点：{}", items.len() + 1, e));
            break;
        }
        let mut host = ScriptHost::new_m9a(panel_cs, &sc);
        match m9a_run(&mut host, &sc) {
            Ok(r) => {
                let mut proj = host.proj.clone();
                proj.push(r.proj_line());
                items.push((sc.name.clone(), proj.join("\u{1f}")));
            }
            Err(e) => {
                let mut proj = host.proj.clone();
                proj.push(format!("E|{}", e));
                items.push((sc.name.clone(), proj.join("\u{1f}")));
            }
        }
    }
    let json = match bad {
        Some(e) => {
            emit(
                &format!(
                    "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"B2-m9a-run\"}}",
                    jstr(&e),
                    jstr(CONTRACT)
                ),
                out,
            );
            return 1;
        }
        None => render_m9arun_json(&items),
    };
    emit(&json, out);
    0
}

// ---- 环境闸（纯判据，可无头自检）

#[derive(Clone, Default)]
struct GateFlags {
    confirm_gui_absent: bool,
    accept_live: bool,
    window_clear: bool,
}

/// 闸的判据。`proc_read_ok=false` 代表连"GUI 在不在场"都没读出来 —— 那种情况**拒跑**，
/// 因为这一档的前提是独占；前提未知不等于前提成立。
fn gate_verdict(flags: &GateFlags, proc_read_ok: bool, forbidden_seen: &[String]) -> (bool, Vec<String>) {
    let mut reasons: Vec<String> = Vec::new();
    if !flags.confirm_gui_absent {
        reasons.push("缺 --i-confirm-gui-absent：独占档要人明确确认 GUI 不在场，探针不替你判断".to_string());
    }
    if !flags.accept_live {
        reasons.push("缺 --i-accept-live：连接段会动助手侧现场（起句柄/加载资源/发连接），要人接下这个后果".to_string());
    }
    if !flags.window_clear {
        reasons.push("缺 --i-window-clear：本轮不能撞 06:00 / 18:00 定时轮，要薄壳查过任务计划程序再说".to_string());
    }
    if !proc_read_ok {
        reasons.push("进程现场读不出来（快照失败）：前提未知按不满足处理，拒跑".to_string());
    }
    for p in forbidden_seen {
        reasons.push(format!("助手进程在场：{} —— 不抢方向盘，也不代杀进程", p));
    }
    (reasons.is_empty(), reasons)
}

/// 真取数：枚举进程名，返回**命中的禁用进程名**（小写比较，只回原名）。
/// 读不出来就返回 Err，让 gate_verdict 走"前提未知 = 拒跑"那一支。
/// 两层"读不出来"都要算：句柄没拿到是一层；句柄拿到了但**一条都没枚举出来**是另一层
/// —— 后者早先是漏的：`Process32FirstW` 失败时循环 0 次、这里回 `Ok(空)`，环境闸照样报
/// `proc_read_ok:true`，于是"在就不抢方向盘"那道护栏从进核起有没有真生效过一次都无从谈起。
/// 任何真实 Windows 上快照都不可能只有 0 条进程，所以 0 条就是硬错，不误伤正常路径。
fn scan_forbidden_processes() -> Result<Vec<String>, String> {
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap.is_null() || snap as isize == INVALID_HANDLE_ISIZE {
        return Err("CreateToolhelp32Snapshot 失败".to_string());
    }
    let mut entry: ProcessEntry32W = unsafe { std::mem::zeroed() };
    entry.dw_size = PROCESSENTRY32W_DWSIZE;
    let mut hits: Vec<String> = Vec::new();
    let mut ok = unsafe { Process32FirstW(snap, &mut entry) } != 0;
    let mut seen = 0usize;
    while ok {
        seen += 1;
        let name = String::from_utf16_lossy(&entry.sz_exe_file);
        let name = name.trim_end_matches('\0').to_string();
        if FORBIDDEN_PROCESSES.iter().any(|f| f.eq_ignore_ascii_case(&name)) && !hits.contains(&name) {
            hits.push(name);
        }
        ok = unsafe { Process32NextW(snap, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snap) };
    if seen == 0 {
        return Err("进程快照一条都没枚举出来(dwSize/结构布局与内核期望不符)".to_string());
    }
    Ok(hits)
}

// ---- 调用序列（把"要调哪些"从流程里剥出来，好让自检能判它）

/// 连接段在某档下**允许**调用的符号序列。三条硬限制写死在这里，不靠文案：
///   1) 只出 `kind.callable` 里的名字（没签名源的一个都不出）；
///   2) `shape` / `resource` / `adb-live` 三档永远不出 `AsstStart` / `MaaTaskerPostTask`
///      —— 这三档的"最小任务"只到 append + set_params（排队但不启动）。第四档 `adb-live-run`
///      （B-2e 乙-2，10-13 开闸）是唯一允许启动队列的一档，判据只住在下面 `runs_task` 那一处；
///   3) `w32cu` 档是空序列（那枚没有本地签名源，连接段对它只查导出）。
/// 真碰设备的有两档：`adb-live`（连上就算）与 `adb-live-run`（连上并且把队列跑完）。
/// 资源段不算：它只让 DLL 读自己的目录，不碰任何现场。
/// 这条判据被**三处共用**（计划装配 / 实际调用 / 参数硬门），所以必须是一个函数而不是一串比较 ——
/// 三处各写各的 `scope != "shape"`，就会有一处把"读事件形状"悄悄升级成"连设备"。
///
/// 第四档的名字**不在这里写第二遍**：它往下走 `runs_task`。理由是那条"一枚档名只剩一处 spelling"的
/// needle 门（§7 那组），它管的就是"装配改了执行不会跟着改"这一族 —— 把 `adb-live-run` 在这里也点一次，
/// 判据本身就变成两处 spelling，下一批改档名时会有一处漏掉。`adb-live` 那枚字面量维持原样，
/// 所以既有三档读数逐字可比。
fn reaches_connect(scope: &str) -> bool {
    scope == "adb-live" || runs_task(scope)
}

/// "这一档到底要不要把任务队列真跑起来"的唯一判据（B-2e 乙-2）。
///
/// 它与 `reaches_connect` 是**两回事**，不是它的子集写第二遍：`adb-live` 那档一字未变 ⇒ 既有读数
/// 继续可比，新增的一档才带真副作用。字符串判据同样**不许做前缀匹配**（自检那几格钉着），
/// 否则 `adb-live` 会被判成跑任务 —— 那就是把"看着只是连一下"的一轮偷偷升级成"动了账号"的一轮。
///
/// 这一跳的形状是被签名源逼出来的，不是风格选择：遗留侧只有**阻塞式** `AsstStart`
/// （asst.py 逐字核实 restype=c_bool、argtypes 一枚 void_p，async 变体压根没进我们的 callable 清单），
/// 它占住调用线程直到队列散场 ⇒ 执行段把它丢进工作线程、主线程只等 `--timeout-ms` 那一个窗口。
/// 无界等待正是 §6 那颗 stdio 雷的形状（整轮卡在收尾之前），所以这一档**必须**带上界。
fn runs_task(scope: &str) -> bool {
    scope == "adb-live-run"
}

/// CLI 认的档名册 —— 它只回答"这个名字存不存在"，射程与副作用**一律由上面两枚判据说话**。
/// 这里刻意不写成 `v != "shape" && v != ...` 的比较链：那会把第四档的档名在源码里点第二遍，
/// 而"一枚档名只剩一处 spelling"那条 needle 门管的就是"装配改了执行不会跟着改"这一族。
const KNOWN_SCOPES: &[&str] = &["shape", "resource", "adb-live", "adb-live-run"];

/// config 的缺省**按侧给**，唯一口径：
/// mf 侧 `MaaAdbControllerCreate` 的第 5 枚要 **JSON 对象** —— 非对象字面量会被框架在建控制器处当场判掉
/// （10-03 首跑对方原话 `Parse config failed, or not object, invalid config: Default`）；
/// ark 侧 `AsstConnect` 的第 4 枚是遗留侧的连接配置名，缺省 `General`。
/// 壳侧原先"空就填 Default"那句已删、探针原先写死的 `General` 也撤了：那两处对 mf 都是错值。
fn default_config(kind_name: &str) -> &'static str {
    if kind_name == "mf" {
        "{}"
    } else {
        "General"
    }
}

fn connect_plan(kind_name: &str, scope: &str, load_resource: bool, minimal_task: bool) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    // shape 档：一枚函数都不调（版本自报除外）—— 它读的是"能不能加载、导出了什么"，不是"能不能连上"。
    if scope == "shape" {
        return v;
    }
    match kind_name {
        "ark" => {
            // 顺序在 10-02 **重排过一次**，重排的理由比顺序本身更要紧，所以钉在这里：
            // 上游 v6.18.0 的 `AsstCaller.cpp` 里 `AsstCreateEx` 开头就是 `if (!inited()) return nullptr;`，
            // 而 `inited()` 读的是 `ResourceLoader::loaded()` —— 遗留侧"能不能建实例"**以"资源已加载"为前置**。
            // R0 原先把 `AsstLoadResource` 排在 `AsstCreateEx` 之后，两遍 ark×resource 实跑因此恒得空句柄；
            // 又因为在空句柄处提前 return，资源那一步**压根没执行**，却照样报 CONNECT-OK（假绿，读数见 §9.4）。
            v.push("AsstSetUserDir");
            if load_resource {
                v.push("AsstLoadResource");
            }
            v.push("AsstCreateEx");
            if reaches_connect(scope) {
                v.push("AsstConnect");
            }
            if minimal_task && reaches_connect(scope) {
                v.push("AsstAppendTask");
                v.push("AsstSetTaskParams");
            }
            // 第四档才真启动（B-2e 乙-2）。`AsstStop` 一并排进计划，但它不是"总要调"的一条而是
            // **收尾备选**：正常跑完时执行段自己写一条 `AsstStop=not-needed` 留痕，
            // `fill_not_attempted` 对留过痕的不碰 —— 于是"没需要"与"压根没试"分得开。
            // `AsstRunning` 刻意**不**排进来：真启动那一跳堵在工作线程里，主线程并发去读运行标志
            // 是本档没有依据的一种猜想（签名源只给了 start/stop/running 三枚同族声明，没说能并发），
            // 而"队列跑完没有"这件事本档由事件流的终态判据来证，不靠多调一枚符号装样子。
            if minimal_task && runs_task(scope) {
                v.push("AsstStart");
                v.push("AsstStop");
            }
            v.push("AsstDestroy");
        }
        "mf" => {
            v.push("MaaResourceCreate");
            v.push("MaaResourceAddSink");
            if load_resource {
                v.push("MaaResourcePostPipeline");
            }
            v.push("MaaResourceLoaded");
            if reaches_connect(scope) {
                v.push("MaaAdbControllerCreate");
                v.push("MaaControllerAddSink");
                v.push("MaaControllerPostConnection");
                v.push("MaaControllerConnected");
                v.push("MaaControllerDestroy");
            }
            v.push("MaaResourceDestroy");
        }
        _ => {}
    }
    v
}

fn plan_is_safe(kind: &Kind, plan: &[&'static str], scope: &str) -> Option<String> {
    for name in plan {
        if !kind.callable.iter().any(|c| c == name) {
            return Some(format!("调用序列里混进了无签名源的符号：{}", name));
        }
    }
    for banned in ["MaaTaskerPostTask", "AsstAttachWindow", "AsstAsyncAttachWindow"] {
        if plan.contains(&banned) {
            return Some(format!("调用序列里出现了本轮禁止的 {}：R0 不执行任务、不挂窗口", banned));
        }
    }
    // 启动与停止这两枚**只对第四档开放**，判据仍只住 `runs_task` 一处（与上面那条射程判据同构）。
    for gated in ["AsstStart", "AsstStop"] {
        if plan.contains(&gated) && !runs_task(scope) {
            return Some(format!("调用序列里出现了 {}：非 adb-live-run 档不执行任务", gated));
        }
    }
    // 反手还有一条，而且它才是这一档要紧的那条：**档名承诺了跑任务，计划里却没有启动符号**
    // = 把"跑"悄悄退成"排队"，读数看着像跑过、其实一步没动 —— "ok 从不证 X"同族第五种。
    // 宁可退 2 出一条带归因的信封，不许印一句"任务已启动"。
    // 这条顺带把 mf 侧挡死了：框架侧的任务提交（`MaaTaskerPostTask`）没有签名源、也仍在禁止列里，
    // 所以 `--kind mf --scope adb-live-run` 得到的是"这一档没排启动符号"，而不是半跑半不跑的混合形状。
    if runs_task(scope) && !plan.contains(&"AsstStart") {
        return Some("adb-live-run 档的计划里没有 AsstStart：这一档的存在理由就是把队列跑起来（不排队跑=假绿）".to_string());
    }
    None
}

// ---- 签名（照装包里的 python 绑定逐条对出来的，两条 ABI 各一套）

type AsstSetUserDirFn = unsafe extern "C" fn(*const i8) -> u8;
type AsstLoadResourceFn = unsafe extern "C" fn(*const i8) -> u8;
type AsstCreateExFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
type AsstDestroyFn = unsafe extern "C" fn(*mut c_void);
type AsstConnectFn = unsafe extern "C" fn(*mut c_void, *const i8, *const i8, *const i8) -> u8;
type AsstAppendTaskFn = unsafe extern "C" fn(*mut c_void, *const i8, *const i8) -> i32;
type AsstSetTaskParamsFn = unsafe extern "C" fn(*mut c_void, i32, *const i8) -> u8;
// 第四档真启动那两枚（B-2e 乙-2）。签名源＝装包 Python 绑定里 start/stop 那两段声明：
// restype 都是 c_bool（这里照 AsstConnectFn 的既有口径收成 u8，非零即真）、argtypes 一枚 void_p。
// 遗留侧**没有** async 启动的声明进我们的 callable 清单，所以 start 是阻塞的 —— 这条决定了
// 执行段必须把它丢到工作线程上，而不是在主线程里直接调。
type AsstStartFn = unsafe extern "C" fn(*mut c_void) -> u8;
type AsstStopFn = unsafe extern "C" fn(*mut c_void) -> u8;
type MaaResourceCreateFn = unsafe extern "C" fn() -> *mut c_void;
type MaaResourceDestroyFn = unsafe extern "C" fn(*mut c_void);
type MaaSinkAddFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i64;
type MaaResourcePostPipelineFn = unsafe extern "C" fn(*mut c_void, *const i8) -> i64;
type MaaResourceLoadedFn = unsafe extern "C" fn(*mut c_void) -> u8;
type MaaAdbControllerCreateFn = unsafe extern "C" fn(*const i8, *const i8, u64, u64, *const i8, *const i8) -> *mut c_void;
type MaaControllerDestroyFn = unsafe extern "C" fn(*mut c_void);
type MaaControllerPostConnFn = unsafe extern "C" fn(*mut c_void) -> i64;
type MaaControllerConnectedFn = unsafe extern "C" fn(*mut c_void) -> u8;

struct ConnectOpts {
    dir: String,
    user_dir: String,
    scope: String,
    adb: String,
    address: String,
    config: String,
    // mf 侧第六枚：上游 Python 绑定逐字核实 = `agent_path`（MaaAgentBinary 目录），
    // 原先硬塞 `"{}"` 是拿非路径字面量当路径喂（10-03 ②）。
    agent_path: String,
    load_resource: bool,
    resource_path: String,
    minimal_task: String,
    timeout_ms: u64,
    screencap_mask: u64,
    input_mask: u64,
    do_free: bool,
    show_path: bool,
}

struct ConnectReport {
    ok: bool,
    why: String,
    kind: String,
    scope: String,
    dir: String,
    user_dir: String,
    loaded: bool,
    resource_load: &'static str,
    resource_rel: String,
    version: Option<String>,
    gate_reasons: Vec<String>,
    calls: Vec<String>,
    events: Vec<Event>,
    main_thread: u32,
    cb_panics: u32,
    waited_ms: u64,
    /// 进连接段那一刻的 FILETIME，`waited_ms` 的基线（**不进信封**：它是口径不是读数）。
    entry_filetime: u64,
    freed: bool,
}

impl ConnectReport {
    fn to_json(&self) -> String {
        let mut p: Vec<String> = Vec::new();
        p.push(format!("\"ok\":{}", if self.ok { "true" } else { "false" }));
        p.push(format!("\"why\":{}", jstr(&self.why)));
        p.push(format!("\"contract\":{}", jstr(CONTRACT)));
        p.push(format!("\"role\":\"R0-connect\""));
        p.push(format!("\"kind\":{}", jstr(&self.kind)));
        p.push(format!("\"scope\":{}", jstr(&self.scope)));
        p.push(format!("\"dir\":{}", jstr(&self.dir)));
        p.push(format!("\"user_dir\":{}", jstr(&self.user_dir)));
        p.push(format!("\"loaded\":{}", if self.loaded { "true" } else { "false" }));
        p.push(format!("\"resource_load\":{}", jstr(self.resource_load)));
        p.push(format!("\"resource_rel\":{}", jstr(&self.resource_rel)));
        p.push(format!("\"self_reported_version\":{}", jopt_str(&self.version)));
        p.push(format!("\"gate_refusals\":{}", jarr(&self.gate_reasons)));
        p.push(format!("\"calls\":{}", jarr(&self.calls)));
        p.push(format!("\"event_count\":{}", self.events.len()));
        p.push(format!("\"main_thread\":{}", self.main_thread));
        p.push(format!("\"callback_panics\":{}", self.cb_panics));
        p.push(format!("\"waited_ms\":{}", self.waited_ms));
        p.push(format!("\"freed\":{}", if self.freed { "true" } else { "false" }));
        let ev: Vec<String> = self
            .events
            .iter()
            .map(|e| {
                format!(
                    "{{\"seq\":{},\"abi\":{},\"code\":{},\"message\":{},\"thread\":{},\"rel_ms\":{},\"raw_len\":{},\"raw_hash\":{},\"details\":{}}}",
                    e.seq,
                    jstr(e.abi),
                    jstr(&e.code),
                    jstr(&e.message),
                    e.thread,
                    e.rel_ms,
                    e.raw_len,
                    e.raw_hash,
                    jstr(&e.details)
                )
            })
            .collect();
        p.push(format!("\"events\":[{}]", ev.join(",")));
        format!("{{{}}}", p.join(","))
    }
}

fn sym(hmod: *mut c_void, name: &str) -> *mut c_void {
    let w = wide_cstr(name);
    unsafe { GetProcAddress(hmod, w.as_ptr()) }
}

/// 把"某个符号在不在、拿到了没"记进 calls，同时给回函数指针。
fn take(r: &mut ConnectReport, hmod: *mut c_void, name: &str) -> Option<*mut c_void> {
    let p = sym(hmod, name);
    if p.is_null() {
        r.calls.push(format!("{}=absent", name));
        None
    } else {
        r.calls.push(format!("{}=resolved", name));
        Some(p)
    }
}

/// 连接段资源落点的**唯一口径**：给了 `--resource-path` 就原样用，没给才回落到各侧旧默认。
/// 这条默认是 09-30 首次实跑纠正出来的，所以**默认一个字都没改**，只是把"能不能指定"开出来：
/// mf 侧把整个 `<dir>\resource` 当一个 Pipeline 喂进去，框架自己按"每个渠道目录各有一份同名 pipeline"
/// 判成重复键而**整批失败**（`key already exists [key=Close1999]` → `parse_config failed` → `load_all_json failed`），
/// 而同一台机器生产侧的框架日志当天 `key already exists` **0 命中**、加载行全落在 `resource/base/**`
/// —— 说明生产不是整目录喂的。落点是**我方选的**，不是框架的既成事实，读数必须能指名喂的是哪条。
fn effective_resource(kind_name: &str, dir: &str, over: &str) -> String {
    if !over.is_empty() {
        return over.to_string();
    }
    let root = dir.trim_end_matches(|c| c == '/' || c == '\\');
    match kind_name {
        // 遗留侧 AsstLoadResource 吃**装包根**：这句原先是"按惯例、未经核验"的推断，10-02 由上游源码
        // 逐字证实 —— `AsstCaller.cpp` 里它就是 `auto res_path = os_path / "resource"; ... load(res_path)`，
        // 传装包根是对的，传 <装包根>\resource 反而会找成 resource\resource。**机制已证，读数仍欠**：
        // 这一档此前恒死在建实例那一步（前置门没开，见 connect_plan 的 ark 段），复跑后才由 resource_load 说。
        // framework 侧的 PostPipeline 吃**目录本身**，所以回落值就是 <dir>\resource（这条已被 09-30 首跑实测判过 failed）。
        "ark" => root.to_string(),
        _ => format!("{}\\resource", root),
    }
}

/// 把落点写进机读面时**只写相对装包根的形态**：`resource\base`。
/// 不在装包目录下就只留最后两段 —— 带盘符的绝对路径不进信封，与"装包只给标签"同一条纪律。
fn resource_rel(dir: &str, rpath: &str) -> String {
    if rpath.is_empty() {
        return String::new();
    }
    let sep = |c: char| c == '/' || c == '\\';
    let root = dir.trim_end_matches(sep);
    let given = rpath.trim_end_matches(sep);
    // 落点就是装包根本身（遗留侧的默认）：相对形态是空串，别让下面那段把盘符拼出来
    if given == root {
        return String::new();
    }
    if given.len() > root.len() && &given[..root.len()] == root {
        return given[root.len()..].trim_start_matches(sep).replace('/', "\\");
    }
    // 落在装包目录之外：只留最后两段，且**含 `:` 的一段都不许出现**（那是盘符，属本机路径泄漏面）
    let parts: Vec<&str> = given.split(sep).filter(|s| !s.is_empty() && !s.contains(':')).collect();
    if parts.len() >= 2 {
        return format!("{}\\{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    }
    parts.last().copied().unwrap_or_default().to_string()
}

/// "资源到底加载成没成"从 calls 里读，**不再靠 ok 兼职**：
/// 首跑那次 `ok=true` 与 `MaaResourceLoaded=0` 是同时成立的，因为 ok 的判据本来就只是
/// "DLL 进得来 + 回调没炸"。把资源面单拆出来，是为了让 CONNECT-OK 这个词没法掩盖资源失败。
fn resource_outcome(calls: &[String], requested: bool) -> &'static str {
    if !requested {
        return "skipped";
    }
    for c in calls {
        for tag in ["MaaResourceLoaded=", "AsstLoadResource="] {
            if let Some(v) = c.strip_prefix(tag) {
                if v == "1" {
                    return "ok";
                }
                if v == "0" {
                    return "failed";
                }
                // "这一步压根没执行"与"执行了但没回执"是两件事，混成一谈就会把顺序缺陷读成对端沉默。
                if v == "not-attempted" {
                    return "not-attempted";
                }
            }
        }
    }
    "not-reported"
}

/// 空句柄的**归因**：只从 calls 里已有的标签读，不在这里第二次问对方、也不猜。
/// 上一版这里只有一句"返回空句柄，后面一步都不做"，读数看着像"对方不给句柄"；
/// 实际是遗留侧的前置门没开（`inited()` 要看资源面），归因写出来才知道该修哪一侧。
fn create_null_reason(calls: &[String], load_requested: bool) -> String {
    let out = resource_outcome(calls, load_requested);
    if load_requested && out != "ok" {
        return format!(
            "AsstCreateEx 返回空句柄：遗留侧它以「资源已加载」为前置（上游 inited()），而这一轮资源面是 {} —— 后面一步都不做",
            out
        );
    }
    "AsstCreateEx 返回空句柄：没让它先载资源时这是必然（前置门同上），后面一步都不做".to_string()
}

/// "连接段到底起步了没有"也从 calls 里读，与 resource_outcome 同构、不另起一套判据：
/// 计划非空时那几枚句柄标签只要**没拿到句柄**就算没起步 —— `null`（调用返回空）、
/// `absent`（符号缺失，等于没法起步）、`not-attempted`（计划里排着却没试到）三种同属。
/// shape 档与空计划压根不会有这些条，所以它对"只读形状"那两档天然无影响
/// —— 把覆盖面加宽不等于把别的档带红。
/// 10-03 加宽一格：`MaaAdbControllerCreate` 原先不在覆盖面里，于是 `ok:true` 与"控制器返回 null"同现、
/// 末行照样印 CONNECT-OK（首跑实读）—— 这是"ok 从不证 X"同族的第四次。
fn handle_lost(calls: &[String]) -> bool {
    calls.iter().any(|c| {
        ["AsstCreateEx=", "MaaResourceCreate=", "MaaAdbControllerCreate="].iter().any(|tag| {
            c.strip_prefix(*tag).map(|v| v == "null" || v == "absent" || v == "not-attempted").unwrap_or(false)
        })
    })
}

/// 计划里排着、执行段**压根没走到**的符号，逐条补 `=not-attempted`。
/// "没执行"与"执行了没回执"分家是 10-02 立的规矩；执行段提前退出（空句柄）时，
/// 后面那几枚动作原先在 calls 里**静默消失**（10-03 首跑：控制器面排了 5 枚、只出现第 1 枚）——
/// 补法从 plan 现取、不另写一份清单，清单一旦有第二份就会漂。
/// 留过痕的（`=resolved` / `=absent` / 值）一律不碰：符号缺失是更响的一枚读数，不许被盖成"没试"。
fn fill_not_attempted(plan: &[&'static str], calls: &mut Vec<String>) {
    for sym in plan {
        let tag = format!("{}=", sym);
        if !calls.iter().any(|c| c.starts_with(&tag)) {
            calls.push(format!("{}=not-attempted", sym));
        }
    }
}

fn connect_run(kind: &'static Kind, o: &ConnectOpts, gate_reasons: Vec<String>) -> ConnectReport {
    let mut r = ConnectReport {
        ok: false,
        why: String::new(),
        kind: kind.name.to_string(),
        scope: o.scope.clone(),
        dir: dir_label(&o.dir, o.show_path),
        user_dir: dir_label(&o.user_dir, o.show_path),
        loaded: false,
        resource_load: "not-attempted",
        resource_rel: String::new(),
        version: None,
        gate_reasons,
        calls: Vec::new(),
        events: Vec::new(),
        main_thread: unsafe { GetCurrentThreadId() },
        cb_panics: 0,
        waited_ms: 0,
        entry_filetime: filetime_now(),
        freed: false,
    };

    // 基线在**进函数**这一步就落（判据的解释在 `rel_ms` 的注释里）：不叫这一下，
    // T0 要等首枚事件才初始化，`rel_ms` 第一枚就永远是 0 那枚假读数。
    // 这条 `let _ =` 是**它唯一的调用位**（自检按字面钉），挪走或复制都会让 rel_ms 的口径重新分家。
    let _ = rel_ms();

    if !r.gate_reasons.is_empty() {
        r.why = "闸没过：这一档什么都不做，DLL 一个都不加载、一个字都不写".to_string();
        return stamp(r);
    }
    if kind.callable.is_empty() {
        r.why = "这一侧的连接段没有任何有签名源的函数，只准查导出、不准调".to_string();
        return stamp(r);
    }
    if !std::path::Path::new(&o.dir).is_dir() {
        r.why = "装包目录不在场（前置不满足，不是代码错）".to_string();
        return stamp(r);
    }
    if std::fs::create_dir_all(&o.user_dir).is_err() {
        r.why = "写入面建不出来：连接段拒绝在落盘收窄失败的情况下继续".to_string();
        return stamp(r);
    }

    let plan = connect_plan(kind.name, &o.scope, o.load_resource, !o.minimal_task.is_empty());
    if let Some(bad) = plan_is_safe(kind, &plan, &o.scope) {
        r.why = bad;
        return stamp(r);
    }

    // 加载 DLL（和只读段同一套候选路径 + 同一枚旗标）
    let mut hmod: *mut c_void = std::ptr::null_mut();
    let mut chosen: Option<String> = None;
    for cand in kind.dll_candidates {
        let full = format!("{}\\{}", o.dir.trim_end_matches(|c| c == '/' || c == '\\'), cand);
        if !std::path::Path::new(&full).exists() {
            continue;
        }
        let w = wide(&full);
        let h = unsafe { LoadLibraryExW(w.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
        if !h.is_null() {
            chosen = Some(cand.to_string());
            hmod = h;
            break;
        }
    }
    let Some(cand) = chosen else {
        r.why = "目标 DLL 加载不了，连接段停在这里（不重试、不猜别的候选）".to_string();
        return stamp(r);
    };
    r.loaded = true;
    r.calls.push(format!("dll={}", dir_label(&cand, true)));

    if !kind.version_fn.is_empty() {
        if let Some(p) = take(&mut r, hmod, kind.version_fn) {
            let f: VersionFn = unsafe { std::mem::transmute(p) };
            let got = unsafe { f() };
            if !got.is_null() {
                r.version = Some(unsafe { std::ffi::CStr::from_ptr(got) }.to_string_lossy().into_owned());
            }
        }
    }

    let wrote = unsafe {
        // 执行段**不看档名，看计划**：计划是空表就一枚都不调。
        // 如果这里再按档名自己比一次，就有了两处各说各话的射程判据 ——
        // 上一版就是这么写的，装配改了、执行没改，"只看形状"会被悄悄跑成"整条连接段"。
        if plan.is_empty() {
            r.calls.push("plan=empty=no_call".to_string());
            "这一档的计划是空表：只加载与查导出，一枚函数都没调（版本自报除外）".to_string()
        } else if kind.name == "ark" {
            run_ark(hmod, o, &mut r)
        } else if kind.name == "mf" {
            run_mf(hmod, o, &mut r)
        } else {
            "这一档没有可用的连接段实现".to_string()
        }
    };
    // `waited_ms` 不在这里算：它由退出位唯一的 setter `stamp` 从"进连接段那一刻"起算。
    // 原先这里拿的是执行段自己的起点，于是提前 return 的六条路（闸没过、无签名源、装包目录不在场、
    // 写入面建不出来、计划不安全、DLL 加载不了）交出的 `waited_ms` 恒为 0 ——
    // 而 0 只证"没走到赋值那行"，不证"没等"，这是一枚会把人引错的读数（10-02 实测踩实）。
    r.why = wrote;
    // 10-03 ③：执行段提前退出时，计划里没走到的符号在 calls 里静默消失 —— 逐条补 not-attempted。
    // 放置刻意在下面 resource_load 计算之前：补出来的标签和既有标签走同一处读取（resource_outcome），
    // 这样早退路上资源面也不会被含糊成 not-reported。
    if !plan.is_empty() {
        fill_not_attempted(&plan, &mut r.calls);
    }

    if let Ok(g) = events().lock() {
        r.events = g.clone();
    }
    r.cb_panics = CB_PANICS.load(Ordering::SeqCst);
    // 资源面单拆一格：落点由 effective_resource 那一处算（不另起炉灶），这里只把结果投影进信封。
    if o.load_resource {
        r.resource_rel = resource_rel(&o.dir, &effective_resource(kind.name, &o.dir, &o.resource_path));
    }
    r.resource_load = resource_outcome(&r.calls, o.load_resource);
    if o.do_free {
        if worker_unresolved(&r.calls) {
            // 工作线程还堵在（或死在）对方 DLL 里面时，FreeLibrary 是**第二次凭空进入对方的栈**：
            // 句柄那一层已经跳过 `AsstDestroy` 了，模块这一层也必须跳过。读数如实留一条"因现场没清而没做"，
            // 不假装释放成功（`freed` 维持 false）—— 进程退出交内核收，这是本档接受的那笔账。
            r.calls.push("FreeLibrary=skipped-worker-alive".to_string());
        } else {
            let c = unsafe { FreeLibrary(hmod) };
            r.freed = c != 0;
            r.calls.push(format!("FreeLibrary={}", if r.freed { "true" } else { "false" }));
        }
    }
    // ok 的覆盖面在 10-02 加过一次、10-03 又加宽一格：它原先只证"DLL 进得来 + 回调没炸"，于是那两遍
    // "空句柄、后面一步都不做"照样换到 CONNECT-OK —— 这是"ok 从不证 X"同族的第三次；10-03 首跑又印出
    // 了第四次（`ok:true` 与 `MaaAdbControllerCreate=null` 同现），所以第三件扩成**计划里那几枚句柄**：
    // 任一没拿到（null / absent / not-attempted）⇒ ok 必须是 false。
    // 判据仍只住一处（只从 calls 里已有的标签读，与 resource_outcome 同构），信封字段一枚没加。
    // B-2e 乙-2 开闸后加第五件：任务面没收到终局（`task_stalled`）也不算 ok。前三档压根出不了
    // 那几枚标签 ⇒ 既有三档读数逐字不受影响（同上面"覆盖面加宽 vs 把别的档带红"那条分界）。
    r.ok = r.calls.iter().any(|c| c.starts_with("dll="))
        && r.cb_panics == 0
        && !handle_lost(&r.calls)
        && !task_stalled(&r.calls);
    stamp(r)
}

unsafe fn run_ark(hmod: *mut c_void, o: &ConnectOpts, r: &mut ConnectReport) -> String {
    let ud = cstr_bytes(&o.user_dir);
    let mut why = "ok".to_string();
    // 不预先赋 null：拿到入口就往下走，拿不到直接 return，赋了反而是条死值（编译器报这个）
    let core: *mut c_void;

    if let Some(p) = take(r, hmod, "AsstSetUserDir") {
        let f: AsstSetUserDirFn = std::mem::transmute(p);
        let got = f(ud.as_ptr());
        r.calls.push(format!("AsstSetUserDir={}", got));
    }

    // 载资源**排在建实例之前**，且这里的 `o.load_resource` 不是自己新算的判据 ——
    // 它和 connect_plan 里那一句是同一枚开关：计划里有这条，执行段才做这条。
    // 上游 `AsstLoadResource(path)` 内部会自己 `path / "resource"`，所以遗留侧的落点**就是装包根**，
    // 这句原先只能写"按惯例、未经实跑核验"，现在由对方源码逐字证实（同一条还管着能不能起步，见上）。
    if o.load_resource {
        if let Some(p) = take(r, hmod, "AsstLoadResource") {
            let d = cstr_bytes(&effective_resource("ark", &o.dir, &o.resource_path));
            let f: AsstLoadResourceFn = std::mem::transmute(p);
            let got = f(d.as_ptr());
            r.calls.push(format!("AsstLoadResource={}", got));
        }
    }

    if let Some(p) = take(r, hmod, "AsstCreateEx") {
        let f: AsstCreateExFn = std::mem::transmute(p);
        core = f(ark_cb as *const () as *mut c_void, std::ptr::null_mut());
        r.calls.push(format!("AsstCreateEx={}", if core.is_null() { "null" } else { "handle" }));
        if core.is_null() {
            // 提前 return 时资源面可能压根没被执行过。不给它补一条标签，resource_outcome 就会把
            // "没做"说成"做了没报"（not-reported）—— 10-02 那两遍实跑印的就是这个错话。
            // 补标签的判据仍然只有 resource_outcome 一处，这里只负责如实留一条"没尝试"。
            if o.load_resource && resource_outcome(&r.calls, true) == "not-reported" {
                r.calls.push("AsstLoadResource=not-attempted".to_string());
            }
            return create_null_reason(&r.calls, o.load_resource);
        }
    } else {
        return "拿不到 AsstCreateEx：这一侧的连接段没有可起步的入口".to_string();
    }

    if reaches_connect(&o.scope) {
        if let Some(p) = take(r, hmod, "AsstConnect") {
            let a = cstr_bytes(&o.adb);
            let ad = cstr_bytes(&o.address);
            let cf = cstr_bytes(&o.config);
            let f: AsstConnectFn = std::mem::transmute(p);
            let got = f(core, a.as_ptr(), ad.as_ptr(), cf.as_ptr());
            r.calls.push(format!("AsstConnect={}", got));
            if got == 0 {
                why = "AsstConnect 返回 false：连接没成，事件面在下面".to_string();
            }
        }
        if !o.minimal_task.is_empty() {
            if let Some(p) = take(r, hmod, "AsstAppendTask") {
                let t = cstr_bytes(&o.minimal_task);
                let j = cstr_bytes("{}");
                let f: AsstAppendTaskFn = std::mem::transmute(p);
                let id = f(core, t.as_ptr(), j.as_ptr());
                r.calls.push(format!("AsstAppendTask={}", id));
                if id != 0 {
                    if let Some(q) = take(r, hmod, "AsstSetTaskParams") {
                        let j2 = cstr_bytes("{\"stage\":[]}");
                        let g: AsstSetTaskParamsFn = std::mem::transmute(q);
                        let got = g(core, id, j2.as_ptr());
                        r.calls.push(format!("AsstSetTaskParams={}", got));
                    }
                    // 第四档真启动（B-2e 乙-2，10-13 开闸）。**前三档走到这里就是原样**：
                    // 排队 + 设参、不调 AsstStart —— "R0 的连接段不执行任务，所以排队读只到回执为止"
                    // 那句话对 shape / resource / adb-live 三档仍然逐字成立，本刀只加了一枚 `runs_task`
                    // 判据，没有把整段重写。
                    // 排队回执为 0（任务名不被认 / 没连上）时不启动：计划里那两枚由 fill_not_attempted
                    // 补成 `not-attempted`，"没试"与"试了没成"仍然分得开。
                    if runs_task(&o.scope) {
                        let (clean, receipt, stop_issued) = start_task(r, hmod, core, o);
                        if receipt == Some(false) {
                            why = "AsstStart 返回 false：这一侧压根没让队列起步（跑没跑成看事件流终态，**别拿信封的 ok 当它**）".to_string();
                        }
                        if !clean {
                            return "任务面没收束：工作线程还堵在 AsstStart 里，本轮不销毁句柄、不 FreeLibrary（进程退出交内核收）".to_string();
                        }
                        // 10-13 两枪真跑（12:35 / 12:42，同形）把本刀带的一条**前提**当场打掉了：对方日志里
                        // `asst::Assistant::start | enter` 与 `| leave, 0 ms` 落在**同一毫秒**，而任务链在
                        // 另一根线程上继续跑，一直跑到我方销毁它为止。⇒ 遗留侧的 `AsstStart` **不阻塞**，
                        // "回执 1 + worker=joined" 只证**启动调用回了话**，一并不证队列散场。
                        // 所以这一档不许等调用，得等**事件流里的终态**：判据仍只有 `terminal_of` 一处
                        // （与 `--events` 那一档同一条，不在这里另起第二套成败口径），上界照用 `--timeout-ms`。
                        // 等不到 = 队列还在跑，此时**真叫停**（只在上头那一枪没打过时才叫，见 `stop_issued`）
                        // 并再给一枚短窗收 `TaskChainStopped` 的回执；
                        // 连回执都收不到才留 `terminal=none`，由 `task_stalled` 把 ok 否掉 ——
                        // 原先那句 `AsstStart` 回执一到就写的 `AsstStop=not-needed` 是把"启动调用回了话"
                        // 说成"它跑完了"，已按实测撤掉，这条留痕改由下面的终态判据给。
                        // 同一枪还打掉**第二条**前提：`terminal=` 只给"末条命中"，而真事件流里
                        // `TaskChainError` 之后还会到一条 `AllTasksCompleted`（那条的本义是队列散场），
                        // 于是 `terminal=succeeded` 与"这轮跑砸了"能同现 ⇒ 这里同时留一枚 `terminal_seen=`
                        // 命中册（`terminal_hits`），把"这一轮见过哪几枚终态、什么序"如实交出来。
                        // 判据仍只有 `terminal_of` 一处，`--events` 的 `verdict` 一字未动。
                        // 反面也写清楚：`AsstStart` 符号缺失时**不进这个等待**。那一形的队列压根没建起来，
                        // 等满一个上界只会多造一条 `terminal=none` 的假读数 —— 那种形状由
                        // `AsstStart=absent` 自己说话（它已在 task_stalled 名单里），而计划里那枚 `AsstStop`
                        // 由 fill_not_attempted 补成 `not-attempted`，"没起步"与"起步了没下文"仍然分得开。
                        if receipt.is_some() || stop_issued {
                            match ring_terminal_until(r.main_thread, o.timeout_ms) {
                                Some((v, seen)) => {
                                    r.calls.push(format!("terminal={}", v));
                                    r.calls.push(format!("terminal_seen={}", seen.join(",")));
                                    // 队列自己散场了才不需要 stop。这条留痕**必须写**：计划里排着 AsstStop，
                                    // 不写就会被 fill_not_attempted 补成"压根没试"，而真相是"没需要" ——
                                    // 两枚读数的意思相反。
                                    r.calls.push("AsstStop=not-needed".to_string());
                                }
                                None => {
                                    if !stop_issued {
                                        if let Some(q) = take(r, hmod, "AsstStop") {
                                            let g: AsstStopFn = std::mem::transmute(q);
                                            let got = g(core);
                                            r.calls.push(format!("AsstStop={}", got));
                                        }
                                    }
                                    match ring_terminal_until(r.main_thread, STOP_GRACE_MS) {
                                        Some((v, seen)) => {
                                            r.calls.push(format!("terminal={}", v));
                                            r.calls.push(format!("terminal_seen={}", seen.join(",")));
                                        }
                                        None => r.calls.push("terminal=none".to_string()),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 前三档的收尾窗**一字未动**（同一个函数、同一条 3 秒上限）⇒ 既有读数逐字可比。
    // 第四档不走这里：它上面已经按上界等过终态了，再固定睡三秒只会把"等到了"和"等死了"糊成一枚读数。
    if !runs_task(&o.scope) {
        sleep_with_events(o.timeout_ms);
    }
    if let Some(p) = take(r, hmod, "AsstDestroy") {
        let f: AsstDestroyFn = std::mem::transmute(p);
        f(core);
        r.calls.push("AsstDestroy=done".to_string());
    }
    why
}

/// 第四档的真启动（B-2e 乙-2，10-13 开闸）。
///
/// 返回 `(能不能安全收尾, AsstStart 的回执, 这一路有没有真打过 AsstStop)`。回执是 `None` 表示压根没拿到
/// （符号缺失 / 工作线程没了 / 上界到了没回话），与 `Some(false)`（对方说没跑成）是三件事，分开留痕 ——
/// 这是本仓"没做 / 做了没回执 / 做了但失败"三分法的第四次落地。
/// 第三枚不是状态位而是**防重枪的凭据**：叫停只许由一处发起，调用方拿它决定还要不要再补一枪
/// （对同一句柄连发两次 AsstStop 没有依据 —— 签名源只说"停止并清空所有任务"，没说重复调用安全）。
///
/// 三处形状都是被东西逼出来的，不是风格选择：
///   1) 这一跳丢给工作线程、主线程只等 `--timeout-ms` 那一个窗口。**理由在 10-13 换过一次**：
///      签名源（asst.py：restype=c_bool、argtypes 一枚 void_p，async 变体没进我们的 callable）那侧写的是
///      **阻塞式**，而两枪现场实测 `asst::Assistant::start | leave, 0 ms` —— 这一版根本不阻塞。
///      工作线程这层**照留**：它防的是"这一版不阻塞、下一版或另一侧真阻塞"时的无界等待
///      （**无界等待就是 §6 那颗 stdio 雷的形状**，整轮卡在收尾之前），而"跑没跑完"从此不由这里判，
///      交回事件流终态那一层（`ring_terminal_until`）。**一个函数只判一件事**。
///      ⇒ 本函数里**不再**写 `AsstStop=not-needed`：回执一到就说"没需要 stop"，正是把"调用回了话"
///      当成"队列散场"，那条前提已被实测打掉。
///   2) 跨线程只递 `usize`（裸指针不是 `Send`）；转出与转回成对落在同一个函数里，不外溢。
///   3) 超时不等于失败：我方主动 `AsstStop`（签名源核实 = 停止并清空所有任务），再按**同一个上界**
///      看工作线程回没回来。没回来就返回 `false`，调用方跳过 `AsstDestroy`、主流程跳过 `FreeLibrary`
///      —— 别人还堵在 DLL 里面时销毁句柄是第二颗雷，进程退出交给内核收。
unsafe fn start_task(
    r: &mut ConnectReport,
    hmod: *mut c_void,
    core: *mut c_void,
    o: &ConnectOpts,
) -> (bool, Option<bool>, bool) {
    let Some(p) = take(r, hmod, "AsstStart") else {
        // `take` 已经留过 `AsstStart=absent` 那枚更响的读数（fill_not_attempted 对留痕的不碰），
        // 这里不重复登记，只把"没起步"如实交回：句柄是我们自己建的，销毁照走。
        return (true, None, false);
    };
    let f: AsstStartFn = std::mem::transmute(p);
    let (tx, rx) = std::sync::mpsc::channel::<u8>();
    let core_raw = core as usize;
    let worker = std::thread::spawn(move || {
        let got = unsafe { f(core_raw as *mut c_void) };
        let _ = tx.send(got);
    });
    let bound = o.timeout_ms.max(1);
    match rx.recv_timeout(Duration::from_millis(bound)) {
        Ok(got) => {
            r.calls.push(format!("AsstStart={}", got));
            let _ = worker.join();
            r.calls.push("worker=joined".to_string());
            (true, Some(got != 0), false)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // 通道先断 = 工作线程已经没了（ panic 被 Runtime 吞掉那种 ）。对方栈上的状态无从知晓：
            // 不追调 AsstStop（那是一次凭空的二次进入），不销毁句柄，读数只留这一枚标签。
            r.calls.push("AsstStart=worker-died".to_string());
            (false, None, false)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            r.calls.push("AsstStart=timeout".to_string());
            let mut stop_issued = false;
            if let Some(q) = take(r, hmod, "AsstStop") {
                let g: AsstStopFn = std::mem::transmute(q);
                let got = g(core);
                r.calls.push(format!("AsstStop={}", got));
                stop_issued = true;
            }
            let deadline = filetime_now() + bound * 10_000;
            while !worker.is_finished() && filetime_now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            if worker.is_finished() {
                let _ = worker.join();
                r.calls.push("worker=joined".to_string());
                // stop 发出去了、线程也回来了，但 AsstStart 的回执这一路**拿不到**（通道已过期）：
                // 回执给 None，不许把"跑完了"和"叫停了"混成一枚 true。
                (true, None, stop_issued)
            } else {
                r.calls.push("worker=not-joined".to_string());
                (false, None, stop_issued)
            }
        }
    }
}

/// 叫停之后收回执的那枚短窗。上限**沿用前三档收尾窗那条 3 秒**（`sleep_with_events` 里的同一个数）：
/// 本仓不许为一档新形状另立一条上界，否则"等多久算久"会变成两处口径。
const STOP_GRACE_MS: u64 = 3_000;

/// "这一轮的环里出现过哪几枚终态" —— **保序**、相邻同枚折叠，成败词仍只从 `terminal_of` 取
/// （不另起第二套词汇表，也**不改** `--events` 那一档的 `verdict` 判据，那条一字未动）。
///
/// 这枚读数不是装饰，是 10-13 首枪真跑（ark × adb-live-run × Award）逼出来的：同一枚环里
/// `TaskChainError`（环序 13）之后**还到了一条** `AllTasksCompleted`（环序 14，只差 0 ms）⇒
/// 按"末条命中"给的 `terminal=succeeded` 与"这一轮任务跑砸了"**同现**。
/// 读上游给的 details 才看清语义：`AllTasksCompleted` 带的是 `finished_tasks`（里面就含着报错那条），
/// 它说的是**队列散场**，不是**任务成功** —— 所以"末条命中"只许当**收束读数**用，
/// 成败必须由这份命中册一起看。这是"`ok` 从不证 X"那一族的第五次，也是本仓第一次拿**真事件流**
/// 把一条从夹具里定的映射推到对面上复核（此前 `AllTasksCompleted → succeeded` 只有回放样本撑）。
///
/// 反面：`terminal_seen=` **不进任何名单** —— 它是读数不是判据，ok 那一侧只认"有没有下文"
/// （`terminal=none`），成败这一格由命中册话说。两份名单都按精确相等比，故天然扫不中它。
fn terminal_hits(events: &[NormEvent]) -> Vec<&'static str> {
    let mut hits: Vec<&'static str> = Vec::new();
    for e in events {
        if let Some(v) = terminal_of(&e.name) {
            if hits.last() != Some(&v) {
                hits.push(v);
            }
        }
    }
    hits
}

/// 第四档等终态：在事件环里找这一轮的终态，上界到了回 `None`（**不等于失败**，只等于"没下文"）。
///
/// 回来的是 `(末条命中的成败, 这一轮环里出现过的全部终态·保序折叠)` —— 第二枚不是修饰，是 10-13
/// 首枪逼出来的（见 `terminal_hits`）。同一次环读里一起取，**不另起第二次读环**：
/// 分两次扫同一个环，两枚读数可能各看到一半下文，那比少一枚读数更难查。
///
/// 三处形状同样是被逼出来的：
///   1) 成败口径**只有 `terminal_of` 一处** —— 与 `--events` 那一档同一条判据。在这里另起一套
///      "看 AsstStart 回执判成败"的口径，就会出现连接段说成功、事件流说失败各说各话。
///   2) 取**环序里最后命中的那条**（与 `event_verdict` 同一条规矩）：实测 `*Starting` 先到、终态后到，
///      先命中一条就 return 会把下文读丢。
///   3) 读环只走 `normalize_ring` 这一处归一入口，不自己解析 raw：raw 里带路径与账号信息，
///      判据只吃归一后的事件名。`main_thread` 是 `normalize_ring` 的既有形参，**不参与本判据**，
///      而且这里刻意**不按线程筛** —— 任务链的事件在对方 worker 线程上（10-13 两枪实测跨线程），
///      筛主线程会把终态整批筛没，那正是"看着像没收束"的假红形状。
///   4) 到点后**还会再读一遍环**才回 `None`（先读环、后判截止）：落在窗口边上的终态不该被丢掉。
///
/// ⚠ 从环头读起是成立的，前提是这一枚进程只跑一次连接会话（换头协议的独占实例）⇒ 环里不会有上一轮的
///   事件。若将来改成一进程跑多轮，这里必须换成带 `from` 游标的版本，否则会把上一轮的终态当本轮的下文。
fn ring_terminal_until(main_thread: u32, bound_ms: u64) -> Option<(&'static str, Vec<&'static str>)> {
    let deadline = filetime_now() + bound_ms.max(1) * 10_000;
    loop {
        let hits = terminal_hits(&normalize_ring(main_thread, 0));
        if let Some(last) = hits.last() {
            return Some((*last, hits));
        }
        if filetime_now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// "任务面到底收束了没有" —— 与 `handle_lost` / `resource_outcome` 同构的一条判据，
/// 只从 calls 里已有的标签读，不另起状态位。六枚标签任一在场就不许印 `ok:true`：
/// `AsstStart=timeout`（上界到了队列还在跑）、`worker=not-joined`（对方栈上还有我们的人）、
/// `AsstStart=worker-died`（工作线程没了、回执无从取得）、
/// `AsstStart=absent`（这一档承诺跑任务而符号缺失）、`AsstStart=not-attempted`（计划排了却没试到）、
/// `terminal=none`（叫停后连事件流的终态回执都没收到 —— 这一枚是 10-13 改等终态时补进来的：
/// 既然 `ok` 的覆盖面已对外写成"任务面收到了终局"，等不到终局就必须由同一枚判据否掉，
/// 不许 usage 里那句话与实际形状脱节）。
/// 前三档压根不会有这六枚标签里的任何一枚（`AsstStart` 只在第四档进计划）⇒ **既有读数不受影响**，
/// 这是"把覆盖面加宽"与"把别的档带红"之间那条分界线的又一次兑现。
///
/// 反面写清楚：`AsstStart=0` **不在**这份名单里。ok 从来不证任务成败（这句在本仓已经写了四遍），
/// 队列跑过而没跑成属事件流终态那一层的读数（`terminal=failed` 照样是收到了终局），
/// 不在"连接段起步了没有"这一格里。
fn task_stalled(calls: &[String]) -> bool {
    stalled_tag(calls, &["AsstStart=timeout", "worker=not-joined", "AsstStart=worker-died",
                         "AsstStart=absent", "AsstStart=not-attempted", "terminal=none"])
}

/// "工作线程那一侧有没有下文" —— 上面那份名单的**真子集**，且只取"对方栈上可能还挂着我们的人"那两枚。
/// 这条只管一件事：还要不要 `FreeLibrary`。`AsstStart=absent` / `not-attempted` 刻意**不在**这里 ——
/// 那两种形状下压根没起步，句柄与模块都是干净的，跳过释放反而是条假读数。
/// 判据分家的理由写在这两枚函数各自的注释里，不许后来者合并成一条。
fn worker_unresolved(calls: &[String]) -> bool {
    stalled_tag(calls, &["worker=not-joined", "AsstStart=worker-died"])
}

/// 两份名单共用的那一句"逐枚精确比对"。相等而非 `starts_with`：`AsstStart=1` / `AsstStart=0`
/// 这些带回执的标签不能被 `AsstStart=` 前缀一并扫中，否则 ok 判据就自己把第四档的正常成功形状判红了。
fn stalled_tag(calls: &[String], tags: &[&str]) -> bool {
    tags.iter()
        .any(|tag| calls.iter().any(|c| c == tag))
}

unsafe fn run_mf(hmod: *mut c_void, o: &ConnectOpts, r: &mut ConnectReport) -> String {
    // 同上：resource 句柄只有创建成功这一条路，失败就 return
    let res: *mut c_void;
    let mut ctl: *mut c_void = std::ptr::null_mut();
    let mut why = "ok".to_string();

    if let Some(p) = take(r, hmod, "MaaResourceCreate") {
        let f: MaaResourceCreateFn = std::mem::transmute(p);
        res = f();
        r.calls.push(format!("MaaResourceCreate={}", if res.is_null() { "null" } else { "handle" }));
    } else {
        return "拿不到 MaaResourceCreate：framework 侧连接段无入口".to_string();
    }
    if let Some(p) = take(r, hmod, "MaaResourceAddSink") {
        let f: MaaSinkAddFn = std::mem::transmute(p);
        let id = f(res, mf_cb as *const () as *mut c_void, std::ptr::null_mut());
        r.calls.push(format!("MaaResourceAddSink={}", id));
    }
    if o.load_resource {
        if let Some(p) = take(r, hmod, "MaaResourcePostPipeline") {
            let d = cstr_bytes(&effective_resource("mf", &o.dir, &o.resource_path));
            let f: MaaResourcePostPipelineFn = std::mem::transmute(p);
            let id = f(res, d.as_ptr());
            r.calls.push(format!("MaaResourcePostPipeline={}", id));
        }
    }
    if let Some(p) = take(r, hmod, "MaaResourceLoaded") {
        let f: MaaResourceLoadedFn = std::mem::transmute(p);
        let deadline = filetime_now() + o.timeout_ms * 10_000;
        let mut loaded = f(res);
        while loaded == 0 && filetime_now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            loaded = f(res);
        }
        r.calls.push(format!("MaaResourceLoaded={}", loaded));
    }

    if reaches_connect(&o.scope) {
        if let Some(p) = take(r, hmod, "MaaAdbControllerCreate") {
            let a = cstr_bytes(&o.adb);
            let ad = cstr_bytes(&o.address);
            let cf = cstr_bytes(&o.config);
            // 第六枚 = agent_path（上游 Python 绑定逐字核实：缺省即装包内 MaaAgentBinary 目录），
            // 不再硬塞 `"{}"`；给了就原样传、没给是空串（生产口径里它恒为真路径，读法见 usage）。
            let ag = cstr_bytes(&o.agent_path);
            let f: MaaAdbControllerCreateFn = std::mem::transmute(p);
            ctl = f(a.as_ptr(), ad.as_ptr(), o.screencap_mask, o.input_mask, cf.as_ptr(), ag.as_ptr());
            r.calls.push(format!("MaaAdbControllerCreate={}", if ctl.is_null() { "null" } else { "handle" }));
        }
        if !ctl.is_null() {
            if let Some(p) = take(r, hmod, "MaaControllerAddSink") {
                let f: MaaSinkAddFn = std::mem::transmute(p);
                let id = f(ctl, mf_cb as *const () as *mut c_void, std::ptr::null_mut());
                r.calls.push(format!("MaaControllerAddSink={}", id));
            }
            if let Some(p) = take(r, hmod, "MaaControllerPostConnection") {
                let f: MaaControllerPostConnFn = std::mem::transmute(p);
                let id = f(ctl);
                r.calls.push(format!("MaaControllerPostConnection={}", id));
            }
            sleep_with_events(o.timeout_ms);
            if let Some(p) = take(r, hmod, "MaaControllerConnected") {
                let f: MaaControllerConnectedFn = std::mem::transmute(p);
                let got = f(ctl);
                r.calls.push(format!("MaaControllerConnected={}", got));
                if got == 0 {
                    why = "连接没成（ControllerConnected=0），事件面在下面".to_string();
                }
            }
            if let Some(p) = take(r, hmod, "MaaControllerDestroy") {
                let f: MaaControllerDestroyFn = std::mem::transmute(p);
                f(ctl);
                r.calls.push("MaaControllerDestroy=done".to_string());
            }
        } else {
            sleep_with_events(o.timeout_ms);
            why = "控制器句柄创建失败（返回 null）：这一枚到底能不能独立起，读数就在这条".to_string();
        }
    } else {
        sleep_with_events(o.timeout_ms);
    }

    if let Some(p) = take(r, hmod, "MaaResourceDestroy") {
        let f: MaaResourceDestroyFn = std::mem::transmute(p);
        f(res);
        r.calls.push("MaaResourceDestroy=done".to_string());
    }
    why
}

fn sleep_with_events(timeout_ms: u64) {
    let cap = if timeout_ms > 3_000 { 3_000 } else { timeout_ms };
    let deadline = filetime_now() + cap * 10_000;
    while filetime_now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---------------------------------------------------------------- 无头自检（不加载任何 DLL）

fn selftest() -> i32 {
    let mut fails: Vec<String> = Vec::new();
    let mut total = 0usize;
    let chk = |cond: bool, label: &str, fails: &mut Vec<String>, total: &mut usize| {
        *total += 1;
        if !cond {
            fails.push(label.to_string());
        }
    };

    // 1. 转义
    chk(
        jstr("a\\b\"c\nd") == "\"a\\\\b\\\"c\\nd\"",
        "jstr 转义反斜杠/引号/换行",
        &mut fails,
        &mut total,
    );
    chk(jstr("\u{1}") == "\"\\u0001\"", "jstr 控制字符走 \\u", &mut fails, &mut total);

    // 2. 两侧清单互不引用对方符号名（§4 第 2 项的验收，纯逻辑这半边先钉住）
    let ark_leaks_mf: Vec<&&str> = ARK_EXPECT.iter().filter(|s| s.starts_with("Maa")).collect();
    let mf_leaks_ark: Vec<&&str> = MF_EXPECT.iter().filter(|s| s.starts_with("Asst")).collect();
    chk(ark_leaks_mf.is_empty(), "ark 侧清单里混进了 Maa* 符号", &mut fails, &mut total);
    chk(mf_leaks_ark.is_empty(), "mf 侧清单里混进了 Asst* 符号", &mut fails, &mut total);
    chk(ARK_EXPECT.len() >= 10, "ark 侧清单短得可疑", &mut fails, &mut total);
    for k in KINDS {
        chk(
            k.version_fn.is_empty() || k.expect.iter().any(|s| *s == k.version_fn),
            &format!("{} 侧：version_fn 不在 expect 清单里（要么别调、要么先声明要它）", k.name),
            &mut fails,
            &mut total,
        );
        chk(
            k.expect.len() == k.expect.iter().collect::<std::collections::HashSet<_>>().len(),
            &format!("{} 侧：expect 清单有重复项", k.name),
            &mut fails,
            &mut total,
        );
        chk(!k.forbid.is_empty(), &format!("{} 侧：forbid 清单是空的（不对称判据丢了）", k.name), &mut fails, &mut total);
    }

    // 3. 资源轴提取器：夹具直接写在代码里，不依赖任何装包在场
    let fixture = r#"{"interface_version": 2, "version": "v9.9.9", "name": "x", "nested": {"version":"v8.8.8"}, "min_version": "v7.7.7"}"#;
    let got = extract_interface_versions(fixture);
    chk(
        got == vec!["v9.9.9".to_string(), "v8.8.8".to_string()],
        "资源轴提取：收到的条数/内容不对",
        &mut fails,
        &mut total,
    );
    chk(extract_interface_schema_version(fixture).as_deref() == Some("2"), "interface_version 轴读错", &mut fails, &mut total);
    chk(extract_interface_versions(r#"{"interface_version":2}"#).is_empty(), "不该收 interface_version", &mut fails, &mut total);
    chk(
        extract_interface_versions(r#"{"version":"a"}{"version": 1}"#) == vec!["a".to_string()],
        "数值型 version 不该收",
        &mut fails,
        &mut total,
    );

    // 4. 目录标签默认只留最后一段（防读数表把整条本机路径抄出去）
    chk(dir_label("D:\\Some\\Where\\Pkg-1", false) == "Pkg-1", "dir_label 默认没剥路径", &mut fails, &mut total);
    chk(
        dir_label("D:\\Some\\Where\\Pkg-2", true) == "D:\\Some\\Where\\Pkg-2",
        "--show-path 没生效",
        &mut fails,
        &mut total,
    );
    chk(dir_label("D:\\x\\Pkg-3\\", false) == "Pkg-3", "尾部反斜杠没处理", &mut fails, &mut total);

    // 5. panic 不悄悄跨这条边界：这里造一次真越界，确认外层 catch **接得住**。
    //    注意方向：闭包一定会 panic，所以"兜得住"= catch_unwind 返回 Err，不是 Ok。
    //    （第一版把这条写成 is_ok()，自检当场红给我看 —— 判据反着写比不写更坏。）
    chk(
        catches_the_panic(),
        "catch_unwind 兜不住：那样探针就成了「有 bug 就一条读数都不剩」",
        &mut fails,
        &mut total,
    );

    // 6. 连接段四档射程的真值表 —— 全是纯逻辑，不需要装包、不需要 GUI、不需要模拟器在场。
    //    档名要能自己说清射程：中档旧名叫 adb-negative，承诺的是"拿假地址试一次负例连接"，
    //    实测后这条路取消了（设备不在场就没有负例，唯一诚实的形状是这一档压根不连）。
    //    名字留着会继续骗读数，所以改成 resource，并把三档一起钉死。
    //    第四档 `adb-live-run`（B-2e 乙-2）是**两枚判据分开钉**的：碰设备 ≠ 真跑任务。
    //    两列各判各的，就是为了让"把连一下偷偷升级成动了账号"这种形状当场红。
    let tiers: [(&str, bool, bool); 4] = [
        ("shape", false, false),
        ("resource", false, false),
        ("adb-live", true, false),
        ("adb-live-run", true, true),
    ];
    for (sc, want, want_run) in tiers {
        chk(
            reaches_connect(sc) == want,
            &format!("{} 档的射程判据不是 {}：四档真值表错了，最高危那一档就没被看住", sc, want),
            &mut fails,
            &mut total,
        );
        chk(
            runs_task(sc) == want_run,
            &format!("{} 档的跑任务判据不是 {}：真副作用的开闸判据错了", sc, want_run),
            &mut fails,
            &mut total,
        );
    }
    chk(!reaches_connect(""), "空档名判成碰设备", &mut fails, &mut total);
    chk(!runs_task(""), "空档名判成真跑任务", &mut fails, &mut total);
    chk(!reaches_connect("adb"), "adb 判成碰设备：字符串判据不许做前缀匹配", &mut fails, &mut total);
    chk(!reaches_connect("live"), "live 判成碰设备（同上）", &mut fails, &mut total);
    chk(!runs_task("adb-live"), "adb-live 判成真跑任务：那一档只连不动账号，升级它要另一档的名字", &mut fails, &mut total);
    chk(!runs_task("adb-live-run-x"), "带尾巴的档名判成真跑任务：跑任务判据也不许做前缀匹配", &mut fails, &mut total);

    // (kind 名, 档, 载资源, 最小任务) 的展开表：每档该出哪些符号、该缺哪些，逐条对
    let cases: [(&str, &str, bool, bool); 12] = [
        ("ark", "shape", true, true),
        ("ark", "resource", true, false),
        ("ark", "resource", false, true),
        ("ark", "adb-live", true, false),
        ("ark", "adb-live", true, true),
        ("ark", "adb-live-run", true, true),
        // 第四档但没给最小任务：计划里排不上启动符号 ⇒ 这一档的存在理由落空，必须**拒**
        ("ark", "adb-live-run", true, false),
        // framework 侧的任务提交没有本地签名源、且仍在禁止列里 ⇒ 第四档对它只能拒，不许半跑半不跑
        ("mf", "adb-live-run", true, true),
        ("mf", "shape", true, true),
        ("mf", "resource", true, true),
        ("mf", "adb-live", true, false),
        ("w32cu", "adb-live", true, true),
    ];
    let connect_words = ["AsstConnect", "AsstAsyncConnect", "MaaAdbControllerCreate", "MaaControllerPostConnection", "MaaControllerConnected"];
    for (kn, sc, lr, mt) in cases {
        let k = match find_kind(kn) {
            Some(k) => k,
            None => {
                fails.push(format!("kind {} 不存在（清单被改坏了）", kn));
                continue;
            }
        };
        let label = format!("{}/{}/lr={}/mt={}", kn, sc, lr, mt);
        let plan = connect_plan(kn, sc, lr, mt);
        let has = |n: &str| plan.iter().any(|x| *x == n);

        // 空表只该出现在三种情况：shape 档、那侧没有签名源、或未知 kind
        if sc == "shape" || kn == "w32cu" {
            chk(plan.is_empty(), &format!("{}：这一档必须一枚都不调", label), &mut fails, &mut total);
            continue;
        }
        chk(!plan.is_empty(), &format!("{}：非 shape 档给出空计划（执行侧会当成什么都不做）", label), &mut fails, &mut total);
        // 该拒 / 不该拒按判据现算，不在表里手工标：第四档若排不出启动符号就是"把跑退成排队"，
        // 宁可一条带归因的信封都不出 —— 表里那三格拒例（ark 没给最小任务、mf 整侧）全靠这一句自己说话。
        let should_refuse = runs_task(sc) && !(mt && kn == "ark");
        let safety = plan_is_safe(k, &plan, sc);
        if should_refuse {
            chk(safety.is_some(), &format!("{}：这一档承诺跑任务却排不出启动符号，必须拒（不排队跑=假绿）", label), &mut fails, &mut total);
        } else {
            chk(safety.is_none(), &format!("{}：连接段计划不安全", label), &mut fails, &mut total);
        }
        chk(plan.iter().all(|p| k.callable.iter().any(|c| c == p)), &format!("{}：计划里混进了无签名源的符号", label), &mut fails, &mut total);
        // 启动类符号只在第四档出现：其余三档的"最小任务"仍然只到"排队 + 回执"，不执行
        let start_expected = runs_task(sc) && mt && kn == "ark";
        chk(has("AsstStart") == start_expected, &format!("{}：启动符号出现得不合规则（只有第四档、ark 侧、给了最小任务时才有）", label), &mut fails, &mut total);
        chk(has("AsstStop") == start_expected, &format!("{}：收尾备选与启动符号不相配（一个排了一个没排）", label), &mut fails, &mut total);
        chk(!has("MaaTaskerPostTask"), &format!("{}：计划里出现了任务提交", label), &mut fails, &mut total);
        chk(!has("AsstAttachWindow") && !has("AsstAsyncAttachWindow"), &format!("{}：计划里出现了挂窗口（本轮禁止）", label), &mut fails, &mut total);
        // 资源段与连接段是分得开的两件事：只有碰设备的两档允许出现连接类符号
        if reaches_connect(sc) {
            chk(plan.iter().any(|p| connect_words.contains(p)), &format!("{}：碰设备档却没有连接类符号（射程写窄了）", label), &mut fails, &mut total);
        } else {
            chk(!plan.iter().any(|p| connect_words.contains(p)), &format!("{}：非碰设备档出现了连接类符号 = 偷偷碰设备", label), &mut fails, &mut total);
        }
        chk(has("AsstLoadResource") == (kn == "ark" && lr), &format!("{}：载资源判据落错（ark 侧）", label), &mut fails, &mut total);
        chk(has("MaaResourcePostPipeline") == (kn == "mf" && lr), &format!("{}：载资源判据落错（mf 侧）", label), &mut fails, &mut total);
        if kn == "ark" {
            // 载资源必须排在**建实例之前**：上游 v6.18.0 的 `AsstCreateEx` 开头是 `if (!inited()) return nullptr;`，
            // 而 `inited()` 读的就是 ResourceLoader 的 loaded() —— 遗留侧"能不能起步"以"资源已加载"为前置。
            // R0 把这条顺序排反过，两遍 ark×resource 实跑各得一句"看着成功、其实一步都没走"的假绿读数
            // （空句柄处提前 return ⇒ 资源那步压根没执行 ⇒ 想拿的遗留侧事件流恒为 0）。这条就是钉顺序的。
            if lr {
                let i_load = plan.iter().position(|p| *p == "AsstLoadResource");
                let i_create = plan.iter().position(|p| *p == "AsstCreateEx");
                chk(
                    matches!((i_load, i_create), (Some(l), Some(c)) if l < c),
                    &format!("{}：ark 侧载资源没排在 AsstCreateEx 之前（遗留侧建实例以「资源已加载」为前置）", label),
                    &mut fails,
                    &mut total,
                );
            }
            chk(has("AsstAppendTask") == (mt && reaches_connect(sc)), &format!("{}：最小任务符号出现得不合规则（只在碰设备那两档、且给了最小任务时才有）", label), &mut fails, &mut total);
            chk(has("AsstSetTaskParams") == (mt && reaches_connect(sc)), &format!("{}：排队与设参两条不相配", label), &mut fails, &mut total);
            chk(has("AsstDestroy"), &format!("{}：没有收尾销毁：独占轮会留下句柄", label), &mut fails, &mut total);
            // 第四档的**次序**：启动必须排在设参之后、销毁之前。排反了两种都是真事故 ——
            // 设在销毁之后 = 永远跑不到；抢在设参之前 = 队列带着空参数开跑（对方按"参数已就绪"为前置）。
            if start_expected {
                let i_params = plan.iter().position(|p| *p == "AsstSetTaskParams");
                let i_start = plan.iter().position(|p| *p == "AsstStart");
                let i_destroy = plan.iter().position(|p| *p == "AsstDestroy");
                chk(
                    matches!((i_params, i_start, i_destroy), (Some(a), Some(b), Some(c)) if a < b && b < c),
                    &format!("{}：真启动没排在设参之后、销毁之前", label),
                    &mut fails,
                    &mut total,
                );
            }
        }
        if kn == "mf" {
            chk(has("MaaResourceDestroy"), &format!("{}：没有收尾销毁：独占轮会留下句柄", label), &mut fails, &mut total);
            if reaches_connect(sc) {
                chk(has("MaaControllerDestroy"), &format!("{}：起了控制器却不相配销毁", label), &mut fails, &mut total);
            }
        }
    }

    // 计划 ↔ 执行必须共用同一条判据。执行段真跑要装包在场、要 GUI 不在场，自检里做不了；
    // 但"档名比较还剩几处"是能从自己的源码里直接数出来的 —— 这条不依赖任何现场、不加载任何 DLL，
    // 旧写法（执行段自己按档名比）一回来它就红。
    // 三条 needle 都**不能**原样写进这个文件：断言读的是自己，字面量落进源码就会把自己数进去。
    // 前两条靠转义或运行期拼接绕开，第三条本来就只是下界、数进去也不会造成假绿。
    //（这个坑这一轮踩了两次：第一次红的是 scope，第二次红的是 callable 那条 —— 自指判据要当场认出来。）
    const SRC: &str = include_str!("main.rs");
    let spell: &str = "scope == \"shape\"";
    chk(
        SRC.matches(spell).count() == 1,
        "按档名比 shape 的地方不止一处：射程判据有了第二处 spelling，装配改了执行不会跟着改",
        &mut fails,
        &mut total,
    );
    let refuse_spell: &str = &["callable", ".is_empty()"].concat();
    chk(
        SRC.matches(refuse_spell).count() == 1,
        "「这一侧一枚都不能调」的判据不止一处：主流程若自己再判一次，规则拒绝会退成用法错（退出码 1 而非 2 + 信封）",
        &mut fails,
        &mut total,
    );
    chk(SRC.matches("reaches_connect(").count() >= 5, "reaches_connect 的使用点太少：计划/调用/参数硬门没共用同一条判据", &mut fails, &mut total);

    // 第四档（B-2e 乙-2）同一条规矩再走一遍：真副作用的开闸判据**只许有一处 spelling**。
    // 字面量写成一枚运行期拼出来的串以外的形式都会把自己数进去，所以照上面那条的写法用转义形式，
    // 源码里那一处函数体是未转义的，两边不互撞（这一族的坑本轮之前已经踩过两次）。
    let run_spell: &str = "scope == \"adb-live-run\"";
    chk(
        SRC.matches(run_spell).count() == 1,
        "按档名判真跑任务的地方不止一处：开闸判据有了第二处 spelling，改了装配执行段不会跟着改",
        &mut fails,
        &mut total,
    );
    // 下界判据：计划装配 / 安全判据（两处）/ 执行段 / 自检的分派都得走同一枚函数。
    chk(SRC.matches("runs_task(").count() >= 6, "runs_task 的使用点太少：四档里\"碰设备\"与\"真跑任务\"又分家了", &mut fails, &mut total);
    // 档名册与射程判据不许各写一套：CLI 那边只认清单，比较链一律不许回来。
    let scope_chain_spell: &str = &["v != \"adb-live", "-run\""].concat();
    chk(
        !SRC.contains(scope_chain_spell),
        "CLI 里又写了一条按档名的比较链：档名册与射程判据分家，改档名时会有一处漏掉",
        &mut fails,
        &mut total,
    );

    // 无签名源的挂载函数：查存在性（在 connect 清单里），但**绝不许调**（不在 callable 里）
    let has_in = |list: &[&str], n: &str| list.iter().any(|x| *x == n);
    chk(has_in(ARK_CONNECT, "AsstAttachWindow"), "ark 连接清单没查 AsstAttachWindow（挂载面读不到形状）", &mut fails, &mut total);
    chk(!has_in(ARK_CALLABLE, "AsstAttachWindow"), "AsstAttachWindow 混进了可调用清单（违反签名源纪律）", &mut fails, &mut total);
    chk(!has_in(ARK_CALLABLE, "AsstConnected"), "AsstConnected 混进了可调用清单", &mut fails, &mut total);
    chk(!has_in(MF_CALLABLE, "MaaWin32ControllerCreate"), "MaaWin32ControllerCreate 没有本地签名源，不许进可调用清单", &mut fails, &mut total);
    for k in KINDS {
        chk(!k.connect_symbols.is_empty(), &format!("{} 侧：连接清单是空的", k.name), &mut fails, &mut total);
        chk(k.expect.iter().all(|s| !k.forbid.iter().any(|f| f == s)), &format!("{} 侧：expect 与 forbid 有同名（判据自相矛盾）", k.name), &mut fails, &mut total);
        chk(k.callable.iter().all(|c| has_in(k.connect_symbols, c)), &format!("{} 侧：callable 里有名字不在连接清单（存在性没被查过就被调）", k.name), &mut fails, &mut total);
        chk(connect_plan(k.name, "adb-live", true, true).iter().all(|p| k.connect_symbols.contains(p)), &format!("{} 侧：adb-live 计划里有用连接清单没查过的符号", k.name), &mut fails, &mut total);
    }

    // 7. 三闸的判据（缺一个都不放行；进程现场读不出来 = 前提未知 = 拒跑）
    let all_three = GateFlags { confirm_gui_absent: true, accept_live: true, window_clear: true };
    chk(gate_verdict(&all_three, true, &[]).0, "三闸齐、现场干净、无 GUI —— 却判了拒", &mut fails, &mut total);
    chk(!gate_verdict(&GateFlags::default(), true, &[]).0, "一个闸都没拨却放行", &mut fails, &mut total);
    chk(gate_verdict(&GateFlags::default(), true, &[]).1.len() == 3, "缺三个闸却只报了一条理由（文案会骗人）", &mut fails, &mut total);
    chk(!gate_verdict(&all_three, false, &[]).0, "进程现场读不出来却放行：前提未知不等于前提成立", &mut fails, &mut total);
    chk(!gate_verdict(&all_three, true, &["MAA.exe".to_string()]).0, "助手进程在场却放行（不抢方向盘这条丢了）", &mut fails, &mut total);
    let partial = GateFlags { confirm_gui_absent: true, accept_live: false, window_clear: true };
    chk(gate_verdict(&partial, true, &[]).1.iter().any(|s| s.contains("--i-accept-live")), "拒跑理由没点名缺哪个闸", &mut fails, &mut total);

    // 7b. 资源落点 + "资源面单拆"（09-30 首次实跑纠正出来的两条，都不许悄悄退回）：
    //     ① 落点是**我方选的**：整目录 `<装包>\resource` 会被框架按"每渠道各有一份同名 pipeline"判重复键而整批失败，
    //        生产侧当天框架日志 0 命中该错、加载行全在 `resource/base/**` —— 默认口径本轮**一个字没改**，只把"可指名"开出来；
    //     ② `ok` 的判据本来就只是"DLL 进得来 + 回调没炸"，首跑那次 `ok=true` 与 `MaaResourceLoaded=0` 同时成立，
    //        所以资源成败必须单拆一格，否则 CONNECT-OK 这个词会掩盖它。
    //     两条 needle 都运行期拼：写全了会被自己数进去（同 §7 上面那三条的坑）。
    let mf_root = effective_resource("mf", "C:\\Fake\\Pkg", "");
    chk(mf_root.ends_with("\\resource"), "mf 侧默认落点被改掉：动默认要另开一轮证据", &mut fails, &mut total);
    chk(effective_resource("ark", "C:\\Fake\\Pkg\\", "") == "C:\\Fake\\Pkg", "ark 侧默认该吃装包根（遗留侧自己去找 resource/），尾分隔符没剥", &mut fails, &mut total);
    chk(effective_resource("mf", "C:\\Fake\\Pkg", "C:\\Fake\\Pkg\\resource\\base") == "C:\\Fake\\Pkg\\resource\\base", "--resource-path 给了却不生效：读数就没法指名落点", &mut fails, &mut total);
    chk(resource_rel("C:\\Fake\\Pkg", "C:\\Fake\\Pkg\\resource\\base") == "resource\\base", "落点没投影成相对形态：信封指名不了喂的是哪条子树", &mut fails, &mut total);
    chk(resource_rel("C:\\Fake\\Pkg", "C:\\Fake\\Pkg") == "", "落点==装包根时相对形态必须是空串，不能把盘符拼进机读面", &mut fails, &mut total);
    let outside = resource_rel("C:\\Fake\\Pkg", "D:\\Other\\thing\\base");
    chk(outside == "thing\\base" && !outside.contains(':') && !resource_rel("C:\\Fake\\Pkg", "D:\\Other").contains(':'), "越界落点漏了盘符", &mut fails, &mut total);
    let first_run_calls: Vec<String> = vec![
        "dll=MaaFramework.dll".to_string(),
        "MaaResourceLoaded=resolved".to_string(),
        "MaaResourceLoaded=0".to_string(),
    ];
    chk(resource_outcome(&first_run_calls, true) == "failed", "首跑那一次的形状没被判成 failed：CONNECT-OK 又能掩盖资源失败", &mut fails, &mut total);
    chk(resource_outcome(&vec!["AsstLoadResource=1".to_string()], true) == "ok", "遗留侧的成功形状没认出来", &mut fails, &mut total);
    chk(resource_outcome(&vec!["MaaResourceLoaded=resolved".to_string()], true) == "not-reported", "只有存在性读数就当资源报过了（resolved 不是 1）", &mut fails, &mut total);
    chk(resource_outcome(&Vec::new(), false) == "skipped", "--no-load-resource 那一档却报了成败", &mut fails, &mut total);
    // 10-02 那两遍假绿读数配套钉的三件：没执行≠没回执、空句柄≠起步、空句柄的话术得指向真根因。
    chk(resource_outcome(&vec!["AsstLoadResource=not-attempted".to_string()], true) == "not-attempted", "「这一步压根没执行」被读成了「做了没报」", &mut fails, &mut total);
    chk(
        resource_outcome(&vec!["AsstLoadResource=resolved".to_string(), "AsstLoadResource=not-attempted".to_string()], true) == "not-attempted",
        "存在性标签(resolved)排在前面时，补进去的 not-attempted 就被挡住了",
        &mut fails,
        &mut total,
    );
    chk(handle_lost(&vec!["dll=MaaCore.dll".to_string(), "AsstCreateEx=null".to_string()]), "空句柄没被认成「没起步」：CONNECT-OK 又会掩盖一步都没走的事实", &mut fails, &mut total);
    chk(!handle_lost(&vec!["dll=MaaCore.dll".to_string(), "AsstCreateEx=handle".to_string()]), "拿到了句柄却被这条判据判成没起步", &mut fails, &mut total);
    chk(!handle_lost(&Vec::new()), "shape 档（一条句柄标签都没有）被新覆盖面带红", &mut fails, &mut total);
    chk(handle_lost(&vec!["MaaResourceCreate=null".to_string()]), "framework 侧的空资源句柄没被认出来", &mut fails, &mut total);
    // 10-03 ②：覆盖面再宽一格 —— 控制器句柄也进判据，"没拿到"的三种形态（null/absent/not-attempted）同属。
    chk(handle_lost(&vec!["MaaAdbControllerCreate=null".to_string()]), "framework 侧的空控制器句柄没被认出来：ok:true 又会与它同现（10-03 首跑原文）", &mut fails, &mut total);
    chk(!handle_lost(&vec!["MaaAdbControllerCreate=handle".to_string()]), "控制器拿到了句柄却被判成没起步", &mut fails, &mut total);
    chk(handle_lost(&vec!["AsstCreateEx=not-attempted".to_string()]), "计划里排着却没试到的句柄该算没拿到（not-attempted 与 null 同属「没起步」）", &mut fails, &mut total);
    chk(handle_lost(&vec!["MaaResourceCreate=absent".to_string()]), "符号缺失（absent）没被算成没起步：三件全空也换得到 CONNECT-OK", &mut fails, &mut total);
    // B-2e 乙-2：真启动那一档的"收束了没有"两份判据。形状全部由合成标签给，不碰 DLL、不碰现场。
    // 五枚 stalled 标签逐枚单测（不是"塞一整串看整体"）：一整串红了无从知道是哪枚没进名单。
    for tag in ["AsstStart=timeout", "worker=not-joined", "AsstStart=worker-died",
                "AsstStart=absent", "AsstStart=not-attempted", "terminal=none"] {
        chk(task_stalled(&vec![tag.to_string()]), &format!("{} 没进「任务面没收束」的名单：ok 又能盖一层", tag), &mut fails, &mut total);
    }
    chk(!task_stalled(&Vec::new()), "前三档（压根没有启动标签）被新覆盖面带红", &mut fails, &mut total);
    // 反面四格，都是这一档真会印出来的形状，不是设想：
    //   `AsstStart=0` = 队列跑过但没跑成 —— 那是事件流终态那一层的读数，不许混进"起步了没有"这格；
    //   `terminal=failed` / `terminal=succeeded` = 收到了终局（成败由 `terminal_of` 那一层说，
    //     本判据只问"有没有下文"，问成败就把一枚判据拆成两套口径）；
    //   `AsstStop=not-needed` = 计划里排了而没需要，与 not-attempted 分家的那条。
    chk(!task_stalled(&vec!["AsstStart=1".to_string(), "terminal=succeeded".to_string(), "AsstStop=not-needed".to_string(), "worker=joined".to_string()]),
        "跑成了的一轮被判成没收束：这一档当场就自红了", &mut fails, &mut total);
    chk(!task_stalled(&vec!["AsstStart=0".to_string(), "terminal=failed".to_string()]),
        "跑砸了的一轮被判成没收束：ok 只问收没收束，成败在终态那一层", &mut fails, &mut total);
    chk(!task_stalled(&vec!["terminal=stopped-x".to_string()]),
        "带尾巴的标签被判成没收束：两份名单都是精确相等，不做前缀", &mut fails, &mut total);
    // 跳过释放的那两枚必须是 stalled 名单的**真子集**；absent / not-attempted / timeout 反过来必须留在外面
    //（那三种形状下要么压根没起步、要么线程已经回来了，跳过 FreeLibrary 会是条假读数）。
    chk(worker_unresolved(&vec!["worker=not-joined".to_string()]), "线程还堵在对方栈里却准备 FreeLibrary", &mut fails, &mut total);
    chk(worker_unresolved(&vec!["AsstStart=worker-died".to_string()]), "线程死在里面却准备 FreeLibrary", &mut fails, &mut total);
    chk(!worker_unresolved(&vec!["AsstStart=absent".to_string()]), "符号缺失被当成现场没清：模块明明是干净的", &mut fails, &mut total);
    chk(!worker_unresolved(&vec!["AsstStart=timeout".to_string(), "worker=joined".to_string()]),
        "叫停后线程回来了却被判成现场没清：这一轮的模块释放被无端跳过", &mut fails, &mut total);
    chk(task_stalled(&vec!["worker=not-joined".to_string()]) && worker_unresolved(&vec!["worker=not-joined".to_string()]),
        "两份判据的真子集关系断了：一份红一份不红，ok 与释放会各说各话", &mut fails, &mut total);
    let null_calls: Vec<String> = vec!["AsstCreateEx=null".to_string(), "AsstLoadResource=not-attempted".to_string()];
    chk(create_null_reason(&null_calls, true).contains("not-attempted"), "空句柄的归因没把资源面读数带出来：话术又会把人指回「对方不给句柄」", &mut fails, &mut total);
    chk(create_null_reason(&Vec::new(), false).contains("前置"), "没给载资源时空句柄的归因该落在前置门上，不是随便一句", &mut fails, &mut total);
    // 10-03 ③：执行段提前退出时，计划里没走到的符号必须逐条留 not-attempted，不许静默消失。
    // 补法从计划现取（单一函数，定义另由下面「判据只住一处」那组 needle 钉）；这里钉"会进补单的那一段"的次序：
    // 建控制器之后、名字仍以 MaaController 开头的那四枚 —— 空句柄路上恰恰是它们没走到。其余符号不该进补单：
    // 末尾的 MaaResourceDestroy 在早退路上走得到（else 分支之后照旧执行）、会留痕，夹具里把这条也钉了。
    let ctrl_plan = connect_plan("mf", "adb-live", true, false);
    let ctrl_tail: Vec<&str> = ctrl_plan
        .iter()
        .skip_while(|s| **s != "MaaAdbControllerCreate")
        .skip(1)
        .take_while(|s| s.starts_with("MaaController"))
        .copied()
        .collect();
    chk(
        ctrl_tail == vec!["MaaControllerAddSink", "MaaControllerPostConnection", "MaaControllerConnected", "MaaControllerDestroy"],
        "控制器尾段清单与计划漂了：补 not-attempted 的清单从计划现取，改这里前先对计划",
        &mut fails,
        &mut total,
    );
    // 夹具故意只给到建控制器那一步的留痕（断在 null 之后）：缺口=资源面后三枚 + 控制器面后四枚，按计划序补。
    let mut ctl_null_calls: Vec<String> = vec![
        "dll=MaaFramework.dll".to_string(),
        "MaaResourceCreate=resolved".to_string(),
        "MaaResourceCreate=handle".to_string(),
        "MaaAdbControllerCreate=resolved".to_string(),
        "MaaAdbControllerCreate=null".to_string(),
        "MaaResourceDestroy=resolved".to_string(),
        "MaaResourceDestroy=done".to_string(),
    ];
    let len_before_fill = ctl_null_calls.len();
    fill_not_attempted(&ctrl_plan, &mut ctl_null_calls);
    let added: Vec<String> = ctl_null_calls.iter().skip(len_before_fill).cloned().collect();
    chk(
        added == vec![
            "MaaResourceAddSink=not-attempted", "MaaResourcePostPipeline=not-attempted", "MaaResourceLoaded=not-attempted",
            "MaaControllerAddSink=not-attempted", "MaaControllerPostConnection=not-attempted", "MaaControllerConnected=not-attempted", "MaaControllerDestroy=not-attempted",
        ],
        "缺口没按计划逐条补全（少补=静默消失，多补=凭空造句）",
        &mut fails,
        &mut total,
    );
    let mut done_calls: Vec<String> = ctrl_plan.iter().map(|s| format!("{}=resolved", s)).collect();
    let done_len = done_calls.len();
    fill_not_attempted(&ctrl_plan, &mut done_calls);
    chk(done_calls.len() == done_len, "已留过痕（=resolved）的 calls 被补了 not-attempted：判据会把正常跑读成缺口", &mut fails, &mut total);
    // 10-03 ①：config 缺省按侧给（壳侧 Default / 探针写死 General 两处错默认都不许回来），
    // 第六枚只许从 o.agent_path 取、不许再拿非路径字面量顶包。
    chk(default_config("mf") == "{}", "mf 侧 config 缺省不是 JSON 对象：建控制器处会当场判掉（10-03 首跑 invalid config 原文）", &mut fails, &mut total);
    chk(default_config("ark") == "General", "ark 侧 config 缺省不是 General：遗留侧连接配置名换默认要另开一轮证据", &mut fails, &mut total);
    let ag_needle: &str = &["&o.", "agent_path"].concat();
    chk(SRC.matches(ag_needle).count() == 1, "第六枚（agent_path）的取数点不唯一或已丢：又会拿非路径字面量当路径喂进去", &mut fails, &mut total);
    let outcome_spell: &str = &["fn ", "resource", "_outcome("].concat();
    let eff_spell: &str = &["effective", "_resource("].concat();
    // 数"判据只住一处"必须**只数 selftest 之外**的那半份源码：自检里对这两个函数的每次调用都是真调用点，
    // 整份一起数就把自己数进去了（同一类坑这一轮第三次踩：needle 只要出现在射程内就会骗读数）。
    // 10-02 把资源面这条 needle 从"数使用点"改成了**数定义点**（`fn ` 前缀）：空句柄归因那一次合法地
    // 多用了两处同一判据，按使用点数会把"复用同一处"读成"起了第二处炉灶"。取舍如实记在这里 ——
    // 它比原先弱一点：挡得住"另写一份算资源成败的逻辑"，挡不住"同一份判据被到处调用"（后者本来就不是违规）。
    // 落点口径那条仍按使用点数（4 = 定义 + mf/ark/投影），因为这一轮它的使用点数一字未变。
    let src_head: &str = SRC.split("fn selftest(").next().unwrap_or("");
    chk(src_head.matches(outcome_spell).count() == 1, "资源成败的判据有了第二处定义：执行面与信封面会各说各话", &mut fails, &mut total);
    chk(src_head.matches(eff_spell).count() == 4, "落点口径没共用同一处（定义 + mf/ark/投影 三处使用）", &mut fails, &mut total);
    // 10-03 ③ 同款：not-attempted 的补法也必须是同一处定义（从计划现取、不另写清单）。
    // 与上面两条同一族（只数定义点）：挡的是"另起一处补法"，不是"同一处被正规调用"。
    let fill_def: &str = &["fn ", "fill", "_not_attempted("].concat();
    chk(src_head.matches(fill_def).count() == 1, "not-attempted 的补法起了第二处定义：执行面各补各的，静默缺口会换个地方长回来", &mut fails, &mut total);

    // 7b. 两枚时间字段的口径（10-02 那一遍实跑量出来的两处假读数，改的是**语义**不是字段名）
    // ① `rel_ms` 的基线是 `OnceLock` 首次调用才落 ⇒ 必须在进连接段时就叫一次，否则首枚事件恒 0；
    // ② `waited_ms` 原先只在执行段末尾赋值 ⇒ 提前 return 的档交出的 0 只证"没走到那行"、不证"没等"。
    // 两枚都按同一族"判据只住一处"的尺子钉：这里数的是**定义点与调用位**，不是掐现场时间。
    chk(elapsed_ms(0, 50_000) == 5, "挂钟等值换算错：FILETIME 一格 100ns，50_000 格该是 5 毫秒", &mut fails, &mut total);
    chk(elapsed_ms(100, 50) == 0, "时钟倒退没饱和成 0：绕回大数会把 waited_ms 印成一枚荒谬读数", &mut fails, &mut total);
    chk(elapsed_ms(0, 10_000_000) == 1000, "一秒没等于一千毫秒：两侧对账的刻度就废了", &mut fails, &mut total);
    let stamp_def: &str = &["fn ", "stamp("].concat();
    // 前导空格是**必要**的：B-2d② 起 src_head 里多了一族 `day_stamp(` / `gd_day_stamp(`（本地日期读数，
    // 与这枚退出位 setter 毫无关系），裸 `stamp(` 会把七枚日期位一起数进来、把这一格顶成假红。
    // 收紧针而不是把期望从 8 改成 15：这一格数的是"connect_run 的退出路有没有漏接 setter"，
    // 改成 15 就等于把判据换成"日期件加了几个"，下次再加一枚日期函数它还是红，而红得没人看得懂。
    let stamp_any: &str = &["", " stamp("].concat();
    let rel_seed: &str = &["let _ = ", "rel_ms("].concat();
    let waited_set: &str = &["r.", "waited_ms = "].concat();
    chk(src_head.matches(waited_set).count() == 1, "waited_ms 有了第二处赋值：那枚恒 0 的读数会换个地方长回来", &mut fails, &mut total);
    chk(src_head.matches(stamp_def).count() == 1, "退出位 setter 起了第二处定义：两套口径会各说各话", &mut fails, &mut total);
    // 8 = 定义 1 + 出口 7（六道前置检查含 DLL 加载失败 + 正常收尾）。
    // 这一格**故意**会随"加一条退出路"而红：漏接一条就又交出一枚恒 0 的 waited_ms，
    // 所以宁可让人来 +1，也不放它自己悄悄过。
    chk(src_head.matches(stamp_any).count() == 8, "connect_run 的退出路与 setter 对不上了（8 = 定义 + 7 条出口），数一下是不是新加的 return 没接进 stamp", &mut fails, &mut total);
    chk(src_head.matches(rel_seed).count() == 1, "rel_ms 的入口基线调用位不唯一（或已丢）：首枚事件又会交 0 那枚假读数", &mut fails, &mut total);

    // 8. 脱敏与哈希：对方回调里的原文**默认当作会漏本机路径**来处理
    let sc = scrub(r#"{"path":"D:\Some\Where\MaaCore.dll","name":"x"}"#);
    chk(!sc.contains("D:\\") && sc.contains("<本机路径>"), "scrub 没剥掉盘符路径", &mut fails, &mut total);
    chk(scrub("plain text 无路径").contains("plain text"), "scrub 把正常内容也吃了", &mut fails, &mut total);
    chk(truncate_chars("中文abc", 3) == "中文a", "按字节截断会切出半个汉字", &mut fails, &mut total);
    chk(fnv1a64(b"abc") == fnv1a64(b"abc") && fnv1a64(b"abc") != fnv1a64(b"abd"), "哈希不稳定：等值对拍判据废了", &mut fails, &mut total);

    // 9. 事件流装配（这一段不加载任何 DLL，只走我们自己的记账逻辑）
    let before = events().lock().map(|g| g.len()).unwrap_or(0);
    push_event("asst", "2".to_string(), "ConnectionInfo".to_string(), r#"{"what":"ScreencapCost","avg":56}"#);
    push_event("framework", String::new(), "Summary.JsonCpp".to_string(), "raw-body");
    let guard = events().lock().map(|g| g.clone()).unwrap_or_default();
    chk(guard.len() == before + 2, "两条事件只落了一条（序号面不可信）", &mut fails, &mut total);
    if guard.len() >= 2 {
        let last = &guard[guard.len() - 1];
        let prev = &guard[guard.len() - 2];
        chk(last.seq == prev.seq + 1, "自增序号不连续：丢失语义没法对账", &mut fails, &mut total);
        chk(prev.thread == last.thread, "同一线程上两条事件线程号不同（取的是别的值）", &mut fails, &mut total);
        chk(last.abi == "framework" && prev.abi == "asst", "abi 标签落错", &mut fails, &mut total);
        chk(last.raw_hash == fnv1a64(b"raw-body"), "哈希不是原文的哈希", &mut fails, &mut total);
        chk(prev.rel_ms >= 0 && last.rel_ms >= prev.rel_ms, "时间戳口径不是非负单调", &mut fails, &mut total);
    }

    // 10. 号↔名表：`Message` 枚举里 **Error 排在段位首位**（10000/20000），Start 是 …0001。
    //     按"Start 打头"的直觉写这张表会整段错位一格，而错位后仍是合法事件名、不会报错 —— 所以钉死。
    chk(ark_msg_name(10000) == "TaskChainError", "号 10000 不是 TaskChainError（Error 不在段位首位就是抄错了）", &mut fails, &mut total);
    chk(ark_msg_name(10001) == "TaskChainStart", "号 10001 不是 TaskChainStart", &mut fails, &mut total);
    chk(ark_msg_name(20000) == "SubTaskError", "号 20000 不是 SubTaskError", &mut fails, &mut total);
    chk(ark_msg_name(20001) == "SubTaskStart", "号 20001 不是 SubTaskStart", &mut fails, &mut total);
    chk(ark_msg_name(2) == "ConnectionInfo", "号 2 不是 ConnectionInfo（低段位三个都查过才敢用）", &mut fails, &mut total);
    chk(ark_msg_name(3) == "AllTasksCompleted" && ark_msg_name(4) == "AsyncCallInfo", "低段位 3/4 的名字互换了位置", &mut fails, &mut total);
    chk(ark_msg_name(30000) == "ReportRequest", "号 30000 不是 ReportRequest", &mut fails, &mut total);
    chk(ark_msg_name(9999) == "UnknownMessage" && ark_msg_name(0) == "InternalError", "未知号没兜住、或 0 号被当成未知", &mut fails, &mut total);
    let msg_nums: [i32; 17] = [
        0, 1, 2, 3, 4, 5, 10000, 10001, 10002, 10003, 10004, 20000, 20001, 20002, 20003, 20004, 30000,
    ];
    let mut names: Vec<&'static str> = Vec::new();
    for m in msg_nums.iter() {
        let n = ark_msg_name(*m);
        if n != "UnknownMessage" {
            names.push(n);
        }
    }
    let uniq = names.iter().collect::<std::collections::HashSet<_>>().len();
    chk(names.len() == 17 && uniq == 17, "17 个具名号没能给出 17 个不同名字（表有重复或漏项）", &mut fails, &mut total);

    // 11. 信封落点：--out 是"我方机读面"的干净出口，写不下去时必须把读数交回 stdout。
    //     这一条不是便利功能：实测两枚缺 plugins 目录的树把对方加载期 ERROR 打进** stdout **，
    //     "stdout 一行 JSON"的口径在真装包上不成立（旧读数表把这条写成 stderr，是错的，见自我纠正 6）。
    let (f_stdout, line_stdout) = render_envelope("{}", &None);
    chk(f_stdout.is_none() && line_stdout == "{}", "没给 --out 时信封该原样走 stdout", &mut fails, &mut total);
    let unwritable = Some("Z:\\gdprobe-不存在\\envelope.json".to_string());
    let (f_fail, line_fail) = render_envelope(r#"{"ok":true}"#, &unwritable);
    chk(
        f_fail.is_none() && line_fail.contains(r#""ok":true"#),
        "--out 写不下去时两头都空：读数会凭空消失，宁可从 stdout 交出来",
        &mut fails,
        &mut total,
    );
    // 出信封的三个 rep-based 出口（只读段 / 连接段拒跑 / 连接段实跑）都得走同一个 emit ——
    // 环境闸那一处是手搓的 format!，不在这条 needle 的射程里，它的 --out 靠实跑验（--preflight --out）。
    let emit_needle: &str = &["&rep", ".to_json()"].concat();
    chk(
        SRC.matches(emit_needle).count() == 3,
        "rep 出信封的地方不是三处且都走 emit：--out 会出现'某一档默默忽略'的假口径",
        &mut fails,
        &mut total,
    );

    // 12. R1 归一层的映射形状（全用手搓的 `Event`，不加载 DLL、不推环 —— 推环那一步在第 9 节已经测过）。
    //     这一节的重点不是"名字对不对得上"，而是**三份各存各的**：对方原名逐字进 `raw_name`，
    //     我方的档位进 `domain` / `state`。哪天有人把 `name` 改成"自己发明的统一名"，这几条就红。
    let mk_event = |seq: u64, abi: &'static str, code: &str, msg: &str, thread: u32| Event {
        seq,
        abi,
        code: code.to_string(),
        message: msg.to_string(),
        thread,
        rel_ms: seq as i64 * 10,
        raw_len: 38,
        raw_hash: 7,
        details: "{}".to_string(),
    };
    let a_start = normalize(&mk_event(1, "asst", "10001", "TaskChainStart", 111), 111);
    chk(
        a_start.raw_name == "TaskChainStart" && a_start.name == "TaskChainStart",
        "归一之后对方原名不在了：ADR-2 字段名那一维的『以对方给的名字为准』丢了",
        &mut fails,
        &mut total,
    );
    chk(a_start.domain == "task_chain" && a_start.state == "Start", "遗留侧 1xxxx 段没归到 task_chain/Start", &mut fails, &mut total);
    chk(a_start.ts_source == "local", "时间戳没打标：『我补的』会被当成『它测的』拿去对拍", &mut fails, &mut total);
    chk(a_start.on_main_thread, "同线程号却判了非主线程", &mut fails, &mut total);
    // 实测连接段两次都是**跨线程**回调（32304 vs 32520、20284 vs 32412），所以"不等 ⇒ false"这一支
    // 必须有门 —— --events 那一档的事件是我方自己推的，恒为 true，构造上就给不出这一支。
    chk(!normalize(&mk_event(1, "asst", "10001", "TaskChainStart", 111), 999).on_main_thread, "线程号不等却判了主线程：跨线程读数会被抹平", &mut fails, &mut total);
    let a_info = normalize(&mk_event(2, "asst", "2", "ConnectionInfo", 111), 111);
    chk(a_info.domain == "core" && a_info.state == "Info", "遗留侧低段位 0..=5 没归到 core", &mut fails, &mut total);
    let f_start = normalize(&mk_event(3, "framework", "", "Resource.Loading.Starting", 111), 111);
    chk(f_start.domain == "Resource" && f_start.state == "Starting", "framework 侧三段名没按首/尾段拆", &mut fails, &mut total);
    chk(f_start.raw_code.is_empty(), "framework 侧 code 恒空串这一读数被改成了别的", &mut fails, &mut total);
    chk(normalize(&mk_event(4, "framework", "", "Resource.Loading.Succeeded", 111), 111).state == "Succeeded", "framework 侧终态名归错", &mut fails, &mut total);
    chk(asst_domain("9999") == "unknown" && asst_domain("not-a-number") == "unknown", "号段外或非数字没兜住（会静默进某一档）", &mut fails, &mut total);
    chk(state_of("Summary.JsonCpp") == "JsonCpp", "认不出尾段时没退成最后一段", &mut fails, &mut total);
    // B-2e 乙-2 第二枪：`terminal=` 只给「末条命中」，而真事件流里 `TaskChainError` 之后还会到一条
    //   `AllTasksCompleted`（那条的本义是队列散场，它带的 details 是 finished_tasks）——
    //   所以命中册 `terminal_hits` 必须单独有门。这一格不是装饰：首枪实测就是靠它才看出
    //   「succeeded」与「这轮砸了」同现。事件全用手搓的，不加载 DLL、不推环。
    let hit_ring: Vec<NormEvent> = vec![
        normalize(&mk_event(1, "asst", "10001", "TaskChainStart", 222), 111),
        normalize(&mk_event(2, "asst", "10000", "TaskChainError", 222), 111),
        normalize(&mk_event(3, "asst", "3", "AllTasksCompleted", 222), 111),
    ];
    chk(terminal_hits(&hit_ring) == vec!["failed", "succeeded"],
        "命中册没按环序把两枚终态都记下来：『这一轮见过哪几枚终态』又只剩末条", &mut fails, &mut total);
    chk(hit_ring.last().is_some() && terminal_hits(&hit_ring).last() == Some(&"succeeded"),
        "末条口径与 terminal_of 分家了：命中册最后一枚就该是 terminal= 的那一枚", &mut fails, &mut total);
    // 空环与「只起始不见收尾」都必须交空册 —— 前者是压根没回调，后者正是 AsstStart 之后无下文的形状。
    chk(terminal_hits(&Vec::new()).is_empty(), "空环给不出空命中册", &mut fails, &mut total);
    chk(terminal_hits(&vec![normalize(&mk_event(1, "asst", "10001", "TaskChainStart", 222), 111)]).is_empty(),
        "只有 TaskChainStart 就出了命中册：把「起了个头」读成了终态", &mut fails, &mut total);
    // 折叠只对相邻同枚生效：中间夹了别的终态再回头，那一枚必须重新记（succeeded,stopped,succeeded 是真形状）。
    let folded: Vec<NormEvent> = vec![
        normalize(&mk_event(1, "asst", "3", "AllTasksCompleted", 222), 111),
        normalize(&mk_event(2, "asst", "3", "AllTasksCompleted", 222), 111),
        normalize(&mk_event(3, "asst", "10002", "TaskChainStopped", 222), 111),
        normalize(&mk_event(4, "asst", "3", "AllTasksCompleted", 222), 111),
    ];
    chk(terminal_hits(&folded) == vec!["succeeded", "stopped", "succeeded"],
        "相邻同枚没折、或不相邻的也被折了：命中册既会虚胖也会漏掉「散场后又报错」这种回头形状", &mut fails, &mut total);
    // 命中册是**读数**不是判据：它不许进任何一份名单，否则 ok 会跟着册子的长短一起漂。
    chk(!task_stalled(&vec!["terminal_seen=failed,succeeded".to_string()]),
        "命中册被算进「任务面没收束」：读数当了判据，两枚终态同现的一轮会当场自红", &mut fails, &mut total);
    chk(!task_stalled(&vec!["terminal_seen=none".to_string()]),
        "terminal_seen= 里的 none 没被当成标签本体：精确相等那一条被前缀放宽了", &mut fails, &mut total);
    chk(!worker_unresolved(&vec!["terminal_seen=failed,succeeded".to_string()]),
        "命中册被算进「现场没清」：模块释放会跟着册子形状走", &mut fails, &mut total);

    // 13. R1 的三条判据：序号对账 / 终态判据 / 出口裁剪。
    //     ① **连续不等于没丢**：没有"已知应到数"只能报 unverified，这是 `loss_semantics` 那一格
    //        在 R0 跑两遍都没被替换的原因（两遍各收到自己那 2 条，没有参照流）。
    let contig = loss_check(&[1, 2, 3, 4], None);
    chk(
        contig.contiguous && contig.verdict == "unverified" && contig.gaps.is_empty(),
        "连续 + 没给应到数 ⇒ 必须 unverified；判成『没丢』就是那一格的假绿",
        &mut fails,
        &mut total,
    );
    chk(loss_check(&[1, 2, 3], Some(3)).verdict == "expected-met", "对上应到数却没判 met", &mut fails, &mut total);
    chk(
        loss_check(&[1, 2], Some(3)).verdict == "expected-mismatch",
        "『该到 3 只到 2』没判 mismatch：这种读数出去了没人管",
        &mut fails,
        &mut total,
    );
    // 条数对上**不等于**没缺号：1,2,4 收到 3 条、应到 3 条 —— 必须报 gapped 并指出缺的是 3 号。
    let gapped = loss_check(&[1, 2, 4], Some(3));
    chk(
        gapped.verdict == "gapped" && gapped.gaps == vec![3u64] && gapped.received == 3 && gapped.first_seq == 1,
        "缺口没被抓出来（或没指出缺哪个号）：条数对不上号时的『齐全』是假的",
        &mut fails,
        &mut total,
    );
    let empty_loss = loss_check(&[], None);
    chk(empty_loss.received == 0 && empty_loss.verdict == "unverified", "空环既不许判『没丢』也不许判 met", &mut fails, &mut total);

    //     ② 成败**只认 message**：夹具 framework 那两条的 details 原文逐字相同 ⇒ 环里两枚 hash 相同，
    //        code 又恒为空串，判据仍然要把"起始 / 终态"分开 —— 这就是那条判读错的证伪样本。
    let fe = fixture_events("framework");
    chk(
        fixture_events("asst").len() == 3
            && fe.len() == 2
            && fixture_events("both").len() == 5
            && fixture_events("asst-stopped").len() == 2
            && fixture_events("asst-failed").len() == 2,
        "夹具条数变了：--expected 的示例读数与旁路对拍门的基线都会跟着漂",
        &mut fails,
        &mut total,
    );
    chk(
        fnv1a64(fe[0].3.as_bytes()) == fnv1a64(fe[1].3.as_bytes()),
        "夹具那两条的原文 hash 不相等：证伪样本本身失效了（按 hash 判不成反证）",
        &mut fails,
        &mut total,
    );
    let fw_start_n = normalize(&mk_event(1, "framework", fe[0].1.as_str(), fe[0].2.as_str(), 111), 222);
    let fw_done_n = normalize(&mk_event(2, "framework", fe[1].1.as_str(), fe[1].2.as_str(), 111), 222);
    let fw_fail_n = normalize(&mk_event(3, "framework", "", "Resource.Loading.Failed", 111), 222);
    let judged = event_verdict(&vec![fw_start_n.clone(), fw_done_n.clone()]);
    chk(
        judged.0 == "succeeded" && judged.1.contains("Succeeded"),
        "code 恒空 + hash 相同的一对里判不出成败：『只认 message』这条落空",
        &mut fails,
        &mut total,
    );
    chk(
        event_verdict(&vec![fw_start_n]).0 == "no-terminal",
        "只有起始没有收尾也敢判成败（『ok 从不证资源』那条判读错的镜像）",
        &mut fails,
        &mut total,
    );
    chk(
        event_verdict(&vec![fw_done_n, fw_fail_n.clone()]).0 == "failed",
        "末条终态没盖过前一条：环序里最后命中的那条才算",
        &mut fails,
        &mut total,
    );
    chk(
        terminal_of("AllTasksCompleted") == Some("succeeded")
            && terminal_of("TaskChainError") == Some("failed")
            && terminal_of("TaskChainStopped") == Some("stopped"),
        "遗留侧三个终态名判错",
        &mut fails,
        &mut total,
    );
    // R3①b：任务取消 / 任务失败两档负例走完整链（夹具 → 归一 → 终态判据），verdict 必须跟着翻，
    // 而不是被"起始 + 成功终态"那种正例盖出来的恒 succeeded 蒙混过去。
    let fs_stop: Vec<NormEvent> = fixture_events("asst-stopped")
        .iter()
        .enumerate()
        .map(|(i, t)| normalize(&mk_event(i as u64 + 1, t.0, t.1.as_str(), t.2.as_str(), 111), 111))
        .collect();
    let fs_fail: Vec<NormEvent> = fixture_events("asst-failed")
        .iter()
        .enumerate()
        .map(|(i, t)| normalize(&mk_event(i as u64 + 1, t.0, t.1.as_str(), t.2.as_str(), 111), 111))
        .collect();
    let v_stop = event_verdict(&fs_stop);
    let v_fail = event_verdict(&fs_fail);
    chk(
        v_stop.0 == "stopped" && v_stop.1.contains("TaskChainStopped"),
        "任务取消样本没判成 stopped：负例在事件流面没有出口",
        &mut fails,
        &mut total,
    );
    chk(
        v_fail.0 == "failed" && v_fail.1.contains("TaskChainError"),
        "任务失败样本没判成 failed：负例在事件流面没有出口",
        &mut fails,
        &mut total,
    );
    chk(terminal_of("ConnectionInfo").is_none() && fw_fail_n.state == "Failed", "非终态事件被判成终态", &mut fails, &mut total);

    //     ③ 出口裁剪：对外那一份**不带** raw_len / raw_hash（实测它们是落点路径长度与原文指纹的直接读数）。
    //        两道：一道数**函数体**（结构上就不许出现），一道数**渲染出来的信封**（真跑出来的读数里也不许有）。
    let body_needle: &str = &["fn render_events", "_json("].concat();
    let body_from = SRC.find(body_needle);
    let body_tail = if let Some(at) = body_from { &SRC[at..] } else { "" };
    let body_to = body_tail.find("\n}").unwrap_or(body_tail.len());
    let body = &body_tail[..body_to];
    chk(!body.is_empty(), "在自家源码里没定位到事件流信封的装配函数：下面那两条裁剪门就在空转", &mut fails, &mut total);
    let cut_len: &str = &["raw_", "len"].concat();
    let cut_hash: &str = &["raw_", "hash"].concat();
    chk(
        !body.contains(cut_len) && !body.contains(cut_hash),
        "装配函数里出现了被裁掉的那两枚读数：ADR §6-3 第①条破约",
        &mut fails,
        &mut total,
    );
    let asst_two: Vec<NormEvent> = vec![
        normalize(&mk_event(1, "asst", "10001", "TaskChainStart", 111), 111),
        normalize(&mk_event(2, "asst", "3", "AllTasksCompleted", 111), 111),
    ];
    let json = render_events_json("asst", 111, 0, &asst_two, &loss_check(&[1, 2], Some(2)));
    chk(
        json.contains(r#""loss":"#) && json.contains(r#""verdict":"succeeded""#),
        "事件流出口没有 loss / verdict 这两格：对拍拿不到判据",
        &mut fails,
        &mut total,
    );
    chk(
        !json.contains(cut_len) && !json.contains(cut_hash),
        "渲染出来的事件流信封里出现了 raw_len/raw_hash",
        &mut fails,
        &mut total,
    );
    chk(json.matches(r#""seq":"#).count() == 2, "events 数组条数与序号读数不一致", &mut fails, &mut total);
    let empty_json = render_events_json("asst", 111, 0, &[], &loss_check(&[], None));
    chk(
        empty_json.contains(r#""ok":false"#) && empty_json.contains("no-terminal"),
        "空环给出 ok:true：这一档会把自己没收到事件说成跑通了",
        &mut fails,
        &mut total,
    );
    // --events、--sv、--farm-intent、--arkskip、--drive-ark-daily、--maa-run、--m9a-run 与 --drive-m9a-daily
    // 八档都必须并入同一条 emit 出口（否则 --out 出现"某一档默默忽略"的假口径），
    // 且各自的分派必须落在「--kind 那条必填」**之前**：这八档都不需要装包，被必填拦死就等于没开。
    // （驱动档 R3③ 是第五档：它同样不装包 —— 现场动作全由壳侧照 ask 执行，探针侧只推链与记账。
    //   运行面回放档 B-2a 是第六档：它也不装包、不碰现场，现场动作全落成投影事件。
    //   m9a 运行面回放档 B-2d② 是第七档：与第六档同一条形状，只是循环体换枚、剧本面多日期与 adb。
    //   m9a 驱动档 B-2d④ 是第八档：与第五档同一条协议、同一套记账，只是链体换枚、名册换一枚 ctx 子对象。）
    // 这里数的是**整份** SRC 而不是 src_head：分派在 run() 里，而 run() 排在 selftest **之后**，
    // 用 src_head 会两处都找不到、双双退成 MAX 再判成红（这一族"针落在射程外"的坑本轮第二次踩）。
    // needle 一律运行期拼，**且失败文案里也不许把那串字面量原样写出来**：整份一起数时，
    // 自家标签先于 run() 命中，门就红在一句人话上（本轮真踩：文案里的 `--kind` 与那条必填连起来写，
    // SRC.find 先逮到的是这条文案自己 —— 同一族坑第四次，判据读的是自己就必须把自己排除在射程外）。
    // 装配回放档同理只保留**一处** emit 调用（错误路径用局部 rc + 单出口收束）：计数断言本身就
    // 是"出口唯一"的守门人，多开一处它先红。两枚驱动档同理 —— 唯一 emit 各收在 run_drive_* 里。
    chk(
        SRC.matches(&["emit(&", "json"].concat()).count() == 9,
        "事件流/监督判定/farm-intent/装配回放/驱动/运行面回放/m9a 运行面回放/m9a 驱动/轮骨架九档没同走一处 emit：--out 在其中一档上是空头承诺",
        &mut fails,
        &mut total,
    );
    let dispatch_at = SRC.find(&["run_events(", "&fixture"].concat()).unwrap_or(usize::MAX);
    let kind_gate_at = SRC.find(&["--kind ", "必填"].concat()).unwrap_or(usize::MAX);
    chk(
        dispatch_at < kind_gate_at,
        "--events 的分派落在那条必填拦路虎之后：这一档会被装包必填拦死，『只吃夹具』成了空话",
        &mut fails,
        &mut total,
    );
    let sv_dispatch_at = SRC.find(&["run_sv(", "&sv_date"].concat()).unwrap_or(usize::MAX);
    chk(
        sv_dispatch_at < kind_gate_at,
        "--sv 的分派落在那条必填拦路虎之后：监督判定这一档会被装包必填拦死，『只吃夹具』成了空话",
        &mut fails,
        &mut total,
    );
    let ak_dispatch_at = SRC.find(&["run_arkskip(", "&panel_cs"].concat()).unwrap_or(usize::MAX);
    chk(
        ak_dispatch_at < kind_gate_at,
        "--arkskip 的分派落在那条必填拦路虎之后：装配回放这一档会被装包必填拦死，『只吃夹具』成了空话",
        &mut fails,
        &mut total,
    );
    let dr_dispatch_at = SRC.find(&["run_drive_ark_daily(", "i_drive"].concat()).unwrap_or(usize::MAX);
    chk(
        dr_dispatch_at < kind_gate_at,
        "--drive-ark-daily 的分派落在那条必填拦路虎之后：换头调度器起步即断（这一档同样不装包）",
        &mut fails,
        &mut total,
    );
    let mr_dispatch_at = SRC.find(&["run_maa_run(", "&run_script"].concat()).unwrap_or(usize::MAX);
    chk(
        mr_dispatch_at < kind_gate_at,
        "--maa-run 的分派落在那条必填拦路虎之后：运行面回放这一档会被装包必填拦死，对拍门根本起不了它",
        &mut fails,
        &mut total,
    );
    let m9_dispatch_at = SRC.find(&["run_m9a_run(", "&run_script"].concat()).unwrap_or(usize::MAX);
    chk(
        m9_dispatch_at < kind_gate_at,
        "--m9a-run 的分派落在那条必填拦路虎之后：m9a 运行面回放这一档会被装包必填拦死，新门根本起不了它",
        &mut fails,
        &mut total,
    );
    // m9a 驱动档（B-2d④）：这一档比 --m9a-run 更该躲开必填 —— 换头调度器起步第一枪就是它，
    // 被拦死等于 1999 那一轮直接不跑（而且它的落点全在 ctx 名册里，那两枚必填根本没有对应物）。
    let dm_dispatch_at = SRC.find(&["run_drive_m9a_daily(", "i_drive"].concat()).unwrap_or(usize::MAX);
    chk(
        dm_dispatch_at < kind_gate_at,
        "--drive-m9a-daily 的分派落在那条必填拦路虎之后：1999 的换头调度器起步即断（这一档同样不装包）",
        &mut fails,
        &mut total,
    );

    // 14. R2 监督四段：逐条转写 tools\panel-cs\Supervisor.SelfTest.cs（56 项 = 16+9+14+17）。
    //     期望字面逐字抄自 lib\supervisor.ps1 的旧实现（四句 Why、现值头、拍行、"读不到/没问/none/原因"话术）——
    //     这里红了就是移植走样，改 sv_*，不改期望（§7 同一条纪律）。不加载任何 DLL、不碰现场：
    //     证据/状态/画面/文档全部由场景直接喂进来。
    let sv_gi = |o: &JVal, k: &str| -> i64 { jv_int(&jv_get(o, k), -1) };
    let sv_gb = |o: &JVal, k: &str| -> bool { jv_truthy(&jv_get(o, k)) };
    let sv_gs = |o: &JVal, k: &str| -> String { jv_str(&jv_get(o, k)) };
    let sv_lines = |r: &JVal| -> Vec<String> {
        let lv = jv_get(r, "Lines");
        jv_list(&lv).into_iter().map(|v| jv_str(v)).collect()
    };
    let sv_has = |ls: &Vec<String>, frag: &str| -> bool { ls.iter().any(|s| s.contains(frag)) };
    let sv_cnt = |ls: &Vec<String>, frag: &str| -> usize { ls.iter().filter(|s| s.contains(frag)).count() };
    let sv_ev = |done: JVal, alive: JVal, aa: JVal, da: JVal| -> JVal {
        jv_o(vec![("Done", done), ("AssistantAlive", alive), ("AsstAgeSec", aa), ("DailyAgeSec", da)])
    };
    let sv_scr = |ok: JVal, blk: JVal, what: JVal, why: JVal| -> JVal {
        jv_o(vec![("Ok", ok), ("Blocking", blk), ("What", what), ("Why", why)])
    };

    // —— settings（16）：没显式 false 就是开 + 三档钳制 ——
    let r = sv_settings(&jv_o(vec![]));
    chk(
        sv_gb(&r, "Enabled") && sv_gi(&r, "IntervalSec") == 60 && sv_gi(&r, "QuietSec") == 1800 && sv_gi(&r, "MaxInterventions") == 1,
        "监督设置：缺省：开/60s/30分/额度 1",
        &mut fails,
        &mut total,
    );
    let r = sv_settings(&jv_o(vec![("supervisor", JVal::B(false))]));
    chk(!sv_gb(&r, "Enabled"), "监督设置：supervisor=false 才关", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("supervisor", JVal::B(true))]));
    chk(sv_gb(&r, "Enabled"), "监督设置：supervisor=true 是开", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("supervisor", JVal::I(0))]));
    chk(!sv_gb(&r, "Enabled"), "监督设置：数值 0 按 PS 等值(x -ne $false)算关", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("supervisor", jv_s("False"))]));
    chk(!sv_gb(&r, "Enabled"), "监督设置：字符串 'False' 是关", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("supervisor", jv_s("false"))]));
    chk(!sv_gb(&r, "Enabled"), "监督设置：字符串 'false' 大小写不敏感也是关", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("supervisor", jv_s("yes"))]));
    chk(sv_gb(&r, "Enabled"), "监督设置：其余字符串按 PS 等值算开(只认 \"False\")", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("intervalSeconds", JVal::I(5))]));
    chk(sv_gi(&r, "IntervalSec") == 60, "监督设置：间隔<15s 归 60(拍太快没意义)", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("intervalSeconds", JVal::I(90))]));
    chk(sv_gi(&r, "IntervalSec") == 90, "监督设置：间隔 90s 原样", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("intervalSeconds", JVal::I(15))]));
    chk(sv_gi(&r, "IntervalSec") == 15, "监督设置：间隔踩线 15s 不钳(<15 才钳)", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("intervalSeconds", JVal::I(0))]));
    chk(sv_gi(&r, "IntervalSec") == 60, "监督设置：间隔 0 是假值 → 走缺省 60", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("quietMinutes", JVal::I(1))]));
    chk(sv_gi(&r, "QuietSec") == 300, "监督设置：静止阈值下限 5 分钟(=300 秒)", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("quietMinutes", JVal::I(45))]));
    chk(sv_gi(&r, "QuietSec") == 2700, "监督设置：静止阈值 45 分 → 2700 秒", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("maxInterventions", JVal::I(0))]));
    chk(sv_gi(&r, "MaxInterventions") == 0, "监督设置：额度可为 0(纯报告模式，0 不是 null)", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("maxInterventions", JVal::I(-3))]));
    chk(sv_gi(&r, "MaxInterventions") == 0, "监督设置：负额度钳 0", &mut fails, &mut total);
    let r = sv_settings(&jv_o(vec![("maxInterventions", JVal::I(2))]));
    chk(sv_gi(&r, "MaxInterventions") == 2, "监督设置：额度 2 原样", &mut fails, &mut total);

    // —— state（9）：done/dead/quiet/busy，判据顺序是认真的 ——
    let r = sv_state(&jv_o(vec![
        ("evidence", sv_ev(JVal::B(true), JVal::B(false), JVal::I(9999), JVal::I(9999))),
        ("everAlive", JVal::B(true)),
    ]));
    chk(sv_gs(&r, "State") == "done", "监督状态：完成线压死一切：进程没了也判 done", &mut fails, &mut total);
    let r = sv_state(&jv_o(vec![
        ("evidence", sv_ev(JVal::B(false), JVal::B(false), JVal::I(10), JVal::I(10))),
        ("everAlive", JVal::B(true)),
    ]));
    chk(sv_gs(&r, "State") == "dead", "监督状态：曾活着+现在没了+没完成 = dead", &mut fails, &mut total);
    let r = sv_state(&jv_o(vec![
        ("evidence", sv_ev(JVal::B(false), JVal::B(false), JVal::I(9999), JVal::I(10))),
        ("everAlive", JVal::B(false)),
    ]));
    chk(sv_gs(&r, "State") == "busy", "监督状态：助手从没活着过 ≠ dead(开轮太早)", &mut fails, &mut total);
    let r = sv_state(&jv_o(vec![
        ("evidence", sv_ev(JVal::B(false), JVal::B(true), JVal::I(9999), JVal::I(9999))),
        ("quietSec", JVal::I(1800)),
    ]));
    chk(sv_gs(&r, "State") == "quiet", "监督状态：两本日志都超阈值才 quiet", &mut fails, &mut total);
    let r = sv_state(&jv_o(vec![
        ("evidence", sv_ev(JVal::B(false), JVal::B(true), JVal::I(9999), JVal::I(10))),
        ("quietSec", JVal::I(1800)),
    ]));
    chk(sv_gs(&r, "State") == "busy", "监督状态：只有助手日志老 = 不算静止", &mut fails, &mut total);
    let r = sv_state(&jv_o(vec![
        ("evidence", sv_ev(JVal::B(false), JVal::B(true), JVal::I(10), JVal::I(9999))),
        ("quietSec", JVal::I(1800)),
    ]));
    chk(sv_gs(&r, "State") == "busy", "监督状态：只有主日志老 = 不算静止", &mut fails, &mut total);
    let r = sv_state(&jv_o(vec![
        ("evidence", sv_ev(JVal::B(false), JVal::B(true), JVal::I(1800), JVal::I(1800))),
        ("quietSec", JVal::I(1800)),
    ]));
    chk(sv_gs(&r, "State") == "busy", "监督状态：刚好踩在阈值上不算静止(严格大于)", &mut fails, &mut total);
    // 这一档刻意**不带** everAlive 键：C# 读法"缺键=null 是假"，与转发壳的缺省 $true 不同 ——
    // 归一形状里它恒在，这条钉的是函数本体对缺键的读法。
    let r = sv_state(&jv_o(vec![("evidence", sv_ev(JVal::B(false), JVal::B(true), JVal::Null, JVal::Null))]));
    chk(sv_gs(&r, "State") == "busy", "监督状态：ages 缺失按 0 收 → busy", &mut fails, &mut total);
    let r = sv_state(&jv_o(vec![
        ("evidence", sv_ev(JVal::B(false), JVal::B(true), JVal::F(1800.5), JVal::F(1801.25))),
        ("quietSec", JVal::I(1800)),
    ]));
    chk(sv_gs(&r, "State") == "quiet", "监督状态：Decimal 年龄(信封里小数)也参与比较", &mut fails, &mut total);

    // —— action（14）：决策表 + 四句 Why 逐字 ——
    let r = sv_action(&jv_o(vec![("state", jv_s("busy"))]));
    chk(sv_gs(&r, "Kind") == "none", "监督动作：busy 不动", &mut fails, &mut total);
    let r = sv_action(&jv_o(vec![("state", jv_s("done"))]));
    chk(sv_gs(&r, "Kind") == "none", "监督动作：done 不动", &mut fails, &mut total);
    let r = sv_action(&jv_o(vec![("state", jv_s("DONE"))]));
    chk(sv_gs(&r, "Kind") == "none", "监督动作：DONE 大写也不动(PS -eq 大小写不敏感)", &mut fails, &mut total);
    let r = sv_action(&jv_o(vec![("state", jv_s("dead"))]));
    chk(
        sv_gs(&r, "Kind") == "notify" && sv_gs(&r, "Why") == "助手进程不见了，而这一轮还没宣布完成",
        "监督动作：dead 通知一次，话术逐字",
        &mut fails,
        &mut total,
    );
    let r = sv_action(&jv_o(vec![("state", jv_s("dead")), ("alreadyNotified", JVal::B(true))]));
    chk(sv_gs(&r, "Kind") == "none", "监督动作：dead 通知过就闭嘴", &mut fails, &mut total);
    let r = sv_action(&jv_o(vec![
        ("state", jv_s("quiet")),
        ("quietStreak", JVal::I(1)),
        ("screen", sv_scr(JVal::B(true), JVal::B(false), jv_s("x"), jv_s(""))),
        ("canRestart", JVal::B(true)),
    ]));
    chk(sv_gs(&r, "Kind") == "none", "监督动作：静止一拍不动手(可能是抖动)", &mut fails, &mut total);
    let r = sv_action(&jv_o(vec![
        ("state", jv_s("quiet")),
        ("quietStreak", JVal::I(2)),
        ("screen", sv_scr(JVal::B(true), JVal::B(true), jv_s("活动弹窗"), jv_s(""))),
        ("canRestart", JVal::B(true)),
    ]));
    chk(
        sv_gs(&r, "Kind") == "none" && sv_gs(&r, "Why") == "画面被「活动弹窗」挡着，交给主流程兜底",
        "监督动作：blocking 交给主流程兜底，话术逐字",
        &mut fails,
        &mut total,
    );
    let r = sv_action(&jv_o(vec![
        ("state", jv_s("quiet")),
        ("quietStreak", JVal::I(2)),
        ("screen", sv_scr(JVal::B(true), JVal::B(false), jv_s("日常界面"), jv_s(""))),
        ("canRestart", JVal::B(true)),
        ("interventions", JVal::I(0)),
        ("maxInterventions", JVal::I(1)),
    ]));
    chk(
        sv_gs(&r, "Kind") == "restart_assistant" && sv_gs(&r, "Why") == "连续静止且画面没有挡路的东西(日常界面)，判定层没醒，重启助手",
        "监督动作：静止+画面干净+许重启 = restart_assistant，话术逐字",
        &mut fails,
        &mut total,
    );
    let r = sv_action(&jv_o(vec![
        ("state", jv_s("quiet")),
        ("quietStreak", JVal::I(2)),
        ("screen", sv_scr(JVal::B(true), JVal::B(false), jv_s("日常界面"), jv_s(""))),
        ("canRestart", JVal::B(true)),
        ("interventions", JVal::I(1)),
        ("maxInterventions", JVal::I(1)),
    ]));
    chk(
        sv_gs(&r, "Kind") == "notify" && sv_gs(&r, "Why") == "连续静止、画面没有弹窗，但这一路不许自动重启(或额度用完)",
        "监督动作：额度用完退回 notify，话术逐字",
        &mut fails,
        &mut total,
    );
    let r = sv_action(&jv_o(vec![
        ("state", jv_s("quiet")),
        ("quietStreak", JVal::I(2)),
        ("screen", sv_scr(JVal::B(true), JVal::B(false), jv_s("日常界面"), jv_s(""))),
        ("canRestart", JVal::B(false)),
    ]));
    chk(sv_gs(&r, "Kind") == "notify", "监督动作：这一路不许重启 → 只 notify", &mut fails, &mut total);
    let r = sv_action(&jv_o(vec![
        ("state", jv_s("quiet")),
        ("quietStreak", JVal::I(2)),
        ("screen", sv_scr(JVal::B(false), JVal::B(false), jv_s(""), jv_s("截图失败"))),
    ]));
    chk(
        sv_gs(&r, "Kind") == "notify" && sv_gs(&r, "Why") == "连续静止，且看不清画面(截图失败)",
        "监督动作：看不清画面只 notify，话术逐字带上下文",
        &mut fails,
        &mut total,
    );
    let r = sv_action(&jv_o(vec![
        ("state", jv_s("quiet")),
        ("quietStreak", JVal::I(2)),
        ("screen", sv_scr(JVal::B(false), JVal::B(false), jv_s(""), jv_s(""))),
    ]));
    chk(
        sv_gs(&r, "Kind") == "notify" && sv_gs(&r, "Why") == "连续静止，且看不清画面",
        "监督动作：没上下文就不缀括号",
        &mut fails,
        &mut total,
    );
    let r = sv_action(&jv_o(vec![
        ("state", jv_s("quiet")),
        ("quietStreak", JVal::I(3)),
        ("screen", sv_scr(JVal::B(false), JVal::B(false), jv_s(""), jv_s(""))),
        ("alreadyNotified", JVal::B(true)),
    ]));
    chk(sv_gs(&r, "Kind") == "none", "监督动作：notify 说过不再重复轰炸", &mut fails, &mut total);
    let r = sv_action(&jv_o(vec![("state", jv_s("whatever")), ("quietStreak", JVal::I(1))]));
    chk(sv_gs(&r, "Kind") == "none", "监督动作：未知状态按旧实现 fall-through 走 quiet 流程(一拍仍不动)", &mut fails, &mut total);

    // —— detail（17）：假文档钉整段话术 ——
    let sv_det = |sv: JVal, mb: Option<i64>| -> JVal {
        let mut kv: Vec<(&str, JVal)> = vec![("sv", sv), ("date", jv_s("20260926"))];
        if let Some(m) = mb {
            kv.push(("maxBeats", JVal::I(m)));
        }
        sv_detail(&jv_o(kv))
    };
    let r = sv_detail(&jv_o(vec![]));
    chk(sv_lines(&r).is_empty(), "监督详情：喂 null(从没监督过)返回空数组而不是抛", &mut fails, &mut total);
    let sv_g1 = jv_o(vec![
        ("game", jv_s("t1")),
        ("state", jv_s("quiet")),
        ("alive", JVal::B(true)),
        ("asst_age", JVal::I(1810)),
        ("done", JVal::B(false)),
        ("screen", jv_s("clear")),
        ("action", jv_s("notify")),
        ("why", jv_s("连续静止，额度用完")),
    ]);
    let sv_g2 = jv_o(vec![
        ("game", jv_s("t2")),
        ("state", jv_s("dead")),
        ("alive", JVal::B(false)),
        ("asst_age", JVal::I(-1)),
        ("done", JVal::B(true)),
        ("screen", JVal::Null),
        ("action", jv_s("none")),
        ("why", jv_s("")),
    ]);
    let sv_beat = jv_o(vec![
        ("ts", jv_s("2026-09-26 13:05:00")),
        ("games", JVal::A(vec![sv_g1, sv_g2])),
    ]);
    let sv_doc1 = jv_o(vec![
        ("pid", JVal::I(4321)),
        ("owner", JVal::I(1)),
        ("started", jv_s("2026-09-26 13:00:00")),
        ("games", jv_s("t1+t2")),
        ("updated", jv_s("2026-09-26 13:05:00")),
        ("stopped", jv_s("")),
        ("beats", JVal::A(vec![sv_beat])),
        ("actions", JVal::A(vec![])),
    ]);
    let ls = sv_lines(&sv_det(sv_doc1, None));
    chk(
        !ls.is_empty() && ls[0] == "现值：PID 4321 · 盯的路线 t1+t2 · 上岗 2026-09-26 13:00:00 · 最后一拍 2026-09-26 13:05:00",
        "监督详情：现值头把 PID/路线/上岗/最后一拍念全",
        &mut fails,
        &mut total,
    );
    chk(!ls.join(" ").contains("收工"), "监督详情：没收工不许出现「收工」二字", &mut fails, &mut total);
    chk(
        sv_has(&ls, "流水：logs\\supervision-20260926.jsonl(一行一拍，留 7 天)"),
        "监督详情：流水行走注入的日期戳",
        &mut fails,
        &mut total,
    );
    chk(sv_cnt(&ls, "  13:05:00 ") == 2, "监督详情：一拍两路 → 展开成两行(谁都没被合并)", &mut fails, &mut total);
    chk(
        sv_has(&ls, "t2=dead 助手日志静默 读不到") && !sv_has(&ls, "-1秒"),
        "监督详情：asst_age=-1 说成\"读不到\"，不是\"-1秒\"",
        &mut fails,
        &mut total,
    );
    chk(sv_has(&ls, "画面 没问"), "监督详情：画面没问过说\"没问\"", &mut fails, &mut total);
    chk(sv_has(&ls, "助手日志静默 1810秒"), "监督详情：数值 age 后缀\"秒\"", &mut fails, &mut total);
    chk(sv_has(&ls, "动作 none") && sv_has(&ls, "动作 notify"), "监督详情：没动作说 none，有动作原样念", &mut fails, &mut total);
    chk(sv_cnt(&ls, "原因 ") == 1, "监督详情：why 为空的那一路不缀\"原因 \"", &mut fails, &mut total);
    chk(
        sv_has(&ls, "这一轮没有介入过(它唯一的干预是重启助手，重启过会记在这里)。"),
        "监督详情：没介入过 → 明说没有",
        &mut fails,
        &mut total,
    );
    chk(!ls.join("\n").contains("|"), "监督详情：整段输出不含 '|'(面板协议分隔符)", &mut fails, &mut total);
    let sv_doc_stopped = jv_o(vec![
        ("pid", JVal::I(4321)),
        ("owner", JVal::I(1)),
        ("started", jv_s("2026-09-26 13:00:00")),
        ("games", jv_s("t1+t2")),
        ("updated", jv_s("2026-09-26 13:05:00")),
        ("stopped", jv_s("2026-09-26 13:20:00")),
        ("beats", JVal::A(vec![])),
        ("actions", JVal::A(vec![jv_o(vec![
            ("ts", jv_s("13:06:00")),
            ("game", jv_s("t1")),
            ("kind", jv_s("restart_assistant")),
            ("ok", JVal::B(false)),
            ("why", jv_s("重启没成")),
        ])])),
    ]);
    let ls = sv_lines(&sv_det(sv_doc_stopped, None));
    chk(
        !ls.is_empty() && ls[0].ends_with(" · 收工 2026-09-26 13:20:00"),
        "监督详情：收工时间真收工时出现在头行",
        &mut fails,
        &mut total,
    );
    chk(
        sv_has(&ls, "这一枚监督进程还没落下第一拍(起拍间隔默认 60 秒)。")
            && sv_has(&ls, "介入记录(1 次)：")
            && sv_has(&ls, "  13:06:00 t1 restart_assistant 没成 — 重启没成"),
        "监督详情：拍被挤掉仍报\"还没落下第一拍\"，且不吞介入记录",
        &mut fails,
        &mut total,
    );
    let sv_mkb = |ts: &str, state: &str, age: i64| -> JVal {
        jv_o(vec![
            ("ts", jv_s(ts)),
            ("games", JVal::A(vec![jv_o(vec![
                ("game", jv_s("t1")),
                ("state", jv_s(state)),
                ("alive", JVal::B(true)),
                ("asst_age", JVal::I(age)),
                ("done", JVal::B(false)),
                ("screen", jv_s("")),
                ("action", jv_s("none")),
                ("why", jv_s("")),
            ])])),
        ])
    };
    let sv_b1 = sv_mkb("2026-09-26 13:01:00", "busy", 5);
    let sv_b2 = sv_mkb("2026-09-26 13:02:00", "busy", 5);
    let sv_b3 = sv_mkb("2026-09-26 13:03:00", "quiet", 30);
    let ls = sv_lines(&sv_det(
        jv_o(vec![
            ("pid", JVal::I(1)),
            ("games", jv_s("t1")),
            ("started", jv_s("s")),
            ("updated", jv_s("u")),
            ("beats", JVal::A(vec![sv_b1.clone(), sv_b2.clone(), sv_b3.clone()])),
            ("actions", JVal::A(vec![])),
        ]),
        Some(2),
    ));
    chk(
        sv_has(&ls, "最近 2 拍(字段：状态 / 助手日志静默 / 进程 / 完成线 / 画面 / 动作)：")
            && sv_cnt(&ls, "  13:0") == 2
            && sv_has(&ls, "13:02:00")
            && sv_has(&ls, "13:03:00")
            && !sv_has(&ls, "13:01:00"),
        "监督详情：MaxBeats=2 → 只念最后两拍，头行说\"最近 2 拍\"",
        &mut fails,
        &mut total,
    );
    let ls = sv_lines(&sv_det(
        jv_o(vec![
            ("pid", JVal::I(1)),
            ("games", jv_s("t1")),
            ("started", jv_s("s")),
            ("updated", jv_s("u")),
            ("beats", JVal::A(vec![sv_b1])),
            ("actions", JVal::A(vec![])),
        ]),
        Some(0),
    ));
    chk(sv_has(&ls, "最近 1 拍"), "监督详情：MaxBeats=0 钳到 1 拍", &mut fails, &mut total);
    chk(sv_has(&ls, "画面 没问"), "监督详情：screen 空串也走\"没问\"", &mut fails, &mut total);

    // —— 夹具与出口自指（5）：三侧对拍门按名做机器对账，档数/分家/唯一性与信封五格在这里钉死 ——
    let sv_cases = sv_fixture_cases("20260926");
    chk(
        sv_cases.len() == 48,
        "监督夹具的档数不是 48：三侧对拍门的场景清单与出口 count 都会跟着漂",
        &mut fails,
        &mut total,
    );
    let n_set = sv_cases.iter().filter(|(n, _)| n.starts_with("set/")).count();
    let n_st = sv_cases.iter().filter(|(n, _)| n.starts_with("st/")).count();
    let n_a = sv_cases.iter().filter(|(n, _)| n.starts_with("a/")).count();
    let n_d = sv_cases.len() - n_set - n_st - n_a;
    chk(
        n_set == 12 && n_st == 11 && n_a == 17 && n_d == 8,
        &format!("监督夹具四族分家不对（set/st/a/d = {}/{}/{}/{}，应 12/11/17/8）", n_set, n_st, n_a, n_d),
        &mut fails,
        &mut total,
    );
    let mut sv_names: Vec<&str> = sv_cases.iter().map(|(n, _)| *n).collect();
    sv_names.sort();
    sv_names.dedup();
    chk(
        sv_names.len() == 48,
        "监督夹具出现重名档：三侧门按名对账会把漏档读成对齐",
        &mut fails,
        &mut total,
    );
    let sv_json = render_sv_json("20260926");
    chk(
        sv_json.contains(r#""ok":true"#)
            && sv_json.contains(r#""role":"R2-supervisor""#)
            && sv_json.contains(r#""fixture":"supervisor""#)
            && sv_json.contains(r#""date":"20260926""#)
            && sv_json.contains(r#""count":48"#),
        "监督判定出口信封缺格：三侧对拍门靠 role/fixture/date/count 这几格判身份",
        &mut fails,
        &mut total,
    );
    chk(
        sv_json.matches(r#""name":"#).count() == 48 && sv_json.contains("supervision-20260926.jsonl"),
        "cases 条数或日期戳注入不可达：详情八档的日期没进信封（对拍靠它钉会话日期）",
        &mut fails,
        &mut total,
    );

    // 15. R2② farm/intent 五段：逐条转写 tools\panel-cs\Orchestration.SelfTest.cs（46 项 = spec 11 + drop 6 + plan 15 + ark 11 + r19 3），
    //     加 6 项自指（分派顺序、夹具名册、信封身份）。期望字面逐字抄自该 SelfTest 与旧 pscustomobject 键序 ——
    //     这里红了就是移植走样，改 fi_*，不改期望（§7 同一条纪律）。不加载任何 DLL、不碰现场。
    //     输入照 SelfTest 版：IdleArk 只两关(CE-5,LS-5)、drop/prefix 是单行 —— 与门档夹具（旧门三关版）刻意不同，
    //     两套输入各钉各的面；ArkIntent 用例走 fi_ark_intent 单步（SelfTest 原样），两步序在门档 fi_ark_case 里。
    let s1 = |x: &str| JVal::S(x.to_string());
    let xg = |o: &JVal, k: &str| -> JVal { jv_get(o, k) };
    let xs = |o: &JVal, k: &str| -> String { jv_str(&jv_get(o, k)) };
    let xb = |o: &JVal, k: &str| -> bool { jv_truthy(&jv_get(o, k)) };
    let xi = |o: &JVal, k: &str| -> i64 { jv_int(&jv_get(o, k), i64::MIN) };
    let xl = |v: &JVal| -> Vec<JVal> { jv_list(v).into_iter().cloned().collect() };
    let x_at = |v: &JVal, i: usize| -> String { jv_list(v).get(i).map(|x| jv_str(*x)).unwrap_or_default() };
    let xnl = |v: &JVal| -> bool { matches!(v, JVal::Null) };

    // —— 目标清单解析：11 ——
    let sp1 = fi_target_spec(&s1("D32钢=30@CE-5"));
    let sp1d = xg(&sp1, "Spec");
    chk(
        xb(&sp1, "Found") && xs(&sp1d, "Key") == "D32钢" && xi(&sp1d, "Target") == 30
            && xl(&xg(&sp1d, "Stages")).len() == 1 && xs(&sp1d, "Bad") == "",
        "farm/intent·spec：名=数@关 拆得开",
        &mut fails,
        &mut total,
    );
    let sp2 = xg(&fi_target_spec(&s1("30135=12@CE-5,CE-4")), "Spec");
    chk(
        xs(&sp2, "Key") == "30135" && xl(&xg(&sp2, "Stages")).len() == 2,
        "farm/intent·spec：代号+多关(逗号/顿号分隔)拆得开",
        &mut fails,
        &mut total,
    );
    let sp2b = xg(&fi_target_spec(&s1("30135=12@CE-5、CE-4；LS-5")), "Spec");
    chk(
        xl(&xg(&sp2b, "Stages")).len() == 3,
        "farm/intent·spec：顿号/分号/空白都是关卡分隔",
        &mut fails,
        &mut total,
    );
    let sp3 = xg(
        &fi_target_spec(&jv_o(vec![
            ("item", s1("D32钢")),
            ("target", JVal::I(20)),
            ("stages", JVal::A(vec![s1("CE-5")])),
        ])),
        "Spec",
    );
    chk(
        xs(&sp3, "Key") == "D32钢" && xi(&sp3, "Target") == 20 && xs(&sp3, "Bad") == "",
        "farm/intent·spec：json 对象写法也吃",
        &mut fails,
        &mut total,
    );
    let sp3b = xg(
        &fi_target_spec(&jv_o(vec![
            ("item", s1("D32钢")),
            ("target", JVal::I(20)),
            ("stages", JVal::A(vec![s1("CE-5，CE-4")])),
        ])),
        "Spec",
    );
    chk(
        xl(&xg(&sp3b, "Stages")).len() == 2,
        "farm/intent·spec：stages 只给一条串时按分隔符再切(旧 $s.Count -eq 1 分支)",
        &mut fails,
        &mut total,
    );
    let sp3c = xg(
        &fi_target_spec(&jv_o(vec![("name", s1("或铝石")), ("count", JVal::I(7))])),
        "Spec",
    );
    chk(
        xs(&sp3c, "Key") == "或铝石" && xi(&sp3c, "Target") == 7 && xl(&xg(&sp3c, "Stages")).is_empty(),
        "farm/intent·spec：name/count 别名也吃，没关卡就是空数组",
        &mut fails,
        &mut total,
    );
    let sp4 = xg(&fi_target_spec(&s1("想刷点火腿")), "Spec");
    chk(
        xs(&sp4, "Bad").contains("材料名=数量") && xs(&sp4, "Raw") == "想刷点火腿",
        "farm/intent·spec：看不懂的行给 Bad 说明而不是丢掉",
        &mut fails,
        &mut total,
    );
    let sp5 = xg(&fi_target_spec(&jv_o(vec![("item", s1("D32钢"))])), "Spec");
    chk(
        xs(&sp5, "Bad").contains("目标数量") && xs(&sp5, "Raw") == "D32钢",
        "farm/intent·spec：对象缺数量也给 Bad(Raw 回显键名)",
        &mut fails,
        &mut total,
    );
    let sp6 = xg(&fi_target_spec(&jv_o(vec![("target", JVal::I(5))])), "Spec");
    chk(
        xs(&sp6, "Bad") == "条目里没有材料名/代号(item)" && xs(&sp6, "Raw") == "{\"target\":5}",
        "farm/intent·spec：对象连 item 都没有 → Bad 里点名(item)，Raw 回压缩 JSON",
        &mut fails,
        &mut total,
    );
    chk(
        !xb(&fi_target_spec(&JVal::Null), "Found") && !xb(&fi_target_spec(&s1("   ")), "Found"),
        "farm/intent·spec：raw 缺失/空串 → Found:false(旧实现返回 $null)",
        &mut fails,
        &mut total,
    );
    let sp7 = xg(&fi_target_spec(&s1("D32钢＝30 @ CE-5")), "Spec");
    chk(
        xi(&sp7, "Target") == 30 && xl(&xg(&sp7, "Stages")).len() == 1,
        "farm/intent·spec：全角等号与 @ 前后空格都认(旧正则 [=＝] 与 \\s*@\\s*)",
        &mut fails,
        &mut total,
    );

    // —— 掉落文本解析：6 ——
    let dsample = "[08:50:26] CE-5 掉落统计:\nD32钢 : 2 (+2)   或铝石 : 1 (+1)   当前次数 : 3\n[08:52:10] LS-5 掉落统计:\n采购凭证 : 30 (30)";
    let dd = fi_drop_text(dsample);
    let dblks = xl(&xg(&dd, "Blocks"));
    chk(
        dblks.len() == 2 && xs(&dblks[0], "Stage") == "CE-5" && xs(&dblks[1], "Stage") == "LS-5",
        "farm/intent·drop：两个块、关卡代号抓得到",
        &mut fails,
        &mut total,
    );
    let ditem0 = xl(&xg(&dblks[0], "Items"));
    chk(
        ditem0.len() == 2 && xs(&ditem0[0], "Name") == "D32钢" && xi(&ditem0[0], "Delta") == 2,
        "farm/intent·drop：时间戳前缀不影响识别；「当前次数」被拒绝表挡掉",
        &mut fails,
        &mut total,
    );
    let ditem1 = xl(&xg(&dblks[1], "Items"));
    chk(
        ditem1.len() == 1 && xi(&ditem1[0], "Delta") == 0,
        "farm/intent·drop：只按 (+N) 记增量，\"(30)\"这种总数不当增量",
        &mut fails,
        &mut total,
    );
    chk(
        xl(&xg(&fi_drop_text(""), "Blocks")).is_empty(),
        "farm/intent·drop：空文本给空数组不抛",
        &mut fails,
        &mut total,
    );
    let db2 = xl(&xg(&fi_drop_text("<12> PR-B-2 掉落统计:术师芯片组 : 2 (+2)"), "Blocks"));
    chk(
        db2.len() == 1 && xs(&db2[0], "Stage") == "PR-B-2",
        "farm/intent·drop：行首 <n> 前缀剥掉后块头照样认",
        &mut fails,
        &mut total,
    );
    let db3 = xl(&xg(
        &fi_drop_text("CE-4 掉落统计:\nD32钢 : 1 (+1)\n这是一句普通说明\n30135 : 9 (+9)"),
        "Blocks",
    ));
    chk(
        db3.len() == 1 && xl(&xg(&db3[0], "Items")).len() == 1,
        "farm/intent·drop：出现不像配额的行就算块结束(后面不再入账)",
        &mut fails,
        &mut total,
    );

    // —— 补刷判表：15 ——
    let n2i = jv_o(vec![("D32钢", s1("30135")), ("或铝石", s1("30115"))]);
    let i2n = jv_o(vec![("30135", s1("D32钢")), ("30115", s1("或铝石"))]);
    let known = jv_o(vec![("CE-5", JVal::B(true)), ("CE-4", JVal::B(true))]);
    let dt = jv_o(vec![(
        "30135",
        jv_o(vec![
            ("CE-5", jv_o(vec![("runs", JVal::I(5)), ("qty", JVal::I(12))])),
            ("CE-4", jv_o(vec![("runs", JVal::I(3)), ("qty", JVal::I(3))])),
        ]),
    )]);
    let fspec = |key: &str, target: i64, stages: Vec<JVal>| -> JVal {
        jv_o(vec![
            ("Key", s1(key)),
            ("Target", JVal::I(target)),
            ("Stages", JVal::A(stages)),
            ("Raw", s1(&format!("{}={}", key, target))),
            ("Bad", s1("")),
        ])
    };
    let pin = |targets: Vec<JVal>, inv: JVal, org: JVal, dtt: JVal, kn: JVal, mx: i64| -> JVal {
        jv_o(vec![
            ("targets", JVal::A(targets)),
            ("nameToId", n2i.clone()),
            ("idToName", i2n.clone()),
            ("inventory", inv),
            ("origin", org),
            ("dropTable", dtt),
            ("knownStages", kn),
            ("maxTimes", JVal::I(mx)),
        ])
    };
    let r0 = fi_plan(&jv_o(vec![("targets", JVal::A(Vec::new()))]));
    chk(
        !xb(&r0, "Active") && xs(&r0, "Reason").contains("farmTargets"),
        "farm/intent·plan：空清单不炸，理由说\"没配 farmTargets\"",
        &mut fails,
        &mut total,
    );
    let rf = fi_plan(&pin(
        vec![fspec("D32钢", 30, vec![s1("CE-5")])],
        jv_o(vec![("30135", JVal::I(18))]),
        jv_o(Vec::new()),
        dt.clone(),
        known.clone(),
        12,
    ));
    chk(
        xb(&rf, "Active") && xi(&rf, "Gap") == 12 && x_at(&xg(&rf, "Stages"), 0) == "CE-5" && xi(&rf, "Times") == 12,
        "farm/intent·plan：缺口=目标-现有；显式填关优先；次数保守折算=缺口(每场掉 1 保底，凑满目标为止)",
        &mut fails,
        &mut total,
    );
    chk(
        xs(&rf, "Reason").contains("还差 12")
            && xs(&rf, "Reason").contains("每场约 2.4")
            && xs(&rf, "Reason").starts_with("补刷 D32钢×12 @CE-5：目标 30，现有 18"),
        "farm/intent·plan：Reason 说清账目(逐字口径：还差 12 / 每场约 2.4 照报但只进括号)",
        &mut fails,
        &mut total,
    );
    let rc = fi_plan(&pin(
        vec![fspec("D32钢", 99, vec![s1("CE-5")])],
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        dt.clone(),
        known.clone(),
        4,
    ));
    chk(
        xi(&rc, "Times") == 4 && xs(&rc, "Reason").contains("(单轮封顶 4，剩下的明天接着刷)"),
        "farm/intent·plan：单轮封顶生效并在 Reason 里说\"明天接着刷\"",
        &mut fails,
        &mut total,
    );
    let rg = fi_plan(&pin(
        vec![fspec("D32钢", 10, vec![s1("CE-5")])],
        jv_o(vec![("30135", JVal::I(12))]),
        jv_o(Vec::new()),
        dt.clone(),
        known.clone(),
        12,
    ));
    chk(
        !xb(&rg, "Active") && xs(&rg, "Reason").contains("D32钢 已够(12/10)"),
        "farm/intent·plan：已够的目标不刷，理由里带\"已够(12/10)\"",
        &mut fails,
        &mut total,
    );
    let ro = fi_plan(&pin(
        vec![fspec("30135", 30, vec![s1("CE-9")])],
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        known.clone(),
        12,
    ));
    chk(
        !xb(&ro, "Active") && xs(&ro, "Reason").contains("不被 MAA 关卡表认识"),
        "farm/intent·plan：只填了不认识的关 → 不刷并说为什么",
        &mut fails,
        &mut total,
    );
    let ru = fi_plan(&pin(
        vec![fspec("点火腿", 5, vec![])],
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        12,
    ));
    chk(
        !xb(&ru, "Active") && xs(&ru, "Reason").contains("item_index"),
        "farm/intent·plan：材料表查不到的名字跳过并说明(不静默)",
        &mut fails,
        &mut total,
    );
    let rn = fi_plan(&pin(
        vec![fspec("D32钢", 5, vec![])],
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        12,
    ));
    chk(
        !xb(&rn, "Active") && xs(&rn, "Reason").contains("@CE-5 这种带关卡的形式"),
        "farm/intent·plan：没学习数据又没填关 → 提示写成带 @关卡 的形式",
        &mut fails,
        &mut total,
    );
    let re = fi_plan(&pin(
        vec![fspec("D32钢", 8, vec![s1("CE-5")])],
        jv_o(vec![("30135", JVal::I(4))]),
        jv_o(vec![("30135", s1("depot"))]),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        12,
    ));
    chk(
        xs(&re, "Reason").contains("估算")
            && xs(&re, "Reason").contains("按每场 1 估")
            && xi(&re, "Times") == 4,
        "farm/intent·plan：depot 标定的现值明说\"是估算\"；无学习数据按每场 1 估",
        &mut fails,
        &mut total,
    );
    let rb = fi_plan(&pin(
        vec![fspec("D32钢", 20, vec![s1("CE-5")]), fspec("或铝石", 100, vec![s1("CE-4")])],
        jv_o(vec![("30135", JVal::I(18)), ("30115", JVal::I(90))]),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        12,
    ));
    chk(
        xs(&rb, "ItemName") == "或铝石" && xi(&rb, "Gap") == 10,
        "farm/intent·plan：多目标一次只锁缺口最大的那个(或铝石差10)",
        &mut fails,
        &mut total,
    );
    let rl = fi_plan(&pin(
        vec![fspec("D32钢", 30, vec![])],
        jv_o(vec![("30135", JVal::I(18))]),
        jv_o(Vec::new()),
        dt.clone(),
        known.clone(),
        12,
    ));
    chk(
        xl(&xg(&rl, "Stages")) == vec![s1("CE-5"), s1("CE-4")] && xi(&rl, "Times") == 12,
        "farm/intent·plan：没填关 → 学习表挑关(平均掉得多的打头，最多 2 关)；次数仍=缺口 12",
        &mut fails,
        &mut total,
    );
    chk(
        xs(&rl, "Reason").contains("每场约 1.9"),
        "farm/intent·plan：合计均值的串面 1.9(1.875 不是中点，ToEven 不生效，实测两代 PS 都给 1.9)",
        &mut fails,
        &mut total,
    );
    let rcode = fi_plan(&pin(
        vec![fspec("30115", 20, vec![s1("CE-4")])],
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        12,
    ));
    chk(
        xs(&rcode, "ItemName") == "或铝石"
            && xs(&rcode, "Reason").starts_with("补刷 或铝石×12 @CE-4：目标 20，现有 0，还差 20，掉落率未知，按每场 1 估"),
        "farm/intent·plan：键是代号时 ItemName 反查回材料名(Reason 用名不用号)，无学习数据按 1 估并封顶",
        &mut fails,
        &mut total,
    );
    let rmix = fi_plan(&pin(
        vec![
            jv_o(vec![
                ("Key", s1("点火腿")),
                ("Target", JVal::I(0)),
                ("Stages", JVal::A(Vec::new())),
                ("Raw", s1("想刷点火腿")),
                ("Bad", s1("看不懂「想刷点火腿」，要写成 材料名=数量[@关卡1,关卡2]")),
            ]),
            fspec("D32钢", 20, vec![s1("CE-5")]),
        ],
        jv_o(vec![("30135", JVal::I(18))]),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        12,
    ));
    chk(
        xb(&rmix, "Active") && xl(&xg(&rmix, "Notes")).len() == 1
            && x_at(&xg(&rmix, "Notes"), 0).starts_with("跳过「想刷点火腿」"),
        "farm/intent·plan：坏行不进方案但 Notes 里留痕(汇总着说，不静默丢)",
        &mut fails,
        &mut total,
    );
    let rz = fi_plan(&pin(
        vec![fspec("D32钢", 3, vec![s1("CE-5")])],
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        jv_o(Vec::new()),
        0,
    ));
    chk(
        xi(&rz, "Times") == 1 && xs(&rz, "Reason").contains("单轮封顶 1"),
        "farm/intent·plan：MaxTimes<1 兜成 1(旧 [int] 兜底)，封顶句跟着说 1",
        &mut fails,
        &mut total,
    );

    // —— 方舟本轮意图：11 ——
    let idle = jv_o(vec![
        ("Mode", s1("idle")),
        ("Stage", s1("CE-5,LS-5")),
        ("StageHint", s1("CE-5,LS-5")),
        ("ClearSanity", JVal::B(true)),
        ("Reason", s1("长草")),
        ("EventKey", JVal::Null),
        ("EventTip", JVal::Null),
        ("Source", s1("weekday")),
    ]);
    let pend = jv_o(vec![
        ("Mode", s1("event-pending")),
        ("Stage", JVal::Null),
        ("StageHint", s1("CT-1")),
        ("ClearSanity", JVal::B(false)),
        ("Reason", s1("等新活动确认")),
        ("EventKey", s1("act1")),
        ("EventTip", s1("sidestory")),
        ("Source", s1("event")),
    ]);
    let fopt = jv_o(vec![
        ("Stages", JVal::A(vec![s1("PR-D-2"), s1("CE-5")])),
        ("Reason", s1("补刷 特种芯片组×8 @PR-D-2")),
    ]);
    let ain = |ark: JVal, stage: &str, force: bool, farm: bool, fo: JVal| -> JVal {
        jv_o(vec![
            ("ark", ark),
            ("stage", s1(stage)),
            ("force", JVal::B(force)),
            ("farm", JVal::B(farm)),
            ("farmOpt", fo),
        ])
    };
    let am = fi_ark_intent(&ain(pend.clone(), "OF-1", false, false, JVal::Null));
    let amk = xg(&am, "Ark");
    chk(
        xs(&amk, "Mode") == "manual" && xs(&amk, "Stage") == "OF-1" && xs(&amk, "StageHint") == "OF-1"
            && xs(&amk, "Reason") == "命令行指定刷 OF-1" && xs(&amk, "Source") == "manual" && xb(&amk, "ClearSanity"),
        "farm/intent·ark：-Stage 手造 manual(逐字：命令行指定刷 OF-1；Source=manual)",
        &mut fails,
        &mut total,
    );
    chk(
        xnl(&xg(&amk, "EventKey")) && xnl(&xg(&amk, "EventTip")),
        "farm/intent·ark：manual 档 EventKey/EventTip 是给过的空值(不是残留活动键)",
        &mut fails,
        &mut total,
    );
    let af = fi_ark_intent(&ain(pend.clone(), "", true, false, JVal::Null));
    let afk = xg(&af, "Ark");
    chk(
        xs(&afk, "Mode") == "event" && xs(&afk, "Stage") == "CT-1"
            && xs(&afk, "Reason") == "(-Force) 不管今天的自证结论(进得去与否)，强行刷 CT-1"
            && xs(&afk, "EventKey") == "act1" && xs(&afk, "EventTip") == "sidestory" && xs(&afk, "Source") == "event",
        "farm/intent·ark：-Force 越自证结论 → event 档取 StageHint，Reason 带(-Force)原话",
        &mut fails,
        &mut total,
    );
    chk(
        xl(&xg(&af, "Warnings")).len() == 1 && x_at(&xg(&af, "Warnings"), 0) == "-Force：本轮忽略活动图自证结论",
        "farm/intent·ark：-Force 的 WARN 文案原样交给壳去打",
        &mut fails,
        &mut total,
    );
    let fn_ = fi_ark_intent(&ain(idle.clone(), "", true, false, JVal::Null));
    chk(
        xl(&xg(&fn_, "Warnings")).is_empty() && xs(&xg(&fn_, "Ark"), "Mode") == "idle" && !xb(&fn_, "HasFarmFields"),
        "farm/intent·ark：-Force 对非 event-pending 不动手(空 Warn，ark 原样)",
        &mut fails,
        &mut total,
    );
    let fd = fi_ark_intent(&ain(idle.clone(), "", false, true, fopt.clone()));
    let fdk = xg(&fd, "Ark");
    chk(
        xs(&fdk, "Mode") == "farm" && xs(&fdk, "Stage") == "PR-D-2,CE-5"
            && xs(&fdk, "Reason") == "材料补刷专用轮 — 补刷 特种芯片组×8 @PR-D-2",
        "farm/intent·ark：专用轮 Mode=farm、Stage 只挂 farm 关(逗号拼)",
        &mut fails,
        &mut total,
    );
    chk(
        xnl(&xg(&fdk, "EventKey")) && xs(&fdk, "Source") == "weekday" && xs(&fd, "FarmMode") == "dedicated"
            && xb(&fd, "HasFarmFields"),
        "farm/intent·ark：专用轮键抄自原 ark(EventKey/EventTip/Source)",
        &mut fails,
        &mut total,
    );
    let a_app = fi_ark_intent(&ain(
        jv_o(vec![
            ("Mode", s1("idle")),
            ("Stage", s1("CE-5,LS-5,CE-5")),
            ("StageHint", s1("CE-5,LS-5")),
            ("ClearSanity", JVal::B(true)),
            ("Reason", s1("长草")),
            ("EventKey", JVal::Null),
            ("EventTip", JVal::Null),
            ("Source", s1("weekday")),
        ]),
        "",
        false,
        false,
        fopt.clone(),
    ));
    let a_appk = xg(&a_app, "Ark");
    chk(
        xs(&a_appk, "Stage") == "PR-D-2,CE-5,LS-5" && xs(&a_app, "FarmMode") == "append",
        "farm/intent·ark：需求优先档：farm 关排最前且去重(CE-5 不重复出现)",
        &mut fails,
        &mut total,
    );
    chk(
        xs(&a_appk, "Reason") == "长草；需求优先 补刷 特种芯片组×8 @PR-D-2",
        "farm/intent·ark：append 的 Reason 用「；需求优先 」拼接(逐字)",
        &mut fails,
        &mut total,
    );
    chk(
        xs(&a_appk, "Mode") == "idle" && xs(&a_appk, "StageHint") == "CE-5,LS-5" && xb(&a_appk, "ClearSanity"),
        "farm/intent·ark：append 档不动 Mode/StageHint/ClearSanity(只改 Stage/Reason)",
        &mut fails,
        &mut total,
    );
    let afl = fi_ark_intent(&ain(idle.clone(), "", false, true, JVal::Null));
    chk(
        xs(&afl, "FarmMode") == ""
            && xb(&afl, "HasFarmFields")
            && x_at(&xg(&afl, "Warnings"), 0) == "-Farm 本轮没有可补的材料 → 按原方案跑"
            && xs(&xg(&afl, "Ark"), "Mode") == "idle",
        "farm/intent·ark：-Farm 但本轮没有可补 → 原方案 + FarmMode 空 + WARN 文案",
        &mut fails,
        &mut total,
    );

    // —— 1999 本轮意图：3 ——
    let pendr = jv_o(vec![
        ("Mode", s1("event-pending")),
        ("Instance", s1("idle")),
        ("Version", s1("v4.7.1")),
        ("VersionName", s1("测试版本")),
        ("EventKey", s1("act123")),
        ("EventTip", s1("活动")),
        ("Reason", s1("待确认")),
        ("DataAge", JVal::Null),
    ]);
    let rin = |plan: &str, force: bool| -> JVal {
        jv_o(vec![
            ("decision", pendr.clone()),
            ("plan", s1(plan)),
            ("force", JVal::B(force)),
        ])
    };
    let rr = fi_r1999_intent(&rin("auto", true));
    let rrk = xg(&rr, "Decision");
    chk(
        xs(&rrk, "Mode") == "event" && xs(&rrk, "Instance") == "event"
            && xs(&rrk, "Reason") == "(-Force) 跳过 1999 活动确认门，直接用活动方案"
            && xs(&rrk, "Version") == "v4.7.1" && xs(&rrk, "VersionName") == "测试版本"
            && xs(&rrk, "EventKey") == "act123" && xnl(&xg(&rrk, "DataAge"))
            && x_at(&xg(&rr, "Warnings"), 0) == "-Force：1999 本轮忽略活动确认门",
        "farm/intent·r19：-Force → event/event，Reason 与 WARN 逐字",
        &mut fails,
        &mut total,
    );
    let rr2 = fi_r1999_intent(&rin("idle", true));
    let rr2k = xg(&rr2, "Decision");
    chk(
        xs(&rr2k, "Mode") == "idle" && xs(&rr2k, "Instance") == "idle" && xs(&rr2k, "Version") == "manual"
            && xs(&rr2k, "VersionName") == "命令行指定" && xs(&rr2k, "Reason") == "命令行指定 idle"
            && xnl(&xg(&rr2k, "EventKey")) && xnl(&xg(&rr2k, "DataAge")),
        "farm/intent·r19：-Plan≠auto 无条件覆盖(连 Force 的结果也覆盖)：Version=manual/VersionName=命令行指定",
        &mut fails,
        &mut total,
    );
    let rr3 = fi_r1999_intent(&rin("auto", false));
    chk(
        xl(&xg(&rr3, "Warnings")).is_empty() && xs(&xg(&rr3, "Decision"), "Mode") == "event-pending",
        "farm/intent·r19：两样都不给 → decision 原样、零 Warn",
        &mut fails,
        &mut total,
    );

    // —— 自指六项：分派顺序 / 夹具名册 / 信封身份 ——
    let fi_dispatch_at = SRC.find(&["run_farm_intent(", "&out_file"].concat()).unwrap_or(usize::MAX);
    chk(
        fi_dispatch_at < kind_gate_at,
        "--farm-intent 的分派落在那条必填拦路虎之后：farm/intent 这一档会被装包必填拦死，『只吃夹具』成了空话",
        &mut fails,
        &mut total,
    );
    let fc = fi_fixture_cases();
    let n_spec = fc.iter().filter(|(n, _)| n.starts_with("spec/")).count();
    let n_drop = fc.iter().filter(|(n, _)| n.starts_with("drop/")).count();
    let n_plan = fc.iter().filter(|(n, _)| n.starts_with("plan/")).count();
    let n_ark = fc.iter().filter(|(n, _)| n.starts_with("ark/")).count();
    let n_r19 = fc.iter().filter(|(n, _)| n.starts_with("r19/")).count();
    chk(
        fc.len() == 46 && n_spec == 13 && n_drop == 6 && n_plan == 14 && n_ark == 9 && n_r19 == 4,
        &format!(
            "farm/intent 夹具五族分家不对（spec/drop/plan/ark/r19 = {}/{}/{}/{}/{}，应 13/6/14/9/4）",
            n_spec, n_drop, n_plan, n_ark, n_r19
        ),
        &mut fails,
        &mut total,
    );
    let mut fi_names: Vec<&str> = fc.iter().map(|(n, _)| *n).collect();
    fi_names.sort();
    fi_names.dedup();
    chk(
        fi_names.len() == 46,
        "farm/intent 夹具出现重名档：三侧门按名对账会把漏档读成对齐",
        &mut fails,
        &mut total,
    );
    let fj = render_farm_intent_json();
    chk(
        fj.contains(r#""ok":true"#)
            && fj.contains(r#""role":"R2-farm-intent""#)
            && fj.contains(r#""fixture":"farm-intent""#)
            && fj.contains(r#""count":46"#),
        "farm/intent 出口信封缺格：三侧对拍门靠 role/fixture/count 这几格判身份",
        &mut fails,
        &mut total,
    );
    chk(
        fj.matches(r#""name":"#).count() == 46,
        "farm/intent 信封 cases 条数与夹具名册不一致",
        &mut fails,
        &mut total,
    );
    chk(
        !fj.contains(r#""date":"#),
        "farm/intent 信封里出现了 date 格：这一档不吃时钟，多了它就对拍门多一个假身份格",
        &mut fails,
        &mut total,
    );

    // 16. R3② ark 认关止损链装配回放（8 档 = live 门第六路同源同量，arksk-1..8）。
    //     出档形状：每档一行投影串 `L=…~…|S=…|D=…`（seq 用 ~ 连接）——三侧对拍门
    //     （tools\rustcore\probe-arkskip-rust-golden.ps1）拿 old/new/rust 三侧逐字比它。
    //     真出口在这 8 档上照生产协议真跑 decide-daily-retry（外部 exe）；自检不许要外部依赖，
    //     这里喂一枚只复刻 Orchestration ark 支语义的替身（跳过清单三值 / retryOnce 合取 /
    //     unknown-format 的 ERROR 句）——替身接错先红，否则后面逐档断言全是假绿。
    //     期望串逐字对的是转写体与 251bd3c 里那一段（106-255）的同一套话术与顺序；
    //     这一节红了先怀疑实现走样，再对着转写源核（§7 同一条纪律：改实现、不改期望）。
    let pj_ok = jv_parse(
        r#"{"ok":true,"exit":0,"verb":"decide-daily-retry","data":{"Retryable":false,"Logs":[{"Level":"WARN","Text":"\u4e2d\u6587\u5e26\"引号\""}],"N":-1,"F":2.5,"E":1e3,"Big":9007199254740993}}"#,
    );
    let pj_good = match &pj_ok {
        Ok(pj) => {
            let data = jv_get(pj, "data");
            let logs_v = jv_get(&data, "Logs");
            let logs = jv_list(&logs_v);
            let lvl0 = logs.first().map(|v| jv_str(&jv_get(v, "Level"))).unwrap_or_default();
            let txt0 = logs.first().map(|v| jv_str(&jv_get(v, "Text"))).unwrap_or_default();
            jv_str(&jv_get(pj, "verb")) == "decide-daily-retry"
                && jv_int(&jv_get(pj, "exit"), -9) == 0
                && !jv_truthy(&jv_get(&data, "Retryable"))
                && lvl0 == "WARN"
                && txt0 == "中文带\"引号\""
                && matches!(jv_get(&data, "N"), JVal::I(-1))
                && matches!(jv_get(&data, "F"), JVal::F(f) if (f - 2.5).abs() < 1e-9)
                && jv_to_d(&jv_get(&data, "E")) == 1000.0
                && matches!(jv_get(&data, "Big"), JVal::I(9007199254740993))
        }
        Err(_) => false,
    };
    chk(
        pj_good,
        "jv_parse 读信封核的常规格不对（verb/exit/布尔/数组/数字面/转义）：--arkskip 真出口的判定回读会跟着错",
        &mut fails,
        &mut total,
    );
    let pj_emoji = jv_parse(r#"{"s":"\ud83d\ude00"}"#);
    chk(
        matches!(&pj_emoji, Ok(v) if jv_str(&jv_get(v, "s")) == "😀"),
        "jv_parse 的代理对（高\\u+低\\u）没拼成一枚字符：面板信封里带代理对的读数会被读坏",
        &mut fails,
        &mut total,
    );
    let pj_rt = r#"{"a":[1,2,{"b":"x"}],"c":true,"d":null}"#;
    chk(
        jv_parse(pj_rt).map(|v| jv_to_json(&v)).unwrap_or_default() == pj_rt,
        "jv_parse → jv_to_json 的往返不逐字：嵌套结构（数组/对象/布尔/null）有一类没接住",
        &mut fails,
        &mut total,
    );
    chk(
        jv_parse(r#"{"a":"#).is_err()
            && jv_parse(r#"{"a":1} x"#).is_err()
            && jv_parse(r#"{"a":tru}"#).is_err()
            && jv_parse(r#"{"a":1,}"#).is_err()
            && jv_parse(r#"[1,2,]"#).is_err(),
        "jv_parse 把截断/尾随垃圾/坏字面量/尾逗号当成了合法信封：坏输入会被读成读数",
        &mut fails,
        &mut total,
    );
    chk(
        arksk_split_stages("1-7,CE-5").join("|") == "1-7|CE-5"
            && arksk_split_stages("1-7,,CE-5").join("|") == "1-7|CE-5"
            && arksk_split_stages(" 1-7 ，CE-5、S4-1；MT-6 ").join("|") == "1-7|CE-5|S4-1|MT-6"
            && arksk_split_stages("").join("|") == ""
            && arksk_split_commas(",1-7,").join("|") == "1-7"
            && arksk_split_commas("1-7，CE-5").join("|") == "1-7，CE-5",
        "arksk_split 两把切分刀与 PS 语义不一致（多分隔符正则 vs 单半角逗号）：剔关/认栽的档位面会静默错",
        &mut fails,
        &mut total,
    );
    chk(
        arksk_lean_stages("1-7,CE-5", &["CE-5".to_string()]).join("|") == "1-7"
            && arksk_lean_stages("1-7,ce-5", &["CE-5".to_string()]).join("|") == "1-7"
            && arksk_lean_stages("ce-5", &["CE-5".to_string()]).join("|") == ""
            && arksk_lean_stages("1-7,CE-5", &[]).join("|") == "1-7|CE-5",
        "arksk_lean_stages 的大小写口径歪了（PS -notcontains 不区分大小写）：混拼档位会被漏剔/错剔",
        &mut fails,
        &mut total,
    );
    let scens = arkskip_scenarios();
    let names: Vec<&str> = scens.iter().map(|s| s.name).collect();
    chk(
        names
            == vec![
                "arksk-1-recover",
                "arksk-2-still-skip",
                "arksk-3-r2-notok",
                "arksk-4-no-lean",
                "arksk-5-not-skipped",
                "arksk-6-check1-throw",
                "arksk-7-check2-throw",
                "arksk-8-retry-then-chain",
            ],
        "arkskip 夹具名册与 live 门第六路脱钩（8 档 arksk-1..8）：三侧门按名对账会把漏档读成对齐",
        &mut fails,
        &mut total,
    );
    // 替身的三个负例面：不重试的两类（inSkip 三值 / retryOnce 没开）+ 不重试但有 ERROR 句的一类。
    let mut fake_decide = |st: &str, ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
        let in_skip = st.eq_ignore_ascii_case("ok")
            || st.eq_ignore_ascii_case("not-started")
            || st.eq_ignore_ascii_case("unknown-format");
        if !in_skip && ro {
            Ok((true, vec![("WARN".to_string(), "方舟本轮未成功 → 收尾后重试一次".to_string())]))
        } else if st.eq_ignore_ascii_case("unknown-format") {
            Ok((
                false,
                vec![(
                    "ERROR".to_string(),
                    "方舟日志标志一条都没认出来 → 重试同样看不懂，直接记入本轮告警(见下面的处理建议)".to_string(),
                )],
            ))
        } else {
            Ok((false, Vec::new()))
        }
    };
    let (fd_rb1, fd_lg1) = fake_decide("stalled", true).unwrap();
    chk(
        fd_rb1 && fd_lg1.len() == 1 && fd_lg1[0].0 == "WARN" && fd_lg1[0].1 == "方舟本轮未成功 → 收尾后重试一次",
        "替身分流与 Orchestration 的 ark 支不一致（该重试侧）：后面的档位断言跟着失真",
        &mut fails,
        &mut total,
    );
    let (fd_rb2, fd_lg2) = fake_decide("stalled", false).unwrap();
    let (fd_rb3, fd_lg3) = fake_decide("ok", true).unwrap();
    let (fd_rb4, fd_lg4) = fake_decide("not-started", true).unwrap();
    chk(
        !fd_rb2 && fd_lg2.is_empty() && !fd_rb3 && fd_lg3.is_empty() && !fd_rb4 && fd_lg4.is_empty(),
        "替身分流与 Orchestration 的 ark 支不一致（不该重试的三侧：单 stalled / ok / not-started）：后面的档位断言跟着失真",
        &mut fails,
        &mut total,
    );
    let (fd_rb5, fd_lg5) = fake_decide("unknown-format", false).unwrap();
    let (fd_rb6, fd_lg6) = fake_decide("unknown-format", true).unwrap();
    chk(
        !fd_rb5
            && fd_lg5.len() == 1
            && fd_lg5[0].0 == "ERROR"
            && fd_lg5[0].1.contains("方舟日志标志一条都没认出来")
            && !fd_rb6
            && fd_lg6 == fd_lg5,
        "替身分流与 Orchestration 的 ark 支不一致（unknown-format 的 ERROR 句与合取）：后面的档位断言跟着失真",
        &mut fails,
        &mut total,
    );
    let mut l8v: Vec<String> = Vec::new();
    let mut ak_run_err: Option<String> = None;
    // B-1a：obsrec 桩的账本净计数跨档连续（与 run_arkskip 同口径）。
    let mut obs_n: i64 = 0;
    for (i, sc) in scens.iter().enumerate() {
        match arkskip_run_case(sc, &mut fake_decide, &mut obs_n) {
            Ok(p) => l8v.push(p),
            Err(e) => {
                ak_run_err = Some(format!("第 {} 档({}) 没跑完：{}", i + 1, sc.name, e));
                break;
            }
        }
    }
    if l8v.len() == 8 {
        let (l1, l2, l3, l4, l5, l6, l7, l8) =
            (&l8v[0], &l8v[1], &l8v[2], &l8v[3], &l8v[4], &l8v[5], &l8v[6], &l8v[7]);
        chk(
            l1.ends_with("S=ok|D=剔关轮收尾正常")
                && l1.matches("CFG#auto#").count() == 2
                && l1.contains("~CFG#auto#1-7~INFO|  MAA(剔关重试)：chg-lean")
                && l1.contains("RUN#Fight+Shop#ok")
                && l1.contains("~RUN#Fight#ok~SKIP#2~")
                && l1.contains("OK|剔关重跑后 Fight 正常打了，长草理智保住 → 今天不记认栽标记")
                && l1.contains("STOP~LEDGER~INFO|掉落记账：BN1~OBS~OBSREC")
                && !l1.contains("MARK#"),
            "arkskip 档1(跳过&剔关&恢复)：两段配置/两轮链/恢复句/收尾记账序 —— 有一处没照转写走",
            &mut fails,
            &mut total,
        );
        // B-1a：mark 桩改真今日（FixtureFx 与"新侧进程内真做"对齐）—— 断言随之动态化。
        chk(
            l2.contains("剔关重试仍被 MAA 判全关(计划实况：空) → 今日理智关卡确认全关")
                && l2.contains("MARK#1-7+CE-5#MAA 判「关卡计划内没有当日开放的关卡」")
                && l2.contains(&format!("已记今日认栽标记({})：当天后续轮不再前置补刷关", gd_local_date()))
                && l2.ends_with("S=ok|D=剔关后收尾正常；Fight 被 MAA 判全关(认栽，已记当日标记)"),
            "arkskip 档2(二查仍跳)：认栽支（计划'空'话术/标记档位串/Detail 拼句）有一处没照转写走",
            &mut fails,
            &mut total,
        );
        chk(
            l3.contains(
                "~RUN#Fight#stalled~WARN|方舟剔关重试 结束状态：stalled — 剔关轮卡住~SKIP#2~WARN|剔关重试本轮以 stalled 收场，说不死关卡开没开 → 不记认栽标记(明天照常)",
            ) && l3.ends_with("S=stalled|D=剔关轮卡住")
                && !l3.contains("MARK#"),
            "arkskip 档3(二查没跳但 r2 非 ok)：『说不死关卡开没开』支或终态有一处没照转写走",
            &mut fails,
            &mut total,
        );
        chk(
            l4.contains("WARN|本轮没有可剔的补刷关(或剔完计划就空了) → 直接认今日全关")
                && l4.contains("MARK#CE-5#MAA 判「关卡计划内没有当日开放的关卡」")
                && l4.matches("SKIP#").count() == 1
                && l4.ends_with("S=ok|D=Fight 被 MAA 判全关(认栽，已记当日标记)"),
            "arkskip 档4(无 Farm)：直接认栽支/单查次数/tailNote 的 else 支（Detail 空）有一处没照转写走",
            &mut fails,
            &mut total,
        );
        chk(
            l5.matches("SKIP#").count() == 1
                && !l5.contains("这个 ok 是假的")
                && !l5.contains("MARK#")
                && l5.ends_with("S=ok|D=一切正常"),
            "arkskip 档5(没跳)：认关链不该动（不剔关/不标记）—— 有一处动过头了",
            &mut fails,
            &mut total,
        );
        chk(
            l6.contains("WARN|认关检查没做起来：读数没起来(不影响本轮)")
                && l6.matches("SKIP#").count() == 1
                && l6.ends_with("S=ok|D=一切正常"),
            "arkskip 档6(首查抛)：WARN 后缀/链不入/终态有一处没照转写走",
            &mut fails,
            &mut total,
        );
        chk(
            l7.contains("WARN|第二次认关检查没做起来：第二次读数炸了~OK|剔关重跑后 Fight 正常打了，长草理智保住 → 今天不记认栽标记~STOP")
                && !l7.contains("第二次读数炸了(不影响本轮)")
                && !l7.contains("MARK#")
                && l7.ends_with("S=ok|D=剔关轮收尾"),
            "arkskip 档7(二查抛)：WARN 形状（无「不影响本轮」后缀）、catch 后链照常评估（r2 ok ⇒ recovered 那记 OK）或终态有一处没照转写走",
            &mut fails,
            &mut total,
        );
        chk(
            l8.contains(
                "RUN#Fight+Shop#stalled~WARN|方舟 结束状态：stalled — 首轮卡住~WARN|方舟本轮未成功 → 收尾后重试一次~STOP~SLEEP~LAUNCH~RUN#Fight+Shop#ok",
            ) && l8.matches("LAUNCH").count() == 2
                && l8.contains("~SKIP#1~")
                && l8.ends_with("S=ok|D=重试后正常"),
            "arkskip 档8(先重试后认关)：分流回放序/重试重拉端游/重试后照样认关查 —— 有一处没照转写走",
            &mut fails,
            &mut total,
        );
    } else {
        chk(
            false,
            &format!(
                "arkskip 8 档回放没跑全（{} 档为止，后续逐档断言整体跳过）",
                ak_run_err.unwrap_or_else(|| format!("只跑出 {}", l8v.len()))
            ),
            &mut fails,
            &mut total,
        );
    }

    // 17. R3③ 驱动档（--drive-ark-daily = 换头重入协议：journal 重放 + ask/res 往返）。
    //     协议六件（拒跑七闸 / journal 装载 / res 合并）先钉死，再走十连真往返：
    //     临时目录里起新账 → 逐动作重入（六轮 ask、一轮 fail、一轮 done、两轮拒跑收场）。
    //     只碰 %TEMP% 下自己的目录，不加载 DLL、不碰任何现场；收尾连目录一起删。
    chk(
        drive_refusal(false, "j", "c", "r", "p", "s", "k").map_or(false, |m| m.contains("--i-drive")),
        "驱动档拒跑闸①走样：缺 --i-drive 没被点名拒跑（默认档可能把换头专用口当普通出口用）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "", "c", "r", "p", "s", "k").map_or(false, |m| m.contains("--journal")),
        "驱动档拒跑闸②走样：journal 没落点没被拒（断点记不住的会话会被放行）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "", "r", "p", "s", "k").map_or(false, |m| m.contains("--ctx")),
        "驱动档拒跑闸③走样：上下文文件没给没被拒（launch/retry_once/decision 没来路）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "c", "", "p", "s", "k").map_or(false, |m| m.contains("--res")),
        "驱动档拒跑闸④走样：回执落点没给没被拒（PS 侧干完一头的落点没处写）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "c", "r", "", "s", "k").map_or(false, |m| m.contains("--panel-cs")),
        "驱动档拒跑闸⑤走样：判定 exe 没指没被拒（重试分流试不了）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "c", "r", "p", "", "k").map_or(false, |m| m.contains("--state")),
        "驱动档拒跑闸⑥走样：state 目录没指没被拒（已搬动作的账会写到没处、静默假绿）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "c", "r", "p", "s", "").map_or(false, |m| m.contains("--log-sink")),
        "驱动档拒跑闸⑦走样：日志侧信道没落点没被拒（B-2c② 换通道后，没落点等于整轮日志不落盘）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "c", "r", "p", "s", "k").is_none(),
        "驱动档七样齐时仍被拒跑：换头调度器根本起不来（拒跑闸的合取写反了）",
        &mut fails,
        &mut total,
    );
    let dr_j_text = "{\"init\":{\"launch\":true}}\n{\"seq\":1,\"action\":\"log\",\"data\":{\"level\":\"INFO\",\"text\":\"x\"}}\n{\"seq\":2,\"action\":\"desk\",\"data\":\"Default\"}\n";
    match drive_load_journal(dr_j_text) {
        Ok((j_init, j_rows, j_done)) => chk(
            jv_to_json(&j_init) == "{\"launch\":true}" && j_rows.len() == 2 && j_done.is_none(),
            "drive_load_journal 把好账读歪了：init 内层/记录行数/done 三格有一格不对",
            &mut fails,
            &mut total,
        ),
        Err(m) => chk(
            false,
            &format!("drive_load_journal 把好账读成坏账：{}", m),
            &mut fails,
            &mut total,
        ),
    }
    chk(
        drive_load_journal("{\"init\":{}}\n{\"seq\":2,\"action\":\"desk\",\"data\":null}\n").is_err()
            && drive_load_journal("{\"init\":{}}\n{\"seq\":1,\"action\":\"desk\",\"data\":null}\n{\"done\":{}}\n{\"seq\":2,\"action\":\"desk\",\"data\":null}\n").is_err()
            && drive_load_journal("{\"init\":{}}\n{\"seq\":1,\"action\":\"desk\",\"data\":null,\"err\":\"x\"}\n").is_err()
            && drive_load_journal("{\"seq\":1,\"action\":\"desk\",\"data\":null}\n").is_err(),
        "drive_load_journal 把四类坏账放行了（seq 跳号/done 后有行/data+err 并存/无 init）：半截账上继续写只会越写越浑",
        &mut fails,
        &mut total,
    );
    let dr_m_ok = drive_merge_res("{\"seq\":1,\"action\":\"desk\",\"ok\":true,\"data\":\"Default\"}", 1).ok().flatten();
    let dr_m_dn = drive_merge_res("{\"seq\":1,\"action\":\"desk\",\"ok\":true}", 1).ok().flatten();
    chk(
        dr_m_ok.as_ref().map_or(false, |row| jv_str(&jv_get(row, "data")) == "Default" && jv_str(&jv_get(row, "action")) == "desk")
            && dr_m_dn.as_ref().map_or(false, |row| matches!(jv_get(row, "data"), JVal::Null)),
        "drive_merge_res 正例走样（data 回读 / 缺 data 补 Null）：好回执被读坏，重入链会当场断",
        &mut fails,
        &mut total,
    );
    chk(
        matches!(drive_merge_res("{\"seq\":3,\"action\":\"desk\",\"ok\":true,\"data\":\"x\"}", 5), Ok(None))
            && drive_merge_res("{\"seq\":7,\"action\":\"desk\",\"ok\":true,\"data\":\"x\"}", 3).is_err(),
        "drive_merge_res 的 seq 口径歪了（落后=幂等忽略、超前=越界报错）：旧回执会被当新断点、错位会被静默吞掉",
        &mut fails,
        &mut total,
    );
    chk(
        drive_merge_res("{\"seq\":1,\"action\":\"desk\",\"ok\":\"true\",\"data\":\"x\"}", 1).is_err()
            && drive_merge_res("{\"seq\":1,\"action\":\"desk\",\"ok\":false,\"data\":\"x\",\"err\":\"m\"}", 1).is_err()
            && drive_merge_res("{\"seq\":1,\"action\":\"desk\",\"ok\":false}", 1).is_err(),
        "drive_merge_res 把三类坏回执放行了（ok 非布尔 / 假却带 data / 假却没给 err）：坏回执会被当断点落进账",
        &mut fails,
        &mut total,
    );
    // —— 十连真往返：%TEMP% 里起一根真会话，逐动作重入 ——
    let dr_dir = std::env::temp_dir().join(format!("gd-drive-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dr_dir);
    let _ = std::fs::create_dir_all(&dr_dir);
    let dr_j = dr_dir.join("journal.ndjson").to_str().unwrap().to_string();
    let dr_c = dr_dir.join("ctx.json").to_str().unwrap().to_string();
    let dr_r = dr_dir.join("res.json").to_str().unwrap().to_string();
    let dr_s = dr_dir.join("state").to_str().unwrap().to_string();
    let dr_k = dr_dir.join("log-sink.ndjson").to_str().unwrap().to_string();
    let _ = std::fs::create_dir_all(&dr_s);
    let dr_p = "unused-panel-cs";
    // B-1a：decide 改成注入点 —— 自检照样传"坏路径真跑"，第六轮的"起不来当场断"断言不变，
    // 但自检从此不依赖 envelope 内部硬连 panel-cs。
    let mut dr_dec = |st: &str, ro: bool| arksk_call_decide(dr_p, st, ro);
    // B-2c③：run 也从 ask 换成注入点。闭包把每次调用看到的 chains 记下来（原第五轮那条
    // "chains 参数面"判据从信封搬到闭包口）并数调用次数 —— 重放段一次都不许再多。ask 时代这条
    // 靠"重放不问"间接成立，真做之后必须自己咬字。两枚读数走 Cell/RefCell：闭包按引用捕获它们，
    // 直接读同名变量会和后面几轮的再借用打架（借用检查器认闭包活着）。
    let dr_run_seen = std::cell::RefCell::new(Vec::<String>::new());
    let dr_run_calls = std::cell::Cell::new(0usize);
    let mut dr_run = |sc: &MaaScen, ch: &[String]| -> Result<ArkRunOut, String> {
        dr_run_calls.set(dr_run_calls.get() + 1);
        *dr_run_seen.borrow_mut() = ch.to_vec();
        // 桩不吃 ctx 那 13 枚（生产闭包 drive_real_run 才吃）；这一组只钉协议形状。
        let _ = sc;
        Ok(ArkRunOut { status: "ok".to_string(), detail: "一切正常".to_string(), duration: None })
    };
    let dr_run_rows = || -> usize {
        std::fs::read_to_string(&dr_j)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains("\"action\":\"run\""))
            .count()
    };
    let _ = std::fs::write(&dr_c, "{\"launch\":true,\"retry_once\":false,\"decision\":{\"Mode\":\"plain\",\"Stage\":\"1-7,CE-5\"}}");
    let (dr_rc1, dr_e1) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    chk(
        dr_rc1 == 0
            && dr_e1.contains("\"kind\":\"ask\"")
            && dr_e1.contains("\"role\":\"R3-drive-ark-daily\"")
            && dr_e1.contains("\"seq\":2")
            && dr_e1.contains("\"action\":\"desk\""),
        "驱动档第一轮没停在 desk：起新账（init+首行日志进侧信道）或 ask 信封装错（换头调度器第一步就接不下去）",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":2,\"action\":\"desk\",\"ok\":true,\"data\":\"Default\"}");
    let (dr_rc2, dr_e2) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    chk(
        dr_rc2 == 0 && dr_e2.contains("\"seq\":3") && dr_e2.contains("\"action\":\"takeover\""),
        "驱动档第二轮没把 desk 回执并账后停在 takeover：重放推进或新区衔接错",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":3,\"action\":\"takeover\",\"ok\":true}");
    let (dr_rc3, dr_e3) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    chk(
        dr_rc3 == 0 && dr_e3.contains("\"seq\":4") && dr_e3.contains("\"params\":{\"kind\":\"main\"}"),
        "驱动档第三轮没停在 cfg(main)：takeover 回执并账或 cfg ask 的 params 装错",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":4,\"action\":\"cfg\",\"ok\":true,\"data\":{\"changes\":[\"chg-main\"],\"chains\":[\"Fight\",\"Shop\"]}}");
    let (dr_rc4, dr_e4) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    chk(
        dr_rc4 == 0 && dr_e4.contains("\"seq\":6") && dr_e4.contains("\"action\":\"launch\""),
        "驱动档第四轮没停在 launch：cfg 回执并账或 seq 基准错（日志占号 —— 侧信道里那句 MAA 变更另行咬字）",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":6,\"action\":\"launch\",\"ok\":true,\"data\":true}");
    let (dr_rc5, dr_e5) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    let dr_seen5 = dr_run_seen.borrow().clone();
    chk(
        dr_rc5 == 1
            && dr_e5.contains("\"kind\":\"fail\"")
            && dr_e5.contains("decide-daily-retry 起不来")
            && dr_run_calls.get() == 1
            && dr_seen5 == vec!["Fight".to_string(), "Shop".to_string()]
            && dr_run_rows() == 1
            && std::fs::read_to_string(&dr_j)
                .unwrap_or_default()
                .contains("{\"seq\":7,\"action\":\"run\",\"data\":{\"status\":\"ok\",\"detail\":\"一切正常\"}}"),
        "驱动档第五轮的 run 真做不对：ask 时代这里该停在 ask run，B-2c③ 后同一轮要一路做过 run（账上一行、chains 交得出、无 duration 就不补键）再撞坏判定 exe 当场断（闭包没被叫=分派没接上；被叫两遍=重放重做；键序/键数不对=账行形状和壳当年写的 res 不同形）",
        &mut fails,
        &mut total,
    );
    // 第六轮：同一本账再起一轮 —— 重放段必须只**消费** run 那行（闭包一次都不许多、账一行都不许多），
    // 然后照旧撞在 decide 上。ask 时代这一轮是"重放不问 run"，真做后判据换成闭包计数。
    let (dr_rc6, dr_e6) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    chk(
        dr_rc6 == 1
            && dr_e6.contains("\"kind\":\"fail\"")
            && dr_e6.contains("decide-daily-retry 起不来")
            && dr_run_calls.get() == 1
            && dr_run_rows() == 1,
        "驱动档第六轮重放把 run 又做了一遍（闭包被再叫或账上多一行）：真做动作的重放只消费账行 —— 一遍真做两遍 MAA，生产就是双起会话",
        &mut fails,
        &mut total,
    );
    let dr_done_row = jv_o(vec![("done", jv_o(vec![("status", jv_s("ok")), ("detail", jv_s("x")), ("duration", JVal::F(1.5))]))]);
    let _ = drive_append_row(&dr_j, &dr_done_row);
    let (dr_rc7, dr_e7) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    chk(
        dr_rc7 == 0
            && dr_e7.contains("\"kind\":\"done\"")
            && dr_e7.contains("{\"status\":\"ok\",\"detail\":\"x\",\"duration\":1.5}")
            && dr_e7.contains("\"logs\":[]")
            && dr_run_calls.get() == 1,
        "驱动档第七轮没把末行 done 直接收成 done 信封，或重放到 done 的一路把 run 又做了一遍（末行是 done 就不该再碰任何注入点）",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_c, "{\"launch\":false,\"retry_once\":false,\"decision\":{}}");
    let (dr_rc8, dr_e8) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    chk(
        dr_rc8 == 1 && dr_e8.contains("驱动协议坏了") && dr_e8.contains("不是同一份"),
        "驱动档第八轮没拒掉换 ctx 续旧账：init/ctx 不同源的会话会被静默接续",
        &mut fails,
        &mut total,
    );
    let (dr_rc9, dr_e9) = drive_ark_daily_envelope(false, &dr_j, &dr_c, &dr_r, dr_p, &dr_s, &dr_k, &mut dr_dec, &mut dr_run);
    chk(
        dr_rc9 == 2 && dr_e9.contains("--i-drive"),
        "驱动档第九轮没在缺 --i-drive 时拒跑（rc2）：默认档能把换头专用口当普通出口用",
        &mut fails,
        &mut total,
    );
    // —— B-2c② 侧信道三条判据（"换通道不换形状"的门侧论据就在这一段）——
    // ①九轮信封的 logs 键一律留恒空；②sink 行数 == journal 里 log 行数（两边都只在新区写 ⇒
    //   重放段一行不写，多一行=重复落、少一行=漏落）；③sink 每行是带 level/text 的合法 JSON 对象
    //   （壳侧照这一把行格式解析，形状一歪整条日志通道空转而无人察觉）。
    chk(
        [
            dr_e1.as_str(),
            dr_e2.as_str(),
            dr_e3.as_str(),
            dr_e4.as_str(),
            dr_e5.as_str(),
            dr_e6.as_str(),
            dr_e7.as_str(),
            dr_e8.as_str(),
            dr_e9.as_str(),
        ]
        .iter()
        .all(|e| e.contains("\"logs\":[]")),
        "九轮信封里有 logs 没留空：B-2c② 定的「留键不删键」没落到某一轮（arkdrive 门的 C-logs 面会把它判成分歧）",
        &mut fails,
        &mut total,
    );
    let dr_sink_txt = std::fs::read_to_string(&dr_k).unwrap_or_default();
    let dr_sink_rows: Vec<&str> = dr_sink_txt.lines().filter(|l| !l.trim().is_empty()).collect();
    let dr_journal_txt = std::fs::read_to_string(&dr_j).unwrap_or_default();
    let dr_log_rows = dr_journal_txt.lines().filter(|l| l.contains("\"action\":\"log\"")).count();
    chk(
        dr_log_rows >= 2 && dr_sink_rows.len() == dr_log_rows,
        &format!(
            "侧信道与 journal 的 log 行数不等（sink={} journal={}）：重放段重复落盘或新区漏落，两种都会让同一句日志打两遍或一遍不打",
            dr_sink_rows.len(),
            dr_log_rows
        ),
        &mut fails,
        &mut total,
    );
    chk(
        dr_sink_rows.iter().any(|l| l.contains("--- 方舟(端游 + MAA)---"))
            && dr_sink_rows.iter().any(|l| l.contains("\"level\":\"STEP\""))
            && dr_sink_rows.iter().any(|l| l.contains("MAA：chg-main")),
        "侧信道没带出链体首行(STEP)或 cfg 那句 MAA 变更行：日志落点从信封换到这里，字面一句没改 —— 红=新通道漏句，不是期望要放宽",
        &mut fails,
        &mut total,
    );
    chk(
        dr_sink_rows.iter().all(|l| matches!(jv_parse(l), Ok(ref v) if jv_has(v, "level") && jv_has(v, "text"))),
        "侧信道行格式不对（不是带 level/text 的合法 JSON 行）：壳侧照这一把行格式增量排空，形状一歪整条日志通道空转",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::remove_dir_all(&dr_dir);
    chk(
        !dr_dir.exists(),
        "驱动档自检的临时目录没收干净：自检会在 %TEMP% 里留垃圾",
        &mut fails,
        &mut total,
    );

    // 18. B-1a 混合执行器：已搬动作（mark/obsrec/admin）进程内真做，其余照旧 ask 重放。
    //     两会话各自独立 journal/state：A=ok 全链（认栽 mark + 观测 obsrec 真写两文件）；
    //     B=not-started 早退（admin 进程内真查、ERROR 句与回执值互反）。迷你驱动器照生产壳
    //     口径起轮、替办 ask、写 res。只碰 %TEMP% 下自己的目录；收尾连目录一起删。
    let d18 = std::env::temp_dir().join(format!("gd-b1a-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d18);
    let _ = std::fs::create_dir_all(&d18);
    // —— 会话 A：ok 全链（skip 跳→无 Farm→直接认栽 mark；收尾 ledger/obs/obsrec）——
    let a_j = d18.join("a-journal.ndjson").to_str().unwrap().to_string();
    let a_c = d18.join("a-ctx.json").to_str().unwrap().to_string();
    let a_r = d18.join("a-res.json").to_str().unwrap().to_string();
    let a_st = d18.join("a-state").to_str().unwrap().to_string();
    let a_k = d18.join("a-log-sink.ndjson").to_str().unwrap().to_string();
    let a2_k = d18.join("a2-log-sink.ndjson").to_str().unwrap().to_string();
    let _ = std::fs::create_dir_all(&a_st);
    let _ = std::fs::write(&a_c, "{\"launch\":true,\"retry_once\":false,\"decision\":{\"Mode\":\"plain\",\"Stage\":\"CE-5\"}}");
    // B-2c③：run 不再出现在 ask 名册里 —— 会话自带 run 闭包给终态，并数调用次数（一次为准）。
    let a_run_calls = std::cell::Cell::new(0usize);
    let mut run_a = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
        a_run_calls.set(a_run_calls.get() + 1);
        Ok(ArkRunOut { status: "ok".to_string(), detail: "一切正常".to_string(), duration: None })
    };
    let mut resp_a = |act: &str, prm: &JVal| drive_selftest_respond(act, prm);
    let (a_asked, a_logs, a_kind, a_result, a_err) =
        drive_selftest_session(&a_j, &a_c, &a_r, &a_st, &a_k, &mut resp_a, &mut run_a);
    let a_asked_want = vec!["desk", "takeover", "cfg", "launch", "skip", "stop", "ledger", "obs"];
    chk(
        a_err.is_none() && a_kind == "done" && a_asked == a_asked_want && a_run_calls.get() == 1,
        &format!(
            "B-1a 会话 A 没走到 done 或 ask 序不对：kind={} asked={:?} run调用次数={} err={:?}（已搬动作还出现在 ask 里=分派没接上；序漂=链体转写走样；run 出现在 ask 名册=B-2c③ 的双删没删净；次数≠1=真做被跳过或重演）",
            a_kind, a_asked, a_run_calls.get(), a_err
        ),
        &mut fails,
        &mut total,
    );
    let a_status = jv_str(&jv_get_ci(&a_result, "status"));
    let a_detail = jv_str(&jv_get_ci(&a_result, "detail"));
    chk(
        a_status == "ok" && a_detail == "一切正常；Fight 被 MAA 判全关(认栽，已记当日标记)",
        &format!("B-1a 会话 A 终态不对：[{}|{}]（认栽尾注该拼上；进程内 mark 的日期没回填）", a_status, a_detail),
        &mut fails,
        &mut total,
    );
    chk(
        a_logs.iter().any(|l| l.contains("已记今日认栽标记("))
            && a_logs.iter().any(|l| l.contains("关卡开放观测已记账(账本现 1 条)")),
        "B-1a 会话 A 的收尾两句话没带上（认栽标记句/观测记账句 —— 进程内动作的回读值没回填进链体日志）",
        &mut fails,
        &mut total,
    );
    chk(
        a_logs.first().map_or(false, |l| l.starts_with("STEP|") && l.contains("--- 方舟(端游 + MAA)---")),
        "B-2c② 会话 A 的侧信道首行不对（级别或文本串位）：壳侧照「级别|文本」逐行增量排空，第一行就错=整条日志通道口径错",
        &mut fails,
        &mut total,
    );
    let a_text = std::fs::read_to_string(&a_j).unwrap_or_default();
    let (_, a_rows, a_done) = match drive_load_journal(&a_text) {
        Ok(t) => t,
        Err(m) => {
            chk(false, &format!("B-1a 会话 A 的 journal 读不过去：{}", m), &mut fails, &mut total);
            (JVal::Null, Vec::new(), None)
        }
    };
    let a_row = |name: &str| a_rows.iter().find(|r| jv_str(&jv_get(r, "action")) == name);
    chk(
        a_row("mark").map_or(false, |r| jv_get(r, "data") == jv_s(&gd_local_date()))
            && a_row("obsrec").map_or(false, |r| jv_get(r, "data") == JVal::I(1))
            && a_row("admin").is_none()
            && a_row("run").map_or(false, |r| jv_to_json(&jv_get(r, "data")) == "{\"status\":\"ok\",\"detail\":\"一切正常\"}")
            && a_done.is_some(),
        "B-1a 会话 A 的 journal 软行不对：mark 该记真今日、obsrec 该记 1、ok 轮不该叫 admin、run 行该是核自己写的两格（无 duration 不补键）、done 行该在（软行=重放的唯一依据）",
        &mut fails,
        &mut total,
    );
    let a_mk_path = std::path::Path::new(&a_st).join("ark-fight-skip-mark.json");
    let a_mk_got = std::fs::read_to_string(&a_mk_path).unwrap_or_default();
    let a_mk_want = format!(
        "{{\r\n  \"Date\": \"{}\",\r\n  \"Stages\": [\r\n    \"CE-5\"\r\n  ],\r\n  \"Reason\": \"MAA 判「关卡计划内没有当日开放的关卡」\"\r\n}}",
        gd_local_date()
    );
    chk(
        a_mk_got == a_mk_want && a_mk_got.len() == 133,
        "B-1a 会话 A 的认栽标记字节与探针模板不符（Date 应为本机今日；\\r\\n/两空格缩进/尾无换行都要逐字节对）",
        &mut fails,
        &mut total,
    );
    let a_ob_path = std::path::Path::new(&a_st).join("ark-stage-weekday-obs.json");
    let a_ob_got = std::fs::read_to_string(&a_ob_path).unwrap_or_default();
    let a_ob_want = "{\r\n  \"Records\": [\r\n    {\r\n      \"Date\": \"2026-10-04\",\r\n      \"Dow\": 7,\r\n      \"Stage\": \"1-7\",\r\n      \"LastBattle\": true,\r\n      \"SawGuiPlan\": false,\r\n      \"Survivors\": [\r\n        \"72\",\r\n        \"30012\"\r\n      ],\r\n      \"Selected\": \"1-7\"\r\n    }\r\n  ]\r\n}";
    chk(
        a_ob_got == a_ob_want && a_ob_got.len() == 252,
        "B-1a 会话 A 的观测账本字节与探针 obsrec-a 模板不符（周日归一到 Dow=7/七键序/两空格缩进）",
        &mut fails,
        &mut total,
    );
    // —— 重放：删两文件 + 截掉末行 done 再推一轮 —— 重放=消费账行，不许重演动作、不许重写文件 ——
    let _ = std::fs::remove_file(&a_mk_path);
    let _ = std::fs::remove_file(&a_ob_path);
    let a_lines: Vec<&str> = a_text.lines().collect();
    let a_keep = if a_lines.is_empty() { 0 } else { a_lines.len() - 1 };
    let mut a_cut = String::new();
    for l in &a_lines[..a_keep] {
        a_cut.push_str(l);
        a_cut.push('\n');
    }
    let _ = std::fs::write(&a_j, &a_cut);
    let mut resp_a2 = |act: &str, prm: &JVal| drive_selftest_respond(act, prm);
    // 重放段的 run 闭包一次都不许被叫：叫了就说明真做动作在重放里重演（生产=双起 MAA 会话）。
    let a2_run_calls = std::cell::Cell::new(0usize);
    let mut run_a2 = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
        a2_run_calls.set(a2_run_calls.get() + 1);
        Ok(ArkRunOut { status: "ok".to_string(), detail: "一切正常".to_string(), duration: None })
    };
    let (a2_asked, a2_logs, a2_kind, a2_result, a2_err) =
        drive_selftest_session(&a_j, &a_c, &a_r, &a_st, &a2_k, &mut resp_a2, &mut run_a2);
    chk(
        a2_err.is_none()
            && a2_kind == "done"
            && a2_asked.is_empty()
            && a2_logs.is_empty()
            && a2_run_calls.get() == 0
            && !a_mk_path.exists()
            && !a_ob_path.exists()
            && jv_str(&jv_get_ci(&a2_result, "detail")) == "一切正常；Fight 被 MAA 判全关(认栽，已记当日标记)",
        &format!(
            "B-1a 截 done 重放不对：kind={} asked={:?} run调用次数={} sink行数={:?} err={:?}（重放该一路消费到 done：不重问、run 真做一次都不许再叫、不重写两文件、侧信道一行不写 —— 重放段落 sink 会让同一句日志打两遍）",
            a2_kind, a2_asked, a2_run_calls.get(), a2_logs.len(), a2_err
        ),
        &mut fails,
        &mut total,
    );
    // —— 会话 B：not-started 早退（admin 进程内真查；ERROR 句与回执值互反，升不升权都判得动）——
    let b_j = d18.join("b-journal.ndjson").to_str().unwrap().to_string();
    let b_c = d18.join("b-ctx.json").to_str().unwrap().to_string();
    let b_r = d18.join("b-res.json").to_str().unwrap().to_string();
    let b_st = d18.join("b-state").to_str().unwrap().to_string();
    let b_k = d18.join("b-log-sink.ndjson").to_str().unwrap().to_string();
    let _ = std::fs::create_dir_all(&b_st);
    let _ = std::fs::write(&b_c, "{\"launch\":true,\"retry_once\":false,\"decision\":{\"Mode\":\"plain\",\"Stage\":\"CE-5\"}}");
    let mut resp_b = |act: &str, prm: &JVal| drive_selftest_respond(act, prm);
    let b_run_calls = std::cell::Cell::new(0usize);
    let mut run_b = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
        b_run_calls.set(b_run_calls.get() + 1);
        Ok(ArkRunOut { status: "not-started".to_string(), detail: "没起得来".to_string(), duration: None })
    };
    let (b_asked, b_logs, b_kind, b_result, b_err) =
        drive_selftest_session(&b_j, &b_c, &b_r, &b_st, &b_k, &mut resp_b, &mut run_b);
    let b_asked_want = vec!["desk", "takeover", "cfg", "launch", "stop", "ledger", "obs"];
    chk(
        b_err.is_none()
            && b_kind == "done"
            && b_asked == b_asked_want
            && b_run_calls.get() == 1
            && jv_str(&jv_get_ci(&b_result, "status")) == "not-started"
            && jv_str(&jv_get_ci(&b_result, "detail")) == "没起得来",
        &format!(
            "B-1a 会话 B 的 ask 序或终态不对：kind={} asked={:?} run调用次数={} err={:?}（not-started 轮不许叫 skip——skip 只挂 ok 支；终态两格现由 run 闭包给，ask 名册里不该再有 run）",
            b_kind, b_asked, b_run_calls.get(), b_err
        ),
        &mut fails,
        &mut total,
    );
    let b_text = std::fs::read_to_string(&b_j).unwrap_or_default();
    let (_, b_rows, _) = match drive_load_journal(&b_text) {
        Ok(t) => t,
        Err(_) => (JVal::Null, Vec::new(), None),
    };
    let b_admin = b_rows.iter().find_map(|r| {
        if jv_str(&jv_get(r, "action")) == "admin" {
            match jv_get(r, "data") {
                JVal::B(v) => Some(v),
                _ => None,
            }
        } else {
            None
        }
    });
    let b_has_line = b_logs.iter().any(|l| l.contains("方舟这一路要管理员权限"));
    chk(
        b_admin.is_some() && b_has_line == (b_admin == Some(false)),
        &format!(
            "B-1a 会话 B 的 admin 互反不对：回执值={:?}、ERROR 句在场={}（进程内真查的值与该不该打 ERROR 句必须互为补）",
            b_admin, b_has_line
        ),
        &mut fails,
        &mut total,
    );
    let _ = std::fs::remove_dir_all(&d18);
    chk(
        !d18.exists(),
        "B-1a 自检的临时目录没收干净：自检会在 %TEMP% 里留垃圾",
        &mut fails,
        &mut total,
    );

    // 19. B-2a 运行面宿主（--maa-run 那一档的地基）：这一组只钉"采集件 + 骨架 + 投影"自己 ——
    //     六枚判定 verb 真跑 panel-cs 是 probe-arkrun-rust-golden 的活，这里一律把判定 exe 指到
    //     不存在的桩路径；正因为挑的两档走到头都不该碰判定，"投影里没有 V| 事件"本身就是断言。
    //     不加载 DLL、不碰现场：文件只写 %TEMP% 下自己的目录，收尾连目录一起删。
    let d19 = std::env::temp_dir().join(format!("gd-b2a-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d19);
    let _ = std::fs::create_dir_all(&d19);
    let d19s = d19.to_str().unwrap_or("").to_string();
    let t19 = |n: &str| format!("{}\\{}", d19s, n);
    chk(
        gd_join_path("", &["a", "b"]).is_empty()
            && gd_join_path("C:\\maa", &["gui.log"]) == "C:\\maa\\gui.log"
            && gd_join_path("C:\\maa", &["", "gui.log"]) == "C:\\maa\\gui.log"
            && gd_join_path("C:\\maa\\", &["gui.log"]) == "C:\\maa\\gui.log"
            && ps_join_path("C:\\maa", "D:\\abs.log") == "D:\\abs.log",
        "B-2a 的 Join-GdPath 复刻走样（空 base / 空段跳过 / 尾分隔符不重复 / 绝对段整替换）：两份日志的落点会和壳侧不一样",
        &mut fails,
        &mut total,
    );
    chk(
        file_stem("MAA.exe") == "MAA"
            && file_stem("C:\\d\\MaaAria2.dll") == "MaaAria2"
            && file_stem("debug.asst.log") == "debug.asst"
            && file_stem("noext") == "noext"
            && file_name("C:\\d\\gui.log") == "gui.log",
        "B-2a 的去扩展名复刻走样：探活用的进程名会认错（ark.exe→ark 与 MAA.exe→MAA 是同一条口径）",
        &mut fails,
        &mut total,
    );
    let mut sc19p = MaaScen {
        name: "paths".to_string(),
        maa_dir: "C:\\maa".to_string(),
        maa_exe: "MAA.exe".to_string(),
        maa_config: "configuration.json".to_string(),
        gui_log: "debug\\gui.log".to_string(),
        ..Default::default()
    };
    let p19 = MaaPaths::of(&sc19p);
    chk(
        p19.exe == "C:\\maa\\MAA.exe"
            && p19.config == "C:\\maa\\configuration.json"
            && p19.gui_log == "C:\\maa\\debug\\gui.log"
            && p19.asst_log == "C:\\maa\\debug\\asst.log"
            && p19.name == "MAA"
            && p19.dir == "C:\\maa",
        "B-2a 的 Get-MaaPaths 复刻走样（asst 缺省 debug\\asst.log / Name 去扩展名 / Config 那格也照算）：读错日志、探错进程",
        &mut fails,
        &mut total,
    );
    chk(
        p19.ps_to_string()
            == "@{Dir=C:\\maa; Exe=C:\\maa\\MAA.exe; Config=C:\\maa\\configuration.json; GuiLog=C:\\maa\\debug\\gui.log; AsstLog=C:\\maa\\debug\\asst.log; Name=MAA}",
        "PS 对象串渲染复刻走样（真身 maa.ps1:228 的 dry-run 话术吃的就是这个形状，属性顺序与 '; ' 分隔都是原话）",
        &mut fails,
        &mut total,
    );
    sc19p.asst_log = "my\\asst.log".to_string();
    chk(
        MaaPaths::of(&sc19p).asst_log == "C:\\maa\\my\\asst.log",
        "配置里给了 maaAsstLog 却没被吃进来：老配置换了落点会读错文件",
        &mut fails,
        &mut total,
    );
    chk(
        is_null_or_white("") && is_null_or_white("   ") && is_null_or_white("\t\r\n") && !is_null_or_white("x"),
        "IsNullOrWhiteSpace 的早退判据走样：会为空文本起一次判定进程（真身两枚转发壳明确不许，maa.ps1:117-138）",
        &mut fails,
        &mut total,
    );
    let mut cm19: Vec<(String, i64)> = Vec::new();
    cmap_set(&mut cm19, "Fight", 1);
    cmap_set(&mut cm19, "fight", 2);
    cmap_set(&mut cm19, "Shop", 1);
    let cm19j = cmap_to_jv(&cm19);
    chk(
        cm19.len() == 2
            && cmap_keys(&cm19).join(",") == "Fight,Shop"
            && jv_int(&jv_get(&cm19j, "Fight"), 0) == 2,
        "completed 容器的 hashtable 语义没复刻（重复键保留首个拼写、覆盖值，投影前 Ordinal 排序）：收尾那句「完成链路：」会拼错",
        &mut fails,
        &mut total,
    );
    let g19 = t19("tail-gui.log");
    chk(
        real_file_tail(&g19, 0) == (String::new(), 0),
        "日志不在场时 Get-GdFileTail 的复刻没给 ('',0)：会把「没日志」读成「读了但看不懂」",
        &mut fails,
        &mut total,
    );
    let _ = write_at(&g19, "abc中文\n".as_bytes(), false);
    let (t19a, o19a) = real_file_tail(&g19, 0);
    let _ = write_at(&g19, "def".as_bytes(), true);
    let (t19b, o19b) = real_file_tail(&g19, o19a);
    let (t19c, o19c) = real_file_tail(&g19, 999999);
    chk(
        t19a == "abc中文\n" && o19a == 10 && t19b == "def" && o19b == 13 && t19c == "abc中文\ndef" && o19c == 13,
        "Get-GdFileTail 的现场口径走样（新偏移=读时总长 / 越界归零全读=轮转那一支 / 增量只取新段）：轮转后的日志会被当成空",
        &mut fails,
        &mut total,
    );
    let sc19t = MaaScen {
        name: "timeline".to_string(),
        maa_dir: d19s.clone(),
        maa_exe: "MAA.exe".to_string(),
        gui_log: "tl-gui.log".to_string(),
        asst_log: "tl-asst.log".to_string(),
        seed_gui: Some("seed\n".to_string()),
        procs: vec![("MAA".to_string(), false)],
        events: vec![
            MaaEv { at: 10.0, kind: "append", target: "gui".to_string(), text: "A\n".to_string(), bytes: 0 },
            MaaEv { at: 10.0, kind: "up", target: "MAA".to_string(), text: String::new(), bytes: 0 },
            MaaEv { at: 25.0, kind: "truncate", target: "gui".to_string(), text: String::new(), bytes: 0 },
        ],
        ..Default::default()
    };
    let _ = maarun_seed(&sc19t);
    let tl_gui = sc19t.gui_log.clone();
    let tl_asst = sc19t.asst_log.clone();
    let mut h19t = ScriptHost::new("gd-no-such-panel", &sc19t);
    h19t.log("STEP", "x");
    h19t.advance(10);
    let grew19 = std::fs::read_to_string(t19(&tl_gui)).unwrap_or_default();
    let up19 = h19t.proc_running("maa");
    let _ = write_at(&t19(&tl_asst), "zz".as_bytes(), false);
    let ap19 = h19t.paths.asst_log.clone();
    let _ = h19t.file_tail(&ap19, 0);
    h19t.sleep(15);
    let trunc19 = std::fs::read_to_string(t19(&tl_gui)).unwrap_or_default();
    let start19 = h19t.start_process("C:\\maa\\MAA.exe", "C:\\maa", &[]);
    let verb19 = h19t.verb("parse-asst-state", &JVal::Null);
    let proj19t = h19t.proj.clone();
    chk(
        grew19 == "seed\nA\n" && up19 && trunc19.is_empty(),
        "剧本时间线走样（append 追加 / 同一时刻按剧本先后 / 睡眠推到才落 / truncate 原地清空）：两侧的起点会不一样",
        &mut fails,
        &mut total,
    );
    chk(
        proj19t.iter().any(|l| l == "W|STEP|x")
            && proj19t.iter().any(|l| l == "P|maa|1")
            && proj19t.iter().any(|l| l == "S|15")
            && proj19t.iter().any(|l| l == "T|tl-asst.log|0|2|2")
            && proj19t.iter().any(|l| l == "R|C:\\maa\\MAA.exe|C:\\maa")
            && proj19t.last().map_or(false, |l| l == "V|parse-asst-state"),
        "ScriptHost 的投影形状走样（W/P/S/T/R/V 六类各钉一条）：门的可见行为面就是这一串，形状漂了整个对拍没意义",
        &mut fails,
        &mut total,
    );
    chk(
        start19.is_ok() && verb19.err().map_or(false, |e| e.contains("起不来")),
        "verb 外调在判定 exe 不在场时没给出「起不来」这句：调用器把「进程起不来」和「判定挂了」混成一谈",
        &mut fails,
        &mut total,
    );
    let sc19d = MaaScen {
        name: "dry".to_string(),
        dry_run: true,
        expected: vec!["Fight".to_string(), "Shop".to_string()],
        maa_dir: "C:\\maa".to_string(),
        maa_exe: "MAA.exe".to_string(),
        maa_config: "configuration.json".to_string(),
        gui_log: "debug\\gui.log".to_string(),
        ..Default::default()
    };
    let mut h19d = ScriptHost::new("gd-no-such-panel", &sc19d);
    let r19d = maa_run(&mut h19d, &sc19d);
    let mut pd19: Vec<String> = h19d.proj.clone();
    if let Ok(rt) = r19d.as_ref() {
        pd19.push(rt.proj_line());
    }
    chk(
        r19d.is_ok()
            && pd19.len() == 2
            && pd19[0] == "W|STEP|[dry-run] 将启动 @{Dir=C:\\maa; Exe=C:\\maa\\MAA.exe; Config=C:\\maa\\configuration.json; GuiLog=C:\\maa\\debug\\gui.log; AsstLog=C:\\maa\\debug\\asst.log; Name=MAA}.Exe(自动开始队列：Fight, Shop)"
            && pd19[1] == "Z|dryrun||-|-",
        "dry-run 那一档的形状走样（只一行 STEP、Detail 空串、Errors 这个键根本不该在）：真身 maa.ps1:228 就这三件事，话术里的对象串也照原样",
        &mut fails,
        &mut total,
    );
    let sc19n = MaaScen {
        name: "nostart".to_string(),
        maa_dir: d19s.clone(),
        maa_exe: "MAA.exe".to_string(),
        gui_log: "ns-gui.log".to_string(),
        ark_exe: "Ak.exe".to_string(),
        ..Default::default()
    };
    let _ = maarun_seed(&sc19n);
    let mut h19n = ScriptHost::new("gd-no-such-panel", &sc19n);
    let r19n = maa_run(&mut h19n, &sc19n);
    let mut pn19: Vec<String> = h19n.proj.clone();
    if let Ok(rt) = r19n.as_ref() {
        pn19.push(rt.proj_line());
    }
    let n_probe = pn19.iter().filter(|l| l.starts_with("P|MAA|0")).count();
    let n_sleep = pn19.iter().filter(|l| l.as_str() == "S|3").count();
    let n_verb = pn19.iter().filter(|l| l.starts_with("V|")).count();
    let msg19n = format!(
        "B-2a「进程起不来」那一档的形状走样：探活={} 睡眠={} 判定调用={} 末行={:?}（90 秒闸按 3 秒一轮该探满 30 次，且这一路一次判定都不该调）",
        n_probe, n_sleep, n_verb, pn19.last()
    );
    chk(
        r19n.is_ok()
            && n_probe == 30
            && n_sleep == 30
            && n_verb == 0
            && pn19.iter().any(|l| l == "W|WARN|等待超时(90s)：MAA 进程启动")
            && pn19.last().map_or(false, |l| l == "Z|error|MAA 未能启动(进程未出现)|-|-"),
        &msg19n,
        &mut fails,
        &mut total,
    );
    let out19 = t19("env.json");
    let rc19s = run_maa_run("", "gd-no-such-panel", &Some(out19.clone()));
    let j19s = std::fs::read_to_string(&out19).unwrap_or_default();
    chk(
        rc19s == 2 && j19s.contains(r#""ok":false"#) && j19s.contains("--script"),
        "--maa-run 缺剧本没被拒跑（该是 rc 2 加一句 ok:false）：这一档会哑到把空剧本集当成跑通了",
        &mut fails,
        &mut total,
    );
    let miss19 = t19("no-such-script.json");
    let rc19p = run_maa_run(&miss19, "", &Some(out19.clone()));
    let j19p = std::fs::read_to_string(&out19).unwrap_or_default();
    chk(
        rc19p == 2 && j19p.contains("--panel-cs"),
        "--maa-run 缺判定 exe 没被拒跑：循环骨架每 tick 都要真跑 decide-maa-run-tick，缺它这一档没开",
        &mut fails,
        &mut total,
    );
    let rc19f = run_maa_run(&miss19, "gd-no-such-panel", &Some(out19.clone()));
    let j19f = std::fs::read_to_string(&out19).unwrap_or_default();
    chk(
        rc19f == 1 && j19f.contains(r#""ok":false"#),
        "剧本文件不在场时没按「读不动」报错（rc 1）：门会拿到一份没有 cases 的假出口",
        &mut fails,
        &mut total,
    );
    let sc19ok = t19("script-ok.json");
    let _ = std::fs::write(
        &sc19ok,
        "[{\"name\":\"b2-dry\",\"dryRun\":true,\"expectedChains\":[\"Fight\",\"Shop\"],\"paths\":{\"maaDir\":\"C:\\\\maa\",\"maaExe\":\"MAA.exe\"}}]",
    );
    let rc19ok = run_maa_run(&sc19ok, "gd-no-such-panel", &Some(out19.clone()));
    let j19ok = std::fs::read_to_string(&out19).unwrap_or_default();
    chk(
        rc19ok == 0
            && j19ok.contains(r#""ok":true"#)
            && j19ok.contains("\"count\":1")
            && j19ok.contains("b2-dry")
            && j19ok.contains("\\u001f"),
        "剧本全过的出口形状走样（ok/count/档名/投影分隔符四格）：门的两侧对账拿不到同一份形状",
        &mut fails,
        &mut total,
    );
    let sc19r = t19("script-nopanel.json");
    let _ = std::fs::write(
        &sc19r,
        &format!("[{{\"name\":\"b2-dry\",\"dryRun\":true,\"paths\":{{\"maaDir\":\"C:\\\\\\\\maa\",\"maaExe\":\"MAA.exe\"}}}},{{\"name\":\"b2-nopanel\",\"procs\":[{{\"name\":\"MAA\",\"up\":true}}],\"paths\":{{\"maaDir\":\"{}\",\"maaExe\":\"MAA.exe\",\"maaGuiLog\":\"np-gui.log\",\"arkExe\":\"Ak.exe\"}}}}]", d19s.replace('\\', "\\\\")),
    );
    let rc19r = run_maa_run(&sc19r, "gd-no-such-panel", &Some(out19.clone()));
    let j19r = std::fs::read_to_string(&out19).unwrap_or_default();
    chk(
        rc19r == 1 && j19r.contains("b2-nopanel") && j19r.contains("decide-maa-run-tick"),
        "有一档没跑完时没按档名 + 那枚判定 verb 报出去：判定 exe 不在场时这一档会假装跑通",
        &mut fails,
        &mut total,
    );
    let sc19k = t19("script-badkind.json");
    let _ = std::fs::write(
        &sc19k,
        "[{\"name\":\"b2-bad\",\"events\":[{\"at\":1,\"kind\":\"bogus\",\"target\":\"gui\"}]}]",
    );
    let rc19k = run_maa_run(&sc19k, "gd-no-such-panel", &Some(out19.clone()));
    let j19k = std::fs::read_to_string(&out19).unwrap_or_default();
    chk(
        rc19k == 1 && j19k.contains("bogus"),
        "剧本里认不出的事件 kind 被放行了：夹具打错一个词会让整档读数变得没法解释",
        &mut fails,
        &mut total,
    );
    let root19q = jv_parse("[{\"name\":\"only-name\"}]").unwrap_or(JVal::Null);
    let lst19q = jv_list(&root19q);
    let q19 = lst19q.first().and_then(|v| maarun_parse(v).ok());
    chk(
        q19.map_or(false, |s| {
            s.name == "only-name"
                && s.start_min == 0
                && s.expected.is_empty()
                && s.procs.is_empty()
                && s.seed_gui.is_none()
        }),
        "剧本缺键没按「没填」取值（或反过来替夹具猜了默认值）：档与档之间的起点会被猜成不一样",
        &mut fails,
        &mut total,
    );
    let mut rh19 = RealHost::new("gd-no-such-panel", &t19("rh19-sink.ndjson"));
    chk(
        !rh19.proc_running("gd-definitely-not-running"),
        "RealHost 的现场读法走样：不存在的进程名被判成在场（探活是 stuck/drain 两档的门槛，这里错一格整轮就变）",
        &mut fails,
        &mut total,
    );
    // B-2d③ 结案登记：上面那枚**只有负例**。恒 false 的读法在负例上照样绿 —— 10-11 起 run 进核之后
    // 现场连报"MAA 未能启动(进程未出现)"，而 MAA 自己的日志显示它起来了并把队列跑完，那格读的
    // 就是这个没被验过的正例面。这里补一枚**自身进程必然在场**的正例：current_exe 的 stem 就是
    // 本进程自己，快照读得出任何一条就必然命中它；读不出任何一条(dwSize/布局走样)当场红。
    let self_exe19 = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let self_name19 = file_stem(&self_exe19);
    chk(
        !self_name19.is_empty() && rh19.proc_running(&self_name19),
        "RealHost 的现场读法读不出任何进程：连自己这个进程都不在场，说明快照枚举一条都没落进来(dwSize/结构布局走样)。这一格恒 false 会把\"进程确实起来了\"记成\"进程未出现\"，整轮的成败读数全部作废",
        &mut fails,
        &mut total,
    );
    // B-2b 把 B-2a 那两枚"恒 false"换成按模块清单给的常量（ark-module.ps1:31 / r1999-module.ps1:27）。
    // 这里钉的是"两路各是什么"这一格本身 —— 树上有没有那枚函数的事实源在 lib 的 Libs 清单里，
    // 清单变了这里就得跟着变（跟着变是应当的，不是回归）。
    chk(
        rh19.rescue_available() && rh19.grab_available(),
        "RealHost(ark) 的两枚可用性走样：ark 一路兜底与抢前台都在场，缺任何一枚 stuck 档就走不到兜底那一步",
        &mut fails,
        &mut total,
    );
    let rh19m = RealHost::for_game("gd-no-such-panel", "r1999", &t19("rh19m-sink.ndjson"));
    chk(
        rh19m.rescue_available() && !rh19m.grab_available(),
        "RealHost(r1999) 的两枚可用性走样：mumu 一路没有抢前台那枚(住 vision-ark.ps1，端游专用)，兜底照旧在场",
        &mut fails,
        &mut total,
    );

    // ---- B-2b：兜底编排的零件与离线全链 ----
    // 这一组同样不加载 DLL、不碰窗口、不起 Python：七枚现场 verb 由剧本夹具喂，纯规则 verb 在自检里
    // 也给夹具（自检的判定 exe 是指向不存在路径的桩）。钉的是"闸序 + 候选循环 + 三种结论话术 +
    // 两本账的落盘形状"这四件，判据一律用真值清单，不拿形状猜测当结论。
    chk(
        tag_like("stuck-3", "stuck")
            && tag_like("STUCK-3", "stuck")
            && tag_like("stuck", "stuck")
            && !tag_like("drain-1", "stuck")
            && !tag_like("stuc", "stuck")
            && !tag_like("", "stuck"),
        "tag 的前缀通配没按 -like 不分大小写复刻（或把「恰好等于前缀」那一格漏了）：三档执行块会认错档，兜底整轮不触发",
        &mut fails,
        &mut total,
    );
    chk(
        kind_eq("Rescue", "rescue")
            && kind_eq("GRAB", "grab")
            && !kind_eq("rescue", "record")
            && !kind_eq("", "grab"),
        "kind 的等值比走样（PS 的 -eq 比字符串不分大小写）：C# 挂来的 effect 会被当成没落到执行块而当场作废",
        &mut fails,
        &mut total,
    );
    let b19exp = expected_dedup(&[
        "Fight".to_string(),
        "fight".to_string(),
        "Shop".to_string(),
    ]);
    chk(
        b19exp.len() == 2
            && cmap_keys(&b19exp).join(",") == "Fight,Shop"
            && cmap_get(&b19exp, "FIGHT") == Some(1)
            && cmap_get(&b19exp, "shop").is_some()
            && cmap_get(&b19exp, "Rescue").is_none(),
        "期望链容器没照 hashtable 语义（重复键留首个拼写、值恒 1、查键不分大小写）：完成数与「跑齐」两条判据都会漂",
        &mut fails,
        &mut total,
    );
    let b19cmp: Vec<(String, i64)> = vec![("FIGHT".to_string(), 3)];
    let b19all: Vec<(String, i64)> = vec![("Fight".to_string(), 1), ("SHOP".to_string(), 2)];
    let b19zero: Vec<(String, i64)> = Vec::new();
    chk(
        done_count(&b19exp, &b19cmp) == 1
            && !all_done(&b19exp, &b19cmp)
            && all_done(&b19exp, &b19all)
            && done_count(&b19exp, &b19zero) == 0
            && !all_done(&b19zero, &b19all),
        "完成数/跑齐两条判据走样（每条期望链最多算 1、键不在场按 0、没有期望链就谈不上跑齐）：drain 档的现场复核守卫会误判",
        &mut fails,
        &mut total,
    );
    chk(
        jv_str(&JVal::I(1520)) == "1520"
            && jv_str(&JVal::F(1.5)) == "1.5"
            && jv_str(&JVal::B(true)) == "True"
            && jv_str(&JVal::Null).is_empty()
            && jv_int(&jv_s(" 42 "), 0) == 42
            && jv_int(&jv_s("x"), -1) == -1,
        "「字符串吃数值」那一格的取数口径走样：C# 回整数坐标时，预告句与 tries 里那两段会被拼成空串",
        &mut fails,
        &mut total,
    );
    let mut b19led: Vec<JVal> = Vec::new();
    for i in 1..=61i64 {
        b19led = ledger_push(b19led, JVal::I(i), 60);
    }
    chk(
        b19led.len() == 60
            && jv_int(&b19led[0], 0) == 2
            && jv_int(&b19led[59], 0) == 61
            && ledger_push(vec![JVal::I(1)], JVal::I(2), 0).len() == 2,
        "救火账本的 Keep 60 滑窗走样（第 61 条该挤掉第 1 条；keep<=0 一枚都不裁）：这本账会无限涨",
        &mut fails,
        &mut total,
    );
    let b19stale = pick_stale(
        vec![
            ("a".to_string(), 100u64),
            ("b".to_string(), 300),
            ("c".to_string(), 200),
            ("d".to_string(), 100),
        ],
        2,
    );
    chk(
        b19stale == vec!["a".to_string(), "d".to_string()]
            && pick_stale(vec![("x".to_string(), 1u64)], 5).is_empty()
            && pick_stale(vec![("x".to_string(), 1u64), ("y".to_string(), 2)], 0).len() == 2,
        "截图目录「留哪几枚」的判据走样（mtime 倒序留 keep、同刻并列按输入先后、keep>=总数时一枚都不删）：Keep 24 会把最新那几张删掉",
        &mut fails,
        &mut total,
    );
    // 夹具：一条全过的 ark 兜底链，七枚返回值各给一份；下面四档只改其中几枚的形状。
    let b19fx_ctx = jv_o(vec![
        ("CanAsk", JVal::B(true)),
        ("Kind", jv_s("ark")),
        ("Name", jv_s("端游")),
        ("Scene", jv_s("rescue")),
        ("UserText", jv_s("这一屏有没有弹窗挡路")),
    ]);
    let b19fx_shot = jv_o(vec![
        ("Ok", JVal::B(true)),
        ("CanContinue", JVal::B(true)),
        ("LogLine", jv_s("截图：窗口客户区 1600x900")),
        ("Path", jv_s("x:\\before.png")),
        ("ImgW", JVal::I(1600)),
        ("ImgH", JVal::I(900)),
        ("ClientLeft", JVal::I(0)),
        ("ClientTop", JVal::I(0)),
        ("ClientW", JVal::I(1600)),
        ("ClientH", JVal::I(900)),
        ("Sig", jv_s("sig-before")),
    ]);
    let b19fx_after = jv_o(vec![
        ("Ok", JVal::B(true)),
        ("Why", jv_s("")),
        ("Sig", jv_s("sig-after")),
    ]);
    let b19fx_ask = jv_o(vec![
        ("Ok", JVal::B(true)),
        ("Verdict", jv_o(vec![
            ("Found", JVal::B(true)),
            ("Blocking", JVal::B(true)),
            ("What", jv_s("活动页")),
            ("X", jv_s("0.95")),
            ("Y", jv_s("0.06")),
            ("Clicks", JVal::A(vec![jv_o(vec![
                ("X", jv_s("0.95")),
                ("Y", jv_s("0.06")),
                ("Why", jv_s("右上角的关闭")),
            ])])),
        ])),
    ]);
    let b19fx_point = jv_o(vec![
        ("Found", JVal::B(true)),
        ("ClientX", JVal::I(1520)),
        ("ClientY", JVal::I(54)),
        ("Why", jv_s("右上角的关闭")),
    ]);
    let b19fx_click = jv_o(vec![
        ("Mode", jv_s("真实点击")),
        ("Sent", JVal::B(true)),
        ("TryPiece", jv_s("点位 1520,54(真实点击)")),
        ("LogLine", jv_s("视觉兜底：活动页 → 点端游 1520,54(右上角的关闭)")),
    ]);
    let b19fx_chg_yes = jv_o(vec![
        ("Known", JVal::B(true)),
        ("Changed", JVal::B(true)),
        ("Detail", jv_s("画面动了")),
    ]);
    let b19live = |tail: Vec<(&str, JVal)>| -> Vec<(String, JVal)> {
        let mut v: Vec<(String, JVal)> = vec![
            ("resolve-rescue-context".to_string(), b19fx_ctx.clone()),
            ("ark-window-shot".to_string(), b19fx_shot.clone()),
            ("vision-ask-model".to_string(), b19fx_ask.clone()),
            ("vision-click-point".to_string(), b19fx_point.clone()),
            ("ark-window-click".to_string(), b19fx_click.clone()),
            ("ark-window-shot".to_string(), b19fx_after.clone()),
        ];
        for (n, d) in tail {
            v.push((n.to_string(), d));
        }
        v
    };
    // 成功档：顺手把 26 张真截图摆在 state-c\\rescue 里，链尾那次裁剪就该落在这批真件上。
    let b19state_c = format!("{}\\state-c", d19s);
    let b19shotdir = format!("{}\\rescue", b19state_c);
    let _ = std::fs::create_dir_all(&b19shotdir);
    for i in 0..26 {
        let _ = std::fs::write(format!("{}\\s{:02}.png", b19shotdir, i), b"x");
    }
    let _ = std::fs::write(format!("{}\\keep.txt", b19shotdir), b"x");
    let b19sc_ok = MaaScen {
        name: "rescue-ok".to_string(),
        state_dir: b19state_c.clone(),
        live: b19live(vec![("shot-changed", b19fx_chg_yes.clone())]),
        ..Default::default()
    };
    let mut b19h_ok = ScriptHost::new("gd-no-such-panel", &b19sc_ok);
    let b19rc_ok = vision_rescue(&mut b19h_ok, &RescueEnv::of_maa(&b19sc_ok), "stuck-1", "MAA 日志 12 分钟无输出");
    let b19pngs = std::fs::read_dir(&b19shotdir).map_or(0, |rd| {
        rd.flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .ends_with(".png")
            })
            .count()
    });
    trim_shot_dir(&format!("{}\\no-such-dir", d19s), 24);
    let b19want_ok: Vec<String> = vec![
        "V|resolve-rescue-context",
        "V|ark-window-shot",
        "W|WARN|截图：窗口客户区 1600x900",
        "V|vision-ask-model",
        "V|vision-click-point",
        "W|WARN|视觉兜底：活动页 → 点端游 1520,54(右上角的关闭)",
        "V|ark-window-click",
        "S|6",
        "V|ark-window-shot",
        "V|shot-changed",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect();
    chk(
        b19rc_ok.as_ref().map_or(false, |r| {
            r.tried && r.done && r.detail == "已按模型判断关掉活动页：点位 1520,54(真实点击)→画面动了"
        }) && b19h_ok.proj == b19want_ok
            && b19h_ok.live.is_empty(),
        "全过那一档的兜底链形状走样（闸序 / 预告句排在点击 verb 之前 / 那 6 秒走宿主睡眠 / 三种结论里的「救活」话术，四件之一）：真兜底在核里就不是同一条链",
        &mut fails,
        &mut total,
    );
    chk(
        b19pngs == 24 && std::fs::metadata(format!("{}\\keep.txt", b19shotdir)).is_ok(),
        "链尾那次 Keep 24 裁剪没落在真盘上（26 张该留 24 张、非 png 一枚都不许碰）：截图目录会涨，而删除的又是刚拍的",
        &mut fails,
        &mut total,
    );
    let b19sc_off = MaaScen {
        name: "rescue-off".to_string(),
        live: vec![(
            "resolve-rescue-context".to_string(),
            jv_o(vec![
                ("CanAsk", JVal::B(false)),
                ("Skip", jv_s("视觉兜底没开：settings 的 visionRescueEnabled=false")),
            ]),
        )],
        ..Default::default()
    };
    let mut b19h_off = ScriptHost::new("gd-no-such-panel", &b19sc_off);
    let b19rc_off = vision_rescue(&mut b19h_off, &RescueEnv::of_maa(&b19sc_off), "quiet-1", "");
    let b19one: Vec<String> = vec!["V|resolve-rescue-context".to_string()];
    chk(
        b19rc_off.as_ref().map_or(false, |r| {
            !r.tried && !r.done && r.detail == "视觉兜底没开：settings 的 visionRescueEnabled=false"
        }) && b19h_off.proj == b19one,
        "开关关着那一档没做到「一次都不试」（或把 verb 的 Skip 改写了）：t13 那句「关了确实一次都不试」的事实源就废了",
        &mut fails,
        &mut total,
    );
    let b19sc_sf = MaaScen {
        name: "rescue-shotfail".to_string(),
        live: vec![
            ("resolve-rescue-context".to_string(), b19fx_ctx.clone()),
            (
                "ark-window-shot".to_string(),
                jv_o(vec![
                    ("Ok", JVal::B(false)),
                    ("Skip", jv_s("截图失败：窗口已消失(没抢到前台)")),
                ]),
            ),
        ],
        ..Default::default()
    };
    let mut b19h_sf = ScriptHost::new("gd-no-such-panel", &b19sc_sf);
    let b19rc_sf = vision_rescue(&mut b19h_sf, &RescueEnv::of_maa(&b19sc_sf), "drain-1", "");
    chk(
        b19rc_sf.as_ref().map_or(false, |r| {
            !r.tried && r.detail == "截图失败：窗口已消失(没抢到前台)"
        }) && b19h_sf.proj.len() == 2,
        "第七道闸（拍不到东西就放弃）没只消费 verb 给的整句 Skip：核里重拼一遍会和 C# 那句双前缀的话术分岔",
        &mut fails,
        &mut total,
    );
    let b19sc_oob = MaaScen {
        name: "rescue-oob".to_string(),
        live: vec![
            ("resolve-rescue-context".to_string(), b19fx_ctx.clone()),
            ("ark-window-shot".to_string(), b19fx_shot.clone()),
            (
                "vision-ask-model".to_string(),
                jv_o(vec![
                    ("Ok", JVal::B(true)),
                    ("Verdict", jv_o(vec![
                        ("Found", JVal::B(true)),
                        ("Blocking", JVal::B(true)),
                        ("What", jv_s("活动页")),
                        ("X", jv_s("0.95")),
                        ("Y", jv_s("0.06")),
                        ("Clicks", JVal::A(vec![
                            jv_o(vec![("Y", jv_s("0.06")), ("Why", jv_s("这一枚没给 X"))]),
                            jv_o(vec![("X", jv_s("0.95")), ("Y", jv_s("0.06")), ("Why", jv_s("越界的那枚"))]),
                        ])),
                    ])),
                ]),
            ),
            (
                "vision-click-point".to_string(),
                jv_o(vec![("Found", JVal::B(false)), ("Why", jv_s("落在标题栏上"))]),
            ),
            ("ark-window-click".to_string(), b19fx_click.clone()),
        ],
        ..Default::default()
    };
    let mut b19h_oob = ScriptHost::new("gd-no-such-panel", &b19sc_oob);
    let b19rc_oob = vision_rescue(&mut b19h_oob, &RescueEnv::of_maa(&b19sc_oob), "stuck-2", "");
    let b19n_point = b19h_oob
        .proj
        .iter()
        .filter(|l| l.starts_with("V|vision-click-point"))
        .count();
    chk(
        b19rc_oob.as_ref().map_or(false, |r| {
            !r.tried && r.detail == "模型说挡路的是活动页，但它给的坐标(0.95,0.06)落不到画面里 → 不点"
        }) && b19n_point == 1
            && !b19h_oob.proj.iter().any(|l| l.starts_with("V|ark-window-click"))
            && !b19h_oob.proj.iter().any(|l| l.starts_with("V|shot-changed")),
        "候选点位的两道本地闸走样（坐标缺一枚就不外调换算、换算不落画面里就不点，且收尾那句吃的是 verdict 的 X/Y 不是客户区坐标）：会白起一次换算并点错地方",
        &mut fails,
        &mut total,
    );
    let b19sc_no = MaaScen {
        name: "rescue-nochange".to_string(),
        live: b19live(vec![(
            "shot-changed",
            jv_o(vec![
                ("Known", JVal::B(false)),
                ("Changed", JVal::B(false)),
                ("Detail", jv_s("画面看不出变化")),
            ]),
        )]),
        ..Default::default()
    };
    let mut b19h_no = ScriptHost::new("gd-no-such-panel", &b19sc_no);
    let b19rc_no = vision_rescue(&mut b19h_no, &RescueEnv::of_maa(&b19sc_no), "stuck-1", "");
    chk(
        b19rc_no.as_ref().map_or(false, |r| {
            r.tried && !r.done && r.detail == "按模型说的把活动页点了一下(点位 1520,54(真实点击)→画面看不出变化)，但画面看不出变化 → 不算救活"
        }) && b19h_no.proj.len() == 10,
        "三种结论里「点了但画面没变」那一支的话术走样（真身 vision-rescue.ps1:386 那句里 $chg.Detail 出现两次：一次并进 tries 尾段、一次跟在「但」后面 —— 原样，不许顺手去重）：会把它说成救活了",
        &mut fails,
        &mut total,
    );
    let b19sc_askerr = MaaScen {
        name: "rescue-askerr".to_string(),
        live: vec![
            ("resolve-rescue-context".to_string(), b19fx_ctx.clone()),
            ("ark-window-shot".to_string(), b19fx_shot.clone()),
        ],
        ..Default::default()
    };
    let mut b19h_ask = ScriptHost::new("gd-no-such-panel", &b19sc_askerr);
    let b19rc_ask = vision_rescue(&mut b19h_ask, &RescueEnv::of_maa(&b19sc_askerr), "stuck-1", "");
    // 四行 = 六闸 / 首拍 / 那句截图 WARN / 问模型(真外调，桩路径必然起不来)。
    chk(
        b19rc_ask.as_ref().map_or(false, |r| {
            !r.tried && r.detail.starts_with("模型调用失败：")
        }) && b19h_ask.proj.len() == 4,
        "问模型这一处没把 verb 的错降级成一句 skip（三处 catch 里唯一吃掉 verb 错的那一处）：模型侧起不来会把整轮兜底打成异常回执",
        &mut fails,
        &mut total,
    );
    // do_rescue 的预算两格：在场时"先无条件 +1 再 -1"，不在场时那一格白加回去（§11.34 ⑦ 的原样怪癖）。
    let b19sc_on = MaaScen {
        name: "budget-on".to_string(),
        rescue_available: true,
        state_dir: format!("{}\\state-on", d19s),
        live: b19live(vec![("shot-changed", b19fx_chg_yes.clone())]),
        ..Default::default()
    };
    let mut b19h_bon = ScriptHost::new("gd-no-such-panel", &b19sc_on);
    let mut b19effx_on: Vec<JVal> = Vec::new();
    let mut b19left_on = 1i64;
    do_rescue(
        &mut b19h_bon,
        &RescueEnv::of_maa(&b19sc_on),
        "rescue",
        &jv_o(vec![
            ("Kind", jv_s("rescue")),
            ("Tag", jv_s("stuck-1")),
            ("Context", jv_s("")),
        ]),
        &jv_o(vec![("rescueLeft", JVal::I(1))]),
        &mut b19effx_on,
        &mut b19left_on,
    );
    let b19sc_off2 = MaaScen {
        name: "budget-off".to_string(),
        state_dir: format!("{}\\state-off", d19s),
        ..Default::default()
    };
    let mut b19h_boff = ScriptHost::new("gd-no-such-panel", &b19sc_off2);
    let mut b19effx_off: Vec<JVal> = Vec::new();
    let mut b19left_off = 0i64;
    do_rescue(
        &mut b19h_boff,
        &RescueEnv::of_maa(&b19sc_off2),
        "rescue",
        &jv_o(vec![
            ("Kind", jv_s("rescue")),
            ("Tag", jv_s("stall-1")),
            ("Context", jv_s("")),
        ]),
        &jv_o(vec![("rescueLeft", JVal::I(0))]),
        &mut b19effx_off,
        &mut b19left_off,
    );
    chk(
        b19left_on == 1
            && b19effx_on.len() == 1
            && jv_str(&jv_get(&b19effx_on[0], "kind")) == "rescue"
            && jv_truthy(&jv_get(&b19effx_on[0], "done")),
        "兜底预算的「state 里的余额先 +1 再 -1」走样，或回执三键(kind/tried/done/detail)没照真身形状出：一轮里的兜底次数会与壳侧算不齐",
        &mut fails,
        &mut total,
    );
    chk(
        b19left_off == 1 && b19effx_off.is_empty() && b19h_boff.proj.is_empty(),
        "树上没有兜底函数那一支没照真身把预算白加回一格（或反过来还去调了 verb）：这是登记过的原样怪癖，不许顺手修",
        &mut fails,
        &mut total,
    );
    let b19sc_led = MaaScen {
        name: "ledger".to_string(),
        state_dir: format!("{}\\state-led", d19s),
        game: "ark".to_string(),
        target: "端游".to_string(),
        live: vec![
            (
                "rescue-entry".to_string(),
                jv_o(vec![
                    ("Time", jv_s("2026-10-08 07:00:00")),
                    ("Game", jv_s("ark")),
                    ("Target", jv_s("端游")),
                    ("Tag", jv_s("quiet-1")),
                    ("Tried", JVal::B(true)),
                    ("Done", JVal::B(false)),
                    ("Detail", jv_s("第一条")),
                ]),
            ),
            (
                "rescue-entry".to_string(),
                jv_o(vec![
                    ("Time", jv_s("2026-10-08 07:00:01")),
                    ("Game", jv_s("ark")),
                    ("Target", jv_s("端游")),
                    ("Tag", jv_s("quiet-2")),
                    ("Tried", JVal::B(false)),
                    ("Done", JVal::B(false)),
                    ("Detail", jv_s("第二条")),
                ]),
            ),
        ],
        ..Default::default()
    };
    let mut b19h_led = ScriptHost::new("gd-no-such-panel", &b19sc_led);
    let b19r1 = add_rescue_record(&mut b19h_led, &RescueEnv::of_maa(&b19sc_led), "quiet-1", "第一条", true, false);
    let b19r2 = add_rescue_record(&mut b19h_led, &RescueEnv::of_maa(&b19sc_led), "quiet-2", "第二条", false, false);
    let b19ltext = std::fs::read_to_string(rescue_ledger_path(&RescueEnv::of_maa(&b19sc_led))).unwrap_or_default();
    let b19lroot = jv_parse(&b19ltext).unwrap_or(JVal::Null);
    let b19lev = jv_get(&b19lroot, "events");
    let b19levs = jv_list(&b19lev);
    chk(
        b19r1.is_ok()
            && b19r2.is_ok()
            && b19ltext.starts_with('{')
            && jv_int(&jv_get(&b19lroot, "version"), 0) == 1
            && !jv_str(&jv_get(&b19lroot, "updated")).is_empty()
            && b19levs.len() == 2
            && jv_str(&jv_get(b19levs[0], "tag")) == "quiet-1"
            && jv_truthy(&jv_get(b19levs[0], "tried"))
            && !jv_truthy(&jv_get(b19levs[1], "tried"))
            && jv_str(&jv_get(b19levs[1], "detail")) == "第二条"
            && !b19h_led.proj.iter().any(|l| l.starts_with("W|WARN")),
        "救火账本的读-改-写落盘走样（version/updated/events 三格、UTF-8 无 BOM 的美化形状、七键一条、文件不在场那次不该惊动 WARN）：这本账会与真身那份不同形",
        &mut fails,
        &mut total,
    );

    let _ = std::fs::remove_dir_all(&d19);
    chk(
        !d19.exists(),
        "B-2a 自检的临时目录没收干净：自检会在 %TEMP% 里留垃圾",
        &mut fails,
        &mut total,
    );

    // 20. B-2c③「run 换真做」：这一组钉真做这一刀自己的判据 —— 闸的优先级（缺 --i-drive 必须先于
    //     夹具错报出来）、`--fixture-run` 的三条错路与一条正路（账行逐字 + 夹具 logs 按序进侧信道、
    //     重放不重做）、ctx 那 13 枚的取键、生产 run 闭包的两条边界（载荷缺失当场拒做、RealHost 的
    //     run 内部行落进链体同一本侧信道）、以及真做抛错仍被链体吞成 error 态。
    //     文件一律写 %TEMP% 下自己的目录，收尾连目录一起删；判定 exe 全指不存在的桩路径。
    let d20 = std::env::temp_dir().join(format!("gd-b2c3-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d20);
    let _ = std::fs::create_dir_all(&d20);
    let d20s = d20.to_str().unwrap_or("").to_string();
    let t20 = |n: &str| format!("{}\\{}", d20s, n);
    let e20 = t20("env.json");
    let out20 = &Some(e20.clone());
    let env20 = || -> String { std::fs::read_to_string(&e20).unwrap_or_default().trim().to_string() };
    let j20 = t20("journal.ndjson");
    let c20 = t20("ctx.json");
    let r20 = t20("res.json");
    let s20 = t20("state");
    let k20 = t20("sink.ndjson");
    let _ = std::fs::create_dir_all(&s20);
    let _ = std::fs::write(
        &c20,
        "{\"launch\":true,\"retry_once\":false,\"decision\":{\"Mode\":\"plain\",\"Stage\":\"1-7,CE-5\"}}",
    );

    // 20a 闸的优先级：缺 --i-drive 却同时喂一枚读不动的夹具，出来的必须是拒跑句而不是夹具错。
    let missfx20 = t20("no-such-fixture.json");
    let rc20a = run_drive_ark_daily(false, &j20, &c20, &r20, "unused-panel-cs", &s20, &k20, &missfx20, out20);
    let ea20 = env20();
    chk(
        rc20a == 2 && ea20.contains("--i-drive") && !ea20.contains("夹具读不动"),
        "驱动档的拒跑闸排在了夹具读取之后：缺 --i-drive 时先报夹具错，会把『参数漏写』伪装成『夹具喂错』——换头开关那句拒跑就此不是唯一出口",
        &mut fails,
        &mut total,
    );

    // 20b/20c 夹具的两条错路：读不动与不是合法 JSON 各自的话术，且都收在 rc=1 的 fail 信封。
    let rc20b = run_drive_ark_daily(true, &j20, &c20, &r20, "unused-panel-cs", &s20, &k20, &missfx20, out20);
    let eb20 = env20();
    chk(
        rc20b == 1
            && eb20.contains("\"kind\":\"fail\"")
            && eb20.contains("--fixture-run 的夹具读不动")
            && eb20.contains("驱动协议坏了"),
        "--fixture-run 指到不存在的文件时没当场 fail（或错话没点名旗标）：给了旗标却喂不到夹具，比不喂更容易被当成『跑过了』",
        &mut fails,
        &mut total,
    );
    let badfx20 = t20("fixture-bad.json");
    let _ = std::fs::write(&badfx20, "{\"status\":\"ok\",");
    let rc20c = run_drive_ark_daily(true, &j20, &c20, &r20, "unused-panel-cs", &s20, &k20, &badfx20, out20);
    let ec20 = env20();
    chk(
        rc20c == 1 && ec20.contains("\"kind\":\"fail\"") && ec20.contains("不是合法 JSON"),
        "--fixture-run 的夹具不是合法 JSON 时没当场 fail：半截夹具会被读成『状态全空的一轮』，门就此把坏档当绿档收",
        &mut fails,
        &mut total,
    );

    // 20d 夹具正路：走**真出口**（run_drive_ark_daily + --out）推满五轮，钉第五轮的 run 账行逐字形状
    //     （夹具给了 duration ⇒ 键要出现，与 ask 时代壳写的 res data 同形）＋夹具 logs 按序进侧信道，
    //     再用第六轮（同账重放）钉这两样都不许翻倍。
    let fx20 = t20("fixture-run.json");
    let _ = std::fs::write(
        &fx20,
        "{\"status\":\"ok\",\"detail\":\"一切正常\",\"duration\":62.5,\"logs\":[{\"level\":\"INFO\",\"text\":\"夹具第一行\"},{\"level\":\"WARN\",\"text\":\"夹具第二行\"}]}",
    );
    let sink_rows20 = |needle: &str| -> Vec<usize> {
        std::fs::read_to_string(&k20)
            .unwrap_or_default()
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains(needle))
            .map(|(i, _)| i)
            .collect()
    };
    let run_rows20 = || -> usize {
        std::fs::read_to_string(&j20)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains("\"action\":\"run\""))
            .count()
    };
    let mut rc20d = 0i32;
    let mut e20d = String::new();
    for (i, res_row) in [
        "",
        "{\"seq\":2,\"action\":\"desk\",\"ok\":true,\"data\":\"Default\"}",
        "{\"seq\":3,\"action\":\"takeover\",\"ok\":true}",
        "{\"seq\":4,\"action\":\"cfg\",\"ok\":true,\"data\":{\"changes\":[\"chg-main\"],\"chains\":[\"Fight\",\"Shop\"]}}",
        "{\"seq\":6,\"action\":\"launch\",\"ok\":true,\"data\":true}",
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            let _ = std::fs::write(&r20, res_row);
        }
        rc20d = run_drive_ark_daily(true, &j20, &c20, &r20, "unused-panel-cs", &s20, &k20, &fx20, out20);
        e20d = env20();
    }
    let run_row20 = std::fs::read_to_string(&j20)
        .unwrap_or_default()
        .lines()
        .find(|l| l.contains("\"action\":\"run\""))
        .unwrap_or_default()
        .to_string();
    let g1_20 = sink_rows20("夹具第一行");
    let g2_20 = sink_rows20("夹具第二行");
    chk(
        rc20d == 1
            && e20d.contains("\"kind\":\"fail\"")
            && e20d.contains("decide-daily-retry 起不来")
            && run_rows20() == 1
            && run_row20 == "{\"seq\":7,\"action\":\"run\",\"data\":{\"status\":\"ok\",\"detail\":\"一切正常\",\"duration\":62.5}}"
            && g1_20.len() == 1
            && g2_20.len() == 1
            && g1_20[0] < g2_20[0],
        "真出口这一路的 run 真做不对：夹具喂进去的终态没原样落到账行（键序/键数/duration 一格都不许动 —— 账行与 ask 时代壳写的 res data 不同形 ⇒ old/new 两代 journal 不可比），或夹具的 logs 没按序落进同一本侧信道（通道不是这一条）",
        &mut fails,
        &mut total,
    );
    let rc20e = run_drive_ark_daily(true, &j20, &c20, &r20, "unused-panel-cs", &s20, &k20, &fx20, out20);
    chk(
        rc20e == 1
            && env20().contains("decide-daily-retry 起不来")
            && run_rows20() == 1
            && sink_rows20("夹具第一行").len() == 1
            && sink_rows20("夹具第二行").len() == 1,
        "同账重放把 run 又做了一遍（账上多一行或侧信道多两行）：真做动作的重放只消费账行 —— 一遍真做两遍 MAA，生产就是双起会话，日志也会整段重播",
        &mut fails,
        &mut total,
    );

    // 20e ctx 的 maaRun 十三枚逐格点名（键名与 config\\settings.json 逐字同名）＋缺键一律按"没填"。
    let full20 = jv_parse(
        "{\"name\":\"ctx-maarun\",\"paths\":{\"maaDir\":\"D:/MAA\",\"maaExe\":\"D:/MAA/MAA.exe\",\"maaConfig\":\"D:/MAA/config.json\",\"maaGuiLog\":\"D:/MAA/gui.log\",\"maaAsstLog\":\"D:/MAA/debug/asst.log\",\"arknightsExe\":\"D:/Ark/Arknights.exe\"},\"runtime\":{\"arkStartTimeoutMinutes\":3,\"arkTimeoutMinutes\":40,\"stallTimeoutMinutes\":10,\"idleConfirmSeconds\":60,\"abortOnMaaStallWarning\":true,\"visionMaxRescue\":5,\"benignMaaErrors\":[\"E1\",\"E2\"]}}",
    )
    .unwrap_or(JVal::Null);
    let p20 = maarun_parse(&full20);
    chk(
        p20.as_ref().map_or(false, |m| {
            m.maa_dir == "D:/MAA"
                && m.maa_exe == "D:/MAA/MAA.exe"
                && m.maa_config == "D:/MAA/config.json"
                && m.gui_log == "D:/MAA/gui.log"
                && m.asst_log == "D:/MAA/debug/asst.log"
                && m.ark_exe == "D:/Ark/Arknights.exe"
                && m.start_min == 3
                && m.timeout_min == 40
                && m.stall_min == 10
                && m.idle_sec == 60
                && m.abort_stall
                && m.vision_max == 5
                && m.benign == vec!["E1".to_string(), "E2".to_string()]
        }),
        "ctx 的 maaRun 十三枚里有一枚接错（paths 六格＋runtime 六格＋benignMaaErrors，键名必须与 config\\settings.json 逐字同名）：真做会拿错路径或错超时去跑 MAA，而门看不出来",
        &mut fails,
        &mut total,
    );
    let p20empty = maarun_parse(&JVal::Null);
    chk(
        p20empty.map_or(false, |m| {
            m.maa_dir.is_empty()
                && m.ark_exe.is_empty()
                && m.start_min == 0
                && m.timeout_min == 0
                && m.stall_min == 0
                && m.idle_sec == 0
                && !m.abort_stall
                && m.vision_max == 0
                && m.benign.is_empty()
                && m.game == "ark"
                && m.target == "ark"
        }),
        "ctx 没给 maaRun（或只给半套键）时十三枚没有一律按『没填』取值：game/target 之外的一律要落成 0/空 —— 缺键被猜成默认值，生产就有一处没人签过字的超时",
        &mut fails,
        &mut total,
    );

    // 20f 生产 run 闭包的第一条边界：ctx 载荷缺失 ⇒ 当场拒做，且不碰侧信道、不猜路径。
    let f_sink20 = t20("f-sink.ndjson");
    let f20 = drive_real_run("unused-panel-cs", &f_sink20, &MaaScen::default(), &["Fight".to_string()]);
    chk(
        f20.as_ref().err().map_or(false, |m| m.contains("maaRun 的 paths") && m.contains("不替生产猜默认路径"))
            && std::fs::metadata(&f_sink20).is_err(),
        "生产 run 闭包在 ctx 载荷缺失时没当场拒做（或拒做的路上顺手造了文件）：真做会拿空路径去 Start-Process，错话还会从链上消失",
        &mut fails,
        &mut total,
    );

    // 20g RealHost 的 log 落点＝链体同一本侧信道（B-2a 那笔"run 内部行往哪去"的留言在此结清）。
    let g_sink20 = t20("g-sink.ndjson");
    let mut rh20 = RealHost::for_game("unused-panel-cs", "ark", &g_sink20);
    MaaHost::log(&mut rh20, "INFO", "生产接线自检的一行");
    chk(
        std::fs::read_to_string(&g_sink20).unwrap_or_default()
            == "{\"level\":\"INFO\",\"text\":\"生产接线自检的一行\"}\n",
        "RealHost::log 没把 MAA 运行面内部行落进链体那一本侧信道（通道另开了一条，或行不是 level/text 两键）：壳侧流式排空会漏掉整段运行面日志",
        &mut fails,
        &mut total,
    );

    // 20h 真做抛错仍被链体吞成 error 态：账上写 err 不写 data，终态话术与 ask 时代壳写 ok:false 同语义。
    let j20h = t20("h-journal.ndjson");
    let mut resp_h20 = |act: &str, prm: &JVal| drive_selftest_respond(act, prm);
    let mut run_h20 = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
        Err("运行面桩故意抛".to_string())
    };
    let (h_asked, _h_logs, h_kind, h_result, h_err) =
        drive_selftest_session(&j20h, &c20, &t20("h-res.json"), &s20, &t20("h-sink.ndjson"), &mut resp_h20, &mut run_h20);
    let h_row20 = std::fs::read_to_string(&j20h)
        .unwrap_or_default()
        .lines()
        .find(|l| l.contains("\"action\":\"run\""))
        .unwrap_or_default()
        .to_string();
    chk(
        h_err.is_none()
            && h_kind == "done"
            && !h_asked.iter().any(|a| a == "run")
            && jv_str(&jv_get(&h_result, "status")) == "error"
            && jv_str(&jv_get(&h_result, "detail")) == "方舟 异常：运行面桩故意抛"
            && h_row20 == "{\"seq\":7,\"action\":\"run\",\"err\":\"运行面桩故意抛\"}",
        "run 真做抛错这一路不对：账上该写 err 而不是 data、链体该把它吞成 status=error＋『方舟 异常：』那句、ask 名册里该再没有 run —— 任一半走样，生产就把一次失败的 MAA 当成成功终态收（重试分流就此失灵）",
        &mut fails,
        &mut total,
    );

    let _ = std::fs::remove_dir_all(&d20);
    chk(
        !d20.exists(),
        "B-2c③ 自检的临时目录没收干净：自检会在 %TEMP% 里留垃圾",
        &mut fails,
        &mut total,
    );

    // 21. B-2d②「m9a run 循环进核」：这一组钉 m9a 档自己的判据 —— ctx 名册 15 枚逐枚点名、
    //     runtime 那枚 m9aCliArgs 的**三态**、三枚起步拒跑闸、事件靶子收窄到 m9alog、跨 00:00 换文件
    //     与 keyed offset、`:347` 那句 throw 落成 E| 且整批继续、以及"日志不在场 / 本轮无新字节"两个
    //     省钱点，末了钉住 ark 那一路**零条 D|**（27 档 1218 步基线一字不动的自证）。
    //     adb 两枚 verb（connect / 1999 pid）的 panel-cs 真身在 B-2d③ 才落 ⇒ 这一组一律由 live 夹具喂，
    //     判定 exe 全指不存在的路径：夹具没喂到的 verb 会 Err 出"起不来"，那正是"核里没现取现场"的证据。
    let d21 = std::env::temp_dir().join(format!("gd-b2d2-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d21);
    let _ = std::fs::create_dir_all(&d21);
    let d21s = d21.to_str().unwrap_or("").to_string();
    let t21 = |n: &str| format!("{}\\{}", d21s, n);
    let out21 = t21("env.json");
    let o21 = &Some(out21.clone());
    let env21 = || -> String { std::fs::read_to_string(&out21).unwrap_or_default().trim().to_string() };
    // 假 adb：Connect-GdAdb 那句 Test-Path 只问在不在场，内容无所谓。
    let adb21 = t21("adb.exe");
    let _ = std::fs::write(&adb21, "fake");
    let day1 = "20261011".to_string();
    let day2 = "20261012".to_string();
    let jva = |v: Vec<&str>| JVal::A(v.iter().map(|s| jv_s(s)).collect());
    let sc21 = |name: &str, log_dir: &str| -> M9aScen {
        M9aScen {
            name: name.to_string(),
            m9a_dir: d21s.clone(),
            m9a_exe: "M9A.exe".to_string(),
            m9a_log_dir: log_dir.to_string(),
            adb: adb21.clone(),
            serial: "127.0.0.1:7555".to_string(),
            instance_id: "1".to_string(),
            instance_name: "MuMu".to_string(),
            package: "com.paz.py".to_string(),
            game: "r1999".to_string(),
            target: "mumu".to_string(),
            state_dir: d21s.clone(),
            procs: vec![("M9A".to_string(), true)],
            days: vec![(0.0, day1.clone())],
            ..Default::default()
        }
    };

    // 21a ctx 名册 15 枚逐枚点名（paths 3 ＋ conn 2 ＋ instance 2 ＋ package 1 ＋ runtime 7），
    //     键名与 config\settings.json 逐字同名；game/target/stateDir 那三格是兜底链的料，不算在册内。
    let full21 = jv_parse(
        "{\"name\":\"ctx15\",\"paths\":{\"m9aDir\":\"D:/M9A\",\"m9aExe\":\"M9A.exe\",\"m9aLogDir\":\"logs\"},\"conn\":{\"adb\":\"D:/adb.exe\",\"serial\":\"127.0.0.1:7555\"},\"instance\":{\"id\":\"7\",\"name\":\"MuMu-7\"},\"package\":\"com.paz.wm\",\"runtime\":{\"m9aInstanceArg\":\"name\",\"m9aForceStart\":true,\"m9aCliArgs\":false,\"visionMaxRescue\":3,\"r1999StartTimeoutMinutes\":4,\"r1999TimeoutMinutes\":45,\"stallTimeoutMinutes\":12},\"rescueAvailable\":true,\"stateDir\":\"D:/st\",\"seed\":\"起点\",\"days\":[{\"at\":0,\"stamp\":\"20261011\"}],\"procs\":[{\"name\":\"M9A\",\"up\":true}],\"events\":[{\"at\":5,\"kind\":\"up\",\"proc\":\"Other\"}],\"live\":[{\"verb\":\"adb-connect\",\"data\":{\"Ok\":true}}]}",
    )
    .unwrap_or(JVal::Null);
    let c21 = m9arun_parse(&full21);
    chk(
        c21.as_ref().map_or(false, |s| {
            s.m9a_dir == "D:/M9A"
                && s.m9a_exe == "M9A.exe"
                && s.m9a_log_dir == "logs"
                && s.adb == "D:/adb.exe"
                && s.serial == "127.0.0.1:7555"
                && s.instance_id == "7"
                && s.instance_name == "MuMu-7"
                && s.package == "com.paz.wm"
                && s.mode == "name"
                && s.force_start
                && s.cli == Some(false)
                && s.vision_max == 3
                && s.start_min == 4
                && s.timeout_min == 45
                && s.stall_min == 12
                && s.rescue_available
                && !s.grab_available
                && s.state_dir == "D:/st"
                && s.seed.as_deref() == Some("起点")
                && s.days.len() == 1
                && s.procs.len() == 1
                && s.events.len() == 1
                && s.live.len() == 1
        }),
        "m9a 的 ctx 名册有一枚接错（15 枚逐枚点名，另加兜底面三格与夹具面四样）：键名漂一枚，生产就拿错实例或错超时去开 M9A，而门看不出来",
        &mut fails,
        &mut total,
    );
    // 缺键一律按"没填"：mode 落成**空串**（真身 `:335` 那句显式转换），三态 cli 落 None，超时落 0。
    let min21 = jv_parse(
        "{\"name\":\"min\",\"paths\":{\"m9aDir\":\"D:/M9A\"},\"conn\":{\"adb\":\"D:/a\"},\"days\":[{\"at\":0,\"stamp\":\"20261011\"}]}",
    )
    .unwrap_or(JVal::Null);
    let m21 = m9arun_parse(&min21);
    chk(
        m21.as_ref().map_or(false, |s| {
            s.mode.is_empty()
                && !s.force_start
                && s.cli.is_none()
                && s.vision_max == 0
                && s.start_min == 0
                && s.timeout_min == 0
                && s.stall_min == 0
                && s.game == "r1999"
                && s.target == "mumu"
                && s.state_dir.is_empty()
                && s.seed.is_none()
                && s.days.len() == 1
                && s.procs.is_empty()
        }),
        "ctx 只给半套键时没一律按『没填』取值（mode 必须是空串而不是 'id'、game/target 的没填才等于真身写死那两枚）：缺键被猜成默认值，生产就有一处没人签过字的超时与参数",
        &mut fails,
        &mut total,
    );
    // 三态：键不存在 / 填 true / 填 false 是三档，塌成两档就把真身两条完全不同的路并成一条。
    let cli21 = |extra: &str| -> Option<bool> {
        let j = format!(
            "{{\"name\":\"t\",\"paths\":{{\"m9aDir\":\"D\"}},\"conn\":{{\"adb\":\"A\"}},\"days\":[{{\"at\":0,\"stamp\":\"20261011\"}}]{}}}",
            extra
        );
        m9arun_parse(&jv_parse(&j).unwrap_or(JVal::Null)).ok().and_then(|s| s.cli)
    };
    chk(
        cli21("").is_none()
            && cli21(",\"runtime\":{\"m9aCliArgs\":true}") == Some(true)
            && cli21(",\"runtime\":{\"m9aCliArgs\":false}") == Some(false),
        "runtime.m9aCliArgs 的三态塌成一态：真身 Get-GdM9ACliEnabled 那句 `if ($null -eq …) {{ return $true }}` 让『没写』走带 -i/-a/-q/-f 的新路子，而『写了 false』走裸启动＋在跑就 throw 的老路子",
        &mut fails,
        &mut total,
    );

    // 21b 三枚起步拒跑闸（都不是判据，是"这一档压根不能开"）。
    let g_dir = m9arun_parse(
        &jv_parse("{\"name\":\"g\",\"conn\":{\"adb\":\"A\"},\"days\":[{\"at\":0,\"stamp\":\"20261011\"}]}").unwrap_or(JVal::Null),
    )
    .err()
    .unwrap_or_default();
    let g_adb = m9arun_parse(
        &jv_parse("{\"name\":\"g\",\"paths\":{\"m9aDir\":\"D\"},\"days\":[{\"at\":0,\"stamp\":\"20261011\"}]}").unwrap_or(JVal::Null),
    )
    .err()
    .unwrap_or_default();
    let g_days = m9arun_parse(&jv_parse("{\"name\":\"g\",\"paths\":{\"m9aDir\":\"D\"},\"conn\":{\"adb\":\"A\"}}").unwrap_or(JVal::Null))
        .err()
        .unwrap_or_default();
    chk(
        g_dir.contains("m9aDir") && g_adb.contains("conn.adb") && g_days.contains("days"),
        "三枚起步拒跑闸有一枚没咬住（m9aDir / conn.adb / days）：真身在这三处要么裸 Join-Path 当场抛、要么 throw 在起循环之前、要么恒读 log-.log —— 闸不咬就等于让内核产出一行『看起来跑完了』的假绿",
        &mut fails,
        &mut total,
    );

    // 21c 靶子收窄：m9a 一路只有 m9alog 一枚；认不出的 kind 同样当场拒跑（静默丢事件比红更糟）。
    let bad_target = m9arun_parse(
        &jv_parse("{\"name\":\"g\",\"paths\":{\"m9aDir\":\"D\"},\"conn\":{\"adb\":\"A\"},\"days\":[{\"at\":0,\"stamp\":\"20261011\"}],\"events\":[{\"at\":1,\"kind\":\"append\",\"target\":\"gui\",\"text\":\"x\"}]}").unwrap_or(JVal::Null),
    )
    .err()
    .unwrap_or_default();
    let ok_proc = m9arun_parse(
        &jv_parse("{\"name\":\"g\",\"paths\":{\"m9aDir\":\"D\"},\"conn\":{\"adb\":\"A\"},\"days\":[{\"at\":0,\"stamp\":\"20261011\"}],\"events\":[{\"at\":1,\"kind\":\"up\",\"proc\":\"M9A\"}]}").unwrap_or(JVal::Null),
    );
    let bad_kind = m9arun_parse(
        &jv_parse("{\"name\":\"g\",\"paths\":{\"m9aDir\":\"D\"},\"conn\":{\"adb\":\"A\"},\"days\":[{\"at\":0,\"stamp\":\"20261011\"}],\"events\":[{\"at\":1,\"kind\":\"bogus\",\"target\":\"m9alog\"}]}").unwrap_or(JVal::Null),
    )
    .err()
    .unwrap_or_default();
    chk(
        bad_target.contains("第 1 条事件")
            && bad_target.contains("m9alog")
            && !bad_target.contains("第 2 条")
            && ok_proc.is_ok()
            && bad_kind.contains("bogus"),
        "剧本面收窄没咬住：文件靶子写 gui/asst 该当场拒跑（m9a 只有一本每轮换名的日志），up/down 的进程靶子该照收，认不出的 kind 该点名 —— 少咬一条，拼错一个字母就有一整档跑成『日志一条没进、判定全按空读数走』",
        &mut fails,
        &mut total,
    );

    // 21d dry-run 两支话术：真身 `:339-343` 只一行 STEP 就回，返回表多带 Args 一格。
    let mut sc21d = sc21("dry", "logs0");
    sc21d.dry_run = true;
    sc21d.live = vec![("m9a-launch-args".to_string(), jv_o(vec![("Args", jva(vec!["-i", "1", "-a"]))]))];
    let mut h21d = ScriptHost::new_m9a("gd-no-such-panel", &sc21d);
    let r21d = m9a_run(&mut h21d, &sc21d);
    let mut pd21 = h21d.proj.clone();
    if let Ok(rt) = r21d.as_ref() {
        pd21.push(rt.proj_line());
    }
    let mut sc21d2 = sc21("dry-noarg", "logs0");
    sc21d2.dry_run = true;
    sc21d2.live = vec![("m9a-launch-args".to_string(), jv_o(vec![("Args", JVal::A(Vec::new()))]))];
    let mut h21d2 = ScriptHost::new_m9a("gd-no-such-panel", &sc21d2);
    let r21d2 = m9a_run(&mut h21d2, &sc21d2);
    let mut pd212 = h21d2.proj.clone();
    if let Ok(rt) = r21d2.as_ref() {
        pd212.push(rt.proj_line());
    }
    chk(
        r21d.is_ok()
            && pd21.len() == 3
            && pd21[0] == "V|m9a-launch-args"
            && pd21[1] == format!("W|STEP|[dry-run] 将启动 {}\\M9A.exe -i 1 -a，ADB 127.0.0.1:7555，以 logs\\log-*.log 判定完成", d21s)
            && pd21[2] == "Z|dryrun||-|0||-|-i 1 -a"
            && r21d2.is_ok()
            && pd212.iter().any(|l| l.contains("(不带参数：老路子，靠 M9A 自己的自动开始)"))
            && pd212.last().map_or(false, |l| l == "Z|dryrun||-|0||-|")
            && !pd21.iter().any(|l| l.contains("adb-connect")),
        "dry-run 那一档的形状走样（一行 STEP／Args 进末行／空参数那支的括号话术／ADB 段根本不该问）：真身 m9a.ps1:339-343 就这四件事，多出来的每一行都是门里的假差异",
        &mut fails,
        &mut total,
    );

    // 21e 跨 00:00 的一轮（plan §11.39 ⑥-2：沙盒里必须造得出换文件，arkrun 的 27 档复用不了）。
    //     起点摆 OLD\n（4 字节）⇒ 偏移从 4 起算；at=10 的行落第一本，at=30 的行落在日期刷新**之后**
    //     ⇒ 落第二本，且第二本不在 PreOffsets 里 ⇒ 从 0 起算（keyed offset 的两格都在这一档上）。
    let mut sc21e = sc21("cross-day", "logs1");
    sc21e.days = vec![(0.0, day1.clone()), (25.0, day2.clone())];
    sc21e.seed = Some("OLD\n".to_string());
    sc21e.events = vec![
        MaaEv { at: 10.0, kind: "append", target: "m9alog".to_string(), text: "L1\n".to_string(), bytes: 0 },
        MaaEv { at: 30.0, kind: "append", target: "m9alog".to_string(), text: "L2\n".to_string(), bytes: 0 },
    ];
    sc21e.live = vec![
        ("m9a-launch-args".to_string(), jv_o(vec![("Args", jva(vec!["-i", "1"]))])),
        ("adb-connect".to_string(), jv_o(vec![("Ok", JVal::B(true))])),
        ("parse-m9a-lines".to_string(), jv_o(vec![("Events", JVal::A(Vec::new()))])),
        (
            "parse-m9a-lines".to_string(),
            jv_o(vec![("Events", JVal::A(vec![jv_o(vec![("Kind", jv_s("TaskDone")), ("Text", jv_s("L2"))])]))]),
        ),
        (
            "apply-m9a-events".to_string(),
            jv_o(vec![
                ("Logs", JVal::A(Vec::new())),
                ("State", jv_o(vec![("eventCount", JVal::I(1))])),
                ("Directives", jv_o(vec![("TouchLastLog", JVal::B(true))])),
            ]),
        ),
        ("adb-game-pid".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        ("adb-game-pid".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        ("adb-game-pid".to_string(), jv_o(vec![("Found", JVal::B(true))])),
        ("resolve-m9a-status".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        ("resolve-m9a-status".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        (
            "resolve-m9a-status".to_string(),
            jv_o(vec![
                ("Found", JVal::B(true)),
                ("Status", jv_s("ok")),
                ("Detail", jv_s("原始结论")),
                ("Tasks", jva(vec!["A"])),
            ]),
        ),
        (
            "decide-m9a-run-tick".to_string(),
            jv_o(vec![
                ("Effects", JVal::A(Vec::new())),
                ("Action", jv_s("return")),
                ("Detail", jv_s("完成整句")),
                ("Duration", JVal::F(30.0)),
            ]),
        ),
    ];
    let _ = m9arun_seed(&sc21e);
    let f121 = t21(format!("logs1\\log-{}.log", day1).as_str());
    let f221 = t21(format!("logs1\\log-{}.log", day2).as_str());
    let mut h21e = ScriptHost::new_m9a("gd-no-such-panel", &sc21e);
    let r21e = m9a_run(&mut h21e, &sc21e);
    let mut pe21 = h21e.proj.clone();
    if let Ok(rt) = r21e.as_ref() {
        pe21.push(rt.proj_line());
    }
    let ncnt = |needle: &str| pe21.iter().filter(|l| l.starts_with(needle)).count();
    let dlines: Vec<String> = pe21.iter().filter(|l| l.starts_with("D|")).cloned().collect();
    let tlines: Vec<String> = pe21.iter().filter(|l| l.starts_with("T|")).cloned().collect();
    chk(
        r21e.is_ok()
            && dlines == vec![format!("D|{}", day1), format!("D|{}", day2)]
            && tlines == vec![
                format!("T|log-{}.log|4|7|3", day1),
                format!("T|log-{}.log|7|7|0", day1),
                format!("T|log-{}.log|0|3|3", day2),
            ]
            && std::fs::read_to_string(&f121).unwrap_or_default() == "OLD\nL1\n"
            && std::fs::read_to_string(&f221).unwrap_or_default() == "L2\n"
            && ncnt("V|parse-m9a-lines") == 2
            && ncnt("V|apply-m9a-events") == 1
            && ncnt("V|adb-game-pid") == 3
            && ncnt("V|resolve-m9a-status") == 3
            && ncnt("V|decide-m9a-run-tick") == 1
            && ncnt("P|M9A|") == 5
            && ncnt("S|10") == 3
            && pe21.iter().any(|l| l == "W|WARN|M9A 已在运行：命令行会被转发给正在跑的实例，按 -i/-f 重新开始任务")
            && pe21.last().map_or(false, |l| l == "Z|ok|完成整句|30|0|A|-|"),
        "跨 00:00 这一档不对（日期只在值变化时落 D|／新文件从 0 起算而起点文件从 PreOffsets 起算／本轮无新字节不起 parse／两本文件的字节各归各）：这一格是 m9a 一路独有的采集面，arkrun 的 27 档替不了它",
        &mut fails,
        &mut total,
    );

    // 21f 日志压根不在场（Test-GdPathReady 那一支：不建 offsets 格、不起进程）＋空参数启动的两处原样怪癖。
    //     先手工摆一本再让 m9arun_seed 删掉 —— 删不掉就是"起点摆不回"，两侧起点会不一样。
    let mut sc21f = sc21("no-log", "logs2");
    sc21f.live = vec![
        ("m9a-launch-args".to_string(), jv_o(vec![("Args", JVal::A(Vec::new()))])),
        ("adb-connect".to_string(), jv_o(vec![("Ok", JVal::B(true))])),
        ("adb-game-pid".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        (
            "resolve-m9a-status".to_string(),
            jv_o(vec![
                ("Found", JVal::B(true)),
                ("Status", jv_s("blocked")),
                ("Detail", jv_s("旧结论")),
                ("Tasks", jva(vec!["T"])),
                ("FailedTasks", jva(vec!["x", "y"])),
            ]),
        ),
        (
            "decide-m9a-run-tick".to_string(),
            jv_o(vec![("Effects", JVal::A(Vec::new())), ("Action", jv_s("return")), ("Detail", jv_s("新结论"))]),
        ),
    ];
    let f21f = t21(format!("logs2\\log-{}.log", day1).as_str());
    let _ = std::fs::write(&f21f, "STALE\n");
    let _ = m9arun_seed(&sc21f);
    let mut h21f = ScriptHost::new_m9a("gd-no-such-panel", &sc21f);
    let r21f = m9a_run(&mut h21f, &sc21f);
    let mut pf21 = h21f.proj.clone();
    if let Ok(rt) = r21f.as_ref() {
        pf21.push(rt.proj_line());
    }
    let launch21 = pf21.iter().find(|l| l.starts_with("W|STEP|启动 M9A：")).cloned().unwrap_or_default();
    let rline21 = pf21.iter().find(|l| l.starts_with("R|")).cloned().unwrap_or_default();
    chk(
        r21f.is_ok()
            && std::fs::metadata(&f21f).is_err()
            && !pf21.iter().any(|l| l.starts_with("T|"))
            && !pf21.iter().any(|l| l.starts_with("V|parse-m9a-lines"))
            && !pf21.iter().any(|l| l.starts_with("V|apply-m9a-events"))
            && launch21.ends_with(' ')
            && launch21.starts_with(&format!("W|STEP|启动 M9A：{}\\M9A.exe", d21s))
            && rline21.matches('|').count() == 2
            && rline21.starts_with("R|")
            && pf21.iter().filter(|l| l.starts_with("P|M9A|")).count() == 3
            && pf21.last().map_or(false, |l| l == "Z|blocked|新结论|-|0|T|x\u{1e}y|"),
        "『日志不在场／空参数启动』这一档不对（seed 该被删掉、零条 T|、句尾空格是真身模板原样、裸启动的 R| 只有两截、FailedTasks 按**键存在**取值）：句尾空格被 trim 或 R| 多一截，门的逐字符比就从这里开始整档红",
        &mut fails,
        &mut total,
    );

    // 21g `:347` 那句 throw 走真出口：单档 Err 落成该档末行 E| 后**继续跑下一档**（不杀整批）。
    let script21g = t21("script-347.json");
    let _ = std::fs::write(
        &script21g,
        "[{\"name\":\"e347\",\"paths\":{\"m9aDir\":\"C:\\\\M9A\",\"m9aExe\":\"M9A.exe\",\"m9aLogDir\":\"logs\"},\"conn\":{\"adb\":\"C:\\\\adb.exe\",\"serial\":\"s\"},\"runtime\":{\"m9aCliArgs\":false},\"procs\":[{\"name\":\"M9A\",\"up\":true}],\"days\":[{\"at\":0,\"stamp\":\"20261011\"}],\"live\":[{\"verb\":\"m9a-launch-args\",\"data\":{\"Args\":[\"-i\",\"1\"]}}]},{\"name\":\"dry2\",\"dryRun\":true,\"paths\":{\"m9aDir\":\"C:\\\\M9A\",\"m9aExe\":\"M9A.exe\"},\"conn\":{\"adb\":\"C:\\\\adb.exe\"},\"days\":[{\"at\":0,\"stamp\":\"20261011\"}],\"live\":[{\"verb\":\"m9a-launch-args\",\"data\":{\"Args\":[\"-i\",\"2\"]}}]}]",
    );
    let rc21g = run_m9a_run(&script21g, "gd-no-such-panel", o21);
    let root21g = jv_parse(&env21()).unwrap_or(JVal::Null);
    let csraw21 = jv_get(&root21g, "cases");
    let cs21g = jv_list(&csraw21);
    let case21 = |n: &str| -> String {
        for c in cs21g.iter() {
            if jv_str(&jv_get(*c, "name")) == n {
                return jv_str(&jv_get(*c, "data"));
            }
        }
        String::new()
    };
    let d347 = case21("e347");
    let l347: Vec<&str> = d347.split('\u{1f}').collect();
    let d2r = case21("dry2");
    chk(
        rc21g == 0
            && jv_str(&jv_get(&root21g, "role")) == "B2-m9a-run"
            && jv_int(&jv_get(&root21g, "count"), 0) == 2
            && l347.last().map_or(false, |l| {
                *l == "E|M9A 已在运行，无法保证按新实例开始(请先关闭，或把 runtime.m9aCliArgs 打开)"
            })
            && l347.iter().filter(|l| l.starts_with("P|")).count() == 1
            && !d347.contains("V|adb-connect")
            && d2r.contains("Z|dryrun"),
        "真出口的『单档 Err 不杀整批』走样：`:347` 该落成该档末行 E|（整字面照真身）、cli=false 时两句在跑判据只问一次进程（&& 短路，投影就一条 P|）、后面的档该照跑 —— 做成整批中止，门里那一档连投影都看不到",
        &mut fails,
        &mut total,
    );

    // 21h 出口的错路：两枚缺参拒跑（rc 2）、剧本读不动与非法 JSON（rc 1）、一档读不动整批中止（rc 1）。
    let rc21h1 = run_m9a_run("", "gd-no-such-panel", o21);
    let j21h1 = env21();
    let rc21h2 = run_m9a_run(&t21("no-such-script.json"), "", o21);
    let j21h2 = env21();
    let rc21h3 = run_m9a_run(&t21("no-such-script.json"), "gd-no-such-panel", o21);
    let j21h3 = env21();
    chk(
        rc21h1 == 2
            && j21h1.contains("--script")
            && rc21h2 == 2
            && j21h2.contains("--panel-cs")
            && rc21h3 == 1
            && j21h3.contains("剧本读不动"),
        "m9a 回放档的三枚缺参/缺料闸没咬住：给了 --m9a-run 却喂不到剧本或判定 exe 时，出口必须是拒跑句而不是空的 cases 数组",
        &mut fails,
        &mut total,
    );
    let bad21 = t21("script-bad.json");
    let _ = std::fs::write(&bad21, "[{\"name\":\"b\",");
    let rc21i1 = run_m9a_run(&bad21, "gd-no-such-panel", o21);
    let j21i1 = env21();
    let nodays21 = t21("script-nodays.json");
    let _ = std::fs::write(&nodays21, "[{\"name\":\"nd\",\"paths\":{\"m9aDir\":\"D\"},\"conn\":{\"adb\":\"A\"}}]");
    let rc21i2 = run_m9a_run(&nodays21, "gd-no-such-panel", o21);
    let j21i2 = env21();
    let badk21 = t21("script-badkind.json");
    let _ = std::fs::write(
        &badk21,
        "[{\"name\":\"bk\",\"paths\":{\"m9aDir\":\"D\"},\"conn\":{\"adb\":\"A\"},\"days\":[{\"at\":0,\"stamp\":\"20261011\"}],\"events\":[{\"at\":1,\"kind\":\"bogus\",\"target\":\"m9alog\"}]}]",
    );
    let rc21i3 = run_m9a_run(&badk21, "gd-no-such-panel", o21);
    let j21i3 = env21();
    chk(
        rc21i1 == 1
            && j21i1.contains("不是合法 JSON")
            && rc21i2 == 1
            && j21i2.contains("剧本第 1 档读不动")
            && j21i2.contains("days")
            && rc21i3 == 1
            && j21i3.contains("bogus"),
        "剧本自身的三种坏法（半截 JSON／某档缺 days 表／认不出的事件 kind）没能整批中止并点名第几档：门会拿一份缺档的出口当绿档收",
        &mut fails,
        &mut total,
    );

    // 21i adb 路径给了但文件不在场：运行期 Err（与 21b 那枚"压根没给"的拒跑闸分档），且这句在 ADB 连接之后。
    let mut sc21j = sc21("no-adb", "logs3");
    sc21j.adb = t21("no-such-adb.exe");
    sc21j.live = vec![(
        "m9a-launch-args".to_string(),
        jv_o(vec![("Args", jva(vec!["-i", "1"]))]),
    )];
    let mut h21j = ScriptHost::new_m9a("gd-no-such-panel", &sc21j);
    let r21j = m9a_run(&mut h21j, &sc21j);
    chk(
        r21j.as_ref().err().map_or(false, |m| m == &format!("adb 不存在：{}", sc21j.adb))
            && h21j.proj.iter().any(|l| l == "W|STEP|ADB 连接 127.0.0.1:7555")
            && !h21j.proj.iter().any(|l| l.starts_with("R|")),
        "adb 给了路径但文件不在场这一档不对：真身 `Connect-GdAdb` 那句 Test-Path 的 throw 在起进程**之前**（throw 的整字面照抄），于是这一路既没有 R| 也没有日志读行 —— 把它做成静默继续就是假绿",
        &mut fails,
        &mut total,
    );

    // 21j effect 的两条落点：认不出的 kind 该 WARN 作废这一拍（真身 m9a.ps1:478 整句），
    //     ask-screen 该被接手；兜底不可用时**连一枚现场 verb 都不问**（三道门槛那两道在核内）。
    let mut sc21k = sc21("bad-eff", "logs4");
    sc21k.seed = Some("OLD\n".to_string());
    sc21k.live = vec![
        ("m9a-launch-args".to_string(), jv_o(vec![("Args", jva(vec!["-i", "1"]))])),
        ("adb-connect".to_string(), jv_o(vec![("Ok", JVal::B(true))])),
        ("adb-game-pid".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        ("adb-game-pid".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        (
            "resolve-m9a-status".to_string(),
            jv_o(vec![("Found", JVal::B(true)), ("Status", jv_s("running"))]),
        ),
        (
            "resolve-m9a-status".to_string(),
            jv_o(vec![("Found", JVal::B(true)), ("Status", jv_s("running"))]),
        ),
        (
            "decide-m9a-run-tick".to_string(),
            jv_o(vec![(
                "Effects",
                JVal::A(vec![jv_o(vec![("Kind", jv_s("bogus")), ("Tag", jv_s("t1"))])]),
            )]),
        ),
        (
            "decide-m9a-run-tick".to_string(),
            jv_o(vec![("Effects", JVal::A(Vec::new())), ("Action", jv_s("return")), ("Detail", jv_s("收"))]),
        ),
    ];
    let _ = m9arun_seed(&sc21k);
    let mut h21k = ScriptHost::new_m9a("gd-no-such-panel", &sc21k);
    let r21k = m9a_run(&mut h21k, &sc21k);
    let mut pk21 = h21k.proj.clone();
    if let Ok(rt) = r21k.as_ref() {
        pk21.push(rt.proj_line());
    }
    let mut sc21l = sc21("ask-eff", "logs5");
    sc21l.seed = Some("OLD\n".to_string());
    sc21l.rescue_available = false;
    sc21l.live = vec![
        ("m9a-launch-args".to_string(), jv_o(vec![("Args", jva(vec!["-i", "1"]))])),
        ("adb-connect".to_string(), jv_o(vec![("Ok", JVal::B(true))])),
        ("adb-game-pid".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        ("adb-game-pid".to_string(), jv_o(vec![("Found", JVal::B(false))])),
        (
            "resolve-m9a-status".to_string(),
            jv_o(vec![("Found", JVal::B(true)), ("Status", jv_s("running"))]),
        ),
        (
            "resolve-m9a-status".to_string(),
            jv_o(vec![("Found", JVal::B(true)), ("Status", jv_s("running"))]),
        ),
        (
            "decide-m9a-run-tick".to_string(),
            jv_o(vec![(
                "Effects",
                JVal::A(vec![jv_o(vec![("Kind", jv_s("ask-screen")), ("Tag", jv_s("t")), ("Context", jv_s("c"))])]),
            )]),
        ),
        (
            "decide-m9a-run-tick".to_string(),
            jv_o(vec![("Effects", JVal::A(Vec::new())), ("Action", jv_s("return")), ("Detail", jv_s("收2"))]),
        ),
    ];
    let _ = m9arun_seed(&sc21l);
    let mut h21l = ScriptHost::new_m9a("gd-no-such-panel", &sc21l);
    let r21l = m9a_run(&mut h21l, &sc21l);
    let pl21 = h21l.proj.clone();
    chk(
        r21k.is_ok()
            && pk21.iter().any(|l| {
                l == "W|WARN|M9A tick：effect(kind=bogus，tag=t1)没有落到执行块，本 tick 作废(下一轮按新读数重判)"
            })
            && pk21.iter().filter(|l| l.starts_with("V|decide-m9a-run-tick")).count() == 2
            && pk21.last().map_or(false, |l| l == "Z|running|收|-|0||-|")
            && r21l.is_ok()
            && !pl21.iter().any(|l| l.contains("没有落到执行块"))
            && pl21.iter().filter(|l| l.starts_with("V|decide-m9a-run-tick")).count() == 2,
        "迷你循环的两条落点不对（未知 kind 该照抄 m9a.ps1:478 那句并作废这一拍、ask-screen 该被接手；兜底不可用时不问任何现场 verb —— 一旦问了，这档会因判定 exe 不在场而当场 Err）：m9a 的 ack kind 叫 ask-screen 而 ark 叫 rescue，这一格是两路的差别，不许顺手统一",
        &mut fails,
        &mut total,
    );

    // 21k 扩面两枚的真身形状 ＋ ark 那一路零条 D|（保 27 档 1218 步基线的自证）＋ 摘名单十二枚。
    let mut rh21 = RealHost::for_game("unused-panel-cs", "r1999", &t21("m-sink.ndjson"));
    let ds21 = MaaHost::day_stamp(&mut rh21);
    let mut ah21 = ScriptHost::new("gd-no-such-panel", &MaaScen::default());
    let ad21 = MaaHost::day_stamp(&mut ah21);
    chk(
        ds21.len() == 8 && ds21.chars().all(|c| c.is_ascii_digit())
            && !m9a_daily_path("X", &ds21).contains("log-.log")
            && ad21.is_empty()
            && ah21.proj.is_empty()
            && ah21.file_of("m9alog").is_none()
            && M9A_STATE_KEYS.len() == 12
            && !M9A_STATE_KEYS.contains(&"lastLog"),
        "B-2d② 两处扩面走样（本地日期读数该是 yyyyMMdd 八位、ark 一路不填 days 就该零条 D| 且不落 m9alog 路径、回灌名单该十二枚且 lastLog 不在内）：D| 一进 ark 那一路，27 档 1218 步的基线就得重拍",
        &mut fails,
        &mut total,
    );

    let _ = std::fs::remove_dir_all(&d21);
    chk(
        !d21.exists(),
        "B-2d② 自检的临时目录没收干净：自检会在 %TEMP% 里留垃圾",
        &mut fails,
        &mut total,
    );

    // 22. B-2d③「mumu 兜底现场件」：这一组钉 mumu 那一路进核自己的判据 ——
    //     ① 闸⑥「对象在场」由 C# 现查（`MumuAlive`），核**只把端点两键转喂 ctx**、不多问一枚 verb
    //        （10-12 实测改口：原先定的是"核先问 adb-connect、把 Ok 当 alive 覆盖格喂进去"，那条让核排在 ctx
    //        之前 ⇒ m9arun 门 H2 红 8 处、且"没配 key"那一档白起两趟真身不起的 adb native；改口明细在
    //        vision_rescue 那段的注释里，plan §11.42 ①-12 就地作废）；
    //     ② kind 分派到 `mumu-screen-shot`／`mumu-screen-click`，那三枚 body 各多带 `adb`/`serial` 两键；
    //     ③ **ark 那一路一个键都不多喂**（22b 拿 B-19 那套 ark 夹具复跑一遍，投影与基线逐字全等）；
    //     ④ 十五格回执在核侧的消费点与 ark **一字不区分**（`ImgW/ImgH` 与四格客户区照样进点位换算的 body）。
    //     六闸的 C# 那半边由 `--livetest` 钉（③-a），这一组只钉"编排等值"：verb 的话术一律原样转达。
    //     判定 exe 全指不存在的路径 ⇒ 夹具没喂到的 verb 当场 Err，那正是"核里没现取现场"的证据。
    //     ⚠ 改口之后闸序与真身一致 ⇒ ③-c 的门里"开关关着/没配 key"这类档**重新可以做剧本**：
    //     两侧在这一问之前都不碰 adb，不再有"谁多起一枚 verb"那两条事实源要分开供。
    let d22 = std::env::temp_dir().join(format!("gd-b2d3-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d22);
    let _ = std::fs::create_dir_all(&d22);
    let d22s = d22.to_str().unwrap_or("").to_string();
    let adb22 = format!("{}\\adb.exe", d22s);
    let _ = std::fs::write(&adb22, "fake");
    let sc22 = |name: &str, live: Vec<(String, JVal)>| -> M9aScen {
        M9aScen {
            name: name.to_string(),
            m9a_dir: d22s.clone(),
            m9a_exe: "M9A.exe".to_string(),
            m9a_log_dir: format!("{}\\logs", d22s),
            adb: adb22.clone(),
            serial: "127.0.0.1:7554".to_string(),
            instance_id: "1".to_string(),
            instance_name: "MuMu".to_string(),
            package: "com.paz.py".to_string(),
            game: "r1999".to_string(),
            target: "mumu".to_string(),
            state_dir: d22s.clone(),
            rescue_available: true,
            days: vec![(0.0, "20261012".to_string())],
            live,
            ..Default::default()
        }
    };
    // 夹具一律抄真身字面：Name 那句来自 `vision-mumu.ps1:179`，Skip 那两句来自 `Live.cs` 的闸②与闸⑥，
    // `Mode` 整串来自 `vision-mumu.ps1:132`（§11.42 ⑤-3：这句逐字进 tries 拼串，不许改字）。
    let m22_ctx = jv_o(vec![
        ("CanAsk", JVal::B(true)),
        ("Kind", jv_s("mumu")),
        ("Name", jv_s("MuMu 模拟器画面")),
        (
            "Scene",
            jv_s("你是《重返未来：1999》自动化流程里的\"看屏幕的人\"。给你一张模拟器屏幕截图，请判断画面上有没有挡住主流程的东西。"),
        ),
        ("UserText", jv_s("这是模拟器里 1999 的当前画面。按规则只回一个 JSON 对象。")),
    ]);
    // 十五格同 ark（§11.42 ⑥-2）：真身 `Save-GdMuMuShot` 只回十二格，那三格由通用层现算，
    // C# 的 mumu 支已补齐 ⇒ 核侧两路消费同一形状，这里喂满十五格并钉消费点。
    let m22_shot = jv_o(vec![
        ("Ok", JVal::B(true)),
        ("CanContinue", JVal::B(true)),
        ("LogLine", jv_s("已拍 MuMu 模拟器画面：1080x1920，平均亮度 62")),
        ("Path", jv_s("z:\\before.png")),
        ("ImgW", JVal::I(1080)),
        ("ImgH", JVal::I(1920)),
        ("ClientLeft", JVal::I(0)),
        ("ClientTop", JVal::I(0)),
        ("ClientW", JVal::I(1080)),
        ("ClientH", JVal::I(1920)),
        ("AvgLum", JVal::I(62)),
        ("Sig", jv_s("mumu-sig-before")),
        ("Hash", jv_s("0123456789abcdef")),
        ("Why", jv_s("")),
        ("Skip", jv_s("")),
    ]);
    let m22_after = jv_o(vec![
        ("Ok", JVal::B(true)),
        ("Why", jv_s("")),
        ("Sig", jv_s("mumu-sig-after")),
    ]);
    let m22_ask = jv_o(vec![
        ("Ok", JVal::B(true)),
        ("Verdict", jv_o(vec![
            ("Found", JVal::B(true)),
            ("Blocking", JVal::B(true)),
            ("What", jv_s("更新提示")),
            ("X", jv_s("0.90")),
            ("Y", jv_s("0.06")),
            ("Clicks", JVal::A(vec![jv_o(vec![
                ("X", jv_s("0.90")),
                ("Y", jv_s("0.06")),
                ("Why", jv_s("右上角的关闭")),
            ])])),
        ])),
    ]);
    let m22_point = jv_o(vec![
        ("Found", JVal::B(true)),
        ("ClientX", JVal::I(972)),
        ("ClientY", JVal::I(115)),
        ("Why", jv_s("右上角的关闭")),
    ]);
    let m22_click = jv_o(vec![
        ("Mode", jv_s("adb input tap(点进虚拟机，不碰宿主机窗口)")),
        ("Sent", JVal::B(true)),
    ]);
    let m22_chg = jv_o(vec![
        ("Known", JVal::B(true)),
        ("Changed", JVal::B(true)),
        ("Detail", jv_s("画面动了")),
    ]);
    let m22live = |tail: Vec<(&str, JVal)>| -> Vec<(String, JVal)> {
        let mut v: Vec<(String, JVal)> = vec![
            ("resolve-rescue-context".to_string(), m22_ctx.clone()),
            ("mumu-screen-shot".to_string(), m22_shot.clone()),
            ("vision-ask-model".to_string(), m22_ask.clone()),
            ("vision-click-point".to_string(), m22_point.clone()),
            ("mumu-screen-click".to_string(), m22_click.clone()),
            ("mumu-screen-shot".to_string(), m22_after.clone()),
        ];
        for (n, d) in tail {
            v.push((n.to_string(), d));
        }
        v
    };

    // 22a/22b 全过那一档：链体形状、结论话术、以及三枚 body 的 adb/serial 分派。
    let sc22a = sc22("mumu-rescue-ok", m22live(vec![("shot-changed", m22_chg.clone())]));
    let mut h22a = ScriptHost::new_m9a("gd-no-such-panel", &sc22a);
    let env22a = RescueEnv::of_m9a(&sc22a);
    let r22a = vision_rescue(&mut h22a, &env22a, "stuck-1", "M9A 连续两轮停在同一个界面");
    let want22a: Vec<String> = vec![
        "V|resolve-rescue-context",
        "V|mumu-screen-shot",
        "W|WARN|已拍 MuMu 模拟器画面：1080x1920，平均亮度 62",
        "V|vision-ask-model",
        "V|vision-click-point",
        "W|WARN|视觉兜底：更新提示 → 点MuMu 模拟器画面 972,115(右上角的关闭)",
        "V|mumu-screen-click",
        "S|6",
        "V|mumu-screen-shot",
        "V|shot-changed",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect();
    chk(
        r22a.as_ref().map_or(false, |r| {
            r.tried && r.done && r.detail == "已按模型判断关掉更新提示：点位 972,115(adb input tap(点进虚拟机，不碰宿主机窗口))→画面动了"
        }) && h22a.proj == want22a && h22a.live.is_empty(),
        "mumu 那一路的兜底链形状走样（闸序 / ctx 之前多问了一枚 verb / 两枚现场 verb 换 mumu 前缀 / 那句预告句仍排在点击 verb 之前 / 「救活」话术里 Mode 整串逐字）：两路共用的编排就不是同一条链",
        &mut fails,
        &mut total,
    );
    let b22ctx = h22a.body_of("resolve-rescue-context").cloned().unwrap_or(JVal::Null);
    let b22ask = h22a.body_of("vision-ask-model").cloned().unwrap_or(JVal::Null);
    let b22point = h22a.body_of("vision-click-point").cloned().unwrap_or(JVal::Null);
    let b22shot = h22a.body_of("mumu-screen-shot").cloned().unwrap_or(JVal::Null);
    let b22click = h22a.body_of("mumu-screen-click").cloned().unwrap_or(JVal::Null);
    chk(
        // 闸⑥由 C# 现查 ⇒ ctx 只收端点两键；`alive` 那枚覆盖格**不许由核喂**（它只归无头自检用）。
        jvo_has(&b22ctx, "target")
            && jv_str(&jv_get(&b22ctx, "target")) == "mumu"
            && jv_str(&jv_get(&b22ctx, "adb")) == sc22a.adb
            && jv_str(&jv_get(&b22ctx, "serial")) == "127.0.0.1:7554"
            && !jvo_has(&b22ctx, "alive")
            // 问模型那一处**原样转达**：两格 1999 话术核一个都不认识（§11.42 ①-11 的硬约束）。
            && jv_str(&jv_get(&b22ask, "scene")) == jv_str(&jv_get(&m22_ctx, "Scene"))
            && jv_str(&jv_get(&b22ask, "userText")) == "这是模拟器里 1999 的当前画面。按规则只回一个 JSON 对象。"
            && jv_str(&jv_get(&b22ask, "imagePath")) == "z:\\before.png"
            // 十五格消费：四格客户区与两枚尺寸取自 shot 回执，mumu 那四格恒 0/W/H。
            && jv_int(&jv_get(&b22point, "imgW"), 0) == 1080
            && jv_int(&jv_get(&b22point, "clientLeft"), -1) == 0
            && jv_int(&jv_get(&b22point, "clientW"), -1) == 1080
            // 三枚 body 各带 adb/serial；补拍那枚的路径由编排侧拼（mumu-<tag>-<时刻>-after1.png）。
            && jv_str(&jv_get(&b22shot, "adb")) == sc22a.adb
            && jv_str(&jv_get(&b22shot, "serial")) == "127.0.0.1:7554"
            && jv_str(&jv_get(&b22shot, "path")).ends_with("-after1.png")
            && jv_str(&jv_get(&b22shot, "path")).contains("mumu-stuck-1-")
            && jv_str(&jv_get(&b22click, "adb")) == sc22a.adb
            && jv_str(&jv_get(&b22click, "name")) == "MuMu 模拟器画面"
            && jv_int(&jv_get(&b22click, "clientX"), 0) == 972,
        "mumu 那四枚 verb 的入参分派走样（ctx 的端点两键漏喂／又多喂了一枚 alive／adb·serial 漏进 shot 或 click 的 body／现场量 path 拼错／1999 话术被核改写）：真跑时 C# 那边一句「adb 或序列号没定下来，拍不了屏」就会把兜底打死，而门吃夹具看不出来",
        &mut fails,
        &mut total,
    );

    // 22b ark 那一路的零影响自证：连 ark 基线投影都直接复用 B-19 那一组（一字不许多、一字不许少），
    //     再钉住 ctx 不喂 alive、两枚现场 verb 的 body 不带 adb/serial、`of_maa` 那两格恒空串。
    let sc22b = MaaScen {
        name: "ark-rescue-ctl".to_string(),
        state_dir: d22s.clone(),
        live: b19live(vec![("shot-changed", b19fx_chg_yes.clone())]),
        ..Default::default()
    };
    let mut h22b = ScriptHost::new("gd-no-such-panel", &sc22b);
    let env22b = RescueEnv::of_maa(&sc22b);
    let r22b = vision_rescue(&mut h22b, &env22b, "stuck-1", "");
    let b22b_ctx = h22b.body_of("resolve-rescue-context").cloned().unwrap_or(JVal::Null);
    let b22b_shot = h22b.body_of("ark-window-shot").cloned().unwrap_or(JVal::Null);
    let b22b_click = h22b.body_of("ark-window-click").cloned().unwrap_or(JVal::Null);
    chk(
        r22b.is_ok()
            && h22b.proj == b19want_ok
            && !h22b.proj.iter().any(|l| l.starts_with("V|adb-connect"))
            && env22b.adb.is_empty()
            && env22b.serial.is_empty()
            && !jvo_has(&b22b_ctx, "alive")
            && !jvo_has(&b22b_shot, "adb")
            && !jvo_has(&b22b_shot, "serial")
            && !jvo_has(&b22b_click, "adb")
            && !jvo_has(&b22b_click, "serial"),
        "端游那一路被 mumu 这一刀带跑了（多问一枚 adb-connect／多喂一个 alive／现场 verb 的 body 多出 adb 键／of_maa 那两格不再是空串）：arkrun 的 27 档 1218 步基线当场就得重拍",
        &mut fails,
        &mut total,
    );

    // 22c 闸⑥在核侧只剩一条消费路径：ctx 回 CanAsk=false ⇒ 那句 Skip 原样落地、投影**恰一行**，
    //     并且**绝不问 adb-connect**（10-12 实测改口：现查移进 C# 的 `MumuAlive` ⇒ 原先"devices 起不来"与
    //     "Ok:false"两种拿不到塌成同一格那条锚随之下沉，现由 `--livetest` 三格钉住；这一组只钉
    //     "核不多问一枚 verb、话术不重拼"）。
    //     反方向同样钉死：ctx 那一问**自己起不来** ⇒ 核**不降级**，把 Err 原样上抛 —— 真身那三处 catch 罩的是
    //     问模型／点击／比签名，不包括这一问；在这儿罩错了会把"编排漏喂端点"打成一句正常 skip。
    let off22 = jv_o(vec![
        ("CanAsk", JVal::B(false)),
        ("Skip", jv_s("MuMu 模拟器画面已经不在了，兜底没有对象")),
    ]);
    let sc22c = sc22(
        "mumu-rescue-offline",
        vec![("resolve-rescue-context".to_string(), off22)],
    );
    let mut h22c = ScriptHost::new_m9a("gd-no-such-panel", &sc22c);
    let r22c = vision_rescue(&mut h22c, &RescueEnv::of_m9a(&sc22c), "quiet-1", "");
    let b22c = h22c.body_of("resolve-rescue-context").cloned().unwrap_or(JVal::Null);
    let sc22c2 = sc22("mumu-rescue-ctxerr", vec![]);
    let mut h22c2 = ScriptHost::new_m9a("gd-no-such-panel", &sc22c2);
    let r22c2 = vision_rescue(&mut h22c2, &RescueEnv::of_m9a(&sc22c2), "quiet-2", "");
    chk(
        r22c.is_ok()
            && r22c.as_ref().map_or(false, |x| {
                !x.tried && x.detail == "MuMu 模拟器画面已经不在了，兜底没有对象"
            })
            && h22c.proj == vec!["V|resolve-rescue-context".to_string()]
            && !h22c.proj.iter().any(|l| l.contains("adb-connect"))
            && jvo_has(&b22c, "adb")
            && jvo_has(&b22c, "serial")
            && !jvo_has(&b22c, "alive")
            && r22c2.is_err()
            && h22c2.proj.len() == 1,
        "闸⑥那句 skip 被核重拼、或核仍然多问了 adb-connect（投影上一枚 verb 恰一行是硬形状）、或 ctx 起不来被降级成正常 skip：现场 adb 一断，兜底该落在 C# 那一句「已经不在了」上，而不是核里编出来的第二种话术",
        &mut fails,
        &mut total,
    );

    // 22d 第七道闸那两句在 mumu 那一路同样**只消费 verb 的整句 Skip**：尺寸句与黑屏句不同源（§11.42 ⑤-2），
    //     核里重拼一遍就是双前缀话术，而 arkrun/arkskip 那两条既有判据会当场分岔。
    let sc22d = sc22(
        "mumu-shot-size",
        vec![
            ("resolve-rescue-context".to_string(), m22_ctx.clone()),
            (
                "mumu-screen-shot".to_string(),
                jv_o(vec![
                    ("Ok", JVal::B(false)),
                    ("Skip", jv_s("截图失败：模拟器截屏尺寸异常 100x100(安卓那边还没起画面)")),
                ]),
            ),
        ],
    );
    let mut h22d = ScriptHost::new_m9a("gd-no-such-panel", &sc22d);
    let r22d = vision_rescue(&mut h22d, &RescueEnv::of_m9a(&sc22d), "drain-1", "");
    let sc22e = sc22(
        "mumu-shot-dark",
        vec![
            ("resolve-rescue-context".to_string(), m22_ctx.clone()),
            (
                "mumu-screen-shot".to_string(),
                jv_o(vec![
                    ("Ok", JVal::B(true)),
                    ("CanContinue", JVal::B(false)),
                    ("AvgLum", JVal::I(3)),
                    ("Skip", jv_s("截图几乎是全黑(平均亮度 3)")),
                ]),
            ),
        ],
    );
    let mut h22e = ScriptHost::new_m9a("gd-no-such-panel", &sc22e);
    let r22e = vision_rescue(&mut h22e, &RescueEnv::of_m9a(&sc22e), "drain-2", "");
    chk(
        r22d.as_ref().map_or(false, |x| {
            !x.tried && x.detail == "截图失败：模拟器截屏尺寸异常 100x100(安卓那边还没起画面)"
        }) && h22d.proj.len() == 2
            && !h22d.proj.iter().any(|l| l.contains("vision-ask-model"))
            && !h22d.proj.iter().any(|l| l.contains("adb-connect"))
            && r22e.as_ref().map_or(false, |x| {
                !x.tried && x.detail == "截图几乎是全黑(平均亮度 3)"
            })
            && h22e.proj.len() == 2
            && !h22e.live.iter().any(|(n, _)| n == "vision-ask-model"),
        "mumu 那一路的第七道闸（拍不到东西／几乎全黑）没只消费 verb 给的整句 Skip，或多问了一次模型：那两句在 C# 各有事实源，核里重拼就是双前缀",
        &mut fails,
        &mut total,
    );

    // 22f `other` 那枚桩已删净（§11.42 ①-1「删桩即回归抓器」）：靶子自报了现场 verb 之外的 kind ⇒ 当场 Err，
    //     话术是"自报了…之外的 kind"，而旧桩那句「现场 verb 还没进核」必须一个字节都不剩。
    let sc22f = sc22(
        "mumu-rescue-badkind",
        vec![
            (
                "resolve-rescue-context".to_string(),
                jv_o(vec![
                    ("CanAsk", JVal::B(true)),
                    ("Kind", jv_s("windows")),
                    ("Name", jv_s("别的窗口")),
                ]),
            ),
        ],
    );
    let mut h22f = ScriptHost::new_m9a("gd-no-such-panel", &sc22f);
    let r22f = vision_rescue(&mut h22f, &RescueEnv::of_m9a(&sc22f), "stuck-9", "");
    chk(
        r22f.as_ref().err().map_or(false, |m| {
            m == "vision_rescue：靶子自报了现场 verb 之外的 kind：windows"
        }) && h22f.proj.len() == 1
            && !h22f.proj.iter().any(|l| l.contains("mumu-screen-shot")),
        "catch-all 那枚桩没删干净，或认不出的 kind 被当 mumu 去拍了屏：真身对这种靶子第一道闸就回 CanAsk=false，核里编一句真身没有的话术就是把假绿写进链体",
        &mut fails,
        &mut total,
    );

    let _ = std::fs::remove_dir_all(&d22);
    chk(
        !d22.exists(),
        "B-2d③ 自检的临时目录没收干净：自检会在 %TEMP% 里留垃圾",
        &mut fails,
        &mut total,
    );

    // 23. B-2d④「r1999 链体进核＋PS 侧 executor」：这一组钉 `--drive-m9a-daily` 自己 ——
    //     七道拒跑闸逐句咬字且七句互不相同、role/why 两枚不同源（信封模板只留内层一份）、
    //     八格动作里真做只有 run（桩名册里没有它 ⇒ 分派漏改当场红）、账行键序
    //     status→detail→duration?→rescued?（后两枚**条件补**，没有就不写这个键）、
    //     重试分流那一支的 stop / sleep 20 / 再 run 一次、rescued 与 retry_once 怎么喂 decide、
    //     名册外五格按真身补、更新闸与审计闸各自的三态话术、init 与 ctx 不同源即 Proto、
    //     done 幂等、生产 run 闭包缺载荷即拒做，末了钉 `--fixture-run` 复用给这一路时的账行形状
    //     （⑤ 的等价门就吃这一枚旗标 —— 协议形状在这里再变一次就是第三次升号）。
    let d23 = std::env::temp_dir().join(format!("gd-b2d4-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d23);
    let _ = std::fs::create_dir_all(&d23);
    let d23s = d23.to_str().unwrap_or("").to_string();
    let t23 = |n: &str| format!("{}\\{}", d23s, n);
    let s23 = t23("state");
    let _ = std::fs::create_dir_all(&s23);
    let c23 = t23("ctx.json");
    let _ = std::fs::write(
        &c23,
        "{\"retry_once\":false,\"m9aRun\":{\"paths\":{\"m9aDir\":\"D:/M9A\",\"m9aExe\":\"D:/M9A/m9a.exe\",\"m9aLogDir\":\"D:/M9A/log\"},\"conn\":{\"adb\":\"D:/adb.exe\",\"serial\":\"127.0.0.1:7555\"},\"instance\":{\"id\":\"0\",\"name\":\"MuMu Android 12\"},\"package\":\"com.example.revenuguild\",\"runtime\":{\"m9aCliArgs\":true,\"m9aInstanceArg\":\"id\",\"m9aForceStart\":true,\"r1999StartTimeoutMinutes\":6,\"r1999TimeoutMinutes\":45,\"stallTimeoutMinutes\":12,\"visionMaxRescue\":3}}}",
    );
    let jlines = |p: &str| -> Vec<String> {
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .lines()
            .map(|s| s.to_string())
            .collect()
    };
    let runrows = |p: &str| -> Vec<String> {
        jlines(p)
            .into_iter()
            .filter(|l| l.contains("\"action\":\"run\""))
            .collect()
    };
    let mut dec_no23 = |_st: &str, _rs: bool, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
        Ok((false, Vec::new()))
    };
    let mut run_no23 = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
        Err("这一枚不该被调到（会话级判据之外的路）".to_string())
    };

    // 23a 七道闸：逐句点名所缺旗标，且七句互不相同（同字面＝诊断里闸的编号没了）。
    let ref23: Vec<Option<String>> = vec![
        drive_m9a_refusal(false, "j", "c", "r", "p", "s", "k"),
        drive_m9a_refusal(true, "", "c", "r", "p", "s", "k"),
        drive_m9a_refusal(true, "j", "", "r", "p", "s", "k"),
        drive_m9a_refusal(true, "j", "c", "", "p", "s", "k"),
        drive_m9a_refusal(true, "j", "c", "r", "", "s", "k"),
        drive_m9a_refusal(true, "j", "c", "r", "p", "", "k"),
        drive_m9a_refusal(true, "j", "c", "r", "p", "s", ""),
    ];
    let keys23 = ["--i-drive", "--journal", "--ctx", "--res", "--panel-cs", "--state", "--log-sink"];
    let mut uniq23: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut named23 = true;
    for (m, k) in ref23.iter().zip(keys23.iter()) {
        match m {
            Some(t) => {
                named23 = named23 && t.contains(*k);
                uniq23.insert(t.clone());
            }
            None => named23 = false,
        }
    }
    chk(
        named23 && uniq23.len() == 7,
        "m9a 驱动档的七道拒跑闸走样（有一道没点名所缺旗标，或两句同字面）：缺参的会话会在动作中途才炸，1999 的换头调度器起不来时看不出是哪一面没给",
        &mut fails,
        &mut total,
    );
    chk(
        ref23[1].as_ref().map_or(false, |m| m.contains("断点没落点"))
            && ref23[2].as_ref().map_or(false, |m| m.contains("retry_once/m9aRun 两格"))
            && ref23[3].as_ref().map_or(false, |m| m.contains("八格现场动作"))
            && ref23[4]
                .as_ref()
                .map_or(false, |m| m.contains("decide-daily-retry"))
            && ref23[5].as_ref().map_or(false, |m| m.contains("救火账"))
            && ref23[6].as_ref().map_or(false, |m| m.contains("1999 链上日志"))
            && drive_m9a_refusal(true, "j", "c", "r", "p", "s", "k").is_none(),
        "m9a 拒跑句里有一枚没交代这一面是干什么的（或七样齐时仍拒跑）：拒跑闸是给人的第一句人话，只报旗标名不报后果就等于让人自己去读码",
        &mut fails,
        &mut total,
    );

    // 23b role/why 两枚不同源：三枚信封模板只有内层一份字面，role/why 是唯一的差别。
    let ask_ark23 = render_drive_ask(2, "desk", &JVal::Null);
    let ask_m9a23 = drive_env_ask(DRIVE_ROLE_M9A, 2, "updgate", &JVal::Null);
    chk(
        ask_ark23.contains("\"role\":\"R3-drive-ark-daily\"")
            && ask_ark23.contains("mark/obsrec/admin")
            && !ask_ark23.contains("B2d-drive-m9a-daily")
            && ask_m9a23.contains("\"role\":\"B2d-drive-m9a-daily\"")
            && ask_m9a23.contains("1999 的链与记账")
            && !ask_m9a23.contains("R3-drive-ark-daily")
            && !ask_m9a23.contains("mark/obsrec/admin")
            && ask_m9a23.contains("\"logs\":[]")
            && ask_m9a23.contains("\"params\":null"),
        "两路信封的 role/why 串了（或 m9a 那句照抄了 ark 的『已搬动作（mark/obsrec/admin）』）：1999 这一路八格里没有这三格，字面沿用就是让核认识一句 1999 事实却说错了",
        &mut fails,
        &mut total,
    );
    chk(
        drive_env_fail(DRIVE_ROLE_M9A, "x").contains("\"role\":\"B2d-drive-m9a-daily\"")
            && drive_env_done(DRIVE_ROLE_M9A, &JVal::Null).contains("\"role\":\"B2d-drive-m9a-daily\"")
            && drive_env_done(DRIVE_ROLE_ARK, &JVal::Null).contains("\"role\":\"R3-drive-ark-daily\""),
        "三枚信封里 role 有一枚没走参数化（ask 换了、done/fail 还写死 ark 那枚）：PS 侧那两枚校验（role 与 contract 成对）会在 done 上认不出这一本账是谁的",
        &mut fails,
        &mut total,
    );

    // 23c 会话正路（真做只有 run）：ask 名册七格里的六格、账行四键齐全、done 只带三格。
    let j23c = t23("c-journal.ndjson");
    let k23c = t23("c-sink.ndjson");
    let run_calls23c = std::cell::Cell::new(0usize);
    let mut resp23c = |a: &str, p: &JVal| drive_m9a_selftest_respond(a, p);
    let mut run23c = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
        run_calls23c.set(run_calls23c.get() + 1);
        Ok(M9aRunOut {
            status: "ok".to_string(),
            detail: "一切正常".to_string(),
            duration: Some(62.5),
            rescued: true,
        })
    };
    let (a23c, l23c, kind23c, res23c, e23c) = drive_m9a_selftest_session(
        &j23c,
        &c23,
        &t23("c-res.json"),
        &s23,
        &k23c,
        &mut resp23c,
        &mut run23c,
        &mut dec_no23,
    );
    chk(
        e23c.is_none()
            && kind23c == "done"
            && a23c == vec![
                "updgate".to_string(),
                "takeover".to_string(),
                "mumu".to_string(),
                "cfg".to_string(),
                "stop".to_string(),
                "audit".to_string()
            ]
            && run_calls23c.get() == 1,
        "m9a 链体的动作序或真做分派不对：ask 名册该是 updgate/takeover/mumu/cfg/stop/audit 六枚（run 真做、不许问），run 该恰好做一次 —— 多出 run 说明壳侧白名单没删干净（双删即回归抓器），少一枚说明链体与真身 :87-165 不同源",
        &mut fails,
        &mut total,
    );
    let rows23c = runrows(&j23c);
    chk(
        rows23c.len() == 1
            && rows23c[0]
                == "{\"seq\":8,\"action\":\"run\",\"data\":{\"status\":\"ok\",\"detail\":\"一切正常\",\"duration\":62.5,\"rescued\":true}}"
            && jv_to_json(&res23c) == "{\"status\":\"ok\",\"detail\":\"一切正常\",\"duration\":62.5}",
        "m9a 的 run 账行或 done result 形状不对：账行键序该 status→detail→duration→rescued（四枚齐全时），done 只带三格（不带 rescued）—— 键序或键数一变，⑤ 的门就没法与 PS 侧 legacy 写的 res data 逐字节比",
        &mut fails,
        &mut total,
    );
    chk(
        l23c == vec![
            "STEP|--- 重返未来：1999(MuMu + M9A)---".to_string(),
            "INFO|M9A 自动更新闸：已是最新，无需更新".to_string(),
            "INFO|  M9A：实例：1999".to_string(),
            "INFO|1999 活动审计：无待审活动".to_string(),
        ],
        "m9a 链上日志没按产生顺序逐行落进 --log-sink（句子话术或先后有一处走样）：更新闸排在起 M9A 之前是真身的硬事实（更新器见进程在跑就拒动），顺序错一轮生产就是白等十几分钟",
        &mut fails,
        &mut total,
    );

    // 23d 两枚条件补 ＋ 审计闸的条件：非 ok/empty-queue 的终态**不许问审计**，且 duration/rescued 不落键。
    let j23d = t23("d-journal.ndjson");
    let mut resp23d = |act: &str, prm: &JVal| drive_m9a_selftest_respond(act, prm);
    let mut run23d = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
        Ok(M9aRunOut {
            status: "blocked".to_string(),
            detail: "停在启动器更新页".to_string(),
            duration: None,
            rescued: false,
        })
    };
    let (a23d, l23d, kind23d, _res23d, e23d) = drive_m9a_selftest_session(
        &j23d,
        &c23,
        &t23("d-res.json"),
        &s23,
        &t23("d-sink.ndjson"),
        &mut resp23d,
        &mut run23d,
        &mut dec_no23,
    );
    let rows23d = runrows(&j23d);
    chk(
        e23d.is_none()
            && kind23d == "done"
            && rows23d.len() == 1
            && rows23d[0]
                == "{\"seq\":8,\"action\":\"run\",\"data\":{\"status\":\"blocked\",\"detail\":\"停在启动器更新页\"}}"
            && a23d == vec![
                "updgate".to_string(),
                "takeover".to_string(),
                "mumu".to_string(),
                "cfg".to_string(),
                "stop".to_string()
            ]
            && l23d.iter().any(|x| x == "WARN|1999 结束状态：blocked — 停在启动器更新页"),
        "m9a 账行的两枚条件补走样了（无 duration/rescued 却把键写出来），或 blocked 终态仍去问审计闸：真身那句条件是 `-in @('ok','empty-queue')`，多问一次就是给一轮没跑完的当日轮记一笔莫须有的活动账",
        &mut fails,
        &mut total,
    );

    // 23e 重试分流那一支：stop → sleep 20 → 再 run 一次（第二次叫「1999重试」）→ 收尾 stop → 审计。
    let j23e = t23("e-journal.ndjson");
    let prm_e23 = std::cell::RefCell::new(String::new());
    let mut resp23e = |act: &str, prm: &JVal| -> Result<Option<JVal>, String> {
        if act == "sleep" {
            *prm_e23.borrow_mut() = jv_to_json(prm);
        }
        drive_m9a_selftest_respond(act, prm)
    };
    let run_calls23e = std::cell::Cell::new(0usize);
    let mut run23e = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
        run_calls23e.set(run_calls23e.get() + 1);
        Ok(M9aRunOut {
            status: "ok".to_string(),
            detail: "第二次跑完".to_string(),
            duration: None,
            rescued: false,
        })
    };
    let mut dec23e = |_st: &str, _rs: bool, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
        Ok((true, vec![("INFO".to_string(), "重试分流：判可重试".to_string())]))
    };
    let (a23e, l23e, kind23e, _res23e, e23e) = drive_m9a_selftest_session(
        &j23e,
        &c23,
        &t23("e-res.json"),
        &s23,
        &t23("e-sink.ndjson"),
        &mut resp23e,
        &mut run23e,
        &mut dec23e,
    );
    let rows23e = runrows(&j23e);
    chk(
        e23e.is_none()
            && kind23e == "done"
            && run_calls23e.get() == 2
            && prm_e23.borrow().as_str() == "20"
            && a23e == vec![
                "updgate".to_string(),
                "takeover".to_string(),
                "mumu".to_string(),
                "cfg".to_string(),
                "stop".to_string(),
                "sleep".to_string(),
                "stop".to_string(),
                "audit".to_string()
            ]
            && rows23e.len() == 2
            && rows23e[1].starts_with("{\"seq\":12,\"action\":\"run\"")
            && l23e.iter().any(|x| x == "INFO|重试分流：判可重试"),
        "m9a 的重试分流这一支不对：sleep 的 params 该是 20（**不是 ark 那一路的 25**，真身 :130 写的就是这个数）、动作序该是 stop/sleep/再 run/收尾 stop、decide 回传的日志该按序进账进侧信道 —— 两套话术不许顺手统一，统一了门就再也分不清是哪一路",
        &mut fails,
        &mut total,
    );

    // 23fg 喂 decide 的三参数 ＋ 名册外五格按真身补（run 闭包看到的 scen 是这两枚判据的现场）。
    let c23g = t23("g-ctx.json");
    let _ = std::fs::write(&c23g, std::fs::read_to_string(&c23).unwrap_or_default().replace("\"retry_once\":false", "\"retry_once\":true"));
    let seen_dec23g = std::cell::RefCell::new(Vec::<String>::new());
    let seen_scen23g = std::cell::RefCell::new(String::new());
    let mut resp23g = |act: &str, prm: &JVal| drive_m9a_selftest_respond(act, prm);
    let mut run23g = |sc: &M9aScen| -> Result<M9aRunOut, String> {
        *seen_scen23g.borrow_mut() = format!(
            "dir={} adb={} serial={} pkg={} mode={} cli={} force={} start={} timeout={} stall={} vision={} game={} target={} state={} rescue={} grab={} dry={}",
            sc.m9a_dir,
            sc.adb,
            sc.serial,
            sc.package,
            sc.mode,
            sc.cli.map_or("none".to_string(), |b| if b { "true".to_string() } else { "false".to_string() }),
            sc.force_start,
            sc.start_min,
            sc.timeout_min,
            sc.stall_min,
            sc.vision_max,
            sc.game,
            sc.target,
            sc.state_dir,
            sc.rescue_available,
            sc.grab_available,
            sc.dry_run
        );
        Ok(M9aRunOut {
            status: "not-started".to_string(),
            detail: "没起来".to_string(),
            duration: None,
            rescued: true,
        })
    };
    let mut dec23g = |st: &str, rs: bool, ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
        seen_dec23g.borrow_mut().push(format!("{}|{}|{}", st, rs, ro));
        Ok((false, Vec::new()))
    };
    let (_a23g, _l23g, kind23g, _res23g, e23g) = drive_m9a_selftest_session(
        &t23("g-journal.ndjson"),
        &c23g,
        &t23("g-res.json"),
        &s23,
        &t23("g-sink.ndjson"),
        &mut resp23g,
        &mut run23g,
        &mut dec23g,
    );
    chk(
        // decide 不是八格现场动作、不记账，所以迷你驱动器每重放一轮都会被**重新调一次**（实测：
        // 账上 log/run 都占 seq，decide 不占）。判据只钉"每一枚都等于同一组现值"，不钉枚数。
        e23g.is_none()
            && kind23g == "done"
            && !seen_dec23g.borrow().is_empty()
            && seen_dec23g.borrow().iter().all(|x| x == "not-started|true|true"),
        "decide-daily-retry 的三枚入参有一枚接错（status 取自账上终态、rescued 来自 run 真做、retry_once 来自 ctx 顶层）：分流判据就此拿错现值，而 C# 那侧的九档话术看不出自己拿到的是假数据",
        &mut fails,
        &mut total,
    );
    let want23g = format!(
        "dir=D:/M9A adb=D:/adb.exe serial=127.0.0.1:7555 pkg=com.example.revenuguild mode=id cli=true force=true start=6 timeout=45 stall=12 vision=3 game=r1999 target=mumu state={} rescue=true grab=false dry=false",
        s23
    );
    chk(
        *seen_scen23g.borrow() == want23g,
        "ctx 名册那 15 枚进核后有一枚接错，或名册外那五格没按真身补（game=r1999 / target=mumu / state_dir=--state / 救火可用 true / 抢前台 false / dry_run 必须为 false）：真做会拿错实例或错超时去起 M9A，而 dry_run 一旦被供成 true，当日轮会一行现场都不碰却报 ok",
        &mut fails,
        &mut total,
    );

    // 23h 更新闸与审计闸的三态：非零退出码 WARN 不挡当日轮；闸本身抛错 WARN 后照常跑。
    let mut resp23h = |act: &str, prm: &JVal| -> Result<Option<JVal>, String> {
        match act {
            "updgate" => Ok(Some(jv_o(vec![
                ("code", JVal::I(3)),
                ("line", jv_s("挑出来的那一行")),
                ("joined", jv_s("更新器输出第一行 第二行")),
            ]))),
            "audit" => Ok(Some(jv_o(vec![
                ("code", JVal::I(1)),
                ("line", jv_s("审计没挑出行")),
                ("joined", jv_s("审计输出整段")),
            ]))),
            other => drive_m9a_selftest_respond(other, prm),
        }
    };
    let mut run23h = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
        Ok(M9aRunOut {
            status: "empty-queue".to_string(),
            detail: "队列是空的".to_string(),
            duration: None,
            rescued: false,
        })
    };
    let (_a23h, l23h, kind23h, _res23h, e23h) = drive_m9a_selftest_session(
        &t23("h-journal.ndjson"),
        &c23,
        &t23("h-res.json"),
        &s23,
        &t23("h-sink.ndjson"),
        &mut resp23h,
        &mut run23h,
        &mut dec_no23,
    );
    chk(
        e23h.is_none()
            && kind23h == "done"
            && l23h.iter().any(|x| x == "WARN|M9A 自动更新未完成(退出码 3)，不挡当日轮：更新器输出第一行 第二行")
            && l23h
                .iter()
                .any(|x| x == "WARN|1999 活动审计没审成(退出码 1)，维持现状：审计输出整段")
            && l23h.iter().any(|x| x == "WARN|1999 结束状态：empty-queue — 队列是空的"),
        "两枚闸的非零退出码话术走样（该带退出码与全量输出、且**不挡当日轮**）：真身这两段是加分项，一句改重就把『更新器今天没配额』升级成了整轮失败",
        &mut fails,
        &mut total,
    );
    let mut resp23h2 = |act: &str, prm: &JVal| -> Result<Option<JVal>, String> {
        if act == "updgate" {
            return Err("更新闸自己炸了".to_string());
        }
        drive_m9a_selftest_respond(act, prm)
    };
    let j23h2 = t23("h2-journal.ndjson");
    let (_a23h2, l23h2, kind23h2, _res23h2, e23h2) = drive_m9a_selftest_session(
        &j23h2,
        &c23,
        &t23("h2-res.json"),
        &s23,
        &t23("h2-sink.ndjson"),
        &mut resp23h2,
        &mut run23h,
        &mut dec_no23,
    );
    let gate_row23h2 = jlines(&j23h2)
        .into_iter()
        .find(|l| l.contains("\"action\":\"updgate\""))
        .unwrap_or_default();
    chk(
        e23h2.is_none()
            && kind23h2 == "done"
            && gate_row23h2 == "{\"seq\":2,\"action\":\"updgate\",\"err\":\"更新闸自己炸了\"}"
            && l23h2
                .iter()
                .any(|x| x == "WARN|M9A 自动更新闸本身抛错，跳过它照常跑：更新闸自己炸了"),
        "闸本身抛错这一路不对：账上该写 err 而不是 data、链体该吞成那句 WARN 并**继续当日轮** —— 抛错被当 Fail 上抛的话，一次更新器崩溃会拖垮整轮（真身的 catch 只包住这一段）",
        &mut fails,
        &mut total,
    );

    // 23i init 与 ctx 不同源 = Proto：任何 catch 都不接，当场 fail 且不写侧信道。
    let j23i = t23("i-journal.ndjson");
    let c23i = t23("i-ctx.json");
    let _ = std::fs::write(
        &c23i,
        "{\"retry_once\":false,\"m9aRun\":{\"paths\":{\"m9aDir\":\"D:/OTHER\",\"m9aExe\":\"D:/OTHER/m9a.exe\",\"m9aLogDir\":\"D:/OTHER/log\"},\"conn\":{\"adb\":\"D:/adb.exe\",\"serial\":\"127.0.0.1:7555\"},\"instance\":{\"id\":\"0\",\"name\":\"MuMu Android 12\"},\"package\":\"com.example.revenuguild\",\"runtime\":{\"m9aCliArgs\":true,\"m9aInstanceArg\":\"id\",\"m9aForceStart\":true,\"r1999StartTimeoutMinutes\":6,\"r1999TimeoutMinutes\":45,\"stallTimeoutMinutes\":12,\"visionMaxRescue\":3}}}",
    );
    let (rc23i1, env23i1) = drive_m9a_daily_envelope(
        true,
        &j23i,
        &c23,
        &t23("i-res.json"),
        "unused-panel-cs",
        &s23,
        &t23("i-sink.ndjson"),
        &mut dec_no23,
        &mut run_no23,
    );
    let (rc23i2, env23i2) = drive_m9a_daily_envelope(
        true,
        &j23i,
        &c23i,
        &t23("i-res.json"),
        "unused-panel-cs",
        &s23,
        &t23("i-sink2.ndjson"),
        &mut dec_no23,
        &mut run_no23,
    );
    chk(
        rc23i1 == 0
            && env23i1.contains("\"action\":\"updgate\"")
            && env23i1.contains("\"seq\":2")
            && rc23i2 == 1
            && env23i2.contains("\"kind\":\"fail\"")
            && env23i2.contains("驱动协议坏了：journal 的 init 与 ctx 不是同一份")
            && std::fs::metadata(&t23("i-sink2.ndjson")).is_err(),
        "同一本 journal 换了 ctx 没被拒（或首问不是 updgate、或拒跑时还往侧信道写行）：拿上一轮的账配这一轮的现值继续跑，1999 会在错误的实例上接着推链 —— 这类不同源必须是 Proto，任何 catch 都不许接住它",
        &mut fails,
        &mut total,
    );

    // 23j done 幂等：账上有 done 行时再起一轮 = 直接 done，一次动作都不重做、一行日志都不再落。
    let before23j = jlines(&j23c).len();
    let (rc23j, env23j) = drive_m9a_daily_envelope(
        true,
        &j23c,
        &c23,
        &t23("j-res.json"),
        "unused-panel-cs",
        &s23,
        &t23("j-sink.ndjson"),
        &mut dec_no23,
        &mut run_no23,
    );
    chk(
        rc23j == 0
            && env23j.contains("\"kind\":\"done\"")
            && env23j.contains("\"role\":\"B2d-drive-m9a-daily\"")
            && env23j.contains("\"status\":\"ok\"")
            && jlines(&j23c).len() == before23j
            && std::fs::read_to_string(&t23("j-sink.ndjson")).unwrap_or_default().is_empty(),
        "done 之后重入没走早退（账又多行 / 侧信道又落行 / 桩 run 被调到）：幂等重入是断点协议的一部分，重做一遍就是当日双起 M9A 会话",
        &mut fails,
        &mut total,
    );

    // 23k 真做抛错仍被链体吞成 error 态：账写 err、话术是「1999 异常：」（ark 那一路是「方舟 异常：」）。
    let j23k = t23("k-journal.ndjson");
    let mut resp23k = |act: &str, prm: &JVal| drive_m9a_selftest_respond(act, prm);
    let mut run23k = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
        Err("运行面桩故意抛".to_string())
    };
    let (a23k, _l23k, kind23k, res23k, e23k) = drive_m9a_selftest_session(
        &j23k,
        &c23,
        &t23("k-res.json"),
        &s23,
        &t23("k-sink.ndjson"),
        &mut resp23k,
        &mut run23k,
        &mut dec_no23,
    );
    let rows23k = runrows(&j23k);
    chk(
        e23k.is_none()
            && kind23k == "done"
            && !a23k.iter().any(|x| x == "run")
            && rows23k.len() == 1
            && rows23k[0] == "{\"seq\":8,\"action\":\"run\",\"err\":\"运行面桩故意抛\"}"
            && jv_str(&jv_get(&res23k, "status")) == "error"
            && jv_str(&jv_get(&res23k, "detail")) == "1999 异常：运行面桩故意抛",
        "run 真做抛错这一路不对：账上写 err 不写 data、链体吞成 status=error＋「1999 异常：」那句（**不是 ark 的「方舟 异常：」**）、ask 名册里不许再出现 run —— 任一半走样，生产就把一次失败的 M9A 当成功终态收",
        &mut fails,
        &mut total,
    );

    // 23l 生产 run 闭包的边界：ctx 载荷残缺 ⇒ 当场拒做，不猜路径、不碰侧信道。
    let f_sink23 = t23("f-sink.ndjson");
    let f23 = drive_real_m9a_run("unused-panel-cs", &f_sink23, &M9aScen::default());
    chk(
        f23
            .as_ref()
            .err()
            .map_or(false, |m| m.contains("m9aRun 的 paths") && m.contains("不替生产猜默认路径"))
            && std::fs::metadata(&f_sink23).is_err(),
        "生产 run 闭包在 ctx 载荷缺失时没当场拒做（或拒做的路上顺手造了文件）：真做会拿空路径去起 M9A，错话还会从链上消失",
        &mut fails,
        &mut total,
    );

    // 23m ctx 那三枚起步闸走 fail 通道（rc=1、带「驱动协议坏了：」前缀），retry_once 非布尔另算一句。
    let cases23m: Vec<(&str, &str, &str)> = vec![
        (
            "m-ctx-a.json",
            "{\"retry_once\":\"yes\"}",
            "retry_once 必须是布尔",
        ),
        (
            "m-ctx-b.json",
            "{\"retry_once\":false,\"m9aRun\":{\"paths\":{}}}",
            "paths.m9aDir 没给",
        ),
        (
            "m-ctx-c.json",
            "{\"retry_once\":false,\"m9aRun\":{\"paths\":{\"m9aDir\":\"D:/M9A\"}}}",
            "conn.adb 没给",
        ),
    ];
    let mut got23m = Vec::new();
    for (n, body, want) in cases23m.iter() {
        let p = t23(n);
        let _ = std::fs::write(&p, body);
        let (rc, env) = drive_m9a_daily_envelope(
            true,
            &t23(&n.replace("ctx", "jou")),
            &p,
            &t23(&n.replace("ctx", "rse")),
            "unused-panel-cs",
            &s23,
            &t23(&n.replace("ctx", "snk")),
            &mut dec_no23,
            &mut run_no23,
        );
        got23m.push(rc == 1 && env.contains("\"kind\":\"fail\"") && env.contains("驱动协议坏了") && env.contains(*want));
    }
    chk(
        got23m == vec![true, true, true],
        "ctx 面的三枚起步闸没走 fail 通道（或错话没点名是哪一格残缺）：retry_once 不是布尔、m9aDir/adb 没给这三档，缺任何一条都会在真做那一步变成『拿空值起进程』，而不是当场一句人话",
        &mut fails,
        &mut total,
    );

    // 23n --fixture-run 复用给这一路（⑤ 的门就吃这枚旗标）：先钉闸的优先级与夹具的两条错路，
    //     再走**真出口**推满五轮（首轮问 updgate，后四轮逐枚回执），钉这一轮的 run 账行逐字形状
    //     （多认的那枚 rescued 必须落到键序末位）。
    //     账行号照实测：链体每句 log 也占一行 seq —— 1 STEP / 2 updgate / 3 更新闸句 / 4 takeover /
    //     5 mumu / 6 cfg / 7 "  M9A：" 句 / **8 run**，所以夹具回执的 seq 是 2·4·5·6 而不是连号。
    let e23n = t23("n-env.json");
    let out23n = &Some(e23n.clone());
    let env23n = || -> String { std::fs::read_to_string(&e23n).unwrap_or_default().trim().to_string() };
    let j23n = t23("n-journal.ndjson");
    let r23n = t23("n-res.json");
    let k23n = t23("n-sink.ndjson");
    let c23n = t23("n-ctx.json");
    let _ = std::fs::copy(&c23, &c23n);
    let missfx23 = t23("no-such-fixture.json");
    let rc23n1 = run_drive_m9a_daily(false, &j23n, &c23n, &r23n, "unused-panel-cs", &s23, &k23n, &missfx23, out23n);
    chk(
        rc23n1 == 2 && env23n().contains("--i-drive") && !env23n().contains("夹具读不动"),
        "m9a 驱动档的拒跑闸排在了夹具读取之后：缺 --i-drive 时先报夹具错，会把『参数漏写』伪装成『夹具喂错』——换头开关那句拒跑就此不是唯一出口",
        &mut fails,
        &mut total,
    );
    let rc23n2 = run_drive_m9a_daily(true, &j23n, &c23n, &r23n, "unused-panel-cs", &s23, &k23n, &missfx23, out23n);
    chk(
        rc23n2 == 1 && env23n().contains("--fixture-run 的夹具读不动") && env23n().contains("驱动协议坏了"),
        "--fixture-run 指到不存在的文件时没当场 fail（或错话没点名旗标）：给了旗标却喂不到夹具，比不喂更容易被当成『跑过了』",
        &mut fails,
        &mut total,
    );
    let fx23n = t23("n-fixture-run.json");
    let _ = std::fs::write(
        &fx23n,
        "{\"status\":\"ok\",\"detail\":\"队列跑完\",\"duration\":33.5,\"rescued\":true,\"logs\":[{\"level\":\"INFO\",\"text\":\"夹具第一行\"},{\"level\":\"WARN\",\"text\":\"夹具第二行\"}]}",
    );
    let mut rc23n3 = 0i32;
    for (i, res_row) in [
        "",
        "{\"seq\":2,\"action\":\"updgate\",\"ok\":true,\"data\":{\"code\":0,\"line\":\"已是最新，无需更新\",\"joined\":\"已是最新，无需更新\"}}",
        "{\"seq\":4,\"action\":\"takeover\",\"ok\":true}",
        "{\"seq\":5,\"action\":\"mumu\",\"ok\":true,\"data\":true}",
        "{\"seq\":6,\"action\":\"cfg\",\"ok\":true,\"data\":{\"changes\":[\"实例：1999\"]}}",
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            let _ = std::fs::write(&r23n, res_row);
        }
        rc23n3 = run_drive_m9a_daily(true, &j23n, &c23n, &r23n, "unused-panel-cs", &s23, &k23n, &fx23n, out23n);
    }
    let rows23n = runrows(&j23n);
    chk(
        rc23n3 == 1
            && env23n().contains("decide-daily-retry 起不来")
            && rows23n.len() == 1
            && rows23n[0]
                == "{\"seq\":8,\"action\":\"run\",\"data\":{\"status\":\"ok\",\"detail\":\"队列跑完\",\"duration\":33.5,\"rescued\":true}}"
            && jlines(&k23n).iter().filter(|l| l.contains("夹具第一行")).count() == 1
            && jlines(&k23n).iter().filter(|l| l.contains("夹具第二行")).count() == 1,
        "真出口这一路的 m9a run 真做不对：夹具多认的那枚 rescued 没落到账行键序末位（⑤ 的门就没法与 PS 侧 legacy 写的 res data 逐字节比），或夹具 logs 没按序落进同一本侧信道",
        &mut fails,
        &mut total,
    );

    let _ = std::fs::remove_dir_all(&d23);
    chk(
        !d23.exists(),
        "B-2d④ 自检的临时目录没收干净：自检会在 %TEMP% 里留垃圾",
        &mut fails,
        &mut total,
    );

    // ================================================================ B-3②-b3 轮骨架与第三答手
    //
    // 这座门的面只有两件事可钉：**取数件的形状**与**谁来答那张表**。判据本身一行都不在这儿
    // （判定/门表/预览那几族的真身在 ask 的另一头），所以逐格断言的全是"喂进去哪几枚键、落下来哪几格账"。
    // ⚠ 一次子进程都不起、一次电源都不碰、一秒都不真睡：三枚注入点（call/nap/es）全给假身。
    //   在位性现问**不给假身**（就用 std::fs）：自检去建一棵临时树喂 --root，
    //   那样"这棵树上装了哪几路"这半段判据仍在被测物里 —— 给假身等于把要钉的事摘出去。
    {
        // 假 verb 现场的回执册：每一枚都照 panel-cs 那侧的**原键名**（PascalCase）摆，
        // 于是"答手取错键"这一类错会当场落成 None/空值而红，而不是悄悄通过。
        fn r24_reply(verb: &str, paths: &JVal) -> JVal {
            match verb {
                "resolve-round-context" => jv_o(vec![
                    (
                        "Runtime",
                        jv_o(vec![
                            ("blockSleep", JVal::B(true)),
                            ("keepScreenOn", JVal::B(false)),
                            ("retryOnce", JVal::B(true)),
                        ]),
                    ),
                    (
                        "Elevation",
                        jv_o(vec![
                            ("arkRequiresElevation", JVal::B(false)),
                            ("launchArkFromHelper", JVal::B(true)),
                        ]),
                    ),
                    ("Paths", paths.clone()),
                    ("NowText", jv_s("2026-10-15 06:00:00")),
                    ("IsAdmin", JVal::B(true)),
                    ("NowMs", JVal::I(1760000000000)),
                    ("Dow", JVal::I(4)),
                ]),
                "ark-desktop-state" => jv_o(vec![
                    ("Desktop", jv_s("Default")),
                    ("Locked", JVal::B(false)),
                    ("LockedKnown", JVal::B(true)),
                ]),
                "resolve-screensaver-verdict" => jv_o(vec![("Detail", jv_s("屏保：没挂 .scr"))]),
                "takeover-processes" => jv_o(vec![
                    ("Done", JVal::B(true)),
                    ("Label", jv_s("方舟/MAA")),
                    ("Stopped", JVal::A(vec![jv_s("MAA")])),
                    ("Lines", JVal::A(vec![jv_s("接管那一句")])),
                ]),
                "apply-maa-daily-config" => jv_o(vec![
                    ("Done", JVal::B(true)),
                    ("Path", jv_s("D:\\No\\Where\\config.json")),
                    ("Changes", JVal::A(vec![jv_s("队列：作战 开")])),
                    ("ExpectedChains", JVal::A(vec![jv_s("combat")])),
                    ("Stage", jv_s("CE-5")),
                    ("Applied", JVal::B(true)),
                    ("Lines", JVal::A(Vec::new())),
                ]),
                "start-ark-client" => jv_o(vec![
                    ("Done", JVal::B(true)),
                    ("Started", JVal::B(true)),
                    ("Lines", JVal::A(vec![jv_s("端游窗口就绪")])),
                ]),
                "stop-maa-and-game" => jv_o(vec![
                    ("Done", JVal::B(true)),
                    ("MaaClosed", JVal::I(1)),
                    ("ArkClosed", JVal::I(0)),
                    ("Lines", JVal::A(vec![jv_s("已关闭 MAA(1 个进程)")])),
                ]),
                "start-mumu" => jv_o(vec![("Done", JVal::B(true)), ("Started", JVal::B(true))]),
                "apply-m9a-instance" => jv_o(vec![
                    ("Done", JVal::B(true)),
                    ("Changes", JVal::A(vec![jv_s("Instances.LastActive: a → b")])),
                    ("Applied", JVal::B(true)),
                    ("Lines", JVal::A(Vec::new())),
                ]),
                "stop-m9a" => jv_o(vec![("Done", JVal::B(true)), ("M9aClosed", JVal::I(1))]),
                // 两枚"读日志尾"的现场件：这一档全按**没读到**回 ⇒ 验的就是答手把 Found=false
                // 折成账上那一枚 null（而不是折成空对象）的那条定档。
                "read-maa-fight-skip" => jv_o(vec![("Found", JVal::B(false)), ("Reason", jv_s("gui-log-missing"))]),
                "read-maa-stage-observation" => jv_o(vec![
                    ("Found", JVal::B(false)),
                    ("LogsRead", JVal::B(false)),
                    ("Reason", jv_s("no-logs")),
                    ("Date", jv_s("2026-10-15")),
                ]),
                "update-ark-farm-ledger" => jv_o(vec![
                    ("Done", JVal::B(true)),
                    ("Summary", jv_s("记账两句"), ),
                    ("Blocks", JVal::I(2)),
                ]),
                "start-run-lock" => jv_o(vec![
                    ("Wrote", JVal::B(true)),
                    ("Pid", JVal::I(4242)),
                    ("Started", jv_s("2026-10-15 06:00:00")),
                    ("Games", jv_s("ark")),
                    ("Tag", jv_s("selftest")),
                    ("Log", jv_s("D:\\No\\Where\\logs\\round.log")),
                    ("PrevAlive", JVal::B(false)),
                    ("Lines", JVal::A(Vec::new())),
                ]),
                "stop-run-lock" => jv_o(vec![
                    ("Had", JVal::B(true)),
                    ("Removed", JVal::B(true)),
                    ("Pid", JVal::I(4242)),
                ]),
                "send-round-notice" => jv_o(vec![
                    ("Title", jv_s("每日日常已完成")),
                    ("Level", jv_s("OK")),
                    ("Time", jv_s("2026-10-15 06:20:00")),
                    ("File", JVal::B(true)),
                    ("FileKey", jv_s("state\\last-notice.json")),
                    ("Webhook", jv_s("none")),
                    ("Toast", jv_s("sent")),
                    ("Lines", JVal::A(Vec::new())),
                ]),
                _ => jv_o(vec![("Done", JVal::B(true))]),
            }
        }
        // 只记**键名**不记值：值里带 Path/Log 那种本机事实，落进投影就等于把"不该看的东西"抄进证据面。
        let r24_keys = |b: &JVal| -> String {
            match b {
                JVal::O(o) => {
                    let mut ks: Vec<&str> = o.iter().map(|(k, _)| k.as_str()).collect();
                    ks.sort_unstable();
                    ks.join(",")
                }
                JVal::Null => "(无参)".to_string(),
                other => jv_to_json(other),
            }
        };

        let d24 = std::env::temp_dir().join(format!("gd-b3b3-selftest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d24);
        let _ = std::fs::create_dir_all(&d24);
        let d24s = d24.to_string_lossy().to_string();
        let t24 = |n: &str| format!("{}\\{}", d24s, n);

        // ---------- 24a 取键名册：三枚 runtime 逐枚点名、elevFlags 去重、pathKeys 带两枚取名键 ----------
        chk(
            ROUND_RUNTIME_KEYS == ["blockSleep", "keepScreenOn", "retryOnce"],
            "runtimeKeys 这本名册改了形状：多一枚就是整块 runtime 往核里漏一格（visionApiKey 住在那个块里），少一枚就有一格现场参数没人取",
            &mut fails,
            &mut total,
        );
        chk(
            round_elev_flags() == vec!["arkRequiresElevation", "launchArkFromHelper"],
            "elevFlags 不对：两路清单同写一枚 arkRequiresElevation 必须**去重**（不去重就是把同一格算两遍），而 launchArkFromHelper 用的是同一把「缺键算开」的尺才被挪进这本名册",
            &mut fails,
            &mut total,
        );
        let pk24 = round_path_keys();
        chk(
            pk24.iter().filter(|k| **k == "maaExe").count() == 1
                && pk24.iter().filter(|k| **k == "m9aExe").count() == 1
                && pk24.contains(&"arknightsExe")
                && pk24.contains(&"maaConfig")
                && pk24.contains(&"m9aAppSettings")
                && pk24.contains(&"mumuManager")
                && !pk24.contains(&"visionApiKey"),
            "pathKeys 不对（两枚取名键没点到/重复，或 EnvProbes 那七枚少了一枚）：环境检查会少一行、takeover 的进程名会取空 —— 而整块 paths 绝不许进来",
            &mut fails,
            &mut total,
        );

        // ---------- 24b 静态清单：六枚 *_act 的字面与"这一路没那格"的空串 ----------
        chk(
            GD_MODULE_ARK.stop_act == "stop:ark"
                && GD_MODULE_ARK.resolve_act == "round-resolve:ark"
                && GD_MODULE_ARK.preview_act == "round-preview:ark"
                && GD_MODULE_ARK.gate_act == "round-gate:ark"
                && GD_MODULE_ARK.history_act == "round-history:ark"
                && GD_MODULE_ARK.probe_act == "round-probe:ark"
                && GD_MODULE_R1999.probe_act.is_empty()
                && GD_MODULE_R1999.env_rows.len() == 3
                && GD_VERBS_ARK.len() == 4
                && GD_VERBS_R1999.len() == 3,
            "清单数据面的动作名或枚数被改了：五枚 *_act 是 b3 新立的静态动作名（借用检查不许 format! 现造），而 r1999 的 probe_act 空串是「这一路根本没有活动图自证那一格」的显式读数，不是名字待定",
            &mut fails,
            &mut total,
        );

        // ---------- 24c 拒跑八闸：顺序即判据，第八闸独有 --root ----------
        let ok8 = ["j", "c", "r", "p", "s", "k", "t"];
        chk(
            round_refusal(false, ok8[0], ok8[1], ok8[2], ok8[3], ok8[4], ok8[5], ok8[6])
                .map_or(false, |m| m.contains("--i-round")),
            "轮骨架档缺 --i-round 时没落拒跑句（或那句没点名旗标）",
            &mut fails,
            &mut total,
        );
        let mut miss24: Vec<String> = Vec::new();
        for (i, blank) in [1usize, 2, 3, 4, 5, 6, 7].iter().enumerate() {
            let mut a = ok8.to_vec();
            a[*blank - 1] = "";
            let m = round_refusal(true, a[0], a[1], a[2], a[3], a[4], a[5], a[6]).unwrap_or_default();
            let want = ["--journal", "--ctx", "--res", "--panel-cs", "--state", "--log-sink", "--root"][i];
            if !m.contains(want) {
                miss24.push(format!("[{}→{}]", want, m));
            }
        }
        chk(
            miss24.is_empty() && round_refusal(true, ok8[0], ok8[1], ok8[2], ok8[3], ok8[4], ok8[5], ok8[6]).is_none(),
            &format!("轮骨架的八道闸有一句不点名所缺旗标（顺序即判据，第八闸 --root 是本档独有）：{}", miss24.join(" / ")),
            &mut fails,
            &mut total,
        );

        // ---------- 24d ctx 解析：pid 必填绝不回落、nameField 只认两档、gates 滤空项 ----------
        let bad_pid = jv_parse("{\"ids\":[\"ark\"]}").unwrap_or(JVal::Null);
        chk(
            matches!(round_ctx_parse(&bad_pid), Err(ref m) if m.contains("pid")),
            "ctx 少 pid 时没当场 Err（或 Err 那句没点名 pid）：撤锁只撤自己登记的那一份，核绝不许拿自己的进程号冒充",
            &mut fails,
            &mut total,
        );
        let neg_pid = jv_parse("{\"pid\":0,\"nameField\":\"ShortName\"}").unwrap_or(JVal::Null);
        chk(
            matches!(round_ctx_parse(&neg_pid), Err(ref m) if m.contains("正整数")),
            "pid=0 没被拦：驱动协议一 ask 一进程，那枚号在下一进程里根本不是本轮",
            &mut fails,
            &mut total,
        );
        let bad_field = jv_parse("{\"pid\":7,\"nameField\":\"Name\"}").unwrap_or(JVal::Null);
        chk(
            matches!(round_ctx_parse(&bad_field), Err(ref m) if m.contains("Id/ShortName")),
            "nameField 第三档没拒住：真身那枚 ValidateSet 就是两档，第三档会让汇总行换个字段名",
            &mut fails,
            &mut total,
        );
        let ok_ctx24 = jv_parse(
            "{\"pid\":4242,\"gates\":[null,{\"id\":\"ark\",\"verb\":\"ShopDone\"}],\"opt\":{},\"nameField\":\"Id\",\"tag\":\"selftest\",\"logLeaf\":\"round.log\",\"maaRun\":{},\"m9aRun\":{\"instance\":{\"id\":\"1\",\"name\":\"1999\"}}}",
        )
        .unwrap_or(JVal::Null);
        let rc24v = round_ctx_parse(&ok_ctx24).unwrap_or_else(|_| round_ctx_empty());
        chk(
            rc24v.gates.len() == 1 && rc24v.notice_owner == "每日" && rc24v.pid == 4242,
            "ctx 的默认值或那对 null 空项处理不对：真身 $Gates | Where-Object { $_ } 是静默滤，NoticeOwner 缺省是「每日」",
            &mut fails,
            &mut total,
        );

        // ---------- 24e 在位性现问：临时树上三档（全装 / 只装 ark / 什么都没装） ----------
        let root_all = t24("root-all");
        let root_ark = t24("root-ark");
        let root_none = t24("root-none");
        let trees: Vec<(String, Vec<&'static str>)> = vec![
            (
                root_all.clone(),
                vec!["ark-module.ps1", "r1999-module.ps1", "maa.ps1", "m9a.ps1", "ark-daily.ps1", "r1999-daily.ps1"],
            ),
            (root_ark.clone(), vec!["ark-module.ps1", "maa.ps1", "ark-daily.ps1"]),
            (root_none.clone(), vec![]),
        ];
        for (r, files) in trees.iter() {
            let _ = std::fs::create_dir_all(format!("{}\\lib", r));
            for f in files.iter() {
                let _ = std::fs::write(format!("{}\\lib\\{}", r, f), "# fake\n");
            }
        }
        let pick_all = select_gd_modules(&rc24v, &root_all);
        let pick_ark = select_gd_modules(&rc24v, &root_ark);
        let pick_none = select_gd_modules(&rc24v, &root_none);
        chk(
            pick_all.map_or(false, |v| v.iter().map(|m| m.id).collect::<Vec<_>>() == vec!["ark", "r1999"])
                && pick_ark.map_or(false, |v| v.len() == 1 && v[0].id == "ark")
                && pick_none.as_ref().err().map_or(false, |e| e.contains("没有可加载的游戏模块")),
            "选路的三档读数不对：注册（*-module.ps1）／装了（助手库）／合体（编排库枚数）三枚判据各是各的，合成一枚就把三种状态糊成一档",
            &mut fails,
            &mut total,
        );
        // 入口 verb：合体树上取长名（-AckArk），非合体树回落短名（-Ack）—— §11.50 ④-4 抓过的那一处。
        chk(
            round_entry_verb("ark", "Ack", &root_all) == "-AckArk"
                && round_entry_verb("ark", "ShopDone", &root_all) == "-ArkShopDone"
                && round_entry_verb("ark", "Ack", &root_ark) == "-Ack"
                && round_entry_verb("r1999", "Ack", &root_ark) == "-Ack",
            "entryverb 的直答不等于真身那两档了：合体判据在场时越过名册去回短名，或该回落时没回落",
            &mut fails,
            &mut total,
        );

        // 一份"六枚探测行全就绪"的 paths 夹具（自检不许要求机器现场状态：树是自己建的）。
        // 六枚 = 两路 EnvProbes 的全部行；m9aExe/maaExe 是取名用的相对原值，不进环境判据。
        let mk_paths = |tag: &str, maa_gone: bool| -> JVal {
            let base = format!("{}\\fx-{}", d24s, tag);
            let maa = if maa_gone { format!("{}\\no-such-dir", d24s) } else { format!("{}\\maa", base) };
            let game = format!("{}\\ark.exe", base);
            let m9a = format!("{}\\m9a", base);
            let mumu = format!("{}\\mumu", base);
            if !maa_gone {
                let _ = std::fs::create_dir_all(&maa);
                let _ = std::fs::create_dir_all(&m9a);
                let _ = std::fs::create_dir_all(&mumu);
                let _ = std::fs::write(format!("{}\\config.json", maa), "{}");
                let _ = std::fs::write(format!("{}\\MAA.exe", maa), "x");
                let _ = std::fs::write(&game, "x");
                let _ = std::fs::write(format!("{}\\appsettings.json", m9a), "{}");
                let _ = std::fs::write(format!("{}\\MuMuManager.exe", mumu), "x");
            }
            jv_o(vec![
                ("maaDir", jv_s(&maa)),
                ("maaConfig", jv_s("config.json")),
                ("maaExe", jv_s("MAA.exe")),
                ("arknightsExe", jv_s(&game)),
                ("m9aDir", jv_s(&m9a)),
                ("m9aAppSettings", jv_s("appsettings.json")),
                ("mumuDir", jv_s(&mumu)),
                ("mumuManager", jv_s("MuMuManager.exe")),
                ("m9aExe", jv_s("M9A.exe")),
            ])
        };
        let paths_ok24 = mk_paths("ok", false);
        let paths_bad24 = mk_paths("bad", true);

        // ---------- 24f 环境检查：就绪条数与"没填"那一档的行文本 ----------
        let mut env24 = match round_env_fetch(&mut |_v: &str, _b: &JVal| Ok(r24_reply("resolve-round-context", &paths_ok24))) {
            Ok(e) => e,
            Err(_) => RoundEnv { runtime: JVal::Null, elevation: JVal::Null, paths: JVal::Null, now_text: String::new() },
        };
        // 半就绪的一份 paths：ark 那两枚在位（MAA 目录／MAA 配置），端游没填、1999 那三路整块没填
        // ⇒ 就绪 2 条、缺失 4 行，且四行**全是** "(没填)" 那一档（不是"路径不存在"那一档）。
        let mut paths_part24 = paths_ok24.clone();
        if let JVal::O(ref mut o) = paths_part24 {
            for (k, v) in o.iter_mut() {
                if k == "arknightsExe" || k == "m9aDir" || k == "mumuDir" {
                    *v = jv_s("");
                }
            }
        }
        env24.paths = paths_part24;
        let mods24: Vec<&'static GdModule> = GD_MODULES.iter().collect();
        let (ok24, bad24) = round_env_check(&mods24, &env24);
        chk(
            ok24 == 2
                && bad24.len() == 4
                && bad24[0] == "方舟端游 → (没填)"
                && bad24[3] == "MuMu Manager → (没填)"
                && env24.proc_name("maaExe") == "MAA"
                && env24.proc_name("m9aExe") == "M9A"
                && env24.flag("blockSleep")
                && !env24.flag("keepScreenOn")
                && !env24.elev_on("arkRequiresElevation")
                && !env24.elev_on("no-such-flag"),
            &format!(
                "环境检查/elev_on 那把尺走样了（就绪条数、(没填) 那一档、或 Elevation 的折真值 —— 缺键按**开**算才是真身那把 -ne $false）：{}",
                bad24.join(" / ")
            ),
            &mut fails,
            &mut total,
        );

        // ---------- 24g 答手名册：逐枚点名「哪一格调哪枚 verb、取哪几枚键、落哪几格账」 ----------
        let mut proj24: Vec<String> = Vec::new();
        let mut slept24: Vec<u32> = Vec::new();
        let mut es24: Vec<u32> = Vec::new();
        let live_dec = round_ark_decision_slot();
        live_dec.replace(jv_o(vec![
            ("Mode", jv_s("farm")),
            ("ClearSanity", JVal::B(true)),
            ("Stage", jv_s("CE-5")),
            ("Farm", jv_o(vec![("Active", JVal::B(true)), ("ItemId", jv_s("30340")), ("Gap", JVal::I(3)), ("Target", JVal::I(12)), ("Times", JVal::I(2))])),
            ("FarmMode", jv_s("plan")),
            ("EventKey", jv_s("")),
            ("EventTip", jv_s("")),
            ("StageHint", jv_s("")),
        ]));
        let inst24 = jv_o(vec![("id", jv_s("1")), ("name", jv_s("1999"))]);
        let mut ask_none: Vec<String> = Vec::new();
        let mut data_of: Vec<String> = Vec::new();
        {
            let mut call24 = |v: &str, b: &JVal| -> Result<JVal, String> {
                proj24.push(format!("{}|{}", v, r24_keys(b)));
                Ok(r24_reply(v, &paths_ok24))
            };
            let mut nap24 = |s: u32| {
                slept24.push(s);
            };
            let mut es24c = |f: u32| -> u32 {
                es24.push(f);
                if f & ES_QUERY_ONLY != 0 {
                    ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
                } else {
                    1
                }
            };
            let mut hk24 = RoundHooks { call: &mut call24, nap: &mut nap24, es: &mut es24c };
            let mut one = |role: &str, action: &str, params: JVal| {
                round_answer(&mut hk24, &env24, &root_all, &t24("state"), &inst24, &live_dec, role, action, &params)
            };
            for (role, action, params) in [
                ("ark", "desk", JVal::Null),
                ("ark", "takeover", JVal::Null),
                ("ark", "cfg", jv_o(vec![("kind", jv_s("main"))])),
                ("ark", "cfg", jv_o(vec![("kind", jv_s("lean")), ("stage", jv_s("1-7"))])),
                ("ark", "entryverb", jv_o(vec![("game", jv_s("ark")), ("verb", jv_s("Ack"))])),
                ("ark", "launch", JVal::Null),
                ("ark", "stop", JVal::Null),
                ("ark", "sleep", JVal::I(25)),
                ("ark", "skip", JVal::Null),
                ("ark", "ledger", JVal::Null),
                ("ark", "obs", JVal::Null),
                ("r1999", "takeover", JVal::Null),
                ("r1999", "mumu", JVal::Null),
                ("r1999", "cfg", JVal::Null),
                ("r1999", "stop", JVal::Null),
                ("r1999", "sleep", JVal::I(20)),
                ("round", ROUND_ADMIN_ACT, JVal::Null),
                ("round", ROUND_LOCK_ACT, jv_o(vec![("pid", JVal::I(4242)), ("games", jv_s("ark")), ("tag", jv_s("selftest")), ("log", jv_s("logs\\x.log"))])),
                ("round", ROUND_UNLOCK_ACT, jv_o(vec![("pid", JVal::I(4242))])),
                (
                    "round",
                    ROUND_NOTICE_ACT,
                    jv_o(vec![("owner", jv_s("每日")), ("allOk", JVal::B(true)), ("text", jv_s("两行"), ), ("level", jv_s("OK"))]),
                ),
                ("round", ROUND_NOSLEEP_ACT, jv_o(vec![("keepScreenOn", JVal::B(true))])),
                ("round", ROUND_NOSLEEP_OFF_ACT, JVal::Null),
            ]
            .iter()
            {
                match one(role, action, params.clone()) {
                    Ok(Some(o)) => data_of.push(format!("{}:{}:{}", role, action, jv_to_json(&o.data))),
                    Ok(None) => ask_none.push(format!("{}:{}", role, action)),
                    Err(m) => ask_none.push(format!("{}:{}→Err({})", role, action, m)),
                }
            }
            // 外发那一族逐枚点名（答 None = 协议没删）：门表/判定/预览/收尾/监督/历史/提权/屏保 + svdetail + updgate/audit。
            for (role, action, params) in [
                ("round", "round-gate:ark", JVal::Null),
                ("round", "round-resolve:ark", JVal::Null),
                ("round", "round-preview:ark", JVal::Null),
                ("round", "stop:ark", JVal::Null),
                ("round", ROUND_SUPERVISOR_START_ACT, JVal::Null),
                ("round", ROUND_SUPERVISOR_STOP_ACT, JVal::Null),
                ("round", "round-history:ark", JVal::Null),
                ("round", ROUND_ELEVATE_ACT, JVal::Null),
                ("round", ROUND_SCREENSAVER_ACT, JVal::Null),
                ("round", "round-probe:ark", JVal::Null),
                ("ark", "svdetail", JVal::Null),
                ("r1999", "updgate", JVal::Null),
                ("r1999", "audit", JVal::Null),
            ]
            .iter()
            {
                if one(role, action, params.clone()).map_or(true, |x| x.is_some()) {
                    ask_none.push(format!("{}:{} 本该外发", role, action));
                }
            }
        }
        chk(
            ask_none.is_empty(),
            &format!("外发那一族被答手接下了（ask/res 是回退通道，②-b3 只「增」答手不「换」协议）：{}", ask_none.join(" / ")),
            &mut fails,
            &mut total,
        );
        chk(
            data_of.iter().any(|l| l.as_str() == "ark:desk:\"Default\"")
                && data_of.iter().any(|l| l == "ark:takeover:true")
                && data_of.iter().any(|l| l == "ark:skip:null")
                && data_of.iter().any(|l| l == "ark:obs:null")
                && data_of.iter().any(|l| l == "ark:entryverb:\"-AckArk\"")
                && data_of.iter().any(|l| l == "ark:launch:true")
                && data_of.iter().any(|l| l == "ark:ledger:\"记账两句\"")
                && data_of.iter().any(|l| l == "r1999:mumu:true")
                && data_of.iter().any(|l| l.starts_with("ark:cfg:{\"changes\":[\"队列：作战 开\"],\"chains\":[\"combat\"]}"))
                && data_of.iter().any(|l| l.starts_with("r1999:cfg:{\"changes\":"))
                && data_of.iter().any(|l| l.starts_with("round:round-lock:{\"Wrote\":true,\"PrevAlive\":false}"))
                && data_of.iter().any(|l| l.starts_with("round:round-unlock:{\"Had\":true,\"Removed\":true}"))
                && data_of.iter().any(|l| l.starts_with("round:round-notice:{\"Title\":\"每日日常已完成\"")),
            &format!(
                "直答那一族的账行形状变了（PS executor 当年写进 res 的 data 是这个形状，改了②-b 的门就没法逐字节比）：{}",
                data_of.join(" § ")
            ),
            &mut fails,
            &mut total,
        );
        chk(
            proj24.iter().filter(|p| p.starts_with("ark-desktop-state|")).count() == 1
                && proj24.iter().any(|p| p == "ark-desktop-state|")
                && proj24.iter().any(|p| p.starts_with("takeover-processes|label,names") && p.matches('|').count() == 1)
                && proj24.iter().any(|p| p == "apply-maa-daily-config|apply,decision")
                && proj24.iter().any(|p| p == "update-ark-farm-ledger|stateDir")
                && proj24.iter().any(|p| p == "read-maa-stage-observation|nowText")
                && proj24.iter().any(|p| p == "apply-m9a-instance|apply,instance")
                && proj24.iter().any(|p| p == "start-run-lock|games,log,pid,tag")
                && proj24.iter().any(|p| p == "send-round-notice|allOk,level,owner,text"),
            &format!(
                "答手喂给 verb 的 body 键名册变了（逐枚点名取键是②的判据，整块 runtime/paths 不许进去）：{}",
                proj24.join(" § ")
            ),
            &mut fails,
            &mut total,
        );
        // lean 那一腿：PS 现场另构造八键哈希、**没有 Farm 键** ⇒ verb 侧 farmActive=false（原样怪癖，不许"顺手带上 Farm"）。
        chk(
            round_lean_decision(&jv_o(vec![("Mode", jv_s("farm")), ("Farm", JVal::B(true)), ("ClearSanity", JVal::B(true))]), "1-7")
                == jv_o(vec![
                    ("Source", jv_s("auto")),
                    ("Mode", jv_s("farm")),
                    ("Stage", jv_s("1-7")),
                    ("ClearSanity", JVal::B(true)),
                    ("EventKey", JVal::Null),
                    ("EventTip", JVal::Null),
                    ("StageHint", JVal::Null),
                ]),
            "lean 那腿的 decision 不是真身那八键：Farm 漏了没剥（剔关轮就带着一份它本来没有的补刷档位），或键序/来源改了",
            &mut fails,
            &mut total,
        );
        chk(
            slept24 == vec![25u32, 20u32]
                && es24.len() == 3
                && es24[0] == (ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
                && es24[1] == (ES_CONTINUOUS | ES_QUERY_ONLY)
                && es24[2] == ES_CONTINUOUS,
            &format!(
                "核内那两枚现场件走错了（sleep 没走注入点=自检会真睡，或 ES 位型/自证顺序不对）：slept={:?} es={:?}",
                slept24, es24
            ),
            &mut fails,
            &mut total,
        );

        // ---------- 24h 一轮十段跑到底：账行序、ask 序、done 的那枚退出码 ----------
        let root24 = root_all.clone();
        let s24 = t24("state");
        let _ = std::fs::create_dir_all(&s24);
        // 路径要按 JSON 转义再进 ctx：这棵树在 %TEMP% 下，反斜杠不 doubling 就是一枚非法转义，
        // 而 ctx 读不动的错话术会指着"不是合法 JSON"——那是自己给自己造的假红，不是协议的错。
        let jesc = |s: &str| s.replace("\\", "\\\\");
        let d24j = jesc(&d24s);
        let s24j = jesc(&s24);
        let j24 = t24("journal.ndjson");
        let r24 = t24("res.json");
        let k24 = t24("sink.ndjson");
        let c24 = t24("ctx.json");
        let _ = std::fs::write(
            &c24,
            &format!(
                "{{\"ids\":[\"ark\"],\"gates\":[],\"opt\":{{}},\"force\":false,\"dryRun\":false,\"skipCleanup\":false,\"noElevate\":false,\"nameField\":\"Id\",\"noticeOwner\":\"每日\",\"tag\":\"selftest\",\"logLeaf\":\"round.log\",\"logPath\":\"logs\\\\round.log\",\"pid\":4242,\"maaRun\":{{\"maaDir\":\"{}\\\\maa\",\"maaExe\":\"MAA.exe\",\"guiLog\":\"\",\"asstLog\":\"\",\"stateDir\":\"{}\",\"maxTicks\":2,\"tickSeconds\":1,\"startTimeoutMinutes\":1,\"timeoutMinutes\":1,\"stallTimeoutMinutes\":1,\"retryOnce\":true,\"launchArkFromHelper\":true,\"closeGameAfter\":false,\"rescueAvailable\":true,\"grabFrontAvailable\":false}},\"m9aRun\":{{\"paths\":{{\"m9aDir\":\"\",\"m9aExe\":\"\",\"m9aLogDir\":\"\"}},\"conn\":{{\"adb\":\"\",\"serial\":\"\"}},\"instance\":{{\"id\":\"1\",\"name\":\"1999\"}},\"package\":\"\",\"runtime\":{{\"m9aInstanceArg\":\"\",\"m9aForceStart\":false,\"visionMaxRescue\":0,\"r1999StartTimeoutMinutes\":1,\"r1999TimeoutMinutes\":1,\"stallTimeoutMinutes\":1}}}}}}",
                d24j, s24j
            ),
        );
        let mut rounds24 = 0usize;
        let mut kinds24: Vec<String> = Vec::new();
        let mut acts24: Vec<String> = Vec::new();
        let mut final_env24 = String::new();
        let mut final_rc24 = 9i32;
        for _ in 0..24 {
            rounds24 += 1;
            let pair = {
                let mut call24 = |_v: &str, _b: &JVal| -> Result<JVal, String> { Ok(r24_reply(_v, &paths_ok24)) };
                let mut nap24 = |_s: u32| {};
                let mut es24c = |f: u32| -> u32 {
                    if f & ES_QUERY_ONLY != 0 {
                        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
                    } else {
                        1
                    }
                };
                let mut hk24 = RoundHooks { call: &mut call24, nap: &mut nap24, es: &mut es24c };
                let mut dec = |_st: &str, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
                    Ok((false, Vec::new()))
                };
                let mut dec9 = |_st: &str, _rs: bool, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
                    Ok((false, Vec::new()))
                };
                let mut run = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
                    Ok(ArkRunOut { status: "ok".to_string(), detail: "队列跑完".to_string(), duration: Some(120.0) })
                };
                let mut run9 = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
                    Ok(M9aRunOut {
                        status: "ok".to_string(),
                        detail: "跑完".to_string(),
                        duration: Some(60.0),
                        rescued: false,
                    })
                };
                drive_round_daily_envelope(
                    true, &j24, &c24, &r24, "unused-panel-cs", &s24, &k24, &root24, &mut hk24, &mut dec, &mut dec9,
                    &mut run, &mut run9,
                )
            };
            final_rc24 = pair.0;
            final_env24 = pair.1;
            let ev = jv_parse(&final_env24).unwrap_or(JVal::Null);
            let kind = jv_str(&jv_get(&ev, "kind"));
            kinds24.push(kind.clone());
            if kind != "ask" {
                break;
            }
            let seq = jv_int(&jv_get(&ev, "seq"), 0);
            let act = jv_str(&jv_get(&ev, "action"));
            acts24.push(act.clone());
            // 假 executor：照 ask 的 seq/action 写回执（外发那一族的 data 形状按 §11.52 ② 的名册给）。
            let data = match act.as_str() {
                a if a == "svdetail" => jv_s("屏保：没挂 .scr"),
                a if a == ROUND_ELEVATE_ACT => jv_o(vec![("Code", JVal::I(0))]),
                a if a.starts_with("round-resolve:") => jv_o(vec![
                    ("Mode", jv_s("farm")),
                    ("ClearSanity", JVal::B(true)),
                    ("Stage", jv_s("CE-5")),
                    ("Farm", JVal::Null),
                    ("FarmMode", jv_s("")),
                ]),
                a if a == ROUND_SUPERVISOR_START_ACT || a == ROUND_SUPERVISOR_STOP_ACT => {
                    jv_o(vec![("Available", JVal::B(true)), ("Err", jv_s(""))])
                }
                _ => jv_o(vec![("Done", JVal::B(true))]),
            };
            let rf = format!("{{\"seq\":{},\"action\":{},\"ok\":true,\"data\":{}}}", seq, jstr(&act), jv_to_json(&data));
            let _ = std::fs::write(&r24, rf);
        }
        let jrows: Vec<String> = std::fs::read_to_string(&j24)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.contains("\"init\""))
            .map(|l| l.to_string())
            .collect();
        let sink24: Vec<String> = std::fs::read_to_string(&k24)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect();
        chk(
            final_rc24 == 0
                && rounds24 == acts24.len() + 1
                && kinds24.last().map(|s| s.as_str()) == Some("done")
                && final_env24.contains("\"role\":\"B3b-round-daily\"")
                && final_env24.contains("\"code\":0")
                && final_env24.contains("\"allOk\":true"),
            &format!(
                "一轮十段没跑到 done（或 done 信封里没带本轮那枚退出码）：rc={} kinds={:?} env={}",
                final_rc24,
                kinds24,
                final_env24.chars().take(900).collect::<String>()
            ),
            &mut fails,
            &mut total,
        );
        // ⚠ 名单里**没有 svdetail**：这一档走的是假 verb 回 `Desktop="Default"` 那一路（桌面正常），
        //   而链体那一格只在"桌面不对、要把屏保读数写进早退原因"时才发生 —— 缺席是**真身的条件**，
        //   不是漏了一格。同理 keepScreenOn=false ⇒ 屏保那格也没问。外发那一族的**完整名册**由 24g
        //   那一段逐枚钉（十三枚全问了一遍），这一段只钉"段与段之间的先后"，两格各管一段。
        chk(
            acts24 == vec![
                "round-resolve:ark".to_string(),
                "round-supervisor-start".to_string(),
                "round-probe:ark".to_string(),
                "round-supervisor-stop".to_string(),
                "stop:ark".to_string(),
                "round-history:ark".to_string(),
            ],
            &format!("外发那一族的**顺序或枚数**变了（骨架的段序就是判据：锁/判定在前、监督先停再收尾、历史排在通知之前）：{:?}", acts24),
            &mut fails,
            &mut total,
        );
        chk(
            jrows.iter().any(|l| l.contains("\"action\":\"round-lock\"") && l.contains("\"Wrote\":true") && !l.contains("logs\\\\round.log"))
                && jrows.iter().any(|l| l.contains("\"action\":\"round-notice\"") && l.contains("每日日常已完成"))
                && jrows.iter().any(|l| l.contains("\"action\":\"round-nosleep\""))
                && jrows.iter().any(|l| l.contains("\"action\":\"round-nosleep-off\""))
                && jrows.iter().any(|l| l.contains("\"action\":\"desk\"") && l.contains("\"data\":\"Default\""))
                && jrows.iter().any(|l| l.contains("\"action\":\"skip\"") && l.contains("\"data\":null"))
                && jrows.iter().any(|l| l.contains("\"action\":\"run\"") && l.contains("队列跑完"))
                && jrows.iter().any(|l| l.contains("\"done\"") && l.contains("\"code\":0")),
            &format!(
                "账面不对：锁那格把本机路径带进了账（直答只留两枚布尔才对）、或十段的直答格没落账、done 没进末行：共 {} 行",
                jrows.len()
            ),
            &mut fails,
            &mut total,
        );
        chk(
            sink24.iter().any(|l| l.contains("环境检查：") && l.contains("项就绪"))
                && sink24.iter().any(|l| l.contains("已临时禁止系统睡眠"))
                && sink24.iter().any(|l| l.contains("=== 本轮结束：全部完成 ==="))
                && sink24.iter().any(|l| l.contains("接管那一句"))
                && sink24.iter().filter(|l| l.contains("接管那一句")).count() == 1,
            &format!(
                "侧信道面不对：十段的日志没按产生顺序逐行落这里，或直答件带回来的 Lines 被补了两遍（重放段一行不写）：{} 行",
                sink24.len()
            ),
            &mut fails,
            &mut total,
        );
        // 重放段的第二道自证：**同一本 journal 再起一轮**，一格副作用都不许重做（Lines 不再补、账不再加行）。
        let before24 = jrows.len();
        let sink_before24 = sink24.len();
        let pair2 = {
            let mut call24 = |_v: &str, _b: &JVal| -> Result<JVal, String> { Ok(r24_reply(_v, &paths_ok24)) };
            let mut nap24 = |_s: u32| {};
            let mut es24c = |_f: u32| -> u32 { 1 };
            let mut hk2 = RoundHooks { call: &mut call24, nap: &mut nap24, es: &mut es24c };
            let mut dec = |_st: &str, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> { Ok((false, Vec::new())) };
            let mut dec9 = |_st: &str, _rs: bool, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
                Ok((false, Vec::new()))
            };
            let mut run = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
                Ok(ArkRunOut { status: "ok".to_string(), detail: "不该再跑".to_string(), duration: Some(120.0) })
            };
            let mut run9 = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
                Ok(M9aRunOut { status: "ok".to_string(), detail: "不该再跑".to_string(), duration: None, rescued: false })
            };
            drive_round_daily_envelope(
                true, &j24, &c24, &r24, "unused-panel-cs", &s24, &k24, &root24, &mut hk2, &mut dec, &mut dec9, &mut run,
                &mut run9,
            )
        };
        let jrows_after: Vec<String> = std::fs::read_to_string(&j24)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.contains("\"init\""))
            .map(|l| l.to_string())
            .collect();
        let sink_after24 = std::fs::read_to_string(&k24).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()).count();
        chk(
            pair2.0 == 0
                && pair2.1.contains("\"kind\":\"done\"")
                && jrows_after.len() == before24
                && sink_after24 == sink_before24,
            "账本已 done 时再起一轮：必须是幂等 done（不重放、不加行、侧信道一行不补）——这一条是回退通道的地基",
            &mut fails,
            &mut total,
        );

        // ---------- 24i 早退那两档：环境不完整退 2、dry-run 退 0 ----------
        for (scen, ids_json, want_code, want_done) in [
            ("env-bad", "{\"ids\":[\"ark\"]}", 2i64, 2i64),
            ("dry-run", "{\"ids\":[\"ark\"],\"dryRun\":true}", 0i64, 0i64),
        ]
        .iter()
        {
            let jj = t24(&format!("j-{}.ndjson", scen));
            let rr = t24(&format!("r-{}.json", scen));
            let kk = t24(&format!("k-{}.ndjson", scen));
            let cc = t24(&format!("c-{}.json", scen));
            let paths_block = if *scen == "env-bad" { paths_bad24.clone() } else { paths_ok24.clone() };
            let _ = std::fs::write(
                &cc,
                &format!(
                    "{{\"ids\":[\"ark\"],\"gates\":[],\"opt\":{{}},\"force\":false,\"dryRun\":{},\"skipCleanup\":false,\"noElevate\":false,\"nameField\":\"Id\",\"noticeOwner\":\"每日\",\"tag\":\"{}\",\"logLeaf\":\"\",\"logPath\":\"\",\"pid\":4242,\"maaRun\":{{\"maaDir\":\"{}\\\\maa\",\"maaExe\":\"MAA.exe\",\"guiLog\":\"\",\"asstLog\":\"\",\"stateDir\":\"{}\",\"maxTicks\":2,\"tickSeconds\":1,\"startTimeoutMinutes\":1,\"timeoutMinutes\":1,\"stallTimeoutMinutes\":1,\"retryOnce\":true,\"launchArkFromHelper\":true,\"closeGameAfter\":false,\"rescueAvailable\":true,\"grabFrontAvailable\":false}},\"m9aRun\":{{\"paths\":{{\"m9aDir\":\"\",\"m9aExe\":\"\",\"m9aLogDir\":\"\"}},\"conn\":{{\"adb\":\"\",\"serial\":\"\"}},\"instance\":{{\"id\":\"1\",\"name\":\"1999\"}},\"package\":\"\",\"runtime\":{{\"m9aInstanceArg\":\"\",\"m9aForceStart\":false,\"visionMaxRescue\":0,\"r1999StartTimeoutMinutes\":1,\"r1999TimeoutMinutes\":1,\"stallTimeoutMinutes\":1}}}}}}",
                    if *scen == "dry-run" { "true" } else { "false" },
                    scen,
                    d24j,
                    s24j
                ),
            );
            let mut out_pair = (9i32, String::new());
            for _ in 0..6 {
                out_pair = {
                    let mut call24 = |v: &str, _b: &JVal| -> Result<JVal, String> {
                        if v == "resolve-round-context" {
                            let mut d = r24_reply(v, &paths_block);
                            if let JVal::O(ref mut o) = d {
                                for (k, val) in o.iter_mut() {
                                    if k == "Paths" {
                                        *val = paths_block.clone();
                                    }
                                }
                            }
                            return Ok(d);
                        }
                        Ok(r24_reply(v, &paths_ok24))
                    };
                    let mut nap24 = |_s: u32| {};
                    let mut es24c = |_f: u32| -> u32 { 1 };
                    let mut hk2 = RoundHooks { call: &mut call24, nap: &mut nap24, es: &mut es24c };
                    let mut dec = |_st: &str, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
                        Ok((false, Vec::new()))
                    };
                    let mut dec9 = |_st: &str, _rs: bool, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
                        Ok((false, Vec::new()))
                    };
                    let mut run = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
                        Ok(ArkRunOut { status: "ok".to_string(), detail: "dry-run 不该真做".to_string(), duration: None })
                    };
                    let mut run9 = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
                        Ok(M9aRunOut { status: "ok".to_string(), detail: "x".to_string(), duration: None, rescued: false })
                    };
                    drive_round_daily_envelope(
                        true, &jj, &cc, &rr, "unused-panel-cs", &s24, &kk, &root_all, &mut hk2, &mut dec, &mut dec9,
                        &mut run, &mut run9,
                    )
                };
                let ev = jv_parse(&out_pair.1).unwrap_or(JVal::Null);
                if jv_str(&jv_get(&ev, "kind")) != "ask" {
                    break;
                }
                let seq = jv_int(&jv_get(&ev, "seq"), 0);
                let act = jv_str(&jv_get(&ev, "action"));
                let data = match act.as_str() {
                    a if a.starts_with("round-resolve:") => jv_o(vec![("Mode", jv_s("farm")), ("Stage", jv_s("CE-5"))]),
                    _ => jv_o(vec![("Done", JVal::B(true))]),
                };
                let _ = std::fs::write(
                    &rr,
                    &format!("{{\"seq\":{},\"action\":{},\"ok\":true,\"data\":{}}}", seq, jstr(&act), jv_to_json(&data)),
                );
            }
            let _ = ids_json;
            let rowsx: Vec<String> = std::fs::read_to_string(&jj)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect();
            let code_x = jv_int(
                &jv_get_ci(&jv_get(&jv_parse(&out_pair.1).unwrap_or(JVal::Null), "result"), "code"),
                -1,
            );
            chk(
                out_pair.0 == 0
                    && code_x == *want_code
                    && rowsx.iter().any(|l| l.contains("\"done\""))
                    && (if *scen == "env-bad" {
                        rowsx.iter().any(|l| l.contains("环境不完整")) && !rowsx.iter().any(|l| l.contains("round-lock"))
                    } else {
                        rowsx.iter().any(|l| l.contains("round-preview:ark"))
                            && !rowsx.iter().any(|l| l.contains("round-lock"))
                    }),
                &format!(
                    "{} 那一档不对：环境不完整要退 2 且不登记活体、dry-run 要预览完就退 0 且不登记活体（真身把这两档排在拿锁之前是刻意的）：rc={} code={} rows={}",
                    scen,
                    out_pair.0,
                    code_x,
                    rowsx.len()
                ),
                &mut fails,
                &mut total,
            );
            let _ = want_done;
        }

        // ---------- 24j 话术护栏：role 第三枚在场、两枚现出口的 why 一字未动 ----------
        chk(
            DRIVE_ROLE_ROUND.role == "B3b-round-daily"
                && DRIVE_WHY_ROUND.contains("轮骨架档是换头专用口")
                && DRIVE_WHY_ROUND.contains("缺 --i-round 直接拒跑")
                && !DRIVE_WHY_ROUND.contains("全自动")
                && !DRIVE_WHY_ROUND.contains("完全不经 PS")
                && DRIVE_ROLE_ARK.why == DRIVE_WHY
                && DRIVE_ROLE_M9A.why == DRIVE_WHY_M9A,
            "第三句 why 越界了（吹成全自动化、或把两枚现出口的 why 顺手动过）：②-d 改 GDLauncher 那两句话术时按这本账走",
            &mut fails,
            &mut total,
        );
        // 名册一处来源自证：直答表的臂数（23）+ 外发那一族不进表 —— 这两句是**形状**判据，不是话术。
        let src_round: &str = SRC.split("fn round_answer(").nth(1).unwrap_or("");
        let src_round: &str = src_round.split("\n}\n\n").next().unwrap_or(src_round);
        chk(
            src_round.matches("=> {").count() + src_round.matches("=> DirectOut").count() + src_round.matches("(\"round\", ROUND_ADMIN_ACT) => {").count() >= 20
                && src_round.contains("_ => return Ok(None)"),
            "答手那张表少了兜底那一臂（Ok(None) = 照旧外发），或臂数掉到 20 以下：名册一散成两处，「谁来答」就没了唯一事实源",
            &mut fails,
            &mut total,
        );

        // ---------- 24k 分钟那一格的渲染（真身 "$([math]::Round($r.Duration / 60, 1)) 分钟"）----------
        // 期望值全部来自现测（pwsh 7.6.6 逐枚跑出来的十七行），一枚都不许按"看起来应该"填：
        // **向偶数舍**（135s→2.2 不是 2.3、75s→1.2、45s→0.8）与 **double 最短表示**（60s→「1 分钟」、
        // 1s→「0 分钟」、3600s→「60 分钟」）是两把独立的尺，`{:.1}` 只中第二把、`f64::round` 只中第一把。
        let mins24k: [(f64, &str); 17] = [
            (60.0, " 1 分钟"),
            (90.0, " 1.5 分钟"),
            (120.0, " 2 分钟"),
            (133.0, " 2.2 分钟"),
            (135.0, " 2.2 分钟"),
            (141.0, " 2.4 分钟"),
            (150.0, " 2.5 分钟"),
            (270.0, " 4.5 分钟"),
            (900.0, " 15 分钟"),
            (1080.0, " 18 分钟"),
            (3600.0, " 60 分钟"),
            (3665.0, " 61.1 分钟"),
            (59.0, " 1 分钟"),
            (75.0, " 1.2 分钟"),
            (45.0, " 0.8 分钟"),
            (8.0, " 0.1 分钟"),
            (1.0, " 0 分钟"),
        ];
        let mut bad24k: Vec<String> = Vec::new();
        for (sec24k, want24k) in mins24k.iter() {
            let got24k = round_minutes_display(*sec24k);
            if got24k != *want24k {
                bad24k.push(format!("{}s→[{}]应为[{}]", sec24k, got24k, want24k));
            }
        }
        chk(
            bad24k.is_empty(),
            &format!(
                "汇总/通知里分钟那一格与真身分岔（舍入档或数字表示有一处没复刻）：{}",
                bad24k.join(" / ")
            ),
            &mut fails,
            &mut total,
        );

        // ---------- 24l 段 7／段 8 的三档现场形状（守卫档一句不吭、屏保崩只 WARN 不崩本轮）----------
        // 这三档都在**外发**那一族，故走 ask/res：假 executor 只改被点名那一格的回执，其余照 24h 的册子答。
        // blockSleep=true + keepScreenOn=true 是把段 8 整段请出场的唯一办法（`es` 由假钩子接，绝不碰真 API）。
        for (scen24l, ask_act, ask_ok, ask_available) in [
            ("sv-missing", "", true, false),
            ("sv-error", "", true, true),
            ("screen-fail", ROUND_SCREENSAVER_ACT, false, true),
        ]
        .iter()
        {
            let jl = t24(&format!("j-{}.ndjson", scen24l));
            let rl = t24(&format!("r-{}.json", scen24l));
            let kl = t24(&format!("k-{}.ndjson", scen24l));
            let cl = t24(&format!("c-{}.json", scen24l));
            let _ = std::fs::write(
                &cl,
                &format!(
                    "{{\"ids\":[\"ark\"],\"gates\":[],\"opt\":{{}},\"force\":false,\"dryRun\":false,\"skipCleanup\":false,\"noElevate\":false,\"nameField\":\"Id\",\"noticeOwner\":\"每日\",\"tag\":\"{}\",\"logLeaf\":\"\",\"logPath\":\"\",\"pid\":4242,\"maaRun\":{{\"maaDir\":\"{}\\\\maa\",\"maaExe\":\"MAA.exe\",\"guiLog\":\"\",\"asstLog\":\"\",\"stateDir\":\"{}\",\"maxTicks\":2,\"tickSeconds\":1,\"startTimeoutMinutes\":1,\"timeoutMinutes\":1,\"stallTimeoutMinutes\":1,\"retryOnce\":false,\"launchArkFromHelper\":true,\"closeGameAfter\":false,\"rescueAvailable\":false,\"grabFrontAvailable\":false}},\"m9aRun\":{{\"paths\":{{\"m9aDir\":\"\",\"m9aExe\":\"\",\"m9aLogDir\":\"\"}},\"conn\":{{\"adb\":\"\",\"serial\":\"\"}},\"instance\":{{\"id\":\"1\",\"name\":\"1999\"}},\"package\":\"\",\"runtime\":{{\"m9aInstanceArg\":\"\",\"m9aForceStart\":false,\"visionMaxRescue\":0,\"r1999StartTimeoutMinutes\":1,\"r1999TimeoutMinutes\":1,\"stallTimeoutMinutes\":1}}}}}}",
                    scen24l, d24j, s24j
                ),
            );
            let mut done_l = (9i32, String::new());
            for _ in 0..24 {
                done_l = {
                    let mut call_l = |v: &str, _b: &JVal| -> Result<JVal, String> {
                        if v == "resolve-round-context" {
                            let mut d = r24_reply(v, &paths_ok24);
                            if let JVal::O(ref mut o) = d {
                                for (k, val) in o.iter_mut() {
                                    if k == "Runtime" {
                                        *val = jv_o(vec![
                                            ("blockSleep", JVal::B(true)),
                                            ("keepScreenOn", JVal::B(true)),
                                            ("retryOnce", JVal::B(false)),
                                        ]);
                                    }
                                }
                            }
                            return Ok(d);
                        }
                        Ok(r24_reply(v, &paths_ok24))
                    };
                    let mut nap_l = |_s: u32| {};
                    let mut es_l = |f: u32| -> u32 {
                        if f & ES_QUERY_ONLY != 0 {
                            ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
                        } else {
                            1
                        }
                    };
                    let mut hk_l = RoundHooks { call: &mut call_l, nap: &mut nap_l, es: &mut es_l };
                    let mut dec_l = |_st: &str, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
                        Ok((false, Vec::new()))
                    };
                    let mut dec9_l = |_st: &str, _rs: bool, _ro: bool| -> Result<(bool, Vec<(String, String)>), String> {
                        Ok((false, Vec::new()))
                    };
                    let mut run_l = |_sc: &MaaScen, _ch: &[String]| -> Result<ArkRunOut, String> {
                        Ok(ArkRunOut {
                            status: "ok".to_string(),
                            detail: "夹具一轮".to_string(),
                            duration: Some(120.0),
                        })
                    };
                    let mut run9_l = |_sc: &M9aScen| -> Result<M9aRunOut, String> {
                        Ok(M9aRunOut {
                            status: "ok".to_string(),
                            detail: "x".to_string(),
                            duration: Some(60.0),
                            rescued: false,
                        })
                    };
                    drive_round_daily_envelope(
                        true, &jl, &cl, &rl, "unused-panel-cs", &s24, &kl, &root_all, &mut hk_l, &mut dec_l,
                        &mut dec9_l, &mut run_l, &mut run9_l,
                    )
                };
                let evl = jv_parse(&done_l.1).unwrap_or(JVal::Null);
                if jv_str(&jv_get(&evl, "kind")) != "ask" {
                    break;
                }
                let seq_l = jv_int(&jv_get(&evl, "seq"), 0);
                let act_l = jv_str(&jv_get(&evl, "action"));
                let is_sup = act_l == ROUND_SUPERVISOR_START_ACT || act_l == ROUND_SUPERVISOR_STOP_ACT;
                let is_target = !ask_act.is_empty() && act_l == *ask_act;
                let rf = if is_target && !ask_ok {
                    format!(
                        "{{\"seq\":{},\"action\":{},\"ok\":false,\"err\":{}}}",
                        seq_l,
                        jstr(&act_l),
                        jstr("屏保读数拿不到（夹具）")
                    )
                } else if is_sup {
                    format!(
                        "{{\"seq\":{},\"action\":{},\"ok\":true,\"data\":{{\"Available\":{},\"Err\":{}}}}}",
                        seq_l,
                        jstr(&act_l),
                        if *ask_available { "true" } else { "false" },
                        jstr(if *ask_available { "起挂了（夹具）" } else { "这棵树上没有 Start-GdSupervisor" })
                    )
                } else {
                    format!("{{\"seq\":{},\"action\":{},\"ok\":true,\"data\":{{\"Done\":true}}}}", seq_l, jstr(&act_l))
                };
                let _ = std::fs::write(&rl, rf);
            }
            let sinkl: Vec<String> = std::fs::read_to_string(&kl)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect();
            let reached_done = jv_str(&jv_get(&jv_parse(&done_l.1).unwrap_or(JVal::Null), "kind")) == "done";
            if *scen24l == "sv-missing" {
                chk(
                    reached_done
                        && !sinkl.iter().any(|l| l.contains("监督 agent 没起来"))
                        && !sinkl.iter().any(|l| l.contains("监督 agent 没停干净")),
                    &format!(
                        "回执里 Available=false 那一档没接住（真身是 Get-Command 守卫不成立 ⇒ 整段一句不吭，骨架不许替旁观层决定要不要监督）：done={} 侧行={:?}",
                        reached_done,
                        sinkl.iter().filter(|l| l.contains("监督")).collect::<Vec<_>>()
                    ),
                    &mut fails,
                    &mut total,
                );
            } else if *scen24l == "sv-error" {
                chk(
                    reached_done
                        && sinkl.iter().any(|l| l.contains("监督 agent 没起来：起挂了（夹具）(这一轮没人监督，主流程不受影响)"))
                        && sinkl.iter().any(|l| l.contains("监督 agent 没停干净：起挂了（夹具）")),
                    &format!(
                        "命令在场但起挂/停不干净那一档话术走样（这两句必须逐字，且都挡不住本轮）：done={} 侧行={:?}",
                        reached_done,
                        sinkl.iter().filter(|l| l.contains("监督")).collect::<Vec<_>>()
                    ),
                    &mut fails,
                    &mut total,
                );
            } else {
                chk(
                    reached_done
                        && done_l.0 == 0
                        && sinkl.iter().any(|l| l.contains("禁止睡眠失败：屏保读数拿不到（夹具）"))
                        && !sinkl.iter().any(|l| l.contains("屏保已关闭"))
                        && !sinkl.iter().any(|l| l.contains("读屏保状态失败"))
                        && !sinkl.iter().any(|l| l.contains("屏保本来就是关闭状态"))
                        && sinkl.iter().any(|l| l.contains("=== 本轮结束")),
                    &format!(
                        "屏保那一格崩了不该崩本轮（真身整段一个 try/catch ⇒ 只留一句「禁止睡眠失败：…」，四档一句都不打、收尾照旧撤屏蔽）：done={} rc={} 侧行={}",
                        reached_done,
                        done_l.0,
                        sinkl.join(" § ").chars().take(700).collect::<String>()
                    ),
                    &mut fails,
                    &mut total,
                );
            }
        }

        // ---------- 24m 选路：清单没进树那一档必须当场报错，不许照跑 ----------
        // 真身查的是现装注册表（`<id>-module.ps1` dot-source 进 `$global:GdModules` 才算注册）；
        // 核内没有注册表，等价判据只能落在**清单文件在不在位**上。只查助手库等于把"清单没进树"
        // 那一路静默接管（真身报错退出、核内照跑两路）。
        // 这一档要请出场的树：ark 全装、r1999 只有助手库在场（清单缺席）。
        let root_nomanifest = t24("root-no-manifest");
        let _ = std::fs::create_dir_all(format!("{}\\lib", root_nomanifest));
        for f in ["ark-module.ps1", "maa.ps1", "m9a.ps1"].iter() {
            let _ = std::fs::write(format!("{}\\lib\\{}", root_nomanifest, f), "# fake\n");
        }
        let mut rc_nom = round_ctx_empty();
        rc_nom.ids = vec!["ark".to_string(), "r1999".to_string()];
        let pick_nom = select_gd_modules(&rc_nom, &root_nomanifest);
        chk(
            matches!(pick_nom, Err(ref e) if e.contains("没有这个模块：r1999(这棵树上有：ark)") && e.contains("<id>-module.ps1 放进 lib")),
            &format!(
                "清单没进树那一档没照真身报错（助手库在场就当这一路能用 ⇒ 静默接管真身要当场退出的那一路）：{:?}",
                pick_nom.as_ref().err()
            ),
            &mut fails,
            &mut total,
        );
        // 反手一格：注册判据不许补过头——两路清单都在位的树照旧选出两路，且序按命令行传入序。
        let pick_back = select_gd_modules(&rc_nom, &root_all);
        chk(
            pick_back.as_ref().map(|v| v.iter().map(|m| m.id).collect::<Vec<_>>() == vec!["ark", "r1999"]).unwrap_or(false),
            &format!("补注册判据补过头了（清单在位的那两路也不再选出来，或传入序被改成名册序）：{:?}", pick_back.as_ref().err()),
            &mut fails,
            &mut total,
        );

        let _ = std::fs::remove_dir_all(&d24);
        chk(
            !d24.exists(),
            "B-3②-b3 自检的临时目录没收干净：自检会在 %TEMP% 里留垃圾",
            &mut fails,
            &mut total,
        );
    }

    if fails.is_empty() {

        println!("RESULT SELFTEST-PASS checks={} contract={}", total, CONTRACT);
        return 0;
    }
    println!("RESULT SELFTEST-FAIL failed={} of {}", fails.len(), total);
    for f in fails {
        println!("  FAIL {}", f);
    }
    return 1;
}

fn catches_the_panic() -> bool {
    // 自检这条例意把默认 panic hook 摘掉再触发，免得 stderr 上冒出一段看起来像探针崩了的
    // backtrace 提示，把"哪条红了"混在里面看不清；跑完立刻装回去。
    let saved = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let got = catch_unwind(AssertUnwindSafe(|| {
        let v: Vec<i32> = Vec::new();
        let _ = v[9]; // 真 out-of-bounds panic，故意的
    }))
    .is_err();
    std::panic::set_hook(saved);
    got
}

// ---------------------------------------------------------------- main

/// 信封落点：`--out` 给了就**只**写文件，stdout 只留一行 `RESULT ENVELOPE-OUT <文件>`；
/// 没给就照旧把 JSON 打在 stdout 的最后一行。
/// 这条不是便利功能，是被实测逼出来的：09-30 复跑发现那两枚缺 `plugins` 目录的树会把
/// `PluginMgr` 的 ERROR 打进 **stdout**（stderr 实测 0 字节），"一行 JSON"的口径当场不成立。
/// 对方 DLL 往哪个标准流里写，探针管不着 —— 能管的是**我方机读面从哪读**，所以开 --out。
/// 写不下去时不丢读数：退回 stdout 并把失败原因进 stderr（stderr 仍不进机读面）。
fn render_envelope(json: &str, out: &Option<String>) -> (Option<String>, String) {
    match out {
        None => (None, json.to_string()),
        Some(p) => {
            let text = format!("{}\n", json);
            match std::fs::write(p, text.as_bytes()) {
                Ok(_) => (Some(text), format!("RESULT ENVELOPE-OUT {}", dir_label(p, false))),
                Err(e) => {
                    eprintln!("--out 写不下去：{} —— 信封改从 stdout 出", e);
                    (None, json.to_string())
                }
            }
        }
    }
}

fn emit(json: &str, out: &Option<String>) {
    // 写文件这一步已经在 render_envelope 里做完了，这里只决定 stdout 上留哪一行
    let (file, line) = render_envelope(json, out);
    if file.is_some() {
        println!("{}", line);
        return;
    }
    println!("{}", json);
}

fn usage() -> ! {
    println!(
        "gdprobe {} —— 探针。只读段随时能跑；连接段要三闸；事件流档只吃夹具。\n\
         \n\
         只读段：gdprobe --kind <ark|mf|w32cu> --dir <装包目录> [--show-path] [--free]\n\
         \t  gdprobe --list-kinds      列出两侧装包形状（含各自必需/连接符号清单）\n\
         \t  gdprobe --preflight       只读环境闸：助手进程在不在场，不加载任何 DLL\n\
         \n\
         连接段：gdprobe --connect --kind <ark|mf> --dir <装包目录> --user-dir <我方写入目录>\n\
         \t              --scope <shape|resource|adb-live|adb-live-run> [--no-load-resource] [--resource-path <资源落点目录>]\n\
         \t              [--adb <路径>] [--address <host:port>] [--config <json>]\n\
         \t              [--agent-path <MaaAgentBinary 目录>] [--minimal-task <类型>] [--timeout-ms N]\n\
         \t              [--screencap-mask N] [--input-mask N] [--free]\n\
         \t              --i-confirm-gui-absent --i-accept-live --i-window-clear\n\
         \t  三个 --i-* 一个都不能少；助手进程在场时**直接拒跑**（不抢方向盘、不代杀进程）。\n\
         \t  档位射程：shape = 只加载与查导出、一枚函数都不调；\n\
         \t\t\tresource = 起实例 / 建资源 / 绑回调，不碰设备、不提权、不点击；\n\
         \t\t\t\t落点默认 mf=<装包>\\\\resource、ark=<装包根>，--resource-path 可指名模块子树（整目录会被框架判重复键）；\n\
         \t\t\t\t资源加载成没成看信封 resource_load，**别拿 ok 当它**（ok 只证 DLL 进得来 + 回调没炸 + 计划里那几枚句柄都拿到了 + 第四档的任务面收到了终局）；\n\
         \t\t\tadb-live = 真碰设备的一档（建控制器 + 发连接回执），连上就算、**不把任务队列跑起来**；\n\
         \t\t\t\tadb-live-run = 在它之上真启动队列（唯一调 AsstStart 的一档，会动账号，必须给 --minimal-task）；\n\
         \t\t\t\t\t这一档等的是**事件流里的终态**：等待上界复用 --timeout-ms，成败判据仍是 terminal_of 那一处（与 --events 同一条，\n\
         \t\t\t\t\t取环序里最后命中的那条，不按线程筛 —— 10-13 两枪实测任务链的事件在对方 worker 线程上）；\n\
         \t\t\t\t\t同两枪还把本刀的一条前提打掉了：遗留侧 AsstStart **不阻塞**（enter 与 leave 落在同一毫秒），\n\
         \t\t\t\t\t所以 calls 里的 AsstStart=1 + worker=joined 只证启动调用回了话，**一并不证队列散场** ——\n\
         \t\t\t\t\t队列有没有下文由 terminal=succeeded / failed / stopped / none 这枚标签说；\n\
         \t\t\t\t\t**但 terminal=succeeded 不证任务成功**：同一枪实测 TaskChainError 之后还会到一条 AllTasksCompleted\n\
         \t\t\t\t\t（那条的本义是「队列散场」，它带的 details 是 finished_tasks、里面就含着报错那条），\n\
         \t\t\t\t\t末条命中判成 succeeded 而这一轮其实是砸的 —— 所以 calls 里另留一枚 terminal_seen= 命中册\n\
         \t\t\t\t\t（这一轮见过哪几枚终态、按环序、相邻同枚折叠；成败词仍只由 terminal_of 一处给）；\n\
         \t\t\t\t\t上界到了没等到终态就主动 AsstStop（= 停止并清空所有任务，只叫一枪不补第二枪），\n\
         \t\t\t\t\t再给一枚短窗收 TaskChainStopped 的回执（上限沿用前三档收尾窗那条 3 秒，不另立数），\n\
         \t\t\t\t\t仍无下文才留 terminal=none 并由 ok 判据否掉；工作线程没回来就不销毁句柄、不释放模块，读数只留标签不猜。\n\
         \t  --config 缺省按侧给：mf = {{}}（JSON 对象 —— 非对象字面量会被框架在建控制器处直接判掉）、ark = General；\n\
         \t  --agent-path 是 mf 侧第六枚（上游 Python 绑定核实：agent_path = 装包内 MaaAgentBinary 目录），缺省空串原样传。\n\
         \t  启动类符号按档放开：shape / resource / adb-live 三档**仍然**一枚都不调 AsstStart，\"最小任务\"只到排队 + 回执；\n\
         \t  只有 adb-live-run 真跑队列（B-2e 乙-2，10-13 开闸）。任务提交（MaaTaskerPostTask）与挂窗口两枚全线仍禁止。\n\
         \t  没有本地签名源的函数（ark 侧 AsstAttachWindow / AsstConnected 等）只查导出、不调用。\n\
         \n\
         事件流档（R1）：gdprobe --events [--fixture <asst|framework|both>] [--expected N] [--seq-gap N] [--out <文件>]\n\
         \t  这一档**只吃内联夹具，不加载任何 DLL、不碰任何现场**，所以它不需要 --kind 也不需要 --dir。\n\
         \t  它给的是归一层的形状与判据：两侧原始事件名逐字保留、域与状态由我方映射、\n\
         \t  序号连续性对账（**连续不等于没丢** —— 没有 --expected 就只报 unverified）、\n\
         \t  成败只认 message（按 details 里的 hash 判必错，夹具里那两条 framework 事件**故意共用同一份原文**）。\n\
         \t  出口信封**不带** raw_len / raw_hash：实测它们是资源落点路径长度与原文指纹的直接读数，属本机事实。\n\
         \t  这一层是旁路对拍用的，按 ADR-2 那条硬门，它**不替换**任何生产判据。\n\
         \n\
         监督纯判定档（R2）：gdprobe --sv [--date <yyyyMMdd>] [--out <文件>]\n\
         \t  与事件流档同理**只吃内联夹具、不加载任何 DLL、不碰任何现场**，所以也不需要 --kind / --dir。\n\
         \t  把监督四段纯判定（开关取值 / 证据→状态 / 状态→动作 / 详情拼句）从与生产同一条调用面\n\
         \t  各过一遍（48 档场景），逐档结果装进一份信封，供三侧对拍门与 old/new 两侧逐字比对。\n\
         \t  --date 是详情那八档的日期戳注入（不给=空串；探针不读时钟 —— C# 侧同口径，日期由壳注入）。\n\
         \n\
         机器补刷判定档（R2②）：gdprobe --farm-intent [--out <文件>]\n\
         \t  与上面两档同理**只吃内联夹具、不加载任何 DLL、不碰任何现场**，所以也不需要 --kind / --dir。\n\
         \t  把 farm/intent 五段纯判定（目标清单解析 / 掉落文本 / 补刷判表 / 方舟本轮意图 / 1999 本轮意图）\n\
         \t  各过一遍（46 档场景），逐档结果装进一份信封，供三侧对拍门与 old/new 两侧逐字比对。\n\
         \t  **没有日期注入** —— 信封里不带 date 格：这一档不吃时钟，对应门也不做日期对账。\n\
         \n\
         装配回放档（R3②）：gdprobe --arkskip --panel-cs <panel-cs.exe 路径> [--out <文件>]\n\
         \t  与上面三档同理**只吃内联夹具、不加载任何 DLL、不碰任何现场**，所以也不需要 --kind / --dir。\n\
         \t  把 ark 认关止损链（Start-GdArkDaily 整函数）的装配逐句回放一遍（8 档场景）：\n\
         \t  日志/桩序列逐条落进投影串，供三侧对拍门逐字比对（old 转写 / new 现壳 / 本出口）。\n\
         \t  **唯一外部调用** = --panel-cs 指的那枚判定 exe：装配真身在这一步就是真跑 decide-daily-retry\n\
         \t  （照生产同一条 --in/--out 信封协议），所以缺它这一档直接拒跑；不喂它任何本机事实。\n\
         \n\
         驱动档（R3③，换头专用）：gdprobe --drive-ark-daily --i-drive --journal <journal 文件> --ctx <上下文文件> --res <回执文件> --panel-cs <判定 exe 路径> --state <state 目录> --log-sink <日志侧信道文件> [--out <文件>]\n\
         \t  gdcore.exe 出厂即带的本档（同一份 main.rs 编两枚 bin，gdprobe 与 gdcore 同体）：PS 侧换头后由调度器起它，\n\
         \t  一次一动作一进程 —— 进程从 journal 重放已办事项回到断点，把下一个要办的动作写成 ask 信封（seq/action/params）退出；\n\
         \t  壳照单执行完，把结果写 --res 回执，再起下一进程续推。\n\
         \t  已搬动作（B-1a 起：mark/obsrec/admin）不再走 ask —— 在本进程内真做（写 state 目录下的认栽标记/观测账本、查管理员令牌）并记账。\n\
         \t  B-2c③ 起 run 也不走 ask：同进程内真做（走 --maa-run 那套循环骨架，现场 verb 仍外调 panel-cs），\n\
         \t  现场输入取自 ctx 的 maaRun 子对象（十三枚，键名与 config\\settings.json 逐字同名；**整块 runtime 不许序列化进来** —— key 就在里面）。\n\
         \t  --fixture-run <文件> 只给对拍门/自检：run 真做换吃这份夹具（{{status,detail,duration?,logs}}），协议形状一字不动，生产不接这个旗标。\n\
         \t  五样 = 一根会话：journal 是唯一权威账（init 行 + 记录行 + 末行 done），ctx 只在起新账时吞成 init，\n\
         \t  res 是单条回执（读后即删），--state 指到生产同一处 state 目录（已搬动作的账要落回生产落点），\n\
         \t  --log-sink 是日志侧信道（B-2c②：链上日志按产生顺序逐行落这里，壳边跑边排空；重放段一行不写，信封 logs 留键恒空）。\n\
         \t  缺 --i-drive 一律拒跑（默认档不许启用它），五样缺一同样拒跑 —— 断点记不住的会话不许开。\n\
         \t  与上面四档同理**不加载任何 DLL、不碰任何现场**，所以也不需要 --kind / --dir。\n\
         \t  **唯一外部调用** = --panel-cs 指的那枚判定 exe：链体重试分流真跑 decide-daily-retry\n\
         \t  （照生产同一条 --in/--out 信封协议），所以缺它这一档直接拒跑；不喂它任何本机事实。\n\
         \n\
         m9a 驱动档（B-2d④，换头专用）：gdprobe --drive-m9a-daily --i-drive --journal <journal 文件> --ctx <上下文文件> --res <回执文件> --panel-cs <判定 exe 路径> --state <state 目录> --log-sink <日志侧信道文件> [--out <文件>]\n\
         \t  与上面那档**同一套协议、同一套记账**（journal 是唯一权威账 / res 读后即删 / --log-sink 边跑边排空 / 缺旗标一律 rc=2），\n\
         \t  推的是 Start-GdR1999Daily 那条 1999 链：八格动作里**真做只有 run**（走 --m9a-run 那套循环骨架，现场 verb 仍外调 panel-cs），\n\
         \t  其余七格（updgate / takeover / mumu / cfg / sleep / stop / audit）一律 ask 给 PS 侧执行 ——\n\
         \t  三处与 ark 那路**故意**不同源，都记在 plan §11.39：没有桌面闸、没有 launch、run 不吃 chains（真身签名里就没有）。\n\
         \t  ctx 只有两格：顶层 retry_once ＋ m9aRun（十五枚，键名与 config\\settings.json 逐字同名；\n\
         \t  **整块 runtime 不许序列化进来** —— 视觉 key 就住在那个块里，进核的只有名册那七枚 runtime 键）。\n\
         \t  role 是这一路自己的「B2d-drive-m9a-daily」，与 ark 那路的「R3-drive-ark-daily」不同源；拒跑七闸同序、同数量、不同句。\n\
         \t  --fixture-run 与 ark 那档共用同一枚旗标和同一份夹具形状（这一路多认一枚可选 rescued），生产不接这个旗标。\n\
         \n\
         轮骨架档（B-3②-b3，换头专用）：gdprobe --daily-round --i-round --journal <journal 文件> --ctx <轮上下文文件> --res <回执文件> --panel-cs <判定 exe 路径> --state <state 目录> --log-sink <日志侧信道文件> --root <工程根> [--fixture-run <夹具>] [--out <文件>]\n\
         \t  同一份 main.rs 的第三枚 role（「B3b-round-daily」）：把 lib\\modules.ps1 的 Invoke-GdDailyRound **十段骨架**逐行复刻在核内\n\
         \t  （选路 / 环境 / 提权闸 / 门表 / 判定 / 锁 / 监督 / 睡眠屏蔽 / 逐路链体 / 收尾汇总通知），协议与上面两档同一条\n\
         \t  （journal 唯一权威账、res 读后即删、--log-sink 边跑边排空、缺旗标 rc=2 拒跑）。\n\
         \t  **谁来答**换了第三档（plan §11.52 ③）：链体那些现场格与本轮的锁/通知/提权读态/睡眠屏蔽在 panel-cs 已有直答件，\n\
         \t  由核内答手直接真做并落账；门表 / 判定 / 预览 / 收尾 / 监督 / 历史 / 提权重启 / 屏保读数那几族**照旧外发 ask**，\n\
         \t  ask/res 协议一行不删 —— 它是回退通道、是三侧门 old 侧真身、是幂等重放的依据（§11.52 ④ 随行定档第 1 条）。\n\
         \t  两枚现出口（--drive-ark-daily / --drive-m9a-daily）**逐字节不变**：那一律不给答手，闭包为空 ⇒ 新区照旧出 ask 信封。\n\
         \t  ctx 由壳写一遍（十枚参数＋logLeaf/logPath/pid＋maaRun/m9aRun 两枚载荷）；\n\
         \t  **pid 必填、绝不允许回落成本进程号** —— 驱动协议一 ask 一进程，拿自己的 PID 登记活锁等于写一份没人能撤的账。\n\
         \t  --root 是本档独有的第八闸：这一档要现问「这棵树上装了哪几路」，而那两条判据不在任何一枚 verb 的回执里；\n\
         \t  本机路径只作参数喂进来，不进 ask 信封、不进日志（环境缺失那行的全路径是唯一在册例外，见 plan §11.55）。\n\
         \t  与上面两档同理**不加载任何 DLL**；睡眠屏蔽是本进程自己的 Win32 句柄语义（外发给壳等于没设），所以核内自设并同进程复验。\n\
         \t  ⚠ 本档此刻**零调用方**：PS 侧入口仍在 lib\\modules.ps1，转接归 ②-d；话术里不许吹「完全不经 PS」\n\
         \t  （updgate / audit 那两格的工具子进程照旧是 pwsh，§11.52 ② 那条降级口径）。\n\
         \n\
         运行面回放档（B-2a，只给对拍门用）：gdcore --maa-run --script <剧本文件> --panel-cs <判定 exe 路径> [--out <文件>]\n\
         \t  与上面五档同理**不加载任何 DLL、不碰任何现场**，所以也不需要 --kind / --dir；生产不接线（这一档不是换头入口）。\n\
         \t  它把 Invoke-MaaRun 的**循环骨架**（启动等待 → 读两份日志 → 喂判定 verb → 回灌状态 → 挂钟收尾）逐行复刻在内核里跑，\n\
         \t  现场动作（睡眠 / 起进程 / 探活 / 读文件尾 / 写日志）全部落成投影事件，不真执行；剧本一档一项（JSON 数组）。\n\
         \t  判据一条都不在这里 —— 六枚 verb 每 tick 真跑 --panel-cs 那枚判定 exe（与 arkskip 档共用同一条 --in/--out 协议），\n\
         \t  所以缺它这一档直接拒跑；两枚兜底可用性本批恒按**不可用**喂，识别到的 effect 一律走真身那句作废话术。\n\
         \t  出口 = 每档一行投影串（动作序列 + 末行状态），供 probe-arkrun-rust-golden 与 PS 真身侧逐字比对。\n\
         \n\
         m9a 运行面回放档（B-2d②，只给对拍门用）：gdcore --m9a-run --script <剧本文件> --panel-cs <判定 exe 路径> [--out <文件>]\n\
         \t  与上面那档同形同规矩（只吃夹具、不加载 DLL、不碰现场，也不需要 --kind / --dir），跑的是 Invoke-M9ARun 的循环骨架：\n\
         \t  启动等待 → 按**当天日期**重算日志名读文件尾 → 逐行喂判定 verb → 回灌状态 → 收结论，五枚 verb 每 tick 真跑 --panel-cs。\n\
         \t  两处与 ark 那档**故意**不同，都记在 plan §11.39：① 偏移按文件路径 keyed（跨 00:00 换文件，ark 侧是两枚固定变量），\n\
         \t  所以剧本必须喂 days 日期表，投影在日期变化时落一条 D|；② 单档 Err（真身 :347 那句 throw）落成该档末行 E| 继续跑下一档，\n\
         \t  不像 ark 那样整批中止 —— 那一格本身就是 M9A 一路的一种正常结论。\n\
         \t  adb 两件（connect / 1999 pid）的 C# 真身已随 **B-2d③-a** 落（`Live.cs` 的 AdbConnect/AdbGamePid），\n\
         \t  **生产接线也已随 ④ 的链体批落地**（§11.42 ⑦-2 那笔欠账结清：--drive-m9a-daily 的 run 真做走 RealHost）。\n\
         \t  回放档里两枚仍一律由夹具喂数据，不许核里现取现场。\n\
         \n\
         自检：gdprobe --selftest        无头，不加载任何 DLL、不碰任何现场\n\
         \n\
         机读出口：任意一档都可加 --out <文件>，把信封**只**写进该文件，stdout 只留一行 RESULT 指引。\n\
         \t  这不是方便：实测对方 DLL 会把加载期噪声打进 **stdout**（缺 plugins 目录的两枚树，stderr 是 0 字节），\n\
         \t  所以「stdout 一行 JSON」在真装包上并不成立。机读请一律走 --out，或取 stdout 的**最后一行**。\n\
         \n\
         退出码：0 正常 / 2 前置不满足（目录或 DLL 不在场、闸没过）/ 1 用法或内部错\n\
         输出：信封是 stdout 的**最后一行**，或 --out 指定文件的唯一一行；stderr 只放我方的话，不进机读面",
        CONTRACT
    );
    exit(1);
}

// 之所以是 `pub`：R3 起这枚源码同时喂两个 bin —— gdprobe(本文件当 crate root)与
// gdcore(src\bin\gdcore.rs 用 #[path] 把它当模块加载、从模块外调 `imp::main()`)。原因与另两条
// 备选(include! / 同 path 双 bin)的实测结果都写在 src\bin\gdcore.rs 头注里。
pub fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let code = match catch_unwind(AssertUnwindSafe(|| run(&args))) {
        Ok(c) => c,
        Err(_) => {
            // panic 不许悄悄跨出去：兜住、说清楚、给非 0 码。这条形状本身就是 §11.2 的落地。
            println!("{{\"ok\":false,\"why\":\"探针内部 panic 被 catch_unwind 兜住，读数不完整\",\"contract\":{},\"role\":{}}}", jstr(CONTRACT), jstr(ROLE));
            eprintln!("INTERNAL PANIC caught");
            1
        }
    };
    exit(code);
}

fn run(args: &[String]) -> i32 {
    if args.is_empty() {
        usage();
    }
    match args[0].as_str() {
        "--selftest" => return selftest(),
        "-h" | "--help" => usage(),
        _ => {}
    }

    if args[0] == "--list-kinds" {
        for k in KINDS {
            println!(
                "{{\"kind\":{},\"dll_candidates\":{},\"expect\":{},\"forbid\":{},\"version_fn\":{},\"reads_interface_json\":{},\"connect_symbols\":{},\"callable\":{},\"signature_source\":{},\"note\":{}}}",
                jstr(k.name),
                jarr(&k.dll_candidates.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
                jarr(&k.expect.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
                jarr(&k.forbid.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
                jstr(k.version_fn),
                if k.reads_interface_json { "true" } else { "false" },
                jarr(&k.connect_symbols.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
                jarr(&k.callable.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
                jstr(k.signature_source),
                jstr(k.note)
            );
        }
        return 0;
    }

    let mut kind: Option<&'static Kind> = None;
    let mut dir: Option<String> = None;
    let mut show_path = false;
    let mut do_free = false;
    let mut out_file: Option<String> = None;
    let mut mode = "probe";
    let mut user_dir: Option<String> = None;
    let mut scope = "shape".to_string();
    let mut adb = String::new();
    let mut address = String::new();
    // 空 = **没给**：真正的缺省按侧给（唯一口径在 default_config），下文 connect 段入口处兜底。
    // 原先把 `General` 写死在这——它对 mf 侧是错值（framework 要 JSON 对象），10-03 首跑实证。
    let mut config = String::new();
    let mut agent_path = String::new();
    let mut load_resource = true;
    let mut resource_path = String::new();
    let mut minimal_task = String::new();
    let mut timeout_ms = 4_000u64;
    let mut screencap_mask = 0xFFFF_FFFFu64;
    let mut input_mask = 0xFFFF_FFFFu64;
    let mut fixture = "both".to_string();
    let mut expected_raw = String::new();
    let mut drop_at = 0u64;
    // --sv（R2）那档的日期戳注入：空 = 没注入，d/ 那八档就照空串念（显式读数，探针永不读时钟）。
    let mut sv_date = String::new();
    // --arkskip（R3②）那档的判定 exe 路径注入：空 = 没给 → 该档直接拒跑（详见 run_arkskip 的头注）。
    let mut panel_cs = String::new();
    // 驱动档（R3③）的开关与三件文件：缺一拒跑（见 drive_refusal 五道闸）；全空 = 这一档没被启用。
    let mut i_drive = false;
    // 轮骨架档（B-3②-b3）：开关与**第八闸那一枚 `--root`**。root 是"这棵树上装了哪几路"的判据落点
    // （lib\maa.ps1 / lib\m9a.ps1 在不在），本机路径只作参数喂进来：不进 journal、不进 ask 信封、不进日志。
    let mut i_round = false;
    let mut round_root = String::new();
    let mut drive_journal = String::new();
    let mut drive_ctx = String::new();
    let mut drive_res = String::new();
    let mut drive_state = String::new();
    let mut drive_log_sink = String::new();
    // --fixture-run（B-2c③，门/自检专用）：非空 = run 真做改吃这份夹具，**协议形状一字不动**。
    // 生产不接线这个旗标（真做走 drive_real_run）；空 = 没给 → 走真做闭包。
    let mut drive_fixture_run = String::new();
    // --maa-run / --m9a-run 两档共用的剧本文件（B-2d② 起两路回放档同吃 `--script`）：
    // 空 = 没给 → 该档自己拒跑（与 --arkskip 缺 --panel-cs 同一口径）。
    // 之所以共用一个变量而不是各立一枚：两档互斥（mode 只能落一个），分立两枚就要多一处
    // "给了 --script 却落到另一档"的哑分支。
    let mut run_script = String::new();
    let mut flags = GateFlags::default();
    let mut i = 0usize;
    while i < args.len() {
        let need = |flag: &str, args: &[String], i: usize| -> String {
            if i + 1 >= args.len() {
                eprintln!("{} 后面少值", flag);
                exit(1);
            }
            args[i + 1].clone()
        };
        match args[i].as_str() {
            "--kind" => {
                let v = need("--kind", args, i);
                kind = match find_kind(&v) {
                    Some(k) => Some(k),
                    None => {
                        // 现有种类从 KINDS 现取，不写死——写死的那份已经漏过第三枚一次
                        eprintln!(
                            "未知 kind：{}（现有 {}）",
                            v,
                            KINDS.iter().map(|k| k.name).collect::<Vec<_>>().join(" / ")
                        );
                        exit(1);
                    }
                };
                i += 2;
            }
            "--dir" => {
                dir = Some(need("--dir", args, i));
                i += 2;
            }
            "--out" => {
                out_file = Some(need("--out", args, i));
                i += 2;
            }
            "--show-path" => {
                show_path = true;
                i += 1;
            }
            "--free" => {
                do_free = true;
                i += 1;
            }
            "--preflight" => {
                mode = "preflight";
                i += 1;
            }
            "--connect" => {
                mode = "connect";
                i += 1;
            }
            "--events" => {
                mode = "events";
                i += 1;
            }
            "--sv" => {
                mode = "sv";
                i += 1;
            }
            "--farm-intent" => {
                mode = "farm-intent";
                i += 1;
            }
            "--arkskip" => {
                mode = "arkskip";
                i += 1;
            }
            "--drive-ark-daily" => {
                mode = "drive-ark-daily";
                i += 1;
            }
            "--drive-m9a-daily" => {
                mode = "drive-m9a-daily";
                i += 1;
            }
            "--daily-round" => {
                mode = "daily-round";
                i += 1;
            }
            "--root" => {
                round_root = need("--root", args, i);
                i += 2;
            }
            "--maa-run" => {
                mode = "maa-run";
                i += 1;
            }
            "--m9a-run" => {
                mode = "m9a-run";
                i += 1;
            }
            "--script" => {
                run_script = need("--script", args, i);
                i += 2;
            }
            "--i-drive" => {
                i_drive = true;
                i += 1;
            }
            "--i-round" => {
                i_round = true;
                i += 1;
            }
            "--journal" => {
                drive_journal = need("--journal", args, i);
                i += 2;
            }
            "--ctx" => {
                drive_ctx = need("--ctx", args, i);
                i += 2;
            }
            "--res" => {
                drive_res = need("--res", args, i);
                i += 2;
            }
            "--state" => {
                drive_state = need("--state", args, i);
                i += 2;
            }
            "--log-sink" => {
                drive_log_sink = need("--log-sink", args, i);
                i += 2;
            }
            "--fixture-run" => {
                drive_fixture_run = need("--fixture-run", args, i);
                i += 2;
            }
            "--date" => {
                sv_date = need("--date", args, i);
                i += 2;
            }
            "--panel-cs" => {
                panel_cs = need("--panel-cs", args, i);
                i += 2;
            }
            "--fixture" => {
                fixture = need("--fixture", args, i);
                i += 2;
            }
            "--expected" => {
                expected_raw = need("--expected", args, i);
                i += 2;
            }
            "--seq-gap" => {
                drop_at = parse_u64("--seq-gap", &need("--seq-gap", args, i));
                i += 2;
            }
            "--scope" => {
                let v = need("--scope", args, i);
                // 档名册按**清单**收，不再逐枚写 `v != "..."` 的比较链：第四档那枚字面量在源码里
                // 只许存在一处（`runs_task` 函数体），这里再点一遍就会把那条 needle 门数成两处。
                // 判据仍只住射程/跑任务那两枚函数，这里只管"这个名字我们不认"。
                if !KNOWN_SCOPES.contains(&v.as_str()) {
                    eprintln!("未知 scope：{}（只有 shape / resource / adb-live / adb-live-run）", v);
                    exit(1);
                }
                scope = v;
                i += 2;
            }
            "--user-dir" => {
                user_dir = Some(need("--user-dir", args, i));
                i += 2;
            }
            "--adb" => {
                adb = need("--adb", args, i);
                i += 2;
            }
            "--address" => {
                address = need("--address", args, i);
                i += 2;
            }
            "--config" => {
                config = need("--config", args, i);
                i += 2;
            }
            "--agent-path" => {
                agent_path = need("--agent-path", args, i);
                i += 2;
            }
            "--minimal-task" => {
                minimal_task = need("--minimal-task", args, i);
                i += 2;
            }
            "--no-load-resource" => {
                load_resource = false;
                i += 1;
            }
            "--resource-path" => {
                // 空串按"没给"处理：各侧默认口径不许被一个 --resource-path "" 悄悄改掉
                resource_path = need("--resource-path", args, i);
                i += 2;
            }
            "--timeout-ms" => {
                timeout_ms = parse_u64("--timeout-ms", &need("--timeout-ms", args, i));
                i += 2;
            }
            "--screencap-mask" => {
                screencap_mask = parse_u64("--screencap-mask", &need("--screencap-mask", args, i));
                i += 2;
            }
            "--input-mask" => {
                input_mask = parse_u64("--input-mask", &need("--input-mask", args, i));
                i += 2;
            }
            "--i-confirm-gui-absent" => {
                flags.confirm_gui_absent = true;
                i += 1;
            }
            "--i-accept-live" => {
                flags.accept_live = true;
                i += 1;
            }
            "--i-window-clear" => {
                flags.window_clear = true;
                i += 1;
            }
            other => {
                eprintln!("未知参数：{}", other);
                exit(1);
            }
        }
    }

    if mode == "events" {
        // 事件流档放在 --kind/--dir 那两必填**之前**返回：它是 R1 的归一层自检面，
        // 只吃内联夹具，不加载任何 DLL，所以"要指哪枚装包"这个问题在这里根本没有意义。
        // 反过来写的话，一次纯粹的归一层对拍会被"忘了给 --dir"拦死，那条错误话术会让人
        // 以为这档也要装包 —— 而它恰恰是**不需要**装包也能跑的那一档。
        let exp = if expected_raw.is_empty() {
            None
        } else {
            Some(parse_u64("--expected", &expected_raw))
        };
        return run_events(&fixture, exp, drop_at, &out_file);
    }

    if mode == "sv" {
        // 监督纯判定档与事件流档同理，也落在两必填**之前**返回：它只吃内联夹具、不加载任何 DLL，
        // "要指哪枚装包"在这里同样没有意义；被必填拦死就等于这一档没开。
        // （这段注释刻意不把那条必填的原话写出来 —— 自检的顺序门数整份源码，写出来先逮到的就是注释。）
        return run_sv(&sv_date, &out_file);
    }

    if mode == "farm-intent" {
        // farm/intent 纯判定档与上面两档同理，同样落在两必填**之前**返回：只吃内联夹具、
        // 不加载任何 DLL，"要指哪枚装包"在这里没有意义；被必填拦死就等于这一档没开。
        // （照 sv 的样式，这条注释刻意不把那条必填的原话写出来 —— 自检的顺序门数整份源码。）
        return run_farm_intent(&out_file);
    }

    if mode == "arkskip" {
        // ark 认关止损链的装配回放档与上面三档同理，同样落在两必填**之前**返回：只吃内联夹具、
        // 不加载任何 DLL，"要指哪枚装包"在这里没有意义。与前三档的唯一不同：这一档的装配真身
        // 本就真跑 decide-daily-retry，所以要 --panel-cs 指到那枚判定 exe（缺它该档自己拒跑，
        // 不是被装包必填拦死）。注释照 sv/farm-intent 的样式，刻意不把那条必填的原话写出来。
        return run_arkskip(&panel_cs, &out_file);
    }

    if mode == "drive-ark-daily" {
        // 驱动档与上面四档同理，同样落在两必填**之前**返回：它只推链与记账、不加载任何 DLL、
        // 不碰任何现场，"要指哪枚装包"在这里没有意义；被必填拦死就等于换头调度器起步即断。
        // （照 sv/farm-intent 的样式，这条注释刻意不把那条必填的原话写出来 —— 自检的顺序门数整份源码。）
        return run_drive_ark_daily(i_drive, &drive_journal, &drive_ctx, &drive_res, &panel_cs, &drive_state, &drive_log_sink, &drive_fixture_run, &out_file);
    }

    if mode == "drive-m9a-daily" {
        // m9a 那一路的驱动档（B-2d④）与上面八档同理，同样落在两必填**之前**返回：只推 1999 的链与记账、
        // 不加载任何 DLL；而且它比 ark 那档更该躲开必填 —— 这一路的落点全在 ctx 的名册里，
        // 那两枚必填在这里根本没有对应物（同 --m9a-run 那档给过的理由）。
        // （照 sv/farm-intent 的样式，这条注释刻意不把那条必填的原话写出来 —— 自检的顺序门数整份源码。）
        return run_drive_m9a_daily(i_drive, &drive_journal, &drive_ctx, &drive_res, &panel_cs, &drive_state, &drive_log_sink, &drive_fixture_run, &out_file);
    }

    if mode == "daily-round" {
        // 轮骨架档（B-3②-b3）与上面九档同理落在两必填**之前**返回：它不加载任何 DLL ——
        // 十段骨架问的全是 panel-cs 那些 verb（子进程）与核内那枚睡眠屏蔽，装包那两枚在这里没有对应物。
        // 这一档比两枚现出口**多一道闸**：`--root`（要现问"这棵树上装了哪几路"，而那两条判据不在任何
        // 一枚 verb 的回执里）。缺它照样是 rc=2 落拒跑句，口径与 --i-drive 那条一字同族。
        // （照 sv/farm-intent 的样式，这条注释刻意不把那条必填的原话写出来 —— 自检的顺序门数整份源码。）
        return run_drive_round_daily(
            i_round,
            &drive_journal,
            &drive_ctx,
            &drive_res,
            &panel_cs,
            &drive_state,
            &drive_log_sink,
            &round_root,
            &drive_fixture_run,
            &out_file,
        );
    }

    if mode == "maa-run" {
        // 运行面宿主的剧本回放档（B-2a）与上面五档同理，同样落在两必填**之前**返回：这一档不加载
        // 任何 DLL、不碰任何真装包现场，"要指哪枚装包"在这里没有意义；被必填拦死就等于这一档没开。
        // 与 ark 认关止损那档一样，循环骨架每 tick 真跑 panel-cs 的判定 verb，所以缺 --panel-cs
        // 由这一档自己拒跑（口径同上两档注释，刻意不把那条必填的原话写出来）。
        return run_maa_run(&run_script, &panel_cs, &out_file);
    }

    if mode == "m9a-run" {
        // m9a 那一路的回放档（B-2d②）与上面六档同理落在两必填**之前**返回：它同样不加载 DLL、
        // 不碰任何真装包现场，而且比 ark 那档更该躲开必填 —— 它的 ctx 形状根本不是"装包+目录"
        // （M9A 的落点在 settings 的 paths 里，--kind/--dir 那两枚在这里没有对应物）。
        // 缺 --panel-cs 由这一档自己拒跑：每 tick 真跑 decide-m9a-run-tick，缺它循环走不动。
        return run_m9a_run(&run_script, &panel_cs, &out_file);
    }

    if mode == "preflight" {
        return preflight(&flags, &out_file);
    }

    let Some(k) = kind else {
        eprintln!("--kind 必填");
        exit(1);
    };
    let Some(d) = dir else {
        eprintln!("--dir 必填");
        exit(1);
    };

    if mode == "connect" {
        // config 缺省按侧给（唯一口径在 default_config）：空 = 没给 —— 壳侧那句"空就填 Default"已删，
        // 它对 mf 是错字面量（10-03 首跑 `invalid config` 原话），对 ark 也不是连接配置名。
        if config.is_empty() {
            config = default_config(k.name).to_string();
        }
        // 顺序是刻意的：**闸先于参数**。三闸没拨属"规则说不干"（2 档 + 机读信封），
        // 缺参数才属"用法"（1 档）。反过来写的话，一次 GUI 在场的违规会被当成用法错报出去，
        // 调用方就分不清是自己忘了拨闸还是忘了给 --user-dir，而前者才是要紧的那条。
        let (read_ok, hits) = match scan_forbidden_processes() {
            Ok(v) => (true, v),
            Err(e) => {
                eprintln!("{}", e);
                (false, Vec::new())
            }
        };
        let (pass, reasons) = gate_verdict(&flags, read_ok, &hits);
        if !pass {
            let opts = ConnectOpts {
                dir: d.clone(),
                user_dir: user_dir.clone().unwrap_or_default(),
                scope: scope.clone(),
                adb: adb.clone(),
                address: address.clone(),
                config: config.clone(),
                agent_path: agent_path.clone(),
                load_resource,
                resource_path: resource_path.clone(),
                minimal_task: minimal_task.clone(),
                timeout_ms,
                screencap_mask,
                input_mask,
                do_free,
                show_path,
            };
            // 闸没过也照实出一条信封：调用方要能区分"探针崩了"和"规则说不干"。
            let rep = connect_run(k, &opts, reasons);
            emit(&rep.to_json(), &out_file);
            return 2;
        }
        // 没有本地签名源的那一档（w32cu）**不在这里退 1**：那是"规则说不干"，不是"参数写错了"。
        // 判据只住在 connect_run 一处（它先于任何加载与落盘返回 ok=false），这里只管参数形状。
        // 所以 --kind w32cu --connect 的出口是：三闸 + --user-dir 给齐 → 一条 ok:false 信封 + 退出码 2。
        let Some(ud) = user_dir else {
            eprintln!("连接段必须给 --user-dir：把写入面收窄到我方目录是硬条件，不是优化");
            exit(1);
        };
        if reaches_connect(&scope) && (adb.is_empty() || address.is_empty()) {
            eprintln!("scope={} 需要 --adb 与 --address 两个值（连谁、走哪条 adb 都得写死，探针不猜）", scope);
            exit(1);
        }
        let opts = ConnectOpts {
            dir: d.clone(),
            user_dir: ud,
            scope: scope.clone(),
            adb,
            address,
            config,
            agent_path,
            load_resource,
            resource_path,
            minimal_task,
            timeout_ms,
            screencap_mask,
            input_mask,
            do_free,
            show_path,
        };
        let rep = connect_run(k, &opts, Vec::new());
        emit(&rep.to_json(), &out_file);
        return if rep.ok { 0 } else { 2 };
    }

    let rep = probe(k, &d, show_path, do_free);
    emit(&rep.to_json(), &out_file);
    // 口径直接沿用现有 verb 信封那三条：0 正常 / 2 前置不满足或规则说不干 / 1 用法或内部错。
    // 探针里所有 !ok 的分支都是前一类（目录不在、DLL 加载不了、符号不齐、不对称前提被推翻），
    // 所以不看文案、不猜 —— 用法与 panic 在上面就已经按 1 走了。
    if rep.ok {
        return 0;
    }
    2
}

fn parse_u64(flag: &str, v: &str) -> u64 {
    match v.parse::<u64>() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("{} 要一个非负整数，收到：{}", flag, v);
            exit(1);
        }
    }
}

/// 只读环境闸：查助手进程在不在场、三闸拨没拨。**这一档不加载任何 DLL**，
/// 所以任何时候都能跑 —— 包括现在这种"想确认窗口干不干净"的时候。
fn preflight(flags: &GateFlags, out: &Option<String>) -> i32 {
    let (read_ok, hits) = match scan_forbidden_processes() {
        Ok(v) => (true, v),
        Err(e) => {
            eprintln!("{}", e);
            (false, Vec::new())
        }
    };
    let (pass, reasons) = gate_verdict(flags, read_ok, &hits);
    emit(
        &format!(
            "{{\"ok\":{},\"why\":{},\"contract\":{},\"role\":\"R0-preflight\",\"proc_read_ok\":{},\"assistant_processes\":{},\"forbidden_checked\":{},\"gate_flags\":{{\"gui_absent\":{},\"accept_live\":{},\"window_clear\":{}}},\"refusals\":{}}}",
            if pass { "true" } else { "false" },
            jstr(if pass {
                "现场干净且三闸齐：连接段可以谈"
            } else {
                "闸没过：连接段一步都不许做"
            }),
            jstr(CONTRACT),
            if read_ok { "true" } else { "false" },
            jarr(&hits),
            jarr(&FORBIDDEN_PROCESSES.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            if flags.confirm_gui_absent { "true" } else { "false" },
            if flags.accept_live { "true" } else { "false" },
            if flags.window_clear { "true" } else { "false" },
            jarr(&reasons)
        ),
        out,
    );
    if pass {
        return 0;
    }
    2
}
