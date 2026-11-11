#requires -Version 7.0
param([ValidateSet('check','test','build','clippy','run','rustc','prepare')][string]$Task='build', [ValidateSet('x64','x86')][string]$Architecture='x64', [string]$Example='', [string]$TestName='', [switch]$NoDesktop)
$ErrorActionPreference='Stop'
$savedEnvironment=[Environment]::GetEnvironmentVariables('Process')
Push-Location $PSScriptRoot
try {
    $local=Join-Path $PSScriptRoot '.tools/native-toolchain/bin'
    if(Test-Path (Join-Path $local 'cargo.exe')) {
        $env:PATH=$local+';'+$env:PATH
        $env:CARGO_HOME=Join-Path $PSScriptRoot '.tools/cargo'
        $env:RUSTUP_HOME=Join-Path $PSScriptRoot '.tools/rustup'
        $env:RUSTUP_TOOLCHAIN='thief-local'
    }
    if((rustc --version) -notmatch '^rustc 1\.99\.1 ') {throw 'Install Rust 1.99.1 with rust-src, rustfmt and clippy.'}
    $sysroot=rustc --print sysroot
    if(-not(Test-Path (Join-Path $sysroot 'lib/rustlib/src/rust/library/Cargo.toml'))) {throw 'Matching rust-src component is required.'}
    $env:CARGO_PROFILE_DEV_CODEGEN_UNITS='1'
    $env:CARGO_PROFILE_TEST_CODEGEN_UNITS='1'
    $env:CARGO_PROFILE_DEV_BUILD_OVERRIDE_CODEGEN_UNITS='1'
    $env:CARGO_PROFILE_TEST_BUILD_OVERRIDE_CODEGEN_UNITS='1'
    $env:CARGO_PROFILE_DEV_DEBUG='0'
    $env:CARGO_PROFILE_TEST_DEBUG='0'
    $env:CARGO_PROFILE_DEV_INCREMENTAL='false'
    $env:CARGO_PROFILE_TEST_INCREMENTAL='false'
    $env:RUSTC_BOOTSTRAP='1' # only this build process; required to rebuild std on stable
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vs=& $vswhere -version '[16.0,17.0)' -products '*' -property installationPath
    if(-not $vs) {throw 'Visual Studio 2019 v142 is required.'}
    $vc=Join-Path $vs 'VC/Tools/MSVC/14.29.30133'
    if(-not(Test-Path $vc)) {throw 'Required v142 14.29.30133 toolset is missing.'}
    $arch=if($Architecture -eq 'x64') {'x64'} else {'x86'}
    $target=if($Architecture -eq 'x64') {'x86_64-win7-windows-msvc'} else {'i686-win7-windows-msvc'}
    $sdk=Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10'
    $env:PATH=(Join-Path $vc "bin/Hostx64/$arch")+';'+$env:PATH
    $env:LIB=(Join-Path $vc "lib/$arch")+';'+(Join-Path $sdk "Lib/10.0.19041.0/um/$arch")+';'+(Join-Path $sdk "Lib/10.0.19041.0/ucrt/$arch")
    $env:INCLUDE=(Join-Path $vc 'include')+';'+(Join-Path $sdk 'Include/10.0.19041.0/ucrt')+';'+(Join-Path $sdk 'Include/10.0.19041.0/shared')+';'+(Join-Path $sdk 'Include/10.0.19041.0/um')
    [Environment]::SetEnvironmentVariable('CC_'+$target.Replace('-','_'),(Join-Path $vc "bin/Hostx64/$arch/cl.exe"))
    [Environment]::SetEnvironmentVariable('AR_'+$target.Replace('-','_'),(Join-Path $vc "bin/Hostx64/$arch/lib.exe"))
    $key='CARGO_TARGET_'+$target.Replace('-','_').ToUpper()+'_RUSTFLAGS'
    [Environment]::SetEnvironmentVariable($key,'-C target-feature=+crt-static -Zthreads=1 --cfg windows_slim_errors')
    $linkKey='CARGO_TARGET_'+$target.Replace('-','_').ToUpper()+'_LINKER'
    [Environment]::SetEnvironmentVariable($linkKey,(Join-Path $vc "bin/Hostx64/$arch/link.exe"))
    # Keep the 109 MB generated API crate in an ignored build cache, not Git.
    $overlay=Join-Path $PSScriptRoot '.tools/windows-api-0.62.2-win7'
    $stamp=Join-Path $overlay '.win7-patch-v1'
    if(-not(Test-Path -LiteralPath $stamp)) {
        $cargoHome=if($env:CARGO_HOME) {$env:CARGO_HOME} else {Join-Path $HOME '.cargo'}
        $sources=@(Get-ChildItem -Path (Join-Path $cargoHome 'registry/src/*/windows-0.62.2') -Directory -ErrorAction SilentlyContinue)
        if(-not $sources.Count) {
            & cargo fetch
            if($LASTEXITCODE) {throw 'Cannot fetch the pinned Windows API source.'}
            $sources=@(Get-ChildItem -Path (Join-Path $cargoHome 'registry/src/*/windows-0.62.2') -Directory)
        }
        $source=$sources | Select-Object -First 1
        $relative='src/Windows/Win32/System/Com/mod.rs'
        if((Get-FileHash -LiteralPath (Join-Path $source.FullName $relative)).Hash -ne 'BAAFC59EA14E937EFF7ECFD57A9E29A4774B56D822D7DADD19652F792735CAE3') {throw 'Windows 0.62.2 COM source hash mismatch.'}
        New-Item -ItemType Directory -Path $overlay -Force | Out-Null
        Get-ChildItem -LiteralPath $source.FullName -Force | Where-Object {$_.Name -notlike '.cargo*'} | Copy-Item -Destination $overlay -Recurse -Force
        $api=Join-Path $overlay $relative
        $code=[IO.File]::ReadAllText($api)
        foreach($function in @('CoCreateFreeThreadedMarshaler','CoTaskMemAlloc','CoTaskMemFree')) {
            $from='windows_core::link!("combase.dll" "system" fn '+$function
            $to='windows_core::link!("ole32.dll" "system" fn '+$function
            if(([regex]::Matches($code,[regex]::Escape($from))).Count -ne 1) {throw "Patch mismatch: $function"}
            $code=$code.Replace($from,$to)
        }
        [IO.File]::WriteAllText($api,$code,[Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText($stamp,'Windows 0.62.2 COM imports use Win7 ole32 exports.')
    }
    if($Task -eq 'prepare') {Write-Output 'Pinned Win7 API overlay prepared.'; return}
    $windowsPatch='patch.crates-io.windows.path='+($overlay | ConvertTo-Json -Compress)
    $command=if($Task -eq 'build') {'rustc'} else {$Task}
    $args=@($command,'--config',$windowsPatch,'-Z','build-std','--target',$target,'-j','1')
    if($Task -in @('build','run')) {$args+='--release'}
    if($Task -eq 'run') {if(-not $Example) {throw 'Specify -Example for an offscreen validation program.'}; $args+=@('--example',$Example)}
    if($NoDesktop) {$args+='--no-default-features'}
    if($Task -eq 'rustc') {if($Example) {$args+=@('--example',$Example)} elseif($TestName) {$args+=@('--test',$TestName)} else {throw 'Specify -Example or -TestName'}}
    if($Task -eq 'test' -and $TestName) {$args+=@('--test',$TestName)}
    if($Task -eq 'clippy') {$args+=@('--all-targets','--','-D','warnings')}
    if($Task -eq 'build') {$args+=@('--bin','thief-reader')}
    if($Task -in @('build','rustc')) {$args+=@('--','--jobs-backend=2')}
    & cargo @args
    if($LASTEXITCODE) {throw "Compatibility $Task failed ($Architecture)."}
} finally {
    foreach($name in [Environment]::GetEnvironmentVariables('Process').Keys) {
        if(-not $savedEnvironment.Contains($name)) {[Environment]::SetEnvironmentVariable($name,$null,'Process')}
    }
    foreach($name in $savedEnvironment.Keys) {[Environment]::SetEnvironmentVariable($name,[string]$savedEnvironment[$name],'Process')}
    Pop-Location
}
