using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;
using Microsoft.Win32.SafeHandles;

// 消息窗口只接收原始输入。可见界面、键盘焦点与滚轮接收均属于气质花面板。
internal sealed class DdDeliveryCheck : NativeWindow, IDisposable {
    const int One = 0x31;
    const string ExpectedHash = "72FBA9C7721AC394A0803D0C97835B0DA21C9D6DDA8076FC0584A566CD7B2A9D";
    readonly string output;
    readonly IntPtr panel;
    readonly HashSet<string> devices = new HashSet<string>();
    readonly Hook keyboardObserver;
    readonly IntPtr keyboardHook;
    int down, up;
    bool sampling, wheelSample;

    [StructLayout(LayoutKind.Explicit, Size=40)]
    struct Input {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public ushort Vk;
        [FieldOffset(10)] public ushort Scan;
        [FieldOffset(12)] public uint KeyFlags;
        [FieldOffset(16)] public uint MouseData;
        [FieldOffset(20)] public uint MouseFlags;
        [FieldOffset(24)] public UIntPtr Tag;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct RawDevice { public ushort Page, Usage; public uint Flags; public IntPtr Target; }
    [StructLayout(LayoutKind.Sequential)]
    struct Point { public int X,Y; }
    [StructLayout(LayoutKind.Sequential)]
    struct Rect { public int Left,Top,Right,Bottom; }
    delegate IntPtr Hook(int n,IntPtr msg,IntPtr data);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl,SetLastError=true)] delegate int Button(int code);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl,SetLastError=true)] delegate int Key(int code,int flag);
    [DllImport("user32")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32")] static extern IntPtr GetAncestor(IntPtr hwnd,uint flags);
    [DllImport("user32")] static extern IntPtr WindowFromPoint(Point p);
    [DllImport("user32")] static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
    [DllImport("user32")] static extern bool GetCursorPos(out Point p);
    [DllImport("user32")] static extern bool SetCursorPos(int x,int y);
    [DllImport("user32")] static extern short GetAsyncKeyState(int key);
    [DllImport("user32")] static extern uint MapVirtualKeyW(uint code,uint kind);
    [DllImport("user32",SetLastError=true)] static extern uint SendInput(uint count,Input[] data,int size);
    [DllImport("user32",SetLastError=true)] static extern IntPtr SetWindowsHookExW(int kind,Hook callback,IntPtr module,uint tid);
    [DllImport("user32")] static extern IntPtr CallNextHookEx(IntPtr hook,int n,IntPtr msg,IntPtr data);
    [DllImport("user32")] static extern bool UnhookWindowsHookEx(IntPtr hook);
    [DllImport("user32",SetLastError=true)] static extern bool RegisterRawInputDevices(RawDevice[] devices,uint count,uint size);
    [DllImport("user32")] static extern uint GetRawInputData(IntPtr input,uint command,[Out] byte[] data,ref uint size,uint headerSize);
    [DllImport("user32",CharSet=CharSet.Unicode)] static extern uint GetRawInputDeviceInfoW(IntPtr dev,uint command,StringBuilder buffer,ref uint size);
    [DllImport("kernel32",CharSet=CharSet.Unicode)] static extern IntPtr GetModuleHandleW(string name);
    [DllImport("kernel32",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr LoadLibraryExW(string path,IntPtr file,uint flags);
    [DllImport("kernel32")] static extern bool FreeLibrary(IntPtr lib);
    [DllImport("kernel32",ExactSpelling=true,CharSet=CharSet.Ansi)] static extern IntPtr GetProcAddress(IntPtr lib,string name);
    [DllImport("kernel32")] static extern void SetLastError(uint error);
    [DllImport("kernel32",CharSet=CharSet.Unicode,SetLastError=true)] static extern SafeFileHandle CreateFileW(string name,uint access,uint share,IntPtr attr,uint disposition,uint flags,IntPtr template);

    DdDeliveryCheck(string file,IntPtr owner) {
        output=file;
        panel=owner;
        CreateHandle(new CreateParams { Parent=new IntPtr(-3) }); // HWND_MESSAGE，不参与前台窗口切换。
        var raw=new[]{new RawDevice{Page=1,Usage=6,Flags=0x100,Target=Handle},new RawDevice{Page=1,Usage=2,Flags=0x100,Target=Handle}};
        Log("原始键鼠设备监听="+RegisterRawInputDevices(raw,2,(uint)Marshal.SizeOf(typeof(RawDevice))));
        keyboardObserver=Observe;
        keyboardHook=SetWindowsHookExW(13,keyboardObserver,GetModuleHandleW(null),0);
        Log("低级键盘监听="+(keyboardHook!=IntPtr.Zero));
    }

    void Log(string text) { File.AppendAllText(output,text+Environment.NewLine,Encoding.UTF8); }
    IntPtr Observe(int n,IntPtr msg,IntPtr data) {
        if(n==0 && sampling && !wheelSample && Marshal.ReadInt32(data)==One) {
            if((Marshal.ReadInt32(data,8)&0x80)==0) down++; else up++;
        }
        return CallNextHookEx(IntPtr.Zero,n,msg,data);
    }
    protected override void WndProc(ref Message m) {
        if(m.Msg==0xff && sampling && GetForegroundWindow()==panel) ReadRaw(m.LParam);
        base.WndProc(ref m);
    }
    void ReadRaw(IntPtr handle) {
        const uint header=24;
        uint size=0;
        if(GetRawInputData(handle,0x10000003,null,ref size,header)!=0 || size<40 || size>1024) return;
        var data=new byte[size];
        if(GetRawInputData(handle,0x10000003,data,ref size,header)!=size) return;
        var kind=BitConverter.ToUInt32(data,0);
        bool matches=wheelSample ? kind==0 && (BitConverter.ToUInt16(data,28)&0x0400)!=0
                                : kind==1 && BitConverter.ToUInt16(data,30)==One;
        if(!matches) return;
        var device=new IntPtr(BitConverter.ToInt64(data,8));
        uint chars=0;
        if(device==IntPtr.Zero || GetRawInputDeviceInfoW(device,0x20000007,null,ref chars)!=0 || chars==0 || chars>8192) return;
        var path=new StringBuilder((int)chars);
        if(GetRawInputDeviceInfoW(device,0x20000007,path,ref chars)!=uint.MaxValue) devices.Add(path.ToString());
    }
    void CheckFocus() {
        if(panel==IntPtr.Zero || GetForegroundWindow()!=panel) throw new IOException("面板失焦，停止发送。");
        foreach(int key in new[]{0x10,0x11,0x12,0x5b,0x5c})
            if(GetAsyncKeyState(key)<0) throw new IOException("修饰键处于按下状态，停止发送。");
    }
    bool OverPanel(Point point) { return GetAncestor(WindowFromPoint(point),2)==panel; }
    async Task Sample(string name,bool wheel,Func<string> send) {
        CheckFocus();
        Point original=new Point(), position=new Point();
        bool moved=false;
        try {
            if(wheel) {
                if(!GetCursorPos(out original)) throw new IOException("无法取得鼠标位置。");
                if(!OverPanel(original)) {
                    Rect rect;
                    if(!GetWindowRect(panel,out rect) || !SetCursorPos((rect.Left+rect.Right)/2,(rect.Top+rect.Bottom)/2))
                        throw new IOException("无法定位测试滚轮。");
                    moved=true;
                    GetCursorPos(out position);
                }
            }
            devices.Clear(); down=up=0; wheelSample=wheel; sampling=true;
            Log("【"+name+"】 "+DateTimeOffset.Now.ToString("o"));
            for(int i=0;i<3;i++) {
                CheckFocus();
                Point point;
                if(wheel && (!GetCursorPos(out point) || !OverPanel(point))) throw new IOException("鼠标已移出面板，停止滚轮测试。");
                long start=DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
                Log("第 "+(i+1)+" 次："+await Task.Run(()=>{
                    CheckFocus();
                    return send();
                }));
                await Task.Delay(300);
                CheckFocus();
                long end=DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
                File.AppendAllText(Path.ChangeExtension(output,"samples"),name+"\t"+start+"\t"+end+Environment.NewLine);
                await Task.Delay(90);
            }
            if(!wheel) Log("低级钩子按下/抬起："+down+"/"+up);
            Log("原始输入设备："+(devices.Count==0?"未观察到设备路径":String.Join("；",devices)));
        } finally {
            sampling=false;
            Point now;
            if(moved && GetCursorPos(out now) && now.X==position.X && now.Y==position.Y) SetCursorPos(original.X,original.Y);
        }
    }
    static T Export<T>(IntPtr lib,string name) where T:class {
        var proc=GetProcAddress(lib,name);
        if(proc==IntPtr.Zero) throw new IOException("SDK 缺少 "+name);
        return (T)(object)Marshal.GetDelegateForFunctionPointer(proc,typeof(T));
    }
    async Task Test(string sdk) {
        Log("网吧模式送达检测 "+DateTimeOffset.Now.ToString("o"));
        await Sample("轻量按键",false,()=>{
            var scan=MapVirtualKeyW(One,4);
            if(scan==0 || scan>255) throw new IOException("不能确定主键区数字 1 的扫描码。");
            var press=new Input {Type=1,Scan=(ushort)scan,KeyFlags=8,Tag=new UIntPtr(0x51485844)};
            var release=press; release.KeyFlags|=2;
            return "SendInput 提交 "+SendInput(2,new[]{press,release},40)+"/2";
        });
        await Sample("轻量滚轮",true,()=>"SendInput 提交 "+SendInput(1,new[]{new Input{MouseData=120,MouseFlags=0x0800}},40)+"/1");
        if(!new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator))
            throw new IOException("普通权限，已保留轻量对照；DD SDK 需要管理员权限。");
        using(var stream=File.OpenRead(sdk)) using(var sha=SHA256.Create()) {
            string hash=BitConverter.ToString(sha.ComputeHash(stream)).Replace("-","");
            Log("SDK："+sdk+"；SHA256="+hash);
            if(hash!=ExpectedHash) throw new IOException("DD 文件校验失败。");
            using(var device=CreateFileW(@"\\.\dd63330",0x80000000,0,IntPtr.Zero,3,0x80,IntPtr.Zero)) {
                int error=Marshal.GetLastWin32Error();
                Log(device.IsInvalid?"初始化前打开设备错误="+error:"初始化前可独占打开，已释放。");
                if(device.IsInvalid && error==32) throw new IOException("DD 驱动被其他程序占用。");
            }
            CheckFocus();
            var lib=LoadLibraryExW(sdk,IntPtr.Zero,0x800);
            if(lib==IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            try {
                var init=Export<Button>(lib,"DD_btn");
                int ready=await Task.Run(()=>init(0)); Log("DD_btn(0)="+ready);
                if(ready!=1) throw new IOException("DD 初始化未通过。");
                var key=Export<Key>(lib,"DD_key");
                var dd=Export<Button>(lib,"DD_todc")(One);
                Log("主键区数字 1，VK=49，DD 键码="+dd);
                if(dd<=0) throw new IOException("数字 1 没有有效 DD 键码。");
                await Sample("网吧按键",false,()=>{
                    SetLastError(0); int press=key(dd,1); int pe=Marshal.GetLastWin32Error();
                    Thread.Sleep(12);
                    SetLastError(0); int release=key(dd,2); int re=Marshal.GetLastWin32Error();
                    return String.Format("按下 ret={0} os_error={1}；抬起 ret={2} os_error={3}",press,pe,release,re);
                });
                var scroll=Export<Button>(lib,"DD_whl");
                await Sample("网吧滚轮",true,()=>{
                    SetLastError(0); int value=scroll(1); int err=Marshal.GetLastWin32Error();
                    return "DD_whl ret="+value+" os_error="+err;
                });
            } finally { FreeLibrary(lib); }
        }
    }
    public void Dispose() {
        if(keyboardHook!=IntPtr.Zero) UnhookWindowsHookEx(keyboardHook);
        DestroyHandle();
    }
    [STAThread] static int Main(string[] args) {
        if(args.Length<2) return 2;
        try {
            if(args[0]=="events" && args.Length==3) { IncidentWindowsErrors.Collect(args[1],args[2]); return 0; }
            long hwnd;
            if(args.Length!=4 || args[0]!="delivery" || !Int64.TryParse(args[3],out hwnd)) return 2;
            var context=new WindowsFormsSynchronizationContext();
            SynchronizationContext.SetSynchronizationContext(context);
            int result=0;
            using(var probe=new DdDeliveryCheck(args[1],new IntPtr(hwnd))) {
                context.Post(async unused=>{
                    try { await probe.Test(args[2]); }
                    catch(Exception e) { result=1; probe.Log("检测中止："+e.Message); }
                    finally { Application.ExitThread(); }
                },null);
                Application.Run();
            }
            return result;
        } catch(Exception e) {
            File.AppendAllText(args[1],"采集失败："+e.Message+Environment.NewLine,Encoding.UTF8);
            return 1;
        }
    }
}
