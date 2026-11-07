; Build with tools/Build-Setup.ps1. Payload directories are immutable so an
; Explorer-loaded DLL from the previous build cannot block an upgrade.
#ifndef PayloadDir
  #error PayloadDir must point to a verified portable package
#endif
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef BuildId
  #error BuildId is required
#endif
#define AppExe "{app}\versions\" + BuildId + "\LiteTray.exe"

[Setup]
AppId={{5E194991-FAEA-4BD7-BE88-D7B6B639EA3E}
AppName=LiteTray
AppVersion={#AppVersion}
AppVerName=LiteTray {#AppVersion}
AppPublisher=LiteTray
AppPublisherURL=https://github.com/Lianzy-Baimiao/LiteTray
AppSupportURL=https://github.com/Lianzy-Baimiao/LiteTray/issues
VersionInfoVersion={#AppVersion}.0
VersionInfoDescription=LiteTray 安装程序
DefaultDirName={localappdata}\Programs\LiteTray
DefaultGroupName=LiteTray
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0.22000
WizardStyle=modern dynamic
WizardSizePercent=110
SetupIconFile=..\assets\litetray.ico
UninstallDisplayIcon={#AppExe}
UninstallDisplayName=LiteTray {#AppVersion}
OutputDir=..\dist
OutputBaseFilename=LiteTray-{#AppVersion}-Setup-x64
Compression=lzma2
SolidCompression=yes
CloseApplications=no
RestartApplications=no
SetupLogging=yes
UsePreviousAppDir=yes
DisableDirPage=auto
ShowLanguageDialog=no

[Languages]
Name: "zhcn"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; The content hash in BuildId covers all three binaries. Check the actual
; file bytes on reinstall, so intact, loaded DLLs never need overwriting.
Source: "{#PayloadDir}\LiteTray.exe"; DestDir: "{app}\versions\{#BuildId}"; Flags: ignoreversion; Check: NeedsBinary('LiteTray.exe', '{#ExeHash}')
Source: "{#PayloadDir}\litetray_taskbar_adapter.dll"; DestDir: "{app}\versions\{#BuildId}"; Flags: ignoreversion; Check: NeedsBinary('litetray_taskbar_adapter.dll', '{#AdapterHash}')
Source: "{#PayloadDir}\litetray_popup_hook.dll"; DestDir: "{app}\versions\{#BuildId}"; Flags: ignoreversion; Check: NeedsBinary('litetray_popup_hook.dll', '{#HookHash}')
Source: "{#PayloadDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\README.en.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\NOTICE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs
Source: "{#PayloadDir}\crates\*"; DestDir: "{app}\crates"; Flags: ignoreversion recursesubdirs
Source: "{#PayloadDir}\data\*"; DestDir: "{app}\data"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\LiteTray"; Filename: "{#AppExe}"; Parameters: "--settings"; WorkingDir: "{app}\versions\{#BuildId}"
Name: "{group}\{cm:UninstallProgram,LiteTray}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\LiteTray"; Filename: "{#AppExe}"; Parameters: "--settings"; WorkingDir: "{app}\versions\{#BuildId}"; Tasks: desktopicon

[Run]
Filename: "{#AppExe}"; Parameters: "--background"; Description: "{cm:LaunchProgram,LiteTray}"; Flags: nowait postinstall skipifsilent

[CustomMessages]
zhcn.CloseFailed=LiteTray 未能正常退出。请从 LiteTray 托盘菜单退出后重试，安装程序不会强制结束 Explorer。
en.CloseFailed=LiteTray could not exit normally. Exit it from its tray menu and retry. Setup will not terminate Explorer.

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
var
  StoppedExe: String;
  WasStartupEnabled: Boolean;
  Installed: Boolean;

function GetWindowThreadProcessId(Window: HWND; var ProcessId: Cardinal): Cardinal;
  external 'GetWindowThreadProcessId@user32.dll stdcall';
function OpenProcess(Access: Cardinal; Inherit: Boolean; ProcessId: Cardinal): THandle;
  external 'OpenProcess@kernel32.dll stdcall';
function CloseHandle(Handle: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';
function QueryFullProcessImageName(Process: THandle; Flags: Cardinal; Name: String; var Size: Cardinal): Boolean;
  external 'QueryFullProcessImageNameW@kernel32.dll stdcall';
function WaitForSingleObject(Handle: THandle; Milliseconds: Cardinal): Cardinal;
  external 'WaitForSingleObject@kernel32.dll stdcall';

function NeedsBinary(Name, ExpectedHash: String): Boolean;
var
  Path: String;
begin
  Path := ExpandConstant('{app}\versions\{#BuildId}\') + Name;
  Result := True;
  if FileExists(Path) then
    Result := CompareText(GetSHA256OfFile(Path), ExpectedHash) <> 0;
end;

function StopHost(OnlyInstalled: Boolean): Boolean;
var
  Window: HWND;
  ProcessId, Size, Attempts: Cardinal;
  Process: THandle;
  Path: String;
begin
  Result := True;
  Window := FindWindowByClassName('LiteTray.Settings.1');
  if Window = 0 then Exit;
  GetWindowThreadProcessId(Window, ProcessId);
  Process := OpenProcess($00101000, False, ProcessId); { SYNCHRONIZE | QUERY_LIMITED_INFORMATION }
  if Process = 0 then begin
    Result := False;
    Exit;
  end;
  try
    Size := 32768;
    SetLength(Path, Size);
    if not QueryFullProcessImageName(Process, 0, Path, Size) then begin
      Result := False;
      Exit;
    end;
    SetLength(Path, Size);
    if CompareText(ExtractFileName(Path), 'LiteTray.exe') <> 0 then begin
      Result := False;
      Exit;
    end;
    if OnlyInstalled and (Pos(Lowercase(AddBackslash(ExpandConstant('{app}'))), Lowercase(Path)) <> 1) then Exit;
    Log('Requesting normal LiteTray exit: ' + Path);
    PostMessage(Window, $0111, 33, 0); { WM_COMMAND / EXIT: restores owned taskbar state }
    Result := False;
    for Attempts := 1 to 160 do begin
      if WaitForSingleObject(Process, 0) = 0 then begin
        Result := True;
        if not OnlyInstalled then StoppedExe := Path;
        Break;
      end;
      Sleep(50);
    end;
  finally
    CloseHandle(Process);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ExistingStartup: String;
begin
  WasStartupEnabled := RegQueryStringValue(HKCU, RunKey, 'LiteTray', ExistingStartup) and (ExistingStartup <> '');
  if not StopHost(False) then Result := CustomMessage('CloseFailed')
  else Result := '';
end;

procedure CurStepChanged(Step: TSetupStep);
begin
  if Step = ssPostInstall then begin
    { Keep the user's existing preference, but point it at the installed build. }
    if WasStartupEnabled then
      RegWriteStringValue(HKCU, RunKey, 'LiteTray', '"' + ExpandConstant('{#AppExe}') + '" --background');
    Installed := True;
  end;
end;

procedure DeinitializeSetup;
var
  ResultCode: Integer;
begin
  { A cancelled or failed install must not leave the previous host stopped. }
  if (not Installed) and (StoppedExe <> '') and FileExists(StoppedExe) then
    Exec(StoppedExe, '--background', ExtractFileDir(StoppedExe), SW_HIDE, ewNoWait, ResultCode);
end;

function InitializeUninstall: Boolean;
begin
  Result := StopHost(True);
  if not Result then SuppressibleMsgBox(CustomMessage('CloseFailed'), mbError, MB_OK, IDOK);
end;

procedure CurUninstallStepChanged(Step: TUninstallStep);
var
  Startup: String;
begin
  if (Step = usUninstall) and RegQueryStringValue(HKCU, RunKey, 'LiteTray', Startup) then
    if Pos('"' + Lowercase(AddBackslash(ExpandConstant('{app}'))), Lowercase(Startup)) = 1 then
      RegDeleteValue(HKCU, RunKey, 'LiteTray');
  { Never delete %LOCALAPPDATA%\LiteTray or credentials: these are user data. }
end;
