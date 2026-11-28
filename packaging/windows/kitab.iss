; SPDX-License-Identifier: AGPL-3.0-or-later
; Kitab Reader's Windows installer (Inno Setup 6). Per machine: installed to
; Program Files\Kitab, behind an administrator prompt. Up to 1.6.3 it was per
; user, in %LOCALAPPDATA%\Programs\Kitab: Setup moves such a copy here (see
; "An earlier per-user Kitab" in [Code]). kitab.exe carries its
; own C runtime (tools/build_release.py), so nothing else is installed: a
; clean Windows runs it as it is.
;
; Built by release.yml:
;   iscc /DAppVersion=0.1.0 /DSourceDir=..\..\dist\Kitab packaging\windows\kitab.iss
; and for Windows on Arm, /DArch=arm64.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif

[Setup]
AppId={{6B2E3F4A-9C1D-4E7B-8A5F-2D0C9E1B7A63}
AppName=Kitab Reader
AppVersion={#AppVersion}
AppVerName=Kitab Reader {#AppVersion}
AppPublisher=t21 dev
AppPublisherURL=https://kitab.t21.dev
AppSupportURL=https://github.com/t21dev/kitab/issues
AppUpdatesURL=https://github.com/t21dev/kitab/releases
DefaultDirName={autopf}\Kitab
DefaultGroupName=Kitab Reader
DisableProgramGroupPage=yes
; No "just for me" choice: that would put it back in %LOCALAPPDATA%.
PrivilegesRequired=admin
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
MinVersion=10.0.17763
OutputDir=..\..\dist
OutputBaseFilename=Kitab_{#AppVersion}_{#Arch}_setup
SetupIconFile=kitab.ico
UninstallDisplayIcon={app}\kitab.exe
UninstallDisplayName=Kitab Reader
LicenseFile=..\..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
; The kitab command task changes the user PATH: tell Explorer, so new
; terminals see it (after uninstalling too).
ChangesEnvironment=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
; On by default: the console bin\kitab.exe on the user PATH. Settings >
; Advanced > Command line turns it on or off later (kitab-os cli_path.rs).
Name: "addtopath"; Description: "Add the kitab command to the terminal (for scripts and AI agents)"

[Files]
Source: "{#SourceDir}\kitab.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\kitab_shell.dll"; DestDir: "{app}"; Flags: ignoreversion
; The kitab command (crate kitab-cli, built as kitab-cli.exe): a console
; program, since the windowed kitab.exe can't print to a terminal.
Source: "{#SourceDir}\bin\kitab.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"
Source: "..\..\NOTICE"; DestDir: "{app}"; DestName: "NOTICE.txt"

[Icons]
Name: "{autoprograms}\Kitab Reader"; Filename: "{app}\kitab.exe"; AppUserModelID: "dev.t21.kitab"
Name: "{autodesktop}\Kitab Reader"; Filename: "{app}\kitab.exe"; Tasks: desktopicon
; The taskbar pin of an earlier per-user Kitab, pointed at this one (only if
; there was one). It's the user's pin, so uninstalling leaves it alone.
Name: "{userappdata}\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\Kitab Reader"; Filename: "{app}\kitab.exe"; AppUserModelID: "dev.t21.kitab"; Check: HadTaskbarPin; Flags: uninsneveruninstall

[Run]
Filename: "{app}\kitab.exe"; Description: "{cm:LaunchProgram,Kitab Reader}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Take Kitab's file types and thumbnails off this account first.
Filename: "{app}\kitab.exe"; Parameters: "--unregister"; Flags: runhidden waituntilterminated; RunOnceId: "KitabUnregister"

[Code]
{ The kitab command on the user PATH (HKCU\Environment Path): added when the
  "addtopath" task is chosen, removed on uninstall (Settings may have added
  it too). Only Kitab's own entry is ever touched: every other entry is kept
  as it is, a missing or empty PATH is handled, and entries are compared
  ignoring case, quotes and a trailing backslash. kitab-os cli_path.rs does
  the same from Settings. }

const
  EnvKey = 'Environment';

{ One PATH entry as compared. }
function NormEntry(S: String): String;
begin
  Result := Trim(S);
  if (Length(Result) >= 2) and (Result[1] = '"') and (Result[Length(Result)] = '"') then
    Result := Trim(Copy(Result, 2, Length(Result) - 2));
  while (Length(Result) > 0) and
        ((Result[Length(Result)] = '\') or (Result[Length(Result)] = '/')) do
    Delete(Result, Length(Result), 1);
  Result := AnsiLowercase(Result);
end;

function SameEntry(Entry, Dir: String): Boolean;
var
  E: String;
begin
  E := NormEntry(Entry);
  Result := (E <> '') and (E = NormEntry(Dir));
end;

{ Path without every entry that is Dir; Found says whether there was one. }
function PathWithout(Path, Dir: String; var Found: Boolean): String;
var
  Rest, Entry: String;
  P: Integer;
begin
  Result := '';
  Found := False;
  Rest := Path;
  repeat
    P := Pos(';', Rest);
    if P = 0 then
    begin
      Entry := Rest;
      Rest := '';
    end
    else
    begin
      Entry := Copy(Rest, 1, P - 1);
      Delete(Rest, 1, P);
    end;
    if SameEntry(Entry, Dir) then
      Found := True
    else
    begin
      if Result <> '' then
        Result := Result + ';';
      Result := Result + Entry;
    end;
  until P = 0;
end;

{ The user's PATH as stored (unexpanded). False if it exists but can't be
  read: then nothing is written over it. }
function ReadUserPath(var Path: String): Boolean;
begin
  Path := '';
  if not RegValueExists(HKEY_CURRENT_USER, EnvKey, 'Path') then
    Result := True
  else
    Result := RegQueryStringValue(HKEY_CURRENT_USER, EnvKey, 'Path', Path);
end;

procedure AddToPath(Dir: String);
var
  Path, Rest: String;
  Found: Boolean;
begin
  if not ReadUserPath(Path) then
  begin
    Log('PATH could not be read; the kitab command was not added.');
    Exit;
  end;
  { Only Found matters here: already there, nothing to do. }
  Rest := PathWithout(Path, Dir, Found);
  if Found then
    Exit;
  Rest := Path;
  while (Length(Rest) > 0) and (Rest[Length(Rest)] = ';') do
    Delete(Rest, Length(Rest), 1);
  if Trim(Rest) = '' then
    Rest := Dir
  else
    Rest := Rest + ';' + Dir;
  if RegWriteExpandStringValue(HKEY_CURRENT_USER, EnvKey, 'Path', Rest) then
    Log('Added to PATH: ' + Dir)
  else
    Log('Could not add to PATH: ' + Dir);
end;

procedure RemoveFromPath(Dir: String);
var
  Path, Kept: String;
  Found: Boolean;
begin
  if not ReadUserPath(Path) then
    Exit;
  Kept := PathWithout(Path, Dir, Found);
  if not Found then
    Exit;
  if Trim(Kept) = '' then
    Found := RegDeleteValue(HKEY_CURRENT_USER, EnvKey, 'Path')
  else
    Found := RegWriteExpandStringValue(HKEY_CURRENT_USER, EnvKey, 'Path', Kept);
  if Found then
    Log('Removed from PATH: ' + Dir)
  else
    Log('Could not remove from PATH: ' + Dir);
end;

{ ── An earlier per-user Kitab ──
  Up to 1.6.3 Setup installed per user to %LOCALAPPDATA%\Programs\Kitab, or
  with "for all users" still under the installing account's AppData. Setup
  now installs to Program Files and moves such a copy, keeping what the user
  had:
  - its files go (only the ones Setup put there, never the folder's other
    contents; one still in use goes at the next restart), and so do its
    Apps entry, Start menu shortcut and desktop icon;
  - the kitab command stays on PATH (or off it), and the desktop icon and
    taskbar pin come back, pointed at the new copy;
  - file types, thumbnails and right-click tools are pointed at the new
    kitab.exe (kitab.exe --relink, as the signed-in user). Its uninstaller is
    not run: that would take the file types off this account.
  Settings, the library and reading positions are in %APPDATA%\t21\Kitab
  and aren't touched. }

const
  { AppId's Apps entry. }
  UninstKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{6B2E3F4A-9C1D-4E7B-8A5F-2D0C9E1B7A63}_is1';
  PinLink = '\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\Kitab Reader.lnk';

var
  { The earlier copy's folder, '' if there's none to move. }
  OldDir: String;
  { Its Apps entry is this user's (HKCU), not the machine's. }
  OldPerUser: Boolean;
  OldHadCli, OldOnPath, OldDesktop, OldPin: Boolean;
  ChoicesKept, Moved: Boolean;

function InAppData(Dir: String): Boolean;
begin
  Result := Pos('\appdata\', AnsiLowercase(AddBackslash(Dir))) > 0;
end;

function NewDir: String;
begin
  Result := RemoveBackslashUnlessRoot(ExpandConstant('{app}'));
end;

procedure FindEarlierCopy;
var
  Loc, Path: String;
begin
  OldDir := '';
  if RegQueryStringValue(HKEY_CURRENT_USER, UninstKey, 'InstallLocation', Loc) and (Trim(Loc) <> '') then
  begin
    OldDir := RemoveBackslashUnlessRoot(Trim(Loc));
    OldPerUser := True;
  end
  else if RegQueryStringValue(HKEY_LOCAL_MACHINE, UninstKey, 'InstallLocation', Loc) and InAppData(Loc) then
  begin
    OldDir := RemoveBackslashUnlessRoot(Trim(Loc));
    OldPerUser := False;
  end;
  if OldDir = '' then
    Exit;
  OldHadCli := FileExists(OldDir + '\bin\kitab.exe');
  OldOnPath := False;
  if ReadUserPath(Path) then
    Path := PathWithout(Path, OldDir + '\bin', OldOnPath);
  OldDesktop := FileExists(ExpandConstant('{userdesktop}\Kitab Reader.lnk')) or
                FileExists(ExpandConstant('{commondesktop}\Kitab Reader.lnk'));
  OldPin := FileExists(ExpandConstant('{userappdata}') + PinLink);
  Log('Earlier Kitab in ' + OldDir + ': moving it.');
end;

function HadTaskbarPin: Boolean;
begin
  Result := Moved and OldPin;
end;

{ The tasks start as the earlier copy had them (once: the user may change
  them on the page). The PATH one only if that copy had the command. }
procedure KeepEarlierChoices;
begin
  if (OldDir = '') or ChoicesKept then
    Exit;
  ChoicesKept := True;
  if OldHadCli then
    if OldOnPath then
      WizardSelectTasks('addtopath')
    else
      WizardSelectTasks('!addtopath');
  if OldDesktop then
    WizardSelectTasks('desktopicon');
end;

{ Some process runs Exe (the earlier copy can't go while it's open). }
function IsRunning(Exe: String): Boolean;
var
  Locator, Service, Found: Variant;
begin
  Result := False;
  if not FileExists(Exe) then
    Exit;
  try
    StringChangeEx(Exe, '\', '\\', True);
    StringChangeEx(Exe, '''', '\''', True);
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Service := Locator.ConnectServer('.', 'root\CIMV2');
    Found := Service.ExecQuery('SELECT ProcessId FROM Win32_Process WHERE ExecutablePath = ''' + Exe + '''');
    Result := Found.Count > 0;
  except
    Log('Could not check whether Kitab is open: ' + GetExceptionMessage);
  end;
end;

procedure InitializeWizard;
begin
  FindEarlierCopy;
  { Never back into AppData (1.6.3's "for all users" put it there). }
  if InAppData(WizardForm.DirEdit.Text) then
    WizardForm.DirEdit.Text := ExpandConstant('{autopf}\Kitab');
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpSelectTasks then
    KeepEarlierChoices;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if WizardSilent then
    KeepEarlierChoices;
  Moved := (OldDir <> '') and not SameText(OldDir, NewDir);
  if Moved and (IsRunning(OldDir + '\kitab.exe') or IsRunning(OldDir + '\bin\kitab.exe')) then
    Result := 'Kitab Reader is open (from ' + OldDir + '). Close it, then run Setup again.';
end;

{ Remove one of the earlier copy's files; one in use (Explorer may hold
  kitab_shell.dll) goes at the next restart. True if it had to wait. }
function RemoveEarlierFile(F: String): Boolean;
begin
  Result := False;
  if FileExists(F) and not DeleteFile(F) then
  begin
    RestartReplace(F, '');
    Result := True;
    Log('In use, removed at the next restart: ' + F);
  end;
end;

{ Remove a folder of the earlier copy if nothing else is in it. }
procedure RemoveEarlierDir(D: String; Waiting: Boolean);
begin
  if DirExists(D) and not RemoveDir(D) then
    if Waiting then
      RestartReplace(D, '')
    else
      Log('Not empty, left in place: ' + D);
end;

procedure MoveEarlierCopy;
var
  Waiting, InBin: Boolean;
begin
  if Moved then
  begin
    { Exactly the files 1.6.3 and older installed (and their uninstaller). }
    InBin := RemoveEarlierFile(OldDir + '\bin\kitab.exe');
    RemoveEarlierDir(OldDir + '\bin', InBin);
    Waiting := InBin;
    if RemoveEarlierFile(OldDir + '\kitab.exe') then Waiting := True;
    if RemoveEarlierFile(OldDir + '\kitab_shell.dll') then Waiting := True;
    if RemoveEarlierFile(OldDir + '\LICENSE.txt') then Waiting := True;
    if RemoveEarlierFile(OldDir + '\NOTICE.txt') then Waiting := True;
    if RemoveEarlierFile(OldDir + '\unins000.exe') then Waiting := True;
    if RemoveEarlierFile(OldDir + '\unins000.dat') then Waiting := True;
    if RemoveEarlierFile(OldDir + '\unins000.msg') then Waiting := True;
    RemoveEarlierDir(OldDir, Waiting);
    if OldPerUser then
    begin
      DeleteFile(ExpandConstant('{userprograms}\Kitab Reader.lnk'));
      DeleteFile(ExpandConstant('{userdesktop}\Kitab Reader.lnk'));
    end;
    { The new bin goes on PATH by the task, which started as it was. }
    RemoveFromPath(OldDir + '\bin');
  end;
  { This install's own entry is the machine's (HKLM) now. }
  if (OldDir <> '') and OldPerUser then
    RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER, UninstKey);
end;

procedure RunAsUser(Exe, Params: String; Wait: TExecWait);
var
  Code: Integer;
begin
  if not ExecAsOriginalUser(Exe, Params, '', SW_HIDE, Wait, Code) then
    Log('Could not run ' + Exe + ' ' + Params);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep <> ssPostInstall then
    Exit;
  MoveEarlierCopy;
  if WizardIsTaskSelected('addtopath') then
    AddToPath(ExpandConstant('{app}\bin'));
  { File types, thumbnails and right-click tools whose program is gone
    point at this kitab.exe (nothing to do on a plain update). As the
    signed-in user: they're in that account's registry. }
  RunAsUser(ExpandConstant('{app}\kitab.exe'), '--relink', ewWaitUntilTerminated);
  { 1.6.3's in-app update reopens Kitab from the old folder, which is gone
    now: open the new one instead, when it asked to (KITAB_RELAUNCH is in
    the environment of the update helper that started Setup, which the
    original-user process keeps; elevated Setup doesn't see it). }
  if Moved and WizardSilent then
    RunAsUser(ExpandConstant('{cmd}'),
      '/d /c if "%KITAB_RELAUNCH%"=="1" start "" "' + ExpandConstant('{app}\kitab.exe') + '"',
      ewNoWait);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}\bin'));
end;
