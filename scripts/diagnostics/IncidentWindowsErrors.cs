using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Diagnostics.Eventing.Reader;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Security.Cryptography.X509Certificates;
using System.Security.Principal;
using System.Text;
using System.Text.RegularExpressions;
using System.Xml.Linq;
using Microsoft.Win32;

internal static class IncidentWindowsErrors {
    // 本地零点可以与当前时刻属于不同的夏令时偏移。
    static DateTimeOffset From, Until;
    static readonly Regex Related = new Regex(@"dd63330|dd40800|TgKeyboard|interception|keyboard\.sys|kbdclass|kbdhid|(?:hid|usb)\\vid_[0-9a-f]{4}|flair[- ]?bloom|DdDeliveryCheck|气质花|千机魔盒",RegexOptions.IgnoreCase);
    static bool InWindow(DateTime utc) { return utc>=From.UtcDateTime && utc<=Until.UtcDateTime; }
    static string Time(DateTime utc) { return new DateTimeOffset(utc.ToUniversalTime()).ToLocalTime().ToString("MM-dd HH:mm:ss.fff"); }
    static string Compact(string text,int max) {
        string s=Regex.Replace(text??"",@"\s+"," ").Trim();
        s=Regex.Replace(s,@"(?i)(\\Users\\)[^\\]+\\","$1*\\");
        return s.Length<=max?s:s.Substring(0,max)+"…";
    }
    static void Events(TextWriter w,string channel) {
        w.WriteLine("\n【"+channel+"】");
        try {
            using(var cfg=new EventLogConfiguration(channel)) {
                w.WriteLine("通道当前启用="+cfg.IsEnabled+"；记录模式="+cfg.LogMode);
            }
            string format="yyyy-MM-ddTHH:mm:ss.fffZ";
            string xpath="*[System[TimeCreated[@SystemTime>='"+From.UtcDateTime.ToString(format,CultureInfo.InvariantCulture)+"' and @SystemTime<='"+Until.UtcDateTime.ToString(format,CultureInfo.InvariantCulture)+"']]]";
            int read=0, matched=0;
            var corrected = new Dictionary<string, Tuple<int,string,string>>();
            using(var reader=new EventLogReader(new EventLogQuery(channel,PathType.LogName,xpath))) {
                EventRecord record;
                while((record=reader.ReadEvent())!=null) using(record) {
                    read++;
                    var doc=XElement.Parse(record.ToXml());
                    var fields=doc.Descendants().Where(x=>x.Name.LocalName=="Data").ToList();
                    string provider=record.ProviderName??"";
                    bool wer=Regex.IsMatch(provider,@"Windows Error Reporting|WER-SystemErrorReporting|Application Error|Application Hang|BugCheck|WHEA-Logger",RegexOptions.IgnoreCase);
                    bool relevant=Related.IsMatch(doc.Value)||Related.IsMatch(provider);
                    bool serious=record.Level.HasValue && record.Level>=1 && record.Level<=2;
                    bool driverWarn=record.Level==3 && Regex.IsMatch(provider,@"Kernel-PnP|DriverFrameworks|Kernel-Power|CodeIntegrity|HID",RegexOptions.IgnoreCase);
                    if(!relevant && !wer && !serious && !driverWarn) continue;
                    matched++;
                    if (provider == "Microsoft-Windows-WHEA-Logger" && record.Id == 17) {
                        string key = Compact(String.Join("；",fields.Where(f => Regex.IsMatch((string)f.Attribute("Name") ?? "", @"^(Segment|Bus|Device|Function|PrimaryDeviceName|SecondaryDeviceName|CorrectableErrorStatus)$")).Select(f => (string)f.Attribute("Name")+"="+f.Value)),750);
                        string at = Time(record.TimeCreated.Value.ToUniversalTime());
                        Tuple<int,string,string> prior;
                        corrected[key] = corrected.TryGetValue(key,out prior) ? Tuple.Create(prior.Item1+1,prior.Item2,at) : Tuple.Create(1,at,at);
                        continue;
                    }
                    w.WriteLine("{0}  {1}  ID={2}  记录号={3}  级别={4}",Time(record.TimeCreated.Value.ToUniversalTime()),provider,record.Id,record.RecordId,record.Level);
                    if(provider.Equals("Windows Error Reporting",StringComparison.OrdinalIgnoreCase)) {
                        foreach(var d in fields) {
                            string key=(string)d.Attribute("Name")??"";
                            if(Regex.IsMatch(key,@"^(EventName|Bucket|BucketType|ReportStatus|P[1-9]|P10)$",RegexOptions.IgnoreCase)) w.WriteLine("  "+key+"="+Compact(d.Value,120));
                        }
                    } else {
                        string message=null; try { message=record.FormatDescription(); } catch(EventLogException) {}
                        if(String.IsNullOrWhiteSpace(message)) message=String.Join("；",fields.Select(d=>((string)d.Attribute("Name")??"数据")+"="+d.Value));
                        w.WriteLine("  "+Compact(message,750));
                    }
                }
            }
            foreach(var error in corrected) w.WriteLine("WHEA 17 ×"+error.Value.Item1+"；首次 "+error.Value.Item2+"；末次 "+error.Value.Item3+"："+error.Key);
            w.WriteLine("时间窗内读取 "+read+" 条，匹配 "+matched+" 条。");
        } catch(Exception e) { w.WriteLine("读取未完成："+e.Message); }
    }
    static void WerFolder(TextWriter w,string root,string label) {
        w.WriteLine("\n【WER "+label+"】");
        int scanned=0,matched=0,denied=0,fallback=0;
        try {
            foreach(var dir in Directory.EnumerateDirectories(root)) {
                try {
                    var fields=new Dictionary<string,string>(StringComparer.OrdinalIgnoreCase);
                    string file=Path.Combine(dir,"Report.wer");
                    using(var stream=new FileStream(file,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete))
                    using(var reader=new StreamReader(stream,Encoding.UTF8,true)) {
                        string s;
                        while((s=reader.ReadLine())!=null) {
                            int i=s.IndexOf('='); if(i<=0) continue; string k=s.Substring(0,i);
                            if(Regex.IsMatch(k,@"^(EventType|EventTime|AppName|Sig\[\d+\]\.(Name|Value))$")) fields[k]=s.Substring(i+1);
                        }
                    }
                    scanned++; string raw; long ft; DateTime time;
                    if(fields.TryGetValue("EventTime",out raw)&&Int64.TryParse(raw,out ft)&&ft>0 && ft<=2650467743999999999L) time=DateTime.FromFileTimeUtc(ft);
                    else {time=File.GetLastWriteTimeUtc(file);fallback++;}
                    if(!InWindow(time)) continue;
                    matched++; w.WriteLine(Time(time)+"  "+Compact(fields.ContainsKey("EventType")?fields["EventType"]:"未知类型",80));
                    if(fields.ContainsKey("AppName")) w.WriteLine("  应用="+Compact(fields["AppName"],160));
                    foreach(var pair in fields.Where(x=>x.Key.StartsWith("Sig[")&&x.Key.EndsWith(".Name"))) {
                        string value;if(fields.TryGetValue(pair.Key.Replace(".Name",".Value"),out value)) w.WriteLine("  "+Compact(pair.Value,70)+"="+Compact(value,180));
                    }
                } catch(FileNotFoundException) {} catch(DirectoryNotFoundException) {} catch(UnauthorizedAccessException) {denied++;} catch(IOException) {denied++;}
            }
            w.WriteLine("已读报告 "+scanned+" 份，时间窗内 "+matched+" 份，无法读取 "+denied+" 份，使用文件时间 "+fallback+" 份。");
        } catch(DirectoryNotFoundException) {w.WriteLine("目录不存在。");} catch(Exception e){w.WriteLine("读取未完成："+e.Message);}
    }
    static void Driver(TextWriter w,string name) {
        using(var k=Registry.LocalMachine.OpenSubKey(@"SYSTEM\CurrentControlSet\Services\"+name)) {
            if(k==null){w.WriteLine(name+"：当前未注册。");return;}
            string value=k.GetValue("ImagePath") as string;
            string path=String.IsNullOrWhiteSpace(value)?@"System32\drivers\"+name+".sys":value.Trim('"');
            if(path.StartsWith(@"\??\"))path=path.Substring(4);
            if(path.StartsWith(@"\SystemRoot\",StringComparison.OrdinalIgnoreCase))path=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows),path.Substring(12));
            if(!Path.IsPathRooted(path))path=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows),path);
            w.WriteLine(name+"："+Compact(path,240)+(value==null?"（按默认路径查询）":""));
            if(!File.Exists(path)){w.WriteLine("  当前未找到文件。");return;}
            var v=FileVersionInfo.GetVersionInfo(path);
            w.WriteLine("  厂商="+v.CompanyName+"；文件版本="+v.FileVersion+"；描述="+v.FileDescription);
            try { using(var cert=new X509Certificate2(X509Certificate.CreateFromSignedFile(path))) w.WriteLine("  嵌入签名主体="+cert.Subject); }
            catch(System.Security.Cryptography.CryptographicException){w.WriteLine("  未读到嵌入签名，可能使用目录签名。");}
        }
    }
    public static void Collect(string path,string localDate) {
        var day=DateTime.ParseExact(localDate,"yyyy-MM-dd",CultureInfo.InvariantCulture,DateTimeStyles.AssumeLocal).Date;
        From=new DateTimeOffset(day);
        var last=new DateTimeOffset(day.AddDays(1)).AddTicks(-1);
        var now=DateTimeOffset.Now;
        Until=now<last?now:last;
        using(var w=new StreamWriter(path,false,new UTF8Encoding(true))) {

            w.WriteLine("气质花故障 · Windows 系统事件与错误报告");
            w.WriteLine("本机时间窗："+From.ToString("o")+" 至 "+Until.ToString("o"));
            w.WriteLine("管理员="+new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator));
            foreach(string log in new[]{"System","Application","Microsoft-Windows-Kernel-PnP/Configuration","Microsoft-Windows-DriverFrameworks-UserMode/Operational","Microsoft-Windows-CodeIntegrity/Operational","Microsoft-Windows-Windows Defender/Operational"})
                Events(w,log);
            foreach(var home in new[]{Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData),Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData)})
                foreach(string type in new[]{"ReportArchive","ReportQueue"})
                    WerFolder(w,Path.Combine(home,@"Microsoft\Windows\WER",type),(home==Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData)?"系统/":"用户/")+type);
            w.WriteLine("\n【采集当下的键鼠驱动文件】");
            var services = new SortedSet<string>(new[]{"keyboard","kbdclass","kbdhid","mouclass","mouhid","DD63330","DD40800"},StringComparer.OrdinalIgnoreCase);
            foreach(var entry in new[]{Tuple.Create("键盘","{4d36e96b-e325-11ce-bfc1-08002be10318}"),Tuple.Create("鼠标","{4d36e96f-e325-11ce-bfc1-08002be10318}")}) {
                using(var k=Registry.LocalMachine.OpenSubKey(@"SYSTEM\CurrentControlSet\Control\Class\"+entry.Item2)) {
                    if(k==null) continue;
                    foreach(string property in new[]{"UpperFilters","LowerFilters"}) {
                        var filters=(k.GetValue(property) as string[])??new string[0];
                        w.WriteLine(entry.Item1+"类 "+property+"="+String.Join(", ",filters));
                        services.UnionWith(filters);
                    }
                }
            }
            foreach(string name in services) {
                try {Driver(w,name);}catch(Exception e){w.WriteLine(name+" 文件信息读取失败："+e.Message);}
            }
            w.WriteLine("\n收集结束。");
        }
    }
}
