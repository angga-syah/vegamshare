; VegamShare Windows installer (Inno Setup 6).
; Build:  "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\vegamshare.iss
; after:  flutter build windows --release   (in app\)

#define AppName "VegamShare"
#define AppVersion "0.2.0"
#define AppExe "vegamshare.exe"
#define BuildDir "..\app\build\windows\x64\runner\Release"

[Setup]
AppId={{6E3B7C21-5A4D-4F0B-9C55-2B7C1E8D9A41}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppName}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; Admin rights are needed to add the firewall rule.
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=VegamShare-Setup-{#AppVersion}
LicenseFile=..\LICENSE
SetupIconFile=..\app\windows\runner\resources\app_icon.ico
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "id"; MessagesFile: "compiler:Languages\Indonesian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
id.SendTo=Tambahkan "VegamShare" ke menu Kirim ke (Send to) di Explorer
en.SendTo=Add "VegamShare" to Explorer's Send to menu

; "Start with Windows" is a toggle inside the app (Settings), not set here.
[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "sendto"; Description: "{cm:SendTo}"

[Files]
Source: "{#BuildDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"
Source: "..\THIRD_PARTY_LICENSES.txt"; DestDir: "{app}"

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon
Name: "{usersendto}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: sendto

[Run]
; Inbound QUIC + discovery, on Private networks only (home/office Wi-Fi).
; Public networks stay closed unless the user changes the network profile.
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""{#AppName}"""; Flags: runhidden; StatusMsg: "Firewall..."
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""{#AppName}"" dir=in action=allow program=""{app}\{#AppExe}"" protocol=UDP profile=private enable=yes"; Flags: runhidden
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""{#AppName}"""; Flags: runhidden; RunOnceId: "fw"
