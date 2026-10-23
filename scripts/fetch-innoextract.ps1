# Downloads innoextract 1.9 (zlib licence) and puts innoextract.exe where Tauri expects the sidecar.
# innoextract unpacks Inno Setup installers without running them.
$ErrorActionPreference = 'Stop'
$Url = 'https://github.com/dscharrer/innoextract/releases/download/1.9/innoextract-1.9-windows.zip'
$ZipSha256 = '6989342c9b026a00a72a38f23b62a8e6a22cc5de69805cf47d68ac2fec993065'
$ExeSha256 = '946bd06d8b3722a791fea4d84f496139eef6faef61640924cb72f5f1998a68e1'
$Root = Split-Path -Parent $PSScriptRoot
$BinDir = Join-Path $Root 'launcher\src-tauri\binaries'
$Target = Join-Path $BinDir 'innoextract-x86_64-pc-windows-msvc.exe'

if ((Test-Path $Target) -and ((Get-FileHash $Target -Algorithm SHA256).Hash -eq $ExeSha256)) {
    Write-Host "innoextract already in place"
    exit 0
}
New-Item -ItemType Directory -Force $BinDir | Out-Null
$Zip = Join-Path ([IO.Path]::GetTempPath()) 'innoextract-1.9-windows.zip'
Invoke-WebRequest $Url -OutFile $Zip
if ((Get-FileHash $Zip -Algorithm SHA256).Hash -ne $ZipSha256) { throw "innoextract zip hash mismatch" }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$Archive = [IO.Compression.ZipFile]::OpenRead($Zip)
try {
    $Entry = $Archive.GetEntry('innoextract.exe')
    [IO.Compression.ZipFileExtensions]::ExtractToFile($Entry, $Target, $true)
} finally {
    $Archive.Dispose()
}
if ((Get-FileHash $Target -Algorithm SHA256).Hash -ne $ExeSha256) { throw "innoextract.exe hash mismatch" }
Write-Host "innoextract ready: $Target"
