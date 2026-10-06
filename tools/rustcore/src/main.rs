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

const CONTRACT: &str = "RustCore-R3.0";
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
}

/// 进程快照用（连接段的环境闸）。INVALID_HANDLE 用 -1 的位型判。
const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
const INVALID_HANDLE_ISIZE: isize = -1;

/// `PROCESSENTRY32W` 的**现代布局**（Win8 起多了末尾那两个 ULONG_PTR）。
/// 少了它们，`Process32FirstW` 会因为 `dwSize` 对不上直接失败 —— 那是实测过的形状，
/// 所以这里按含两枚尾字段写，并且**失败一律当"闸读不出来"处理**（宁可拒跑，不许放行）。
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
    th32_memory_base: usize,
    th32_check_sum: u32,
}

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
            ("Reason", JVal::S(format!("(-Force) 跳过活动确认门，直接刷 {}", hint))),
            ("EventKey", fi_kv(&ark, "EventKey")),
            ("EventTip", fi_kv(&ark, "EventTip")),
            ("Source", fi_kv(&ark, "Source")),
        ]);
        warnings.push(JVal::S("-Force：本轮忽略活动确认门".to_string()));
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

/// 夹具世界（8 档）的 Fx（把 live 门第六路/三侧门的桩语义原样平移进 trait 实现：调用序与入参
/// 进 seq、队列消费、固定返回、admin 恒 false）——链体从真身抽出后，"8 档投影逐字不变"这一半
/// 由它保。投影串里的动作裸名（DESK/CFG/RUN/SKIP/…）都是这一层的历史格式，链体不碰。
struct FixtureFx<'a> {
    sc: &'a ArkskScen,
    seq: Vec<String>,
    cfg_i: usize,
    run_i: usize,
    skip_i: usize,
    skip_n: i32,
}

impl<'a> FixtureFx<'a> {
    fn new(sc: &'a ArkskScen) -> Self {
        FixtureFx { sc, seq: Vec::new(), cfg_i: 0, run_i: 0, skip_i: 0, skip_n: 0 }
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
        // Set-GdArkFightSkipMark 桩的固定返回（不读时钟）。
        Ok("2026-10-05".to_string())
    }
    fn ledger(&mut self) -> Result<String, ArkAbort> {
        self.raw("LEDGER".to_string());
        Ok("BN1".to_string())
    }
    fn obs(&mut self) -> Result<JVal, ArkAbort> {
        self.raw("OBS".to_string());
        Ok(JVal::Null)
    }
    fn obsrec(&mut self, _obs: &JVal) -> Result<Option<i64>, ArkAbort> {
        self.raw("OBSREC".to_string());
        Ok(Some(5))
    }
    /// Test-GdAdmin 恒 false（与 live 门第六路同桩）。
    fn admin(&mut self) -> Result<bool, ArkAbort> {
        Ok(false)
    }
}

/// 按 Invoke-GdCli 的同一条协议真跑 decide-daily-retry：Body 写 --in 信封、读回 --out 信封、
/// 校验 verb / 退出码 / ok —— 任一环对不上就 Err（不带着半截结论往下跑）。临时文件成败都删。
/// 错误文案里"C# 侧(…)"的句式照 Invoke-GdCli 原话；起不来（exe 不在）这句是本档自己的补充。
fn arksk_call_decide(
    panel_cs: &str,
    status: &str,
    retry_once: bool,
) -> Result<(bool, Vec<(String, String)>), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir();
    let in_path = dir.join(format!("gd-arkskip-{}-{}.in.json", std::process::id(), stamp));
    let out_path = dir.join(format!("gd-arkskip-{}-{}.out.json", std::process::id(), stamp));
    let body = jv_to_json(&jv_o(vec![
        ("game", jv_s("ark")),
        ("status", jv_s(status)),
        ("retryOnce", JVal::B(retry_once)),
    ]));
    let r = (|| -> Result<(bool, Vec<(String, String)>), String> {
        std::fs::write(&in_path, body.as_bytes()).map_err(|e| format!("Body 信封写不下去：{}", e))?;
        let outp = std::process::Command::new(panel_cs)
            .arg("decide-daily-retry")
            .arg(format!("--in={}", in_path.display()))
            .arg(format!("--out={}", out_path.display()))
            .output()
            .map_err(|e| format!("decide-daily-retry 起不来（{}）：{}", panel_cs, e))?;
        let code = outp.status.code().unwrap_or(-1);
        let text = match std::fs::read_to_string(&out_path) {
            Ok(t) => t,
            Err(_) => return Err(format!("C# 侧(decide-daily-retry)没吐出结果文件(退出码 {})", code)),
        };
        let env0 = jv_parse(&text).map_err(|e| format!("C# 侧(decide-daily-retry)信封读不动：{}", e))?;
        if !jv_str(&jv_get(&env0, "verb")).eq_ignore_ascii_case("decide-daily-retry") {
            return Err("C# 侧(decide-daily-retry)信封不对或缺 verb 字段".to_string());
        }
        let env_exit = jv_int(&jv_get(&env0, "exit"), 0);
        if env_exit != code as i64 {
            return Err(format!(
                "C# 侧(decide-daily-retry)退出码({})与信封 exit({})不一致",
                code, env_exit
            ));
        }
        if !jv_truthy(&jv_get(&env0, "ok")) {
            return Err(jv_str(&jv_get(&env0, "error")));
        }
        let data = jv_get(&env0, "data");
        let retryable = jv_truthy(&jv_get(&data, "Retryable"));
        let mut logs = Vec::new();
        for l in jv_list(&jv_get(&data, "Logs")) {
            logs.push((jv_str(&jv_get(l, "Level")), jv_str(&jv_get(l, "Text"))));
        }
        Ok((retryable, logs))
    })();
    let _ = std::fs::remove_file(&in_path);
    let _ = std::fs::remove_file(&out_path);
    r
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
        // 入口参数名：单游戏仓库未 Sync 过的表给短名；合体工程 Sync 后是 -AckArk —— 那是
        // 装配输入（表状态），不在本刀范围，8 档也不触这支。
        let ack = fx.entryverb("ark", "Ack")?;
        fx.log(
            "WARN",
            &format!("本轮跳过理智作战(只跑公招/基建/信用/领奖)；自己打过活动图后跑 daily-clear.ps1 {}", ack),
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

/// 一档的回放：夹具 Fx + 夹具 decision 走一遍链体，返回投影串（`L=…|S=…|D=…`，与 live 门同形）。
/// `decide` 是重试分流处的注入点：真出口传 arksk_call_decide（真跑 panel-cs.exe），
/// 自检传同语义的替身（自检不许要外部依赖）。Ask 只属于驱动档 —— 夹具世界冒出来就是内部错。
fn arkskip_run_case(
    sc: &ArkskScen,
    decide: &mut dyn FnMut(&str, bool) -> Result<(bool, Vec<(String, String)>), String>,
) -> Result<String, String> {
    let input = ArkDInput {
        launch: sc.launch,
        retry_once: sc.retry_once,
        decision: arksk_fixture_decision(sc),
    };
    let mut fx = FixtureFx::new(sc);
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
// 退出码：0（ask/done）/ 2（缺旗标拒跑）/ 1（协议或内部错）；role 恒 "R3-drive-ark-daily"。

const DRIVE_WHY: &str = "驱动档是换头专用口：本进程只推链与记账，现场动作全由 PS 侧照 ask 的 seq/action 执行后写 --res 回执再重入（缺 --i-drive 直接拒跑）";

/// 五道拒跑闸（顺序即优先级）。五句都点名所缺旗标 —— 自检逐句咬字。
fn drive_refusal(i_drive: bool, journal: &str, ctx: &str, res: &str, panel_cs: &str) -> Option<String> {
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

/// 驱动侧的 Fx：把链体的 16 个宿主需求翻译成"重放校验 / 新动作 ask"。缺口语义在这里：
/// - 重放段动作对不上 = journal 与现树不同源 → io_err（Proto，任何 catch 都不接）；
/// - 新区写 journal 失败 → io_err（同上，断点记不住不许继续）；
/// - 新区动作 = 记 pending 断点 + Err(Ask) 冒泡（链体不 catch Ask —— 它就是本档的暂停信号）。
struct DriveFx<'a> {
    journal: &'a str,
    rows: Vec<JVal>,
    /// 启动时（load+merge 后）的记录行数：这之后的才是本轮新区。
    replay_end: usize,
    /// 已消费行数（重放推进与新区写入同步推进）。
    consumed: usize,
    /// 本轮已写新行数（seq 基准 = replay_end + written + 1）。
    written: usize,
    /// 新区日志收集（重放不收集 —— 那些行上一轮的信封里已经报过）。
    logs: Vec<(String, String)>,
    /// 新区动作的断点：ask 信封从这里渲染。
    pending: Option<(i64, &'static str, JVal)>,
    /// 驱动协议错。log 没有 Result 通道，只能落这里；下个动作 check_io 冒泡。
    io_err: Option<String>,
}

impl<'a> DriveFx<'a> {
    fn new(journal: &'a str, rows: Vec<JVal>) -> Self {
        let replay_end = rows.len();
        DriveFx { journal, rows, replay_end, consumed: 0, written: 0, logs: Vec::new(), pending: None, io_err: None }
    }
    fn replaying(&self) -> bool {
        self.consumed < self.replay_end
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
}

impl ArkDailyFx for DriveFx<'_> {
    fn log(&mut self, level: &str, text: &str) {
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
            self.logs.push((level.to_string(), text.to_string()));
        }
    }
    fn desk(&mut self) -> Result<String, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("desk")? {
                DriveTake::Data(d) => Ok(jv_str(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("desk", JVal::Null)
        }
    }
    fn svdetail(&mut self) -> Result<String, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("svdetail")? {
                DriveTake::Data(d) => Ok(jv_str(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("svdetail", JVal::Null)
        }
    }
    fn takeover(&mut self) -> Result<(), ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("takeover")? {
                DriveTake::Data(_) => Ok(()),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("takeover", JVal::Null)
        }
    }
    fn cfg(&mut self, kind: ArkCfgKind) -> Result<ArkCfgOut, ArkAbort> {
        self.check_io()?;
        let params = match &kind {
            ArkCfgKind::Main => jv_o(vec![("kind", jv_s("main"))]),
            ArkCfgKind::Lean { stage } => jv_o(vec![("kind", jv_s("lean")), ("stage", jv_s(stage))]),
        };
        if self.replaying() {
            match self.take_act("cfg")? {
                DriveTake::Data(d) => Ok(ArkCfgOut {
                    changes: jv_list(&jv_get_ci(&d, "changes")).into_iter().map(jv_str).collect(),
                    chains: jv_list(&jv_get_ci(&d, "chains")).into_iter().map(jv_str).collect(),
                }),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("cfg", params)
        }
    }
    fn entryverb(&mut self, game: &str, verb: &str) -> Result<String, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("entryverb")? {
                DriveTake::Data(d) => Ok(jv_str(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("entryverb", jv_o(vec![("game", jv_s(game)), ("verb", jv_s(verb))]))
        }
    }
    fn launch(&mut self) -> Result<bool, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("launch")? {
                DriveTake::Data(d) => Ok(jv_truthy(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("launch", JVal::Null)
        }
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
            self.ask("run", JVal::A(chains.iter().map(|c| jv_s(c)).collect()))
        }
    }
    fn stop(&mut self) -> Result<(), ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("stop")? {
                DriveTake::Data(_) => Ok(()),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("stop", JVal::Null)
        }
    }
    fn sleep(&mut self, seconds: u32) -> Result<(), ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("sleep")? {
                DriveTake::Data(_) => Ok(()),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("sleep", JVal::I(seconds as i64))
        }
    }
    fn skip(&mut self) -> Result<Option<ArkSkipData>, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("skip")? {
                DriveTake::Data(d) => {
                    if matches!(d, JVal::Null) {
                        return Ok(None);
                    }
                    Ok(Some(ArkSkipData {
                        skipped: jv_truthy(&jv_get_ci(&d, "skipped")),
                        plan: jv_list(&jv_get_ci(&d, "plan")).into_iter().map(jv_str).collect(),
                        evidence: jv_list(&jv_get_ci(&d, "evidence")).into_iter().map(jv_str).collect(),
                    }))
                }
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("skip", JVal::Null)
        }
    }
    fn mark(&mut self, stages: &[String], reason: &str) -> Result<String, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("mark")? {
                DriveTake::Data(d) => Ok(jv_str(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask(
                "mark",
                jv_o(vec![
                    ("stages", JVal::A(stages.iter().map(|s| jv_s(s)).collect())),
                    ("reason", jv_s(reason)),
                ]),
            )
        }
    }
    fn ledger(&mut self) -> Result<String, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("ledger")? {
                DriveTake::Data(d) => Ok(jv_str(&d)),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("ledger", JVal::Null)
        }
    }
    fn obs(&mut self) -> Result<JVal, ArkAbort> {
        self.check_io()?;
        if self.replaying() {
            match self.take_act("obs")? {
                DriveTake::Data(d) => Ok(d),
                DriveTake::Fail(m) => Err(ArkAbort::Fail(m)),
            }
        } else {
            self.ask("obs", JVal::Null)
        }
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
            self.ask("obsrec", obs.clone())
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
            self.ask("admin", JVal::Null)
        }
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

fn drive_logs_json(logs: &[(String, String)]) -> String {
    let items: Vec<String> =
        logs.iter().map(|(l, t)| format!("{{\"level\":{},\"text\":{}}}", jstr(l), jstr(t))).collect();
    format!("[{}]", items.join(","))
}

/// ask 信封：链体停在哪个动作上（seq/action/params），壳照单执行后写 res 回执再重入。
fn render_drive_ask(seq: i64, action: &str, params: &JVal, logs: &[(String, String)]) -> String {
    format!(
        "{{\"ok\":true,\"why\":{},\"contract\":{},\"role\":\"R3-drive-ark-daily\",\"kind\":\"ask\",\"seq\":{},\"action\":{},\"params\":{},\"logs\":{}}}",
        jstr(DRIVE_WHY),
        jstr(CONTRACT),
        seq,
        jstr(action),
        jv_to_json(params),
        drive_logs_json(logs)
    )
}

/// done 信封：链体整条走完（含 not-started 早退等一切终态），result 就是真身返回的
/// @{Status;Detail;Duration?}。
fn render_drive_done(result: &JVal, logs: &[(String, String)]) -> String {
    format!(
        "{{\"ok\":true,\"why\":{},\"contract\":{},\"role\":\"R3-drive-ark-daily\",\"kind\":\"done\",\"result\":{},\"logs\":{}}}",
        jstr(DRIVE_WHY),
        jstr(CONTRACT),
        jv_to_json(result),
        drive_logs_json(logs)
    )
}

/// fail 信封：链体 Fail 与驱动协议错共用（后者 error 里带"驱动协议坏了："前缀）。
fn render_drive_fail(error: &str, logs: &[(String, String)]) -> String {
    format!(
        "{{\"ok\":false,\"why\":{},\"contract\":{},\"role\":\"R3-drive-ark-daily\",\"kind\":\"fail\",\"error\":{},\"logs\":{}}}",
        jstr(DRIVE_WHY),
        jstr(CONTRACT),
        jstr(error),
        drive_logs_json(logs)
    )
}

/// 驱动档一轮的信封装配（纯函数；`run_drive_ark_daily` 与 `--selftest` 共用同一处）。
/// 一轮 = 从 journal 重放回断点 → 再走一步链体 → 落 ask / done / fail 三种收场之一。
fn drive_ark_daily_envelope(
    i_drive: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
) -> (i32, String) {
    if let Some(m) = drive_refusal(i_drive, journal, ctx, res, panel_cs) {
        return (2, render_drive_fail(&m, &[]));
    }
    // ---- ctx：壳写一遍的三格（launch/retry_once/decision） ----
    let ctx_text = match std::fs::read_to_string(ctx) {
        Ok(t) => t,
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：ctx 读不动：{}", e), &[])),
    };
    let ctx_val = match jv_parse(&ctx_text) {
        Ok(v) => v,
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：ctx 不是合法 JSON：{}", e), &[])),
    };
    let launch = match jv_get_ci(&ctx_val, "launch") {
        JVal::B(b) => b,
        _ => return (1, render_drive_fail("驱动协议坏了：ctx 的 launch/retry_once 必须是布尔", &[])),
    };
    let retry_once = match jv_get_ci(&ctx_val, "retry_once") {
        JVal::B(b) => b,
        _ => return (1, render_drive_fail("驱动协议坏了：ctx 的 launch/retry_once 必须是布尔", &[])),
    };
    let decision = jv_get_ci(&ctx_val, "decision");
    if !matches!(decision, JVal::O(_)) {
        return (1, render_drive_fail("驱动协议坏了：ctx 的 decision 必须是对象", &[]));
    }
    // ---- journal：账本在手就重放，不在就现起一本 ----
    let mut rows: Vec<JVal>;
    match std::fs::read_to_string(journal) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let init_row = jv_o(vec![("init", ctx_val.clone())]);
            if let Err(m) = drive_append_row(journal, &init_row) {
                return (1, render_drive_fail(&format!("驱动协议坏了：journal 建不起来：{}", m), &[]));
            }
            let _ = std::fs::remove_file(res);
            rows = Vec::new();
        }
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：journal 读不动：{}", e), &[])),
        Ok(text) => {
            let (init, r, done) = match drive_load_journal(&text) {
                Ok(t) => t,
                Err(m) => return (1, render_drive_fail(&format!("驱动协议坏了：journal 读不动：{}", m), &[])),
            };
            if jv_to_json(&init) != jv_to_json(&ctx_val) {
                return (
                    1,
                    render_drive_fail("驱动协议坏了：journal 的 init 与 ctx 不是同一份（换了 ctx 就得起新 journal）", &[]),
                );
            }
            if let Some(drow) = done {
                let _ = std::fs::remove_file(res);
                return (0, render_drive_done(&drive_done_result(&jv_get(&drow, "done")), &[]));
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
                        return (1, render_drive_fail(&format!("驱动协议坏了：journal 追加失败：{}", m), &[]));
                    }
                    rows.push(row);
                }
                Ok(None) => {}
                Err(m) => return (1, render_drive_fail(&format!("驱动协议坏了：res 对不上：{}", m), &[])),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return (1, render_drive_fail(&format!("驱动协议坏了：res 读不动：{}", e), &[])),
    }
    // ---- 一轮：重放到断点，之后再走一步 ----
    let input = ArkDInput { launch, retry_once, decision };
    let mut fx = DriveFx::new(journal, rows);
    let r = ark_daily_chain(&input, &mut fx, &mut |st: &str, ro: bool| arksk_call_decide(panel_cs, st, ro));
    match r {
        Ok(o) => {
            if let Some(m) = fx.io_err.take() {
                return (1, render_drive_fail(&format!("驱动协议坏了：{}", m), &fx.logs));
            }
            let mut kv: Vec<(&str, JVal)> = vec![("status", jv_s(&o.status)), ("detail", jv_s(&o.detail))];
            if let Some(d) = o.duration {
                kv.push(("duration", JVal::F(d)));
            }
            let result = jv_o(kv);
            let done_row = jv_o(vec![("done", result.clone())]);
            if let Err(m) = drive_append_row(journal, &done_row) {
                return (1, render_drive_fail(&format!("驱动协议坏了：journal 追加失败：{}", m), &fx.logs));
            }
            (0, render_drive_done(&result, &fx.logs))
        }
        Err(ArkAbort::Ask) => {
            if let Some(m) = fx.io_err.take() {
                return (1, render_drive_fail(&format!("驱动协议坏了：{}", m), &fx.logs));
            }
            match fx.pending.take() {
                Some((seq, action, params)) => (0, render_drive_ask(seq, action, &params, &fx.logs)),
                None => (1, render_drive_fail("驱动协议坏了：Ask 出档却没记断点（内部错）", &fx.logs)),
            }
        }
        Err(ArkAbort::Fail(m)) => (1, render_drive_fail(&m, &fx.logs)),
        Err(ArkAbort::Proto(m)) => (1, render_drive_fail(&format!("驱动协议坏了：{}", m), &fx.logs)),
    }
}

/// 驱动档那一档的出口（`--drive-ark-daily`）。
fn run_drive_ark_daily(
    i_drive: bool,
    journal: &str,
    ctx: &str,
    res: &str,
    panel_cs: &str,
    out: &Option<String>,
) -> i32 {
    let (rc, json) = drive_ark_daily_envelope(i_drive, journal, ctx, res, panel_cs);
    emit(&json, out);
    rc
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
        for sc in &scens {
            let mut dec = |st: &str, ro: bool| arksk_call_decide(panel_cs, st, ro);
            match arkskip_run_case(sc, &mut dec) {
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
fn scan_forbidden_processes() -> Result<Vec<String>, String> {
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap.is_null() || snap as isize == INVALID_HANDLE_ISIZE {
        return Err("CreateToolhelp32Snapshot 失败".to_string());
    }
    let mut entry: ProcessEntry32W = unsafe { std::mem::zeroed() };
    entry.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
    let mut hits: Vec<String> = Vec::new();
    let mut ok = unsafe { Process32FirstW(snap, &mut entry) } != 0;
    while ok {
        let name = String::from_utf16_lossy(&entry.sz_exe_file);
        let name = name.trim_end_matches('\0').to_string();
        if FORBIDDEN_PROCESSES.iter().any(|f| f.eq_ignore_ascii_case(&name)) && !hits.contains(&name) {
            hits.push(name);
        }
        ok = unsafe { Process32NextW(snap, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snap) };
    Ok(hits)
}

// ---- 调用序列（把"要调哪些"从流程里剥出来，好让自检能判它）

/// 连接段在某档下**允许**调用的符号序列。三条硬限制写死在这里，不靠文案：
///   1) 只出 `kind.callable` 里的名字（没签名源的一个都不出）；
///   2) 永远不出 `AsstStart` / `MaaTaskerPostTask` —— R0 不执行任何任务，
///      "最小任务"这一档只到 append + set_params（排队但不启动）；
///   3) `w32cu` 档是空序列（那枚没有本地签名源，连接段对它只查导出）。
/// 三档射程里唯一"真碰设备"的那一档。资源段不算：它只让 DLL 读自己的目录，不碰任何现场。
/// 这条判据被**三处共用**（计划装配 / 实际调用 / 参数硬门），所以必须是一个函数而不是一串比较 ——
/// 三处各写各的 `scope != "shape"`，就会有一处把"读事件形状"悄悄升级成"连设备"。
fn reaches_connect(scope: &str) -> bool {
    scope == "adb-live"
}

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

fn plan_is_safe(kind: &Kind, plan: &[&'static str]) -> Option<String> {
    for name in plan {
        if !kind.callable.iter().any(|c| c == name) {
            return Some(format!("调用序列里混进了无签名源的符号：{}", name));
        }
    }
    for banned in ["AsstStart", "MaaTaskerPostTask", "AsstAttachWindow", "AsstAsyncAttachWindow"] {
        if plan.contains(&banned) {
            return Some(format!("调用序列里出现了本轮禁止的 {}：R0 不执行任务、不挂窗口", banned));
        }
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
    if let Some(bad) = plan_is_safe(kind, &plan) {
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
        let c = unsafe { FreeLibrary(hmod) };
        r.freed = c != 0;
        r.calls.push(format!("FreeLibrary={}", if r.freed { "true" } else { "false" }));
    }
    // ok 的覆盖面在 10-02 加过一次、10-03 又加宽一格：它原先只证"DLL 进得来 + 回调没炸"，于是那两遍
    // "空句柄、后面一步都不做"照样换到 CONNECT-OK —— 这是"ok 从不证 X"同族的第三次；10-03 首跑又印出
    // 了第四次（`ok:true` 与 `MaaAdbControllerCreate=null` 同现），所以第三件扩成**计划里那几枚句柄**：
    // 任一没拿到（null / absent / not-attempted）⇒ ok 必须是 false。
    // 判据仍只住一处（只从 calls 里已有的标签读，与 resource_outcome 同构），信封字段一枚没加。
    r.ok = r.calls.iter().any(|c| c.starts_with("dll=")) && r.cb_panics == 0 && !handle_lost(&r.calls);
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
                }
            }
        }
        // 这里**故意不调 AsstStart**：R0 的连接段不执行任务，所以排队读只到回执为止。
    }

    sleep_with_events(o.timeout_ms);
    if let Some(p) = take(r, hmod, "AsstDestroy") {
        let f: AsstDestroyFn = std::mem::transmute(p);
        f(core);
        r.calls.push("AsstDestroy=done".to_string());
    }
    why
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

    // 6. 连接段三档射程的真值表 —— 全是纯逻辑，不需要装包、不需要 GUI、不需要模拟器在场。
    //    档名要能自己说清射程：中档旧名叫 adb-negative，承诺的是"拿假地址试一次负例连接"，
    //    实测后这条路取消了（设备不在场就没有负例，唯一诚实的形状是这一档压根不连）。
    //    名字留着会继续骗读数，所以改成 resource，并把三档一起钉死。
    let tiers: [(&str, bool); 3] = [("shape", false), ("resource", false), ("adb-live", true)];
    for (sc, want) in tiers {
        chk(
            reaches_connect(sc) == want,
            &format!("{} 档的射程判据不是 {}：三档真值表错了，最高危那一档就没被看住", sc, want),
            &mut fails,
            &mut total,
        );
    }
    chk(!reaches_connect(""), "空档名判成碰设备", &mut fails, &mut total);
    chk(!reaches_connect("adb"), "adb 判成碰设备：字符串判据不许做前缀匹配", &mut fails, &mut total);
    chk(!reaches_connect("live"), "live 判成碰设备（同上）", &mut fails, &mut total);

    // (kind 名, 档, 载资源, 最小任务) 的展开表：每档该出哪些符号、该缺哪些，逐条对
    let cases: [(&str, &str, bool, bool); 9] = [
        ("ark", "shape", true, true),
        ("ark", "resource", true, false),
        ("ark", "resource", false, true),
        ("ark", "adb-live", true, false),
        ("ark", "adb-live", true, true),
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
        chk(plan_is_safe(k, &plan).is_none(), &format!("{}：连接段计划不安全", label), &mut fails, &mut total);
        chk(plan.iter().all(|p| k.callable.iter().any(|c| c == p)), &format!("{}：计划里混进了无签名源的符号", label), &mut fails, &mut total);
        // 启动类符号一次都不许出现：R0 的最小任务只到"排队 + 回执"，不执行
        chk(!has("AsstStart"), &format!("{}：计划里出现了 AsstStart", label), &mut fails, &mut total);
        chk(!has("MaaTaskerPostTask"), &format!("{}：计划里出现了任务提交", label), &mut fails, &mut total);
        chk(!has("AsstAttachWindow") && !has("AsstAsyncAttachWindow"), &format!("{}：计划里出现了挂窗口（本轮禁止）", label), &mut fails, &mut total);
        // 资源段与连接段是分得开的两件事：只有 adb-live 允许出现连接类符号
        if reaches_connect(sc) {
            chk(plan.iter().any(|p| connect_words.contains(p)), &format!("{}：adb-live 档却没有连接类符号（射程写窄了）", label), &mut fails, &mut total);
        } else {
            chk(!plan.iter().any(|p| connect_words.contains(p)), &format!("{}：非 adb-live 档出现了连接类符号 = 偷偷碰设备", label), &mut fails, &mut total);
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
            chk(has("AsstAppendTask") == (mt && reaches_connect(sc)), &format!("{}：最小任务符号出现得不合规则（只在 adb-live 档、且给了最小任务时才有）", label), &mut fails, &mut total);
            chk(has("AsstSetTaskParams") == (mt && reaches_connect(sc)), &format!("{}：排队与设参两条不相配", label), &mut fails, &mut total);
            chk(has("AsstDestroy"), &format!("{}：没有收尾销毁：独占轮会留下句柄", label), &mut fails, &mut total);
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
    let stamp_any: &str = &["", "stamp("].concat();
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
    // --events、--sv、--farm-intent、--arkskip 与 --drive-ark-daily 五档都必须并入同一条 emit 出口（否则 --out 出现"某一档默默忽略"的假口径），
    // 且各自的分派必须落在「--kind 那条必填」**之前**：这五档都不需要装包，被必填拦死就等于没开。
    // （驱动档 R3③ 是第五档：它同样不装包 —— 现场动作全由壳侧照 ask 执行，探针侧只推链与记账。）
    // 这里数的是**整份** SRC 而不是 src_head：分派在 run() 里，而 run() 排在 selftest **之后**，
    // 用 src_head 会两处都找不到、双双退成 MAX 再判成红（这一族"针落在射程外"的坑本轮第二次踩）。
    // needle 一律运行期拼，**且失败文案里也不许把那串字面量原样写出来**：整份一起数时，
    // 自家标签先于 run() 命中，门就红在一句人话上（本轮真踩：文案里的 `--kind` 与那条必填连起来写，
    // SRC.find 先逮到的是这条文案自己 —— 同一族坑第四次，判据读的是自己就必须把自己排除在射程外）。
    // 装配回放档同理只保留**一处** emit 调用（错误路径用局部 rc + 单出口收束）：计数断言本身就
    // 是"出口唯一"的守门人，多开一处它先红。驱动档同理 —— 它的唯一 emit 收在 run_drive_ark_daily 里。
    chk(
        SRC.matches(&["emit(&", "json"].concat()).count() == 5,
        "事件流/监督判定/farm-intent/装配回放/驱动五档没同走一处 emit：--out 在其中一档上是空头承诺",
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
            && xs(&afk, "Reason") == "(-Force) 跳过活动确认门，直接刷 CT-1"
            && xs(&afk, "EventKey") == "act1" && xs(&afk, "EventTip") == "sidestory" && xs(&afk, "Source") == "event",
        "farm/intent·ark：-Force 跳确认门 → event 档取 StageHint，Reason 带(-Force)原话",
        &mut fails,
        &mut total,
    );
    chk(
        xl(&xg(&af, "Warnings")).len() == 1 && x_at(&xg(&af, "Warnings"), 0) == "-Force：本轮忽略活动确认门",
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
    for (i, sc) in scens.iter().enumerate() {
        match arkskip_run_case(sc, &mut fake_decide) {
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
        chk(
            l2.contains("剔关重试仍被 MAA 判全关(计划实况：空) → 今日理智关卡确认全关")
                && l2.contains("MARK#1-7+CE-5#MAA 判「关卡计划内没有当日开放的关卡」")
                && l2.contains("已记今日认栽标记(2026-10-05)：当天后续轮不再前置补刷关")
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
    //     协议五件（拒跑五闸 / journal 装载 / res 合并）先钉死，再走十连真往返：
    //     临时目录里起新账 → 逐动作重入（六轮 ask、一轮 fail、一轮 done、两轮拒跑收场）。
    //     只碰 %TEMP% 下自己的目录，不加载 DLL、不碰任何现场；收尾连目录一起删。
    chk(
        drive_refusal(false, "j", "c", "r", "p").map_or(false, |m| m.contains("--i-drive")),
        "驱动档拒跑闸①走样：缺 --i-drive 没被点名拒跑（默认档可能把换头专用口当普通出口用）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "", "c", "r", "p").map_or(false, |m| m.contains("--journal")),
        "驱动档拒跑闸②走样：journal 没落点没被拒（断点记不住的会话会被放行）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "", "r", "p").map_or(false, |m| m.contains("--ctx")),
        "驱动档拒跑闸③走样：上下文文件没给没被拒（launch/retry_once/decision 没来路）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "c", "", "p").map_or(false, |m| m.contains("--res")),
        "驱动档拒跑闸④走样：回执落点没给没被拒（PS 侧干完一头的落点没处写）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "c", "r", "").map_or(false, |m| m.contains("--panel-cs")),
        "驱动档拒跑闸⑤走样：判定 exe 没指没被拒（重试分流试不了）",
        &mut fails,
        &mut total,
    );
    chk(
        drive_refusal(true, "j", "c", "r", "p").is_none(),
        "驱动档五样齐时仍被拒跑：换头调度器根本起不来（拒跑闸的合取写反了）",
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
    let dr_p = "unused-panel-cs";
    let _ = std::fs::write(&dr_c, "{\"launch\":true,\"retry_once\":false,\"decision\":{\"Mode\":\"plain\",\"Stage\":\"1-7,CE-5\"}}");
    let (dr_rc1, dr_e1) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc1 == 0
            && dr_e1.contains("\"kind\":\"ask\"")
            && dr_e1.contains("\"role\":\"R3-drive-ark-daily\"")
            && dr_e1.contains("\"seq\":2")
            && dr_e1.contains("\"action\":\"desk\"")
            && dr_e1.contains("--- 方舟(端游 + MAA)---"),
        "驱动档第一轮没停在 desk：起新账（init+首行日志）或 ask 信封装错（换头调度器第一步就接不下去）",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":2,\"action\":\"desk\",\"ok\":true,\"data\":\"Default\"}");
    let (dr_rc2, dr_e2) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc2 == 0 && dr_e2.contains("\"seq\":3") && dr_e2.contains("\"action\":\"takeover\""),
        "驱动档第二轮没把 desk 回执并账后停在 takeover：重放推进或新区衔接错",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":3,\"action\":\"takeover\",\"ok\":true}");
    let (dr_rc3, dr_e3) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc3 == 0 && dr_e3.contains("\"seq\":4") && dr_e3.contains("\"params\":{\"kind\":\"main\"}"),
        "驱动档第三轮没停在 cfg(main)：takeover 回执并账或 cfg ask 的 params 装错",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":4,\"action\":\"cfg\",\"ok\":true,\"data\":{\"changes\":[\"chg-main\"],\"chains\":[\"Fight\",\"Shop\"]}}");
    let (dr_rc4, dr_e4) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc4 == 0
            && dr_e4.contains("\"seq\":6")
            && dr_e4.contains("\"action\":\"launch\"")
            && dr_e4.contains("  MAA：chg-main"),
        "驱动档第四轮没停在 launch 或 MAA 变更日志没带上：cfg 回执的 changes 渲染或 seq 基准错（日志占号）",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":6,\"action\":\"launch\",\"ok\":true,\"data\":true}");
    let (dr_rc5, dr_e5) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc5 == 0 && dr_e5.contains("\"seq\":7") && dr_e5.contains("\"action\":\"run\"") && dr_e5.contains("[\"Fight\",\"Shop\"]"),
        "驱动档第五轮没停在 run 或 chains 参数面丢了：launch 回执并账或 run ask 的 params 装错",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_r, "{\"seq\":7,\"action\":\"run\",\"ok\":true,\"data\":{\"status\":\"ok\",\"detail\":\"一切正常\"}}");
    let (dr_rc6, dr_e6) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc6 == 1 && dr_e6.contains("\"kind\":\"fail\"") && dr_e6.contains("decide-daily-retry 起不来"),
        "驱动档第六轮没在坏判定 exe 处当场断（唯一外部调用）：真跑 decide-daily-retry 的失败会被静默吞掉",
        &mut fails,
        &mut total,
    );
    let dr_done_row = jv_o(vec![("done", jv_o(vec![("status", jv_s("ok")), ("detail", jv_s("x")), ("duration", JVal::F(1.5))]))]);
    let _ = drive_append_row(&dr_j, &dr_done_row);
    let (dr_rc7, dr_e7) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc7 == 0
            && dr_e7.contains("\"kind\":\"done\"")
            && dr_e7.contains("{\"status\":\"ok\",\"detail\":\"x\",\"duration\":1.5}")
            && dr_e7.contains("\"logs\":[]"),
        "驱动档第七轮没把末行 done 直接收成 done 信封：重入到已完成会话应原样报结（result 三格/logs 空）",
        &mut fails,
        &mut total,
    );
    let _ = std::fs::write(&dr_c, "{\"launch\":false,\"retry_once\":false,\"decision\":{}}");
    let (dr_rc8, dr_e8) = drive_ark_daily_envelope(true, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc8 == 1 && dr_e8.contains("驱动协议坏了") && dr_e8.contains("不是同一份"),
        "驱动档第八轮没拒掉换 ctx 续旧账：init/ctx 不同源的会话会被静默接续",
        &mut fails,
        &mut total,
    );
    let (dr_rc9, dr_e9) = drive_ark_daily_envelope(false, &dr_j, &dr_c, &dr_r, dr_p);
    chk(
        dr_rc9 == 2 && dr_e9.contains("--i-drive"),
        "驱动档第九轮没在缺 --i-drive 时拒跑（rc2）：默认档能把换头专用口当普通出口用",
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
         \t              --scope <shape|resource|adb-live> [--no-load-resource] [--resource-path <资源落点目录>]\n\
         \t              [--adb <路径>] [--address <host:port>] [--config <json>]\n\
         \t              [--agent-path <MaaAgentBinary 目录>] [--minimal-task <类型>] [--timeout-ms N]\n\
         \t              [--screencap-mask N] [--input-mask N] [--free]\n\
         \t              --i-confirm-gui-absent --i-accept-live --i-window-clear\n\
         \t  三个 --i-* 一个都不能少；助手进程在场时**直接拒跑**（不抢方向盘、不代杀进程）。\n\
         \t  档位射程：shape = 只加载与查导出、一枚函数都不调；\n\
         \t\t\tresource = 起实例 / 建资源 / 绑回调，不碰设备、不提权、不点击；\n\
         \t\t\t\t落点默认 mf=<装包>\\\\resource、ark=<装包根>，--resource-path 可指名模块子树（整目录会被框架判重复键）；\n\
         \t\t\t\t资源加载成没成看信封 resource_load，**别拿 ok 当它**（ok 只证 DLL 进得来 + 回调没炸 + 计划里那几枚句柄都拿到了）；\n\
         \t\t\tadb-live = 唯一真碰设备的一档（建控制器 + 发连接回执）。\n\
         \t  --config 缺省按侧给：mf = {{}}（JSON 对象 —— 非对象字面量会被框架在建控制器处直接判掉）、ark = General；\n\
         \t  --agent-path 是 mf 侧第六枚（上游 Python 绑定核实：agent_path = 装包内 MaaAgentBinary 目录），缺省空串原样传。\n\
         \t  连接段永远不调 AsstStart / 任务提交：R0 的\"最小任务\"只到排队 + 回执。\n\
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
         驱动档（R3③，换头专用）：gdprobe --drive-ark-daily --i-drive --journal <journal 文件> --ctx <上下文文件> --res <回执文件> --panel-cs <判定 exe 路径> [--out <文件>]\n\
         \t  gdcore.exe 出厂即带的本档（同一份 main.rs 编两枚 bin，gdprobe 与 gdcore 同体）：PS 侧换头后由调度器起它，\n\
         \t  一次一动作一进程 —— 进程从 journal 重放已办事项回到断点，把下一个要办的动作写成 ask 信封（seq/action/params）退出；\n\
         \t  壳照单执行完，把结果写 --res 回执，再起下一进程续推。\n\
         \t  三件文件 = 一根会话：journal 是唯一权威账（init 行 + 记录行 + 末行 done），ctx 只在起新账时吞成 init；\n\
         \t  缺 --i-drive 一律拒跑（默认档不许启用它），三件文件缺一同样拒跑 —— 断点记不住的会话不许开。\n\
         \t  与上面四档同理**不加载任何 DLL、不碰任何现场**，所以也不需要 --kind / --dir。\n\
         \t  **唯一外部调用** = --panel-cs 指的那枚判定 exe：链体重试分流真跑 decide-daily-retry\n\
         \t  （照生产同一条 --in/--out 信封协议），所以缺它这一档直接拒跑；不喂它任何本机事实。\n\
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
    let mut drive_journal = String::new();
    let mut drive_ctx = String::new();
    let mut drive_res = String::new();
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
            "--i-drive" => {
                i_drive = true;
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
                if v != "shape" && v != "resource" && v != "adb-live" {
                    eprintln!("未知 scope：{}（只有 shape / resource / adb-live）", v);
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
        return run_drive_ark_daily(i_drive, &drive_journal, &drive_ctx, &drive_res, &panel_cs, &out_file);
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
