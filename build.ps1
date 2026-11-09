param([ValidateSet('test','build','release','package')][string]$Task='release')
$ErrorActionPreference='Stop'
Push-Location $PSScriptRoot
try {
    $local=Join-Path $PSScriptRoot '.tools\native-toolchain\bin'
    if(Test-Path -LiteralPath (Join-Path $local 'cargo.exe')) {
        $env:PATH=$local+';'+$env:PATH
        $env:CARGO_HOME=Join-Path $PSScriptRoot '.tools\cargo'
        $env:RUSTUP_HOME=Join-Path $PSScriptRoot '.tools\rustup'
        $env:RUSTUP_TOOLCHAIN='thief-local'
    }
    if(-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'Install Rust MSVC and Visual Studio C++ Build Tools. https://rust-lang.org/tools/install/'
    }
    $archive=Get-ChildItem 'target\debug\build\skia-bindings-*\out\.cache\*.tar.gz' -ErrorAction SilentlyContinue | Select-Object -First 1
    if($archive) { $env:SKIA_BINARIES_URL='file://'+$archive.FullName.Replace('\','/') }
    switch($Task) {
        test { cargo test --locked; if($LASTEXITCODE){throw 'Tests failed'} }
        build { cargo build --locked; if($LASTEXITCODE){throw 'Build failed'} }
        default {
            cargo build --locked --release; if($LASTEXITCODE){throw 'Release build failed'}
            if($Task -eq 'package') {
                $stage=Join-Path $PSScriptRoot 'dist\Loafing-Reader'
                New-Item -ItemType Directory -Force $stage | Out-Null
                Copy-Item -LiteralPath 'target\release\thief-reader.exe' -Destination (Join-Path $stage 'LoafingReader.exe')
                Copy-Item -LiteralPath 'LICENSE','README.md' -Destination $stage
                $notices=Join-Path $stage 'licenses'
                New-Item -ItemType Directory -Force $notices | Out-Null
                Copy-Item -LiteralPath 'docs\Slint-License.md' -Destination $notices
                if(Test-Path -LiteralPath 'docs\verification.md') {
                    Copy-Item -LiteralPath 'docs\verification.md' -Destination $stage
                }
                $vswhere=Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
                if(Test-Path -LiteralPath $vswhere) {
                    $vs=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
                    $redist=Get-ChildItem -LiteralPath (Join-Path $vs 'VC\Redist\MSVC') -Directory -ErrorAction SilentlyContinue |
                        Where-Object { $_.Name -match "^\d+\." } | Sort-Object Name -Descending | Select-Object -First 1
                    if($redist) {
                        $crt=Join-Path $redist.FullName 'x64\Microsoft.VC143.CRT'
                        Get-ChildItem -LiteralPath $crt -Filter '*.dll' -ErrorAction SilentlyContinue |
                            ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination $stage}
                    }
                }
                Compress-Archive -Path (Join-Path $stage '*') -DestinationPath 'dist\Loafing-Reader-win64.zip' -Force
                Get-Item -LiteralPath 'dist\Loafing-Reader-win64.zip' | Select-Object FullName,Length
            }
        }
    }
} finally { Pop-Location }
