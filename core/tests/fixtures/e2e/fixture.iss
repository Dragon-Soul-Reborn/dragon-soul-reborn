; A tiny Inno Setup installer built from synthetic files, for the end-to-end test only.
; CI compiles it with Inno Setup 5.6.1 (the original installer is Inno Setup 5.4.2).
[Setup]
AppName=DSR Fixture
AppVersion=1.0
DefaultDirName={localappdata}\DSRFixture
OutputDir=.
OutputBaseFilename=fixture_setup
Compression=lzma
SolidCompression=yes
Uninstallable=no
PrivilegesRequired=lowest

[Files]
Source: "payload\*"; DestDir: "{app}"; Flags: recursesubdirs
