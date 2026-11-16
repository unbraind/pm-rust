/** Windows owns descendants with a kernel job, including children orphaned between snapshots. */
export function windowsJob(command: string, args: string[], marker: string): { command: string; args: string[] } {
  // Assign the kill-on-close job as a creation attribute before resuming. Even
  // interruption during CreateProcess cannot leave an unowned suspended child.
  const declarations = `
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class ProofJob {
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct Startup {
    public int cb; public string reserved, desktop, title;
    public int x,y,xSize,ySize,xChars,yChars,fill,flags; public short show,reserved2;
    public IntPtr reservedPtr,input,output,error;
  }
  [StructLayout(LayoutKind.Sequential)] struct StartupEx { public Startup startup; public IntPtr attributes; }
  [StructLayout(LayoutKind.Sequential)] struct ProcessInfo { public IntPtr process,thread; public int pid,tid; }
  [DllImport("kernel32", SetLastError=true, CharSet=CharSet.Unicode)] static extern bool CreateProcess(string app,StringBuilder cmd,IntPtr pa,IntPtr ta,bool inherit,int flags,IntPtr env,string cwd,ref StartupEx startup,out ProcessInfo info);
  [DllImport("kernel32", SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr attributes,string name);
  [DllImport("kernel32", SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job,int kind,IntPtr data,int length);
  [DllImport("kernel32", SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr list,int count,int flags,ref IntPtr bytes);
  [DllImport("kernel32", SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr list,int flags,IntPtr attribute,IntPtr value,IntPtr size,IntPtr previous,IntPtr returned);
  [DllImport("kernel32")] static extern void DeleteProcThreadAttributeList(IntPtr list);
  [DllImport("kernel32", SetLastError=true)] static extern bool QueryInformationJobObject(IntPtr job,int kind,IntPtr data,int length,IntPtr returned);
  [DllImport("kernel32", SetLastError=true)] static extern bool TerminateJobObject(IntPtr job,uint code);
  [DllImport("kernel32")] static extern uint ResumeThread(IntPtr thread);
  [DllImport("kernel32")] static extern uint WaitForSingleObject(IntPtr handle,uint milliseconds);
  [DllImport("kernel32")] static extern bool GetExitCodeProcess(IntPtr process,out uint code);
  [DllImport("kernel32")] static extern bool TerminateProcess(IntPtr process,uint code);
  [DllImport("kernel32")] static extern bool CloseHandle(IntPtr handle);
  [DllImport("kernel32")] static extern IntPtr GetStdHandle(int id);
  static void Check(bool ok) { if (!ok) throw new System.ComponentModel.Win32Exception(); }
  public static int Run(string command,string marker) {
    IntPtr job=CreateJobObject(IntPtr.Zero,marker), data=Marshal.AllocHGlobal(144);
    ProcessInfo child=new ProcessInfo(); bool assigned=false, initialized=false;
    IntPtr attributes=IntPtr.Zero, jobList=IntPtr.Zero, bytes=IntPtr.Zero;
    try {
      Check(job!=IntPtr.Zero);
      for(int i=0;i<144;i++) Marshal.WriteByte(data,i,0);
      Marshal.WriteInt32(data,16,0x2000); // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
      Check(SetInformationJobObject(job,9,data,IntPtr.Size==8?144:112));
      InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref bytes);
      attributes=Marshal.AllocHGlobal(bytes); jobList=Marshal.AllocHGlobal(IntPtr.Size);
      Check(InitializeProcThreadAttributeList(attributes,1,0,ref bytes)); initialized=true;
      Marshal.WriteIntPtr(jobList,job);
      Check(UpdateProcThreadAttribute(attributes,0,new IntPtr(0x2000d),jobList,new IntPtr(IntPtr.Size),IntPtr.Zero,IntPtr.Zero)); // PROC_THREAD_ATTRIBUTE_JOB_LIST
      StartupEx startup=new StartupEx(); startup.startup.cb=Marshal.SizeOf(startup); startup.startup.flags=0x100; startup.attributes=attributes;
      startup.startup.input=GetStdHandle(-10); startup.startup.output=GetStdHandle(-11); startup.startup.error=GetStdHandle(-12);
      Check(CreateProcess(null,new StringBuilder(command),IntPtr.Zero,IntPtr.Zero,true,0x80004,IntPtr.Zero,null,ref startup,out child)); assigned=true;
      Check(ResumeThread(child.thread)!=0xffffffff);
      Check(WaitForSingleObject(child.process,0xffffffff)==0);
      uint code; Check(GetExitCodeProcess(child.process,out code));
      Check(TerminateJobObject(job,code));
      for(int i=0;;i++) {
        Check(QueryInformationJobObject(job,1,data,48,IntPtr.Zero));
        if(Marshal.ReadInt32(data,40)==0) break; // ActiveProcesses
        if(i>=500) throw new Exception("Windows job did not drain");
        Thread.Sleep(10);
      }
      Console.Error.Write(marker);
      return unchecked((int)code);
    } finally {
      if(!assigned && child.process!=IntPtr.Zero) TerminateProcess(child.process,1);
      if(child.thread!=IntPtr.Zero) CloseHandle(child.thread);
      if(child.process!=IntPtr.Zero) CloseHandle(child.process);
      if(job!=IntPtr.Zero) CloseHandle(job);
      if(initialized) DeleteProcThreadAttributeList(attributes);
      if(attributes!=IntPtr.Zero) Marshal.FreeHGlobal(attributes);
      if(jobList!=IntPtr.Zero) Marshal.FreeHGlobal(jobList);
      Marshal.FreeHGlobal(data);
    }
  }
}`;
  /** Quote one argv element using the Windows CRT backslash and quote rules. */
  const quote = (value: string): string => `"${value.replace(/(\\*)"/g, '$1$1\\"').replace(/(\\+)$/g, '$1$1')}"`;
  const line = [command, ...args].map(quote).join(' ');
  const payload = Buffer.from(JSON.stringify({ command, args, line, marker })).toString('base64');
  const script = `$ErrorActionPreference='Stop'\n$invocation = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('${payload}')) | ConvertFrom-Json\nAdd-Type -TypeDefinition @'\n${declarations}\n'@\nexit ([ProofJob]::Run($invocation.line,$invocation.marker))`;
  return { command: 'powershell.exe', args: ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')] };
}

/** Terminate the uniquely named kernel job without looking up or trusting a process ID. */
export function windowsJobStop(marker: string): { command: string; args: string[] } {
  const declarations = `using System; using System.Runtime.InteropServices; using System.Threading;
public static class StopProofJob {
 [DllImport("kernel32",SetLastError=true,CharSet=CharSet.Unicode)] static extern IntPtr OpenJobObject(int access,bool inherit,string name);
 [DllImport("kernel32",SetLastError=true)] static extern bool TerminateJobObject(IntPtr job,uint code);
 [DllImport("kernel32",SetLastError=true)] static extern bool QueryInformationJobObject(IntPtr job,int kind,IntPtr data,int length,IntPtr returned);
 [DllImport("kernel32")] static extern bool CloseHandle(IntPtr handle);
 public static void Run(string name) {
  IntPtr job=OpenJobObject(12,false,name);
  if(job==IntPtr.Zero) { if(Marshal.GetLastWin32Error()==2) return; throw new System.ComponentModel.Win32Exception(); }
  IntPtr data=Marshal.AllocHGlobal(48);
  try {
   if(!TerminateJobObject(job,1)) throw new System.ComponentModel.Win32Exception();
   for(int i=0;;i++) {
    if(!QueryInformationJobObject(job,1,data,48,IntPtr.Zero)) throw new System.ComponentModel.Win32Exception();
    if(Marshal.ReadInt32(data,40)==0) return;
    if(i>=500) throw new Exception("Windows job did not drain");
    Thread.Sleep(10);
   }
  } finally { Marshal.FreeHGlobal(data); CloseHandle(job); }
 }
}`;
  const name = Buffer.from(marker).toString('base64');
  const script = `$ErrorActionPreference='Stop'\nAdd-Type -TypeDefinition @'\n${declarations}\n'@\n[StopProofJob]::Run([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('${name}')))`;
  return { command: 'powershell.exe', args: ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')] };
}
