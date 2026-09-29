// GdDesk.cs - B0(2026-09-29)：这段 P/Invoke 原来内联在 lib\common.ps1 的 Get-GdInputDesktopName 里，
// 逐字搬来，类名/成员名一字未改。它是"会话现在落在哪个输入桌面"那一线读数(锁屏/屏保桌面接管)的现场调用。
// 装载：lib\common.ps1 的 Add-GdInterop 对 tools\gdinterop\*.cs 现编一次；编不过 Add-GdInterop 只落状态不抛，
// 调用方按"读不到"处理(返回空串)，与旧 here-string 编译失败时走 catch 返回 '' 的语义一致。
// 硬约束同 GdPower.cs：C#5 上限、只引 BCL、不许出现 System.Drawing 类型、本目录允许中文注释。
// 注意(2026-09-26 实测在案)：这里的 Add-Type 绝不能加 -ErrorAction SilentlyContinue —— 那样会
// "编译成功但类型不落地"，之后每次 [GdDesk]:: 都静默走 catch 返回空串，排查过一次。
// B0-VERBATIM-BELOW
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class GdDesk {
    [DllImport("user32.dll")] public static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool GetUserObjectInformationW(IntPtr hObj, int nIndex, StringBuilder pvInfo, uint nLength, out uint lpnLengthNeeded);
    [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr hDesktop);
}
