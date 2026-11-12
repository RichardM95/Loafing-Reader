#requires -Version 7.0
param([ValidateSet('test','build','release','package','clippy')][string]$Task='release')
$ErrorActionPreference='Stop'
$dist=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'dist'))
function Assert-DistPath([string]$Path) {
    $full=[IO.Path]::GetFullPath($Path)
    if(-not $full.StartsWith($dist+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) {throw "Unsafe output path: $full"}
    $cursor=$full
    while($cursor -and $cursor -ne $PSScriptRoot) {
        if((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {throw "Output is a link: $cursor"}
        $cursor=[IO.Path]::GetDirectoryName($cursor)
    }
    if((Test-Path -LiteralPath $full -PathType Container) -and (Get-ChildItem -LiteralPath $full -Recurse -Force | Where-Object {$_.Attributes -band [IO.FileAttributes]::ReparsePoint})) {throw "Output contains links: $full"}
}
function Test-Archive([string]$Zip,[string]$Directory) {
    $archive=[IO.Compression.ZipFile]::OpenRead($Zip)
    try {
        $files=@(Get-ChildItem -LiteralPath $Directory -Recurse -File)
        if(@($archive.Entries | Where-Object Name).Count -ne $files.Count) {throw 'ZIP entry count mismatch.'}
        foreach($file in $files) {
            $entry=$archive.GetEntry([IO.Path]::GetRelativePath($Directory,$file.FullName).Replace('\','/'))
            if(-not $entry) {throw "Missing ZIP entry: $($file.Name)"}
            $stream=$entry.Open()
            try {$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream))} finally {$stream.Dispose()}
            if($hash -ne (Get-FileHash -LiteralPath $file.FullName).Hash) {throw 'ZIP hash mismatch.'}
        }
    } finally {$archive.Dispose()}
}
Push-Location $PSScriptRoot
try {
    foreach($arch in @('x64','x86')) {
        $action=if($Task -in @('release','package')) {'build'} else {$Task}
        & (Join-Path $PSScriptRoot 'compat-build.ps1') -Task $action -Architecture $arch
    }
    if($Task -ne 'package') {return}
    New-Item -ItemType Directory -Path $dist -Force | Out-Null
    $work=Join-Path $dist ('.package-'+[guid]::NewGuid().ToString('N'))
    Assert-DistPath $work
    New-Item -ItemType Directory -Path $work | Out-Null
    $outputs=@(); $backups=@(); $installed=@()
    try {
        $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        $vs=& $vswhere -version '[16.0,17.0)' -products '*' -property installationPath
        $dumpbin=Join-Path $vs 'VC/Tools/MSVC/14.29.30133/bin/Hostx64/x64/dumpbin.exe'
        foreach($bit in @('64','32')) {
            $target=if($bit -eq '64') {'x86_64-win7-windows-msvc'} else {'i686-win7-windows-msvc'}
            $stage=Join-Path $work "Loafing-Reader-win$bit"
            New-Item -ItemType Directory -Path (Join-Path $stage 'licenses') -Force | Out-Null
            $exe=Join-Path $stage 'LoafingReader.exe'
            Copy-Item -LiteralPath "target/$target/release/thief-reader.exe" -Destination $exe
            $imports=& $dumpbin /NOLOGO /IMPORTS $exe
            if($LASTEXITCODE) {throw 'Dependency inspection failed.'}
            # Static CRT builds must not reintroduce GPU, WinRT, modern RNG or VC DLL imports.
            if(($imports -join "`n") -match '(?i)\b(?:d3d12\.dll|combase\.dll|bcryptprimitives\.dll|api-ms-win-core-winrt-[\w.-]+|(?:msvcp|vcruntime|concrt)\d[\w.-]*\.dll|SetProcessDpiAwarenessContext|GetDpiForWindow|GetDpiForMonitor|SystemParametersInfoForDpi|ProcessPrng)\b') {throw "Unsupported loader import in $target"}
            [IO.File]::WriteAllLines((Join-Path $work "imports-win$bit.txt"),[string[]]$imports)
            Copy-Item -LiteralPath LICENSE -Destination $stage
            Copy-Item -LiteralPath licenses/Slint-License.md -Destination (Join-Path $stage 'licenses')
            Copy-Item -LiteralPath vendor/windows-core/license-mit -Destination (Join-Path $stage 'licenses/windows-core-MIT.txt')
            Copy-Item -LiteralPath vendor/windows-core/license-apache-2.0 -Destination (Join-Path $stage 'licenses/windows-core-Apache-2.0.txt')
            Copy-Item -LiteralPath licenses/EPUB-Dependencies.txt -Destination (Join-Path $stage 'licenses')
            $usage=@"
Loafing Reader 0.1 · 制作者：扯蛋虾米
适用于 Win7 SP1 至 Win11；Win7 SP1 已由用户反馈实机验证通过。此包为 $bit 位。
运行 LoafingReader.exe 打开 TXT 或未加密的 EPUB 文字书籍。关闭设置窗口只会隐藏它。
完整退出：托盘“退出”；窗口找回：托盘“找回窗口”或 Ctrl+Alt+R。
配置、阅读进度和字体位于 %APPDATA%\Thief-Reader。
更新前先通过托盘退出旧版，再解压替换文件。
GitHub：https://github.com/RichardM95/Loafing-Reader
欢迎点一个 Star 支持作者。
"@
            [IO.File]::WriteAllText((Join-Path $stage '使用说明.txt'),$usage,[Text.UTF8Encoding]::new($true))
            $zip=Join-Path $work "Loafing-Reader-win$bit.zip"
            [IO.Compression.ZipFile]::CreateFromDirectory($stage,$zip)
            Test-Archive $zip $stage
            $outputs+=@{Source=$zip;Destination=(Join-Path $dist "Loafing-Reader-win$bit.zip")}
            $outputs+=@{Source=$stage;Destination=(Join-Path $dist "Loafing-Reader-win$bit")}
            $outputs+=@{Source=(Join-Path $work "imports-win$bit.txt");Destination=(Join-Path $dist "imports-win$bit.txt")}
        }
        # Explicit source allowlist also works before new source files are committed.
        $source=Join-Path $work 'source'
        New-Item -ItemType Directory -Path $source | Out-Null
        $files=@('.gitattributes','.gitignore','Cargo.toml','Cargo.lock','build.rs','build.ps1','compat-build.ps1','README.md','LICENSE')
        foreach($dir in @('src','ui','assets','licenses','vendor')) {
            $files+=Get-ChildItem -LiteralPath $dir -Recurse -File | ForEach-Object {[IO.Path]::GetRelativePath($PSScriptRoot,$_.FullName)}
        }
        foreach($file in $files) {
            if(-not(Test-Path -LiteralPath $file -PathType Leaf)) {continue}
            $destination=Join-Path $source $file
            New-Item -ItemType Directory -Path (Split-Path $destination) -Force | Out-Null
            Copy-Item -LiteralPath $file -Destination $destination
        }
        $sourceZip=Join-Path $work 'Loafing-Reader-source.zip'
        [IO.Compression.ZipFile]::CreateFromDirectory($source,$sourceZip)
        Test-Archive $sourceZip $source
        $outputs+=@{Source=$sourceZip;Destination=(Join-Path $dist 'Loafing-Reader-source.zip')}
        foreach($output in $outputs) {
            Assert-DistPath $output.Destination
            if(Test-Path -LiteralPath $output.Destination) {
                $backup=Join-Path $work ('previous-'+[guid]::NewGuid().ToString('N'))
                Move-Item -LiteralPath $output.Destination -Destination $backup
                $backups+=@{Original=$output.Destination;Backup=$backup}
            }
            Move-Item -LiteralPath $output.Source -Destination $output.Destination
            $installed+=$output.Destination
        }
        foreach($backup in $backups) {Assert-DistPath $backup.Backup;Remove-Item -LiteralPath $backup.Backup -Recurse -Force}
        $backups=@()
        Get-FileHash -LiteralPath (Join-Path $dist 'Loafing-Reader-win64.zip'),(Join-Path $dist 'Loafing-Reader-win32.zip'),(Join-Path $dist 'Loafing-Reader-source.zip')
    } catch {
        foreach($path in $installed) {Assert-DistPath $path; Remove-Item -LiteralPath $path -Recurse -Force}
        foreach($backup in $backups) {Move-Item -LiteralPath $backup.Backup -Destination $backup.Original}
        throw
    } finally {
        if((Test-Path -LiteralPath $work) -and -not(Get-ChildItem -LiteralPath $work -Filter 'previous-*')) {Assert-DistPath $work;Remove-Item -LiteralPath $work -Recurse -Force}
    }
} finally {Pop-Location}
