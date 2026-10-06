param([Parameter(Mandatory = $true)][string]$BinaryPath)
$ErrorActionPreference = 'Stop'
$work = Join-Path $env:RUNNER_TEMP 'flash-md-native-smoke'
New-Item -ItemType Directory -Force -Path $work | Out-Null
$exe = Join-Path $work 'flash-md.exe'
Copy-Item -LiteralPath $BinaryPath -Destination $exe

# A pinned software OpenGL driver lets the Windows VM run the real GUI.
# The test copy uses these DLLs; release packages contain only the app binary.
$archive = Join-Path $work 'mesa.7z'
Invoke-WebRequest 'https://github.com/pal1000/mesa-dist-win/releases/download/26.2.4/mesa3d-26.2.4-release-msvc.7z' -OutFile $archive
if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne '351FC8C8B695878FFB3EAA044B3EAD08672A48B1A045E3C3E3975811DF0F6695') {
    throw 'Mesa archive checksum mismatch'
}
$mesa = Join-Path $work 'mesa'
& 7z x $archive "-o$mesa" -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Mesa extraction failed' }
$driver = Get-ChildItem $mesa -Recurse -Filter opengl32.dll | Where-Object { $_.FullName -match '[\\/]x64[\\/]' } | Select-Object -First 1
if (-not $driver) { throw 'x64 OpenGL driver missing' }
Get-ChildItem $driver.Directory.FullName -Filter '*.dll' | Copy-Item -Destination $work
$env:GALLIUM_DRIVER = 'llvmpipe'
$env:LIBGL_ALWAYS_SOFTWARE = 'true'
$env:APPDATA = Join-Path $work 'config'

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NativeSmoke {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr wp, IntPtr lp, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wp, IntPtr lp);
    [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint type);
}
'@

function Assert-Responsive([IntPtr]$hwnd) {
    $result = [UIntPtr]::Zero
    if ([NativeSmoke]::SendMessageTimeout($hwnd, 0, [IntPtr]::Zero, [IntPtr]::Zero, 3, 2000, [ref]$result) -eq [IntPtr]::Zero) {
        throw 'Native window message loop did not respond within 2 seconds'
    }
}

function Send-Key([IntPtr]$hwnd, [uint32]$key) {
    $scan = [long][NativeSmoke]::MapVirtualKey($key, 0) -shl 16
    if (-not [NativeSmoke]::PostMessage($hwnd, 0x100, [IntPtr]$key, [IntPtr]($scan -bor 1))) { throw 'Key down failed' }
    if (-not [NativeSmoke]::PostMessage($hwnd, 0x101, [IntPtr]$key, [IntPtr]($scan -bor 0xC0000001L))) { throw 'Key up failed' }
    Start-Sleep -Milliseconds 200
    Assert-Responsive $hwnd
}

function Start-Preview([string]$file, [bool]$visible = $true) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $stderr = Join-Path $work (([Guid]::NewGuid().ToString('N')) + '.log')
    $parameters = @{ FilePath = $exe; PassThru = $true; RedirectStandardError = $stderr }
    if ($file) { $parameters.ArgumentList = "`"$file`"" }
    $process = Start-Process @parameters
    try {
        $hwnd = [IntPtr]::Zero
        while ($timer.Elapsed.TotalSeconds -lt 30) {
            $process.Refresh()
            if ($process.HasExited) { throw "Preview exited during startup: $($process.ExitCode)" }
            $hwnd = [NativeSmoke]::FindWindow($null, "flash-md - 快捷鍵 Markdown 預覽 ($($process.Id))")
            if ($hwnd -ne [IntPtr]::Zero) { break }
            Start-Sleep -Milliseconds 100
        }
        if ($hwnd -eq [IntPtr]::Zero) { throw 'Native window was not created' }
        if ($file) {
            while ($timer.Elapsed.TotalSeconds -lt 30) {
                $process.Refresh()
                if ($process.HasExited) { throw "Preview exited during loading: $($process.ExitCode)" }
                if ((Get-Content $stderr -Raw -ErrorAction SilentlyContinue) -match '已載入預覽') { break }
                Start-Sleep -Milliseconds 100
            }
            if ((Get-Content $stderr -Raw) -notmatch '已載入預覽') { throw 'Document never finished loading' }
        }
        Start-Sleep -Milliseconds 500
        Assert-Responsive $hwnd
        if ([NativeSmoke]::IsWindowVisible($hwnd) -ne $visible) { throw 'Unexpected native window visibility' }
        $startup = [math]::Round($timer.Elapsed.TotalMilliseconds, 2)
        Write-Output "SMOKE opened=$([IO.Path]::GetFileName($file)) visible=$visible startup_ms=$startup"
        if ($visible) {
            if ($file.EndsWith('.md')) {
                Send-Key $hwnd 0x75 # F6: mindmap
                Send-Key $hwnd 0x75 # F6: markdown
                Send-Key $hwnd 0x74 # F5: slides
                Send-Key $hwnd 0x1B # Esc: leave slides
                if ($file -notmatch '\.zip[\\/]') {
                    Send-Key $hwnd 0x45 # E: editor
                    Send-Key $hwnd 0x1B # Esc: leave editor
                }
            }
            Send-Key $hwnd 0x23 # End
            Send-Key $hwnd 0x24 # Home
            $before = $process.TotalProcessorTime.TotalMilliseconds
            Start-Sleep -Milliseconds 1000
            $process.Refresh()
            $idle = [math]::Round($process.TotalProcessorTime.TotalMilliseconds - $before, 2)
            Assert-Responsive $hwnd
            Write-Output "SMOKE responsive=$([IO.Path]::GetFileName($file)) idle_cpu_ms=$idle"
        }
    } catch {
        Get-Content $stderr -Tail 15 -ErrorAction SilentlyContinue | Write-Output
        throw
    } finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
}

$md = Join-Path $work '範例 測試.md'
[IO.File]::WriteAllText($md, @'
# Preview smoke test 中文

| Name | Value |
| :--- | ---: |
| Long content wraps correctly | 42 |

```mermaid
flowchart TD
 A[Read] --> B[Preview]
```

---

## Second slide

Body text.
'@)
$fixtures = @{
    'sample.csv' = "name,note`nJoe,`"first`nsecond`"`nAmy,short"
    'sample.tsv' = "name`tnote`nJoe`tvalue"
    'sample.json' = '{"name":"中文","values":[1,2,3]}'
    'sample.rs' = 'fn main() { println!("Hello"); }'
    'sample.txt' = "Plain text 中文`nSecond line"
    'sample.svg' = '<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="200" height="100" fill="blue"/></svg>'
}
foreach ($entry in $fixtures.GetEnumerator()) { [IO.File]::WriteAllText((Join-Path $work $entry.Key), $entry.Value) }
Add-Type -AssemblyName System.Drawing
$image = New-Object Drawing.Bitmap 100, 100
try { $image.Save((Join-Path $work 'sample.png'), [Drawing.Imaging.ImageFormat]::Png) } finally { $image.Dispose() }
$pdf = New-Object Text.StringBuilder
[void]$pdf.Append("%PDF-1.4`n")
$stream = 'BT /F1 18 Tf 30 700 Td (PDF smoke test) Tj ET'
$objects = @('<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>', '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>', "<< /Length $($stream.Length) >>`nstream`n$stream`nendstream", '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>')
$offsets = @()
for ($index = 0; $index -lt $objects.Count; $index++) {
    $offsets += $pdf.Length
    [void]$pdf.Append("$($index + 1) 0 obj`n$($objects[$index])`nendobj`n")
}
$xref = $pdf.Length
[void]$pdf.Append("xref`n0 6`n0000000000 65535 f `n")
foreach ($offset in $offsets) { [void]$pdf.Append($offset.ToString('D10') + " 00000 n `n") }
[void]$pdf.Append("trailer`n<< /Size 6 /Root 1 0 R >>`nstartxref`n$xref`n%%EOF`n")
[IO.File]::WriteAllBytes((Join-Path $work 'sample.pdf'), [Text.Encoding]::ASCII.GetBytes($pdf.ToString()))
$archivePreview = Join-Path $work 'sample.zip'
Compress-Archive -LiteralPath $md -DestinationPath $archivePreview -Force

$version = Start-Process $exe -ArgumentList '--version' -Wait -PassThru -RedirectStandardOutput (Join-Path $work 'version.txt') -RedirectStandardError (Join-Path $work 'version-error.txt')
if ($version.ExitCode -ne 0) { throw 'CLI version failed' }
$expectedVersion = [regex]::Match((Get-Content (Join-Path $PSScriptRoot '../../Cargo.toml') -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $expectedVersion -or (Get-Content (Join-Path $work 'version.txt') -Raw) -notmatch ('flash-md v' + [regex]::Escape($expectedVersion))) { throw 'CLI version output missing or stale' }
Write-Output 'SMOKE CLI version passed'
Start-Preview $md
foreach ($name in @('sample.csv','sample.tsv','sample.json','sample.rs','sample.txt','sample.svg','sample.png','sample.pdf')) { Start-Preview (Join-Path $work $name) }
Start-Preview (Join-Path $archivePreview ([IO.Path]::GetFileName($md)))
Start-Preview '' $false
Write-Output 'SMOKE all native startup and responsiveness checks passed'
