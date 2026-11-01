; Inno Setup script for SurfCraft. Build with: iscc /DVersion=x.y.z packaging\windows\surfcraft.iss
#ifndef Version
  #define Version "0.0.0"
#endif

[Setup]
AppId={{8C1E3F55-4B7A-4E3C-9B2C-6A3B0B5D2E11}
AppName=SurfCraft
AppVersion={#Version}
AppPublisher=SurfCraft contributors
AppPublisherURL=https://github.com/sseveur/surfcraft
DefaultDirName={autopf}\SurfCraft
DefaultGroupName=SurfCraft
LicenseFile=..\..\LICENSE-MIT
OutputDir=..\..\target\package
OutputBaseFilename=SurfCraft-{#Version}-windows-setup
SetupIconFile=surfcraft.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible arm64
ArchitecturesInstallIn64BitMode=x64compatible arm64
ChangesAssociations=yes
PrivilegesRequiredOverridesAllowed=dialog

[Files]
Source: "..\..\target\release\surfcraft.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\target\release\surfcraft-mcp.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; DestName: "LICENSE-MIT.txt"
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; DestName: "LICENSE-APACHE.txt"

[Icons]
Name: "{group}\SurfCraft"; Filename: "{app}\surfcraft.exe"
Name: "{autodesktop}\SurfCraft"; Filename: "{app}\surfcraft.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Registry]
Root: HKA; Subkey: "Software\Classes\.brd"; ValueType: string; ValueName: ""; ValueData: "SurfCraft.Board"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\SurfCraft.Board"; ValueType: string; ValueName: ""; ValueData: "BoardCAD board"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SurfCraft.Board\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\surfcraft.exe,0"
Root: HKA; Subkey: "Software\Classes\SurfCraft.Board\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\surfcraft.exe"" ""%1"""

[Run]
Filename: "{app}\surfcraft.exe"; Description: "{cm:LaunchProgram,SurfCraft}"; Flags: nowait postinstall skipifsilent
