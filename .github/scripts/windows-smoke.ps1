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
    [StructLayout(LayoutKind.Sequential)] public struct WindowRect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out WindowRect rect);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    private delegate bool EnumCallback(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumCallback callback, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, System.Text.StringBuilder title, int count);
    public static string Title(IntPtr hwnd) { var title = new System.Text.StringBuilder(256); GetWindowText(hwnd, title, title.Capacity); return title.ToString(); }
    public static IntPtr FindProcessWindow(uint processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, _) => {
            uint pid; GetWindowThreadProcessId(hwnd, out pid);
            if (pid == processId && Title(hwnd).StartsWith("flash-md")) { found = hwnd; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
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

function Save-TypographyCapture([IntPtr]$hwnd, [string]$file) {
    if (-not [NativeSmoke]::SetWindowPos($hwnd, [IntPtr]::Zero, 20, 20, 900, 650, 0x40)) { throw 'Typography window positioning failed' }
    [void][NativeSmoke]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 500
    Assert-Responsive $hwnd
    $rect = New-Object NativeSmoke+WindowRect
    if (-not [NativeSmoke]::GetWindowRect($hwnd, [ref]$rect)) { throw 'Typography window bounds unavailable' }
    $capture = New-Object Drawing.Bitmap ($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top)
    $graphics = [Drawing.Graphics]::FromImage($capture)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $capture.Size)
        $capture.Save((Join-Path $work ([IO.Path]::GetFileName($file) + '.png')), [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $capture.Dispose()
    }
    Write-Output "TYPOGRAPHY native screenshot=$([IO.Path]::GetFileName($file)).png"
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
            $hwnd = [NativeSmoke]::FindProcessWindow($process.Id)
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
        Write-Output "SMOKE opened=$([IO.Path]::GetFileName($file)) visible=$visible startup_ms=$startup title=$([NativeSmoke]::Title($hwnd))"
        if ([IO.Path]::GetFileName($file).StartsWith('typography.')) { Save-TypographyCapture $hwnd $file }
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
[IO.File]::WriteAllText((Join-Path $work 'typography.md'), @'
# 中文 Title 123 ⚡ 字體基準線

中文 ABC 0123，括號 (ABC) [123]，全形（測試）。🙂🚀

中文 [連結 ABC 123](#wrapped) ⚡ 後文保持同一行。這段文字用來確認連結與 Emoji 換行後仍在同一個基準線上，English words 123。

| 中文欄位 | English | 1234 | Emoji |
| :--- | :---: | ---: | --- |
| 中文 ABC | English | 1234 | 🙂🚀 |
| 多行中文內容及 English words 123 要維持每一列的第一行對齊，多行中文內容及 English words 123 要維持每一列的第一行對齊 | short | 5678 | ⚡ |

```rust
let 中文_123 = "ABC🙂🚀";
// comment 註解 0123
```

## Wrapped {#wrapped}

中文 **粗體** *斜體* `程式碼 ABC 123` ~~刪除線~~ 維持同一基準線。
'@)
[IO.File]::WriteAllText((Join-Path $work 'typography.csv'), "CJK,English,1234,Emoji`n中文 ABC,English,1234,🙂🚀`n`"多行中文 ABC`n第二行 xyz 5678`",short,5678,⚡")
[IO.File]::WriteAllText((Join-Path $work 'typography.rs'), "let 中文_123 = `"ABC🙂🚀`";`n// 中文 comment 0123`nfn main() { println!(`"Hello 中文⚡`"); }")
Add-Type -AssemblyName System.Drawing
$image = New-Object Drawing.Bitmap 100, 100
try {
    foreach ($format in @('Png', 'Jpeg', 'Gif', 'Bmp', 'Tiff')) {
        $extension = $format.ToLowerInvariant()
        $image.Save((Join-Path $work ('sample.' + $extension)), [Drawing.Imaging.ImageFormat]::$format)
    }
} finally { $image.Dispose() }
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
$bom = Join-Path $work 'bom.md'
[IO.File]::WriteAllText($bom, "# BOM heading`n`n## Second heading", [Text.UTF8Encoding]::new($true))
$zipFolder = Join-Path $work 'download.zip'
New-Item -ItemType Directory -Force -Path $zipFolder | Out-Null
$archivePreview = Join-Path $zipFolder 'sample.zip'
Compress-Archive -LiteralPath $md -DestinationPath $archivePreview -Force

$version = Start-Process $exe -ArgumentList '--version' -Wait -PassThru -RedirectStandardOutput (Join-Path $work 'version.txt') -RedirectStandardError (Join-Path $work 'version-error.txt')
if ($version.ExitCode -ne 0) { throw 'CLI version failed' }
$expectedVersion = [regex]::Match((Get-Content (Join-Path $PSScriptRoot '../../Cargo.toml') -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $expectedVersion -or (Get-Content (Join-Path $work 'version.txt') -Raw) -notmatch ('flash-md v' + [regex]::Escape($expectedVersion))) { throw 'CLI version output missing or stale' }
Write-Output 'SMOKE CLI version passed'
Start-Preview $md
foreach ($name in @('sample.csv','sample.tsv','sample.json','sample.rs','sample.txt','sample.svg','sample.png','sample.jpeg','sample.gif','sample.bmp','sample.tiff','sample.pdf','bom.md')) { Start-Preview (Join-Path $work $name) }
foreach ($name in @('typography.md','typography.csv','typography.rs')) { Start-Preview (Join-Path $work $name) }
Start-Preview (Join-Path $archivePreview ([IO.Path]::GetFileName($md)))
Start-Preview '' $false
Write-Output 'SMOKE all native startup and responsiveness checks passed'
