; Inno Setup script for the MrSizeMe installer.
; Built by the release workflow: iscc /DAppVersion=1.2.3 /DSourceDir=... mrsizeme.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

[Setup]
; Never change the AppId: Windows uses it to find an existing installation.
AppId={{6B1F2C47-3E0A-4D51-9C8E-7A5F2B9D4E31}
AppName=MrSizeMe
AppVersion={#AppVersion}
AppVerName=MrSizeMe {#AppVersion}
AppPublisher=Mr. M
AppPublisherURL=https://github.com/MrMarkus42/MrSizeMe
AppSupportURL=https://github.com/MrMarkus42/MrSizeMe/issues
AppUpdatesURL=https://github.com/MrMarkus42/MrSizeMe/releases
DefaultDirName={autopf}\MrSizeMe
DefaultGroupName=MrSizeMe
DisableProgramGroupPage=yes
; For the current user by default; all users only after a question (admin rights).
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=MrSizeMe-{#AppVersion}-windows-x64-setup
SetupIconFile=..\..\assets\icon\mrsizeme.ico
UninstallDisplayIcon={app}\mrsizeme.exe
LicenseFile=..\..\LICENSE-MIT
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; An update closes the running program first.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\mrsizeme.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\MrSizeMe"; Filename: "{app}\mrsizeme.exe"
Name: "{autodesktop}\MrSizeMe"; Filename: "{app}\mrsizeme.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\mrsizeme.exe"; Description: "{cm:LaunchProgram,MrSizeMe}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Removes the right-click menu entry, if the user had added it in the program.
Filename: "{app}\mrsizeme.exe"; Parameters: "--remove-menu-entry"; Flags: runhidden waituntilterminated; RunOnceId: "RemoveMenuEntry"
