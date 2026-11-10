#requires -Version 7.0
param([ValidateSet('test','build','release','package')][string]$Task='release')
$ErrorActionPreference='Stop'

function Assert-DistPath([string]$Path) {
    $full=[IO.Path]::GetFullPath($Path)
    if(-not $full.StartsWith($script:dist+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) {throw "Unsafe output path: $full"}
    $cursor=$full
    while($cursor -and $cursor -ne $PSScriptRoot) {
        if(Test-Path -LiteralPath $cursor) {
            if((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {throw "Output is a link: $cursor"}
        }
        $cursor=[IO.Path]::GetDirectoryName($cursor)
    }
    if(Test-Path -LiteralPath $full -PathType Container) {
        if(Get-ChildItem -LiteralPath $full -Recurse -Force | Where-Object {$_.Attributes -band [IO.FileAttributes]::ReparsePoint}) {throw "Output contains links: $full"}
    }
}

function Get-DllImports([string]$Path) {
    $lines=& $script:dumpbin /NOLOGO /DEPENDENTS $Path
    if($LASTEXITCODE) {throw "Dependency inspection failed: $Path"}
    @($lines | ForEach-Object {if($_ -match '^\s+([\w.-]+\.dll)\s*$') {$Matches[1].ToLowerInvariant()}} | Sort-Object -Unique)
}

function New-PortablePackage([string]$Exe) {
    $vswhere=Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
    if(-not(Test-Path -LiteralPath $vswhere)) {throw 'Visual Studio Installer vswhere.exe is required for packaging.'}
    $vs=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if($LASTEXITCODE -or -not $vs) {throw 'Visual Studio C++ Build Tools not found.'}
    $msvc=Get-ChildItem -LiteralPath (Join-Path $vs 'VC\Tools\MSVC') -Directory | Sort-Object Name -Descending | Select-Object -First 1
    $script:dumpbin=Join-Path $msvc.FullName 'bin\Hostx64\x64\dumpbin.exe'
    if(-not(Test-Path -LiteralPath $script:dumpbin)) {throw 'MSVC dumpbin.exe not found.'}
    $redist=Get-ChildItem -LiteralPath (Join-Path $vs 'VC\Redist\MSVC') -Directory | Where-Object {$_.Name -match '^\d+\.'} | Sort-Object Name -Descending | Select-Object -First 1
    if(-not $redist) {throw 'Visual C++ x64 redistributable files not found.'}
    $crt=Join-Path $redist.FullName 'x64\Microsoft.VC143.CRT'
    $script:dist=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'dist'))
    $work=Join-Path $script:dist ('.package-'+[guid]::NewGuid().ToString('N'))
    Assert-DistPath $work
    $stage=Join-Path $work 'Loafing-Reader'
    New-Item -ItemType Directory -Path (Join-Path $stage 'licenses') -Force | Out-Null
    $backups=@(); $installed=@()
    try {
        Copy-Item -LiteralPath $Exe -Destination (Join-Path $stage 'LoafingReader.exe')
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE') -Destination $stage
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'licenses\Slint-License.md') -Destination (Join-Path $stage 'licenses')
        $usage=@"
Loafing Reader 0.1 · 制作者：扯蛋虾米

运行 LoafingReader.exe 开始阅读，请保留随包提供的 DLL 和许可证。
关闭设置窗口只会隐藏它；完整退出请使用托盘菜单“退出”。
阅读窗不可见时，使用托盘“找回窗口”，或默认快捷键 Ctrl+Alt+R。
更新前先通过托盘退出旧版，再解压替换文件。
配置、阅读进度及字体保存在 %APPDATA%\Thief-Reader，升级保留原有数据。
GitHub：https://github.com/RichardM95/Loafing-Reader
如果喜欢这款软件，欢迎点一个 Star 支持作者。
"@
        [IO.File]::WriteAllText((Join-Path $stage '使用说明.txt'),$usage,[Text.UTF8Encoding]::new($true))
        $queue=[Collections.Generic.Queue[string]]::new()
        $queue.Enqueue((Join-Path $stage 'LoafingReader.exe'))
        $copied=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        while($queue.Count) {
            foreach($dll in @(Get-DllImports $queue.Dequeue())) {
                if($dll -notmatch '^(?:msvcp|vcruntime|concrt|vccorlib)\d[\w.-]*\.dll$') {continue}
                if($dll -match '\d+d\.dll$') {throw "Debug runtime cannot be distributed: $dll"}
                if($copied.Add($dll)) {
                    $runtime=Join-Path $crt $dll
                    if(-not(Test-Path -LiteralPath $runtime)) {throw "Required x64 runtime missing: $runtime"}
                    $target=Join-Path $stage $dll
                    Copy-Item -LiteralPath $runtime -Destination $target
                    $queue.Enqueue($target)
                }
            }
        }
        $portableZip=Join-Path $work 'Loafing-Reader-win64.zip'
        [IO.Compression.ZipFile]::CreateFromDirectory($stage,$portableZip)
        $zip=[IO.Compression.ZipFile]::OpenRead($portableZip)
        try {
            $files=@(Get-ChildItem -LiteralPath $stage -Recurse -File)
            if(@($zip.Entries | Where-Object {$_.Name}).Count -ne $files.Count) {throw 'Portable ZIP file count mismatch.'}
            foreach($file in $files) {
                $relative=[IO.Path]::GetRelativePath($stage,$file.FullName).Replace('\','/')
                $entry=$zip.GetEntry($relative)
                if(-not $entry) {throw "Missing ZIP entry: $relative"}
                $stream=$entry.Open(); $sha=[Security.Cryptography.SHA256]::Create()
                try {$hash=[Convert]::ToHexString($sha.ComputeHash($stream))} finally {$stream.Dispose();$sha.Dispose()}
                if($hash -ne (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash) {throw "ZIP hash mismatch: $relative"}
            }
        } finally {$zip.Dispose()}

        # Use tracked working-tree files; unpacked source archives have no .git.
        if(Test-Path -LiteralPath (Join-Path $PSScriptRoot '.git')) {
            $sourceFiles=@(git -C $PSScriptRoot -c core.quotePath=false ls-files --cached)
            if($LASTEXITCODE) {throw 'Cannot enumerate Git source files.'}
            $sourceFiles=@($sourceFiles | Where-Object {$_ -notmatch '^(docs|tests|examples|tools|target|dist|\.tools|\.codegraph[^/]*)/|^\.cargo/config\.toml$'})
        } else {
            $sourceFiles=@('.gitattributes','.gitignore','Cargo.lock','Cargo.toml','LICENSE','README.md','build.ps1','build.rs')
            foreach($dir in @('src','ui','assets','licenses')) {
                $sourceFiles+=@(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot $dir) -Recurse -File | ForEach-Object {[IO.Path]::GetRelativePath($PSScriptRoot,$_.FullName).Replace('\','/')})
            }
        }
        if($sourceFiles -notcontains 'licenses/Slint-License.md') {throw 'Track licenses/Slint-License.md before packaging.'}
        $sourceZip=Join-Path $work 'Loafing-Reader-source.zip'
        $archive=[IO.Compression.ZipFile]::Open($sourceZip,[IO.Compression.ZipArchiveMode]::Create)
        try {
            foreach($relative in ($sourceFiles | Sort-Object -Unique)) {
                $path=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot $relative))
                if(-not $path.StartsWith($PSScriptRoot+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) {throw "Invalid source path: $relative"}
                if((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {throw "Source is a link: $relative"}
                [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive,$path,$relative,[IO.Compression.CompressionLevel]::Optimal) | Out-Null
            }
        } finally {$archive.Dispose()}
        $archive=[IO.Compression.ZipFile]::OpenRead($sourceZip)
        try {
            if($archive.Entries.Count -ne @($sourceFiles | Sort-Object -Unique).Count) {throw 'Source ZIP file count mismatch.'}
            foreach($relative in ($sourceFiles | Sort-Object -Unique)) {
                $entry=$archive.GetEntry($relative)
                if(-not $entry) {throw "Source ZIP entry missing: $relative"}
                $stream=$entry.Open(); $sha=[Security.Cryptography.SHA256]::Create()
                try {$hash=[Convert]::ToHexString($sha.ComputeHash($stream))} finally {$stream.Dispose();$sha.Dispose()}
                if($hash -ne (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $relative) -Algorithm SHA256).Hash) {throw "Source ZIP hash mismatch: $relative"}
            }
        } finally {$archive.Dispose()}

        # Replace only validated outputs; retain old versions until all candidates pass.
        foreach($name in @('Loafing-Reader','Loafing-Reader-win64.zip','Loafing-Reader-source.zip')) {
            $destination=Join-Path $script:dist $name
            Assert-DistPath $destination
            if(Test-Path -LiteralPath $destination) {
                $backup=Join-Path $work ('previous-'+$name)
                Move-Item -LiteralPath $destination -Destination $backup
                $backups+=@{Original=$destination;Backup=$backup}
            }
            Move-Item -LiteralPath (Join-Path $work $name) -Destination $destination
            $installed+=$destination
        }
        $backups=@()
        Get-Item -LiteralPath (Join-Path $script:dist 'Loafing-Reader-win64.zip'),(Join-Path $script:dist 'Loafing-Reader-source.zip') | Select-Object FullName,Length
    } catch {
        foreach($path in $installed) {Assert-DistPath $path; Remove-Item -LiteralPath $path -Recurse -Force}
        foreach($backup in $backups) {Move-Item -LiteralPath $backup.Backup -Destination $backup.Original}
        throw
    } finally {
        # Never remove recovery backups if a rollback itself failed.
        if(Test-Path -LiteralPath $work) {
            if(-not(Get-ChildItem -LiteralPath $work -Filter 'previous-*')) {Assert-DistPath $work; Remove-Item -LiteralPath $work -Recurse -Force}
            elseif($backups.Count -eq 0) {Assert-DistPath $work; Remove-Item -LiteralPath $work -Recurse -Force}
        }
    }
}

Push-Location $PSScriptRoot
try {
    $local=Join-Path $PSScriptRoot '.tools\native-toolchain\bin'
    if(Test-Path -LiteralPath (Join-Path $local 'cargo.exe')) {
        $env:PATH=$local+';'+$env:PATH
        $env:CARGO_HOME=Join-Path $PSScriptRoot '.tools\cargo'
        $env:RUSTUP_HOME=Join-Path $PSScriptRoot '.tools\rustup'
        $env:RUSTUP_TOOLCHAIN='thief-local'
    }
    if(-not(Get-Command cargo -ErrorAction SilentlyContinue)) {throw 'Install Rust MSVC and Visual Studio C++ Build Tools. https://rust-lang.org/tools/install/'}
    $cache=Get-ChildItem 'target\debug\build\skia-bindings-*\out\.cache\*.tar.gz' -ErrorAction SilentlyContinue | Where-Object {$_.Length -gt 1MB} | Select-Object -First 1
    if($cache -and -not $env:SKIA_BINARIES_URL) {$env:SKIA_BINARIES_URL='file://'+$cache.FullName.Replace('\','/')}
    switch($Task) {
        test {cargo test --locked -j 1; if($LASTEXITCODE) {throw 'Tests failed'}}
        build {cargo build --locked -j 1; if($LASTEXITCODE) {throw 'Build failed'}}
        default {
            cargo build --locked --release -j 1
            if($LASTEXITCODE) {throw 'Release build failed'}
            if($Task -eq 'package') {
                $targetRoot=if($env:CARGO_TARGET_DIR) {[IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)} else {Join-Path $PSScriptRoot 'target'}
                New-PortablePackage (Join-Path $targetRoot 'release\thief-reader.exe')
            }
        }
    }
} finally {Pop-Location}
