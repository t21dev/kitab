; SPDX-License-Identifier: AGPL-3.0-or-later
; Kitab Reader's Windows installer (Inno Setup 6). Per-user: no administrator
; prompt, installed to %LOCALAPPDATA%\Programs\Kitab. kitab.exe carries its
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
DefaultDirName={localappdata}\Programs\Kitab
DefaultGroupName=Kitab Reader
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
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

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then
    AddToPath(ExpandConstant('{app}\bin'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}\bin'));
end;
