// gdprobe —— 换宿主方案(Rust 核) R0 **只读段**探针。契约号：RustCore-R0.1。
//
// 这个文件只做两件不需要碰游戏、不碰助手实例的事：
//   1) 从**内容**读版本（DLL 自己报的版本号 + 装包里 interface.json 的资源轴字段），
//   2) 用 LoadLibrary + GetProcAddress 确认「本轮要调的函数在不在导出表里」。
// 连接、追加任务、start/stop 那半**不在这里**：按 docs\rust-core-plan.md §4 第 6 项，
// 那半会动助手侧现场，只在 GUI 确认不在场的窗口手动跑，且要留"改了哪些现场"的说明。
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

const CONTRACT: &str = "RustCore-R0.1";
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
fn rel_ms() -> i64 {
    let t0 = *T0_FILETIME.get_or_init(filetime_now);
    filetime_now().saturating_sub(t0) as i64 / 10_000
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

fn connect_plan(kind_name: &str, scope: &str, load_resource: bool, minimal_task: bool) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    // shape 档：一枚函数都不调（版本自报除外）—— 它读的是"能不能加载、导出了什么"，不是"能不能连上"。
    if scope == "shape" {
        return v;
    }
    match kind_name {
        "ark" => {
            v.push("AsstSetUserDir");
            v.push("AsstCreateEx");
            if load_resource {
                v.push("AsstLoadResource");
            }
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
        // 遗留侧 AsstLoadResource 按惯例吃**装包根**（自己去找 resource 目录）—— 此说**未经实跑核验**：本地无 header、
        // asst.py 给的是签名不是语义，M9A 那次实测的是 framework 侧 PostPipeline，不许外推。ark 档跑起来由 resource_load 说。
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
            }
        }
    }
    "not-reported"
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
        freed: false,
    };

    if !r.gate_reasons.is_empty() {
        r.why = "闸没过：这一档什么都不做，DLL 一个都不加载、一个字都不写".to_string();
        return r;
    }
    if kind.callable.is_empty() {
        r.why = "这一侧的连接段没有任何有签名源的函数，只准查导出、不准调".to_string();
        return r;
    }
    if !std::path::Path::new(&o.dir).is_dir() {
        r.why = "装包目录不在场（前置不满足，不是代码错）".to_string();
        return r;
    }
    if std::fs::create_dir_all(&o.user_dir).is_err() {
        r.why = "写入面建不出来：连接段拒绝在落盘收窄失败的情况下继续".to_string();
        return r;
    }

    let plan = connect_plan(kind.name, &o.scope, o.load_resource, !o.minimal_task.is_empty());
    if let Some(bad) = plan_is_safe(kind, &plan) {
        r.why = bad;
        return r;
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
        return r;
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

    let started = filetime_now();
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
    r.waited_ms = filetime_now().saturating_sub(started) / 10_000;
    r.why = wrote;

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
    r.ok = r.calls.iter().any(|c| c.starts_with("dll=")) && r.cb_panics == 0;
    r
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
    if let Some(p) = take(r, hmod, "AsstCreateEx") {
        let f: AsstCreateExFn = std::mem::transmute(p);
        core = f(ark_cb as *const () as *mut c_void, std::ptr::null_mut());
        r.calls.push(format!("AsstCreateEx={}", if core.is_null() { "null" } else { "handle" }));
        if core.is_null() {
            return "AsstCreateEx 返回空句柄，后面一步都不做".to_string();
        }
    } else {
        return "拿不到 AsstCreateEx：这一侧的连接段没有可起步的入口".to_string();
    }

    if o.load_resource {
        if let Some(p) = take(r, hmod, "AsstLoadResource") {
            let d = cstr_bytes(&effective_resource("ark", &o.dir, &o.resource_path));
            let f: AsstLoadResourceFn = std::mem::transmute(p);
            let got = f(d.as_ptr());
            r.calls.push(format!("AsstLoadResource={}", got));
        }
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
            let ag = cstr_bytes("{}");
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
    let outcome_spell: &str = &["resource", "_outcome("].concat();
    let eff_spell: &str = &["effective", "_resource("].concat();
    // 数"判据只住一处"必须**只数 selftest 之外**的那半份源码：自检里对这两个函数的每次调用都是真调用点，
    // 整份一起数就把自己数进去了（同一类坑这一轮第三次踩：needle 只要出现在射程内就会骗读数）。
    let src_head: &str = SRC.split("fn selftest(").next().unwrap_or("");
    chk(src_head.matches(outcome_spell).count() == 2, "资源成败的判据有了第二处 spelling：执行面与信封面会各说各话", &mut fails, &mut total);
    chk(src_head.matches(eff_spell).count() == 4, "落点口径没共用同一处（定义 + mf/ark/投影 三处使用）", &mut fails, &mut total);

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
        "gdprobe {} —— R0 探针。只读段随时能跑；连接段要三闸。\n\
         \n\
         只读段：gdprobe --kind <ark|mf|w32cu> --dir <装包目录> [--show-path] [--free]\n\
         \t  gdprobe --list-kinds      列出两侧装包形状（含各自必需/连接符号清单）\n\
         \t  gdprobe --preflight       只读环境闸：助手进程在不在场，不加载任何 DLL\n\
         \n\
         连接段：gdprobe --connect --kind <ark|mf> --dir <装包目录> --user-dir <我方写入目录>\n\
         \t              --scope <shape|resource|adb-live> [--no-load-resource] [--resource-path <资源落点目录>]\n\
         \t              [--adb <路径>] [--address <host:port>] [--config <json>]\n\
         \t              [--minimal-task <类型>] [--timeout-ms N] [--screencap-mask N] [--input-mask N] [--free]\n\
         \t              --i-confirm-gui-absent --i-accept-live --i-window-clear\n\
         \t  三个 --i-* 一个都不能少；助手进程在场时**直接拒跑**（不抢方向盘、不代杀进程）。\n\
         \t  档位射程：shape = 只加载与查导出、一枚函数都不调；\n\
         \t\t\tresource = 起实例 / 建资源 / 绑回调，不碰设备、不提权、不点击；\n\
         \t\t\t\t落点默认 mf=<装包>\\\\resource、ark=<装包根>，--resource-path 可指名模块子树（整目录会被框架判重复键）；\n\
         \t\t\t\t资源加载成没成看信封 resource_load，**别拿 ok 当它**（ok 只证 DLL 进得来 + 回调没炸）；\n\
         \t\t\tadb-live = 唯一真碰设备的一档（建控制器 + 发连接回执）。\n\
         \t  连接段永远不调 AsstStart / 任务提交：R0 的\"最小任务\"只到排队 + 回执。\n\
         \t  没有本地签名源的函数（ark 侧 AsstAttachWindow / AsstConnected 等）只查导出、不调用。\n\
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

fn main() {
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
    let mut config = "General".to_string();
    let mut load_resource = true;
    let mut resource_path = String::new();
    let mut minimal_task = String::new();
    let mut timeout_ms = 4_000u64;
    let mut screencap_mask = 0xFFFF_FFFFu64;
    let mut input_mask = 0xFFFF_FFFFu64;
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
