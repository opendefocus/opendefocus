; Inno Setup 6 script for the OpenDefocus OpenFX bundle.
; Built by `cargo xtask --package-ofx` with /DAppVersion, /DBundlePath, /O and /F.
; Installs the bundle into %CommonProgramFiles%\OFX\Plugins and writes an uninstaller.
; Requires administrator rights because the target is a machine-wide folder.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef BundlePath
  #define BundlePath "..\..\target\ofx\OpenDefocus.ofx.bundle"
#endif

[Setup]
AppId={{7421A0E5-1196-4E26-BF81-05C41EAF85B7}
AppName=OpenDefocus OFX Plugin
AppVersion={#AppVersion}
AppPublisher=OpenDefocus
AppPublisherURL=https://github.com/opendefocus/opendefocus
DefaultDirName={commonfiles64}\OFX\Plugins
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
Uninstallable=yes
UninstallDisplayName=OpenDefocus OFX Plugin
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Files]
Source: "{#BundlePath}\*"; DestDir: "{app}\OpenDefocus.ofx.bundle"; Flags: ignoreversion recursesubdirs createallsubdirs
