; LightMark Inno Setup Script
; Compile with: iscc installer.iss

#define MyAppName "LightMark"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "LightMark Team"
#define MyAppURL "https://github.com/lightmark/lightmark"
#define MyAppExeName "lightmark.exe"

[Setup]
AppId={{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=LICENSE.txt
OutputDir=.
OutputBaseFilename=LightMark-Setup-{#MyAppVersion}
; SetupIconFile=icon.ico
Compression=lzma
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesInstallIn64BitMode=x64

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "fileassoc_md"; Description: "Associate .md files with LightMark"; GroupDescription: "File Associations"; Flags: unchecked
Name: "fileassoc_txt"; Description: "Associate .txt files with LightMark"; GroupDescription: "File Associations"; Flags: unchecked

[Files]
Source: "target\release\lightmark.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{commondesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Classes\.md"; ValueType: string; ValueData: "LightMark.md"; Flags: uninsdeletekey; Tasks: fileassoc_md
Root: HKCU; Subkey: "Software\Classes\LightMark.md"; ValueType: string; ValueData: "Markdown Document"; Flags: uninsdeletekey; Tasks: fileassoc_md
Root: HKCU; Subkey: "Software\Classes\LightMark.md\DefaultIcon"; ValueType: string; ValueData: "{app}\{#MyAppExeName},0"; Flags: uninsdeletekey; Tasks: fileassoc_md
Root: HKCU; Subkey: "Software\Classes\LightMark.md\shell\open\command"; ValueType: string; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey; Tasks: fileassoc_md

Root: HKCU; Subkey: "Software\Classes\.txt"; ValueType: string; ValueData: "LightMark.txt"; Flags: uninsdeletekey; Tasks: fileassoc_txt
Root: HKCU; Subkey: "Software\Classes\LightMark.txt"; ValueType: string; ValueData: "Text Document"; Flags: uninsdeletekey; Tasks: fileassoc_txt
Root: HKCU; Subkey: "Software\Classes\LightMark.txt\DefaultIcon"; ValueType: string; ValueData: "{app}\{#MyAppExeName},0"; Flags: uninsdeletekey; Tasks: fileassoc_txt
Root: HKCU; Subkey: "Software\Classes\LightMark.txt\shell\open\command"; ValueType: string; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey; Tasks: fileassoc_txt

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{app}"