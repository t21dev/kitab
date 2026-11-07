; SPDX-License-Identifier: AGPL-3.0-or-later
; Kitab Reader's Windows installer (Inno Setup 6). Per-user: no administrator
; prompt, installed to %LOCALAPPDATA%\Programs\Kitab. kitab.exe carries its
; own C runtime (tools/build_release.py), so nothing else is installed: a
; clean Windows runs it as it is.
;
; Built by release.yml:
;   iscc /DAppVersion=0.1.0 /DSourceDir=..\..\dist\Kitab packaging\windows\kitab.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
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
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=..\..\dist
OutputBaseFilename=Kitab_{#AppVersion}_x64_setup
SetupIconFile=kitab.ico
UninstallDisplayIcon={app}\kitab.exe
UninstallDisplayName=Kitab Reader
LicenseFile=..\..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\kitab.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\kitab_shell.dll"; DestDir: "{app}"; Flags: ignoreversion
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
