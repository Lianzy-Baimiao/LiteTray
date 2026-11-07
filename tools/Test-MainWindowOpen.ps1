param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][string]$Label,[ValidateSet('calendar','tray','direct','manage','focus','alarms','entry','notification')][string[]]$Entries=@('calendar','tray','direct','manage','focus','alarms','entry','notification'),[ValidateSet(0,1,2)][int]$StartupShow=0,[switch]$AuditPages,[switch]$AllowOverflow)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class ColdSettings {
 public struct Rect {public int left,top,right,bottom;}
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] public struct Startup { public uint cb; public string reserved,desktop,title; public uint x,y,width,height,charsX,charsY,fill,flags; public ushort show,reservedSize; public IntPtr reserved2,input,output,error; }
 [StructLayout(LayoutKind.Sequential)] public struct ProcessInfo { public IntPtr process,thread; public uint pid,tid; }
 [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] public static extern IntPtr CreateDesktopW(string name,IntPtr device,IntPtr devmode,uint flags,uint access,IntPtr security);
 [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr desktop);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] public static extern bool CreateProcessW(string app,StringBuilder command,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr environment,string directory,ref Startup startup,out ProcessInfo process);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
 [DllImport("kernel32.dll")] public static extern uint WaitForSingleObject(IntPtr handle,uint ms);
 [DllImport("kernel32.dll")] public static extern bool TerminateProcess(IntPtr handle,uint code);
 public delegate bool EnumProc(IntPtr hwnd,IntPtr data);
 [DllImport("user32.dll")] static extern bool EnumDesktopWindows(IntPtr desktop,EnumProc callback,IntPtr data);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr hwnd,StringBuilder text,int max);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hwnd,StringBuilder text,int max);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr hwnd,int id);
 [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr hwnd,uint command);
 [DllImport("user32.dll",EntryPoint="GetWindowLongW")] public static extern uint Style(IntPtr hwnd,int index);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int w,int h,uint flags);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd,out Rect rect);
 [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr hwnd,IntPtr rect,IntPtr region,uint flags);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr parent,IntPtr after,string className,string title);
 [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd,uint msg,UIntPtr w,IntPtr l);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr GetPropW(IntPtr hwnd,string name);
 [DllImport("user32.dll")] static extern IntPtr SendMessageTimeoutW(IntPtr hwnd,uint msg,UIntPtr w,IntPtr l,uint flags,uint ms,out UIntPtr result);
 public static void Click(IntPtr hwnd) { UIntPtr result; if(hwnd==IntPtr.Zero||SendMessageTimeoutW(hwnd,0xf5,UIntPtr.Zero,IntPtr.Zero,2,2000,out result)==IntPtr.Zero)throw new Exception("native click failed"); }
 public static void Command(IntPtr hwnd,int id) {UIntPtr result;if(SendMessageTimeoutW(hwnd,0x111,(UIntPtr)(uint)id,IntPtr.Zero,2,2000,out result)==IntPtr.Zero)throw new Exception("page navigation timed out");}
 public static string Title(IntPtr hwnd) {var b=new StringBuilder(256);GetWindowTextW(hwnd,b,256);return b.ToString();}
 public static string Class(IntPtr hwnd) {var b=new StringBuilder(256);GetClassNameW(hwnd,b,256);return b.ToString();}
 public static IntPtr[] Windows(IntPtr desktop,uint pid,string prefix) {var a=new List<IntPtr>();EnumDesktopWindows(desktop,(h,d)=>{uint p;GetWindowThreadProcessId(h,out p);var b=new StringBuilder(128);GetClassNameW(h,b,128);if(p==pid&&b.ToString().StartsWith(prefix))a.Add(h);return true;},IntPtr.Zero);return a.ToArray();}
}
'@
[void][ColdSettings]::SetThreadDpiAwarenessContext([IntPtr](-4))
$exe=(Resolve-Path -LiteralPath $Executable).Path
$evidenceRoot=Join-Path (Split-Path $PSScriptRoot -Parent) 'local\main-window-checks'
[IO.Directory]::CreateDirectory($evidenceRoot)|Out-Null
$reports=@()
foreach($entry in $Entries) {
 $token=[Guid]::NewGuid().ToString('N')
 $desktopName='LiteTray.PackageCheck.'+$token
 $profile=Join-Path $evidenceRoot $token
 [IO.Directory]::CreateDirectory($profile)|Out-Null
 [IO.File]::WriteAllText((Join-Path $profile 'config.json'),'{"tray":{"enabled":false,"clock_enabled":false},"weather":{"enabled":false},"updates":{"check_on_startup":false}}')
 $desktop=[ColdSettings]::CreateDesktopW($desktopName,[IntPtr]::Zero,[IntPtr]::Zero,0,0x10000000,[IntPtr]::Zero)
 if($desktop -eq [IntPtr]::Zero){throw 'Cannot create non-input test desktop'}
 $child=New-Object ColdSettings+ProcessInfo
 $main=[IntPtr]::Zero
 try {
  $startup=New-Object ColdSettings+Startup
  $startup.cb=[Runtime.InteropServices.Marshal]::SizeOf($startup)
  $startup.desktop=$desktopName
  $startup.flags=1
  $startup.show=$StartupShow
  $priorProfile=$env:LITETRAY_DATA_DIR
  try {
   $env:LITETRAY_DATA_DIR=$profile
   $command=[Text.StringBuilder]::new('"'+$exe+'" --background')
   if(-not [ColdSettings]::CreateProcessW($exe,$command,[IntPtr]::Zero,[IntPtr]::Zero,$false,0x08000000,[IntPtr]::Zero,(Split-Path $exe -Parent),[ref]$startup,[ref]$child)){throw 'Cannot launch cold package'}
  } finally { $env:LITETRAY_DATA_DIR=$priorProfile }
  [void][ColdSettings]::CloseHandle($child.thread)
  $deadline=[DateTime]::UtcNow.AddSeconds(8)
  while([DateTime]::UtcNow -lt $deadline) {
   $found=@([ColdSettings]::Windows($desktop,$child.pid,'LiteTray.Settings.'))
   if($found.Count -eq 1){$main=$found[0];break}
   Start-Sleep -Milliseconds 40
  }
  if($main -eq [IntPtr]::Zero){throw 'Cold package did not create settings'}
  # Wait for prewarming, without invoking ShowWindow on the settings surface.
  $deadline=[DateTime]::UtcNow.AddSeconds(5)
  do {
   $panels=@([ColdSettings]::Windows($desktop,$child.pid,'LiteTray.Calendar.'))
   if($panels.Count -gt 0){break}
   Start-Sleep -Milliseconds 40
  } while([DateTime]::UtcNow -lt $deadline)
  if([ColdSettings]::IsWindowVisible($main)){throw 'Background launch unexpectedly opened settings'}
  if($entry -in @('calendar','manage','focus')) {
   [void][ColdSettings]::PostMessageW($main,0x8004,[UIntPtr]::Zero,[IntPtr]::Zero)
   $deadline=[DateTime]::UtcNow.AddSeconds(3)
   $popup=[IntPtr]::Zero
   do {
    foreach($candidate in @([ColdSettings]::Windows($desktop,$child.pid,'LiteTray.Calendar.'))){if([ColdSettings]::IsWindowVisible($candidate)){$popup=$candidate;break}}
    if($popup -ne [IntPtr]::Zero){break}
    Start-Sleep -Milliseconds 20
   } while([DateTime]::UtcNow -lt $deadline)
   if($popup -eq [IntPtr]::Zero){throw 'Calendar did not open'}
   $calendar=[ColdSettings]::GetDlgItem($popup,950)
   $button=switch($entry){'manage'{1106};'focus'{1104};default{40}}
   [ColdSettings]::Click([ColdSettings]::GetDlgItem($calendar,$button))
  } elseif($entry -eq 'tray') {
   [void][ColdSettings]::PostMessageW($main,0x111,[UIntPtr]40,[IntPtr]::Zero)
  } elseif($entry -eq 'alarms') {
   [void][ColdSettings]::PostMessageW($main,0x8057,[UIntPtr]::Zero,[IntPtr]::Zero)
  } elseif($entry -eq 'entry') {
   [void][ColdSettings]::PostMessageW($main,0x8056,[UIntPtr]::Zero,[IntPtr]::Zero)
  } elseif($entry -eq 'notification') {
   [void][ColdSettings]::PostMessageW($main,0x8001,[UIntPtr]::Zero,[IntPtr]0x405)
  } else {
   [void][ColdSettings]::PostMessageW($main,0x8003,[UIntPtr]::Zero,[IntPtr]::Zero)
  }
  $deadline=[DateTime]::UtcNow.AddMilliseconds(500)
  do {
   $requested=[ColdSettings]::GetPropW($main,'LiteTray.SettingsOpen.Requested').ToInt64()
   $completed=[ColdSettings]::GetPropW($main,'LiteTray.SettingsOpen.Completed').ToInt64()
   if($requested -gt 0 -and $completed -gt 0){break}
   Start-Sleep -Milliseconds 10
  } while([DateTime]::UtcNow -lt $deadline)
  $reports += [pscustomobject]@{label=$Label;entry=$entry;title=[ColdSettings]::Title($main);pid=$child.pid;requested=$requested;completed=$completed;visible=[ColdSettings]::IsWindowVisible($main);minimized=[ColdSettings]::IsIconic($main);showRetried=[ColdSettings]::GetPropW($main,'LiteTray.SettingsOpen.ShowRetried').ToInt64();micros=[ColdSettings]::GetPropW($main,'LiteTray.SettingsOpen.Micros').ToInt64();exe=$exe;hash=(Get-FileHash -LiteralPath $exe).Hash;nonInputDesktop=$true}
  if($requested -ne 1 -or $completed -ne 1 -or -not [ColdSettings]::IsWindowVisible($main) -or [ColdSettings]::IsIconic($main)){throw "One $entry click failed after startup show=$StartupShow (requested=$requested completed=$completed)"}
  if($AuditPages) {
   $dpi=[ColdSettings]::GetDpiForWindow($main)
   [void][ColdSettings]::SetWindowPos($main,[IntPtr]::Zero,0,0,[int](1060*$dpi/96),[int](820*$dpi/96),0x16)
   $navigation=@()
   foreach($pageIndex in @(0,1,7,10,2,3,4,5,6,8,9,1,7,1,7,1,7,1,7)) {
    $watch=[Diagnostics.Stopwatch]::StartNew()
    [ColdSettings]::Command($main,(100+$pageIndex))
    [void][ColdSettings]::RedrawWindow($main,[IntPtr]::Zero,[IntPtr]::Zero,0x180)
    $watch.Stop()
    $page=[ColdSettings]::GetWindow($main,5)
    $visiblePage=[IntPtr]::Zero
    $childState=@()
    while($page -ne [IntPtr]::Zero) {
     $childState += [pscustomobject]@{hwnd=$page.ToInt64();class=[ColdSettings]::Class($page);visible=[ColdSettings]::IsWindowVisible($page)}
     if([ColdSettings]::Class($page) -eq 'LiteTray.Page.1' -and [ColdSettings]::IsWindowVisible($page)){$visiblePage=$page;break}
     $page=[ColdSettings]::GetWindow($page,2)
    }
    if($visiblePage -eq [IntPtr]::Zero){$reports|ConvertTo-Json -Depth 5;$childState|ConvertTo-Json -Depth 5;throw 'No active page'}
    $rect=New-Object ColdSettings+Rect
    [void][ColdSettings]::GetClientRect($visiblePage,[ref]$rect)
    $scroll=([ColdSettings]::Style($visiblePage,-16) -band 0x200000) -ne 0
    $navigation += [pscustomobject]@{page=$pageIndex;milliseconds=[math]::Round($watch.Elapsed.TotalMilliseconds,2);scrollbar=$scroll;width=$rect.right;height=$rect.bottom}
    if($scroll -and -not $AllowOverflow){throw "Unneeded outer scrollbar on page $pageIndex"}
   }
   $navigation|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $evidenceRoot ($Label+'-navigation.json')) -Encoding utf8
   $navigation|ConvertTo-Json -Depth 5
  }
 } finally {
  if($child.process -ne [IntPtr]::Zero) {
   if($main -ne [IntPtr]::Zero){[void][ColdSettings]::PostMessageW($main,0x111,[UIntPtr]33,[IntPtr]::Zero)}
   if([ColdSettings]::WaitForSingleObject($child.process,3000) -ne 0){[void][ColdSettings]::TerminateProcess($child.process,1);[void][ColdSettings]::WaitForSingleObject($child.process,2000)}
   [void][ColdSettings]::CloseHandle($child.process)
  }
  [void][ColdSettings]::CloseDesktop($desktop)
 }
}
$reports|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $evidenceRoot ($Label+'.json')) -Encoding utf8
$reports|ConvertTo-Json -Depth 5
