# scripts/collect.ps1
# 零依赖本地软件与环境线索采集脚本
param (
    [string]$MachineId = $env:COMPUTERNAME,
    [string[]]$CustomPortableDirs = @(),
    [string]$OutputDir = "",
    [switch]$Timing
)

$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

# --- 打点：各步骤耗时（写入 timings.json；-Timing 时同步打印）---
$script:TimingLaps = @()
$script:TimingWatch = [System.Diagnostics.Stopwatch]::StartNew()
$script:TimingLast = 0.0
function Mark-Lap([string]$Name) {
    $now = $script:TimingWatch.Elapsed.TotalSeconds
    $delta = [Math]::Round($now - $script:TimingLast, 3)
    $script:TimingLast = $now
    $script:TimingLaps += [PSCustomObject]@{ step = $Name; seconds = $delta; at = [Math]::Round($now, 3) }
    if ($Timing) { Write-Host ("      [timing] {0,-22} {1,7:N2}s" -f $Name, $delta) -ForegroundColor DarkGray }
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $PSScriptRoot "..\evidence\$MachineId"
}

if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

$screenshotDir = Join-Path $OutputDir "screenshots"
if (-not (Test-Path $screenshotDir)) {
    New-Item -ItemType Directory -Path $screenshotDir -Force | Out-Null
}

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host " 开始采集机器 [$MachineId] 的软件与环境线索..." -ForegroundColor Cyan
Write-Host " 输出目录: $OutputDir" -ForegroundColor DarkGray
Write-Host "==================================================" -ForegroundColor Cyan

# 1. 采集机器基础信息
$machineInfo = [PSCustomObject]@{
    machine_id    = $MachineId
    hostname      = $env:COMPUTERNAME
    username      = $env:USERNAME
    os_name       = (Get-CimInstance Win32_OperatingSystem).Caption
    os_version    = (Get-CimInstance Win32_OperatingSystem).Version
    os_arch       = (Get-CimInstance Win32_OperatingSystem).OSArchitecture
    collected_at  = (Get-Date).ToString("yyyy-MM-ddTHH:mm:sszzz")
}
$machineInfo | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $OutputDir "machine-info.json") -Encoding UTF8
Write-Host "[1/9] 机器系统信息已记录" -ForegroundColor Green
Mark-Lap "1.machine-info"

# 2. 采集注册表已安装软件 (32位 + 64位 + 用户级)
Write-Host "[2/9] 正在读取 Windows 注册表已安装项..." -ForegroundColor Yellow
$regPaths = @(
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*"
)

$registryApps = @()
foreach ($path in $regPaths) {
    Get-ItemProperty $path | ForEach-Object {
        $name = $_.DisplayName
        if ($name -and -not $_.SystemComponent -and ($name -notmatch "^KB\d{6,}") -and ($name -notmatch "Update for Windows")) {
            $registryApps += [PSCustomObject]@{
                name             = $name.Trim()
                version          = if ($_.DisplayVersion) { $_.DisplayVersion.ToString().Trim() } else { "" }
                publisher        = if ($_.Publisher) { $_.Publisher.ToString().Trim() } else { "" }
                install_location = if ($_.InstallLocation) { $_.InstallLocation.ToString().Trim() } else { "" }
                uninstall_string = if ($_.UninstallString) { $_.UninstallString.ToString().Trim() } else { "" }
                display_icon     = if ($_.DisplayIcon) { $_.DisplayIcon.ToString().Trim() } else { "" }
                source           = "registry"
                icon_file        = ""
            }
        }
    }
}
# 去除注册表内部完全同名同版本的重复
$registryApps = $registryApps | Sort-Object name, version -Unique
$registryApps | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "registry-apps.json") -Encoding UTF8
Write-Host "      共读取到 $($registryApps.Count) 个注册表安装条目" -ForegroundColor Green
Mark-Lap "2.registry"

# 3. 采集包管理器 (Winget / Scoop)
Write-Host "[3/9] 检查包管理器 (Winget / Scoop)..." -ForegroundColor Yellow
$wingetApps = @()
if (Get-Command winget -ErrorAction SilentlyContinue) {
    try {
        $rawWinget = winget list --accept-source-agreements 2>$null
        $lines = $rawWinget -split "`r?`n"
        $headerIndex = -1
        for ($i = 0; $i -lt $lines.Count; $i++) {
            if ($lines[$i] -match "^Name\s+Id\s+Version") {
                $headerIndex = $i
                break
            }
        }
        if ($headerIndex -ge 0) {
            for ($j = $headerIndex + 2; $j -lt $lines.Count; $j++) {
                $line = $lines[$j]
                if ($line.Trim().Length -gt 0) {
                    $parts = -split $line
                    if ($parts.Count -ge 2) {
                        $wingetApps += [PSCustomObject]@{
                            name    = $parts[0]
                            id      = $parts[1]
                            version = if ($parts.Count -ge 3) { $parts[2] } else { "" }
                            source  = "winget"
                        }
                    }
                }
            }
        }
    } catch {}
}
$wingetApps | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "winget-apps.json") -Encoding UTF8
Mark-Lap "3.winget"

$scoopApps = @()
if (Get-Command scoop -ErrorAction SilentlyContinue) {
    try {
        $rawScoop = scoop list 2>$null
        $lines = $rawScoop -split "`r?`n"
        foreach ($line in $lines) {
            if ($line -match "^\s*([a-zA-Z0-9\._\-]+)\s+([0-9a-zA-Z\.\-_]+)") {
                $scoopApps += [PSCustomObject]@{
                    name    = $matches[1]
                    version = $matches[2]
                    source  = "scoop"
                }
            }
        }
    } catch {}
}
$scoopApps | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "scoop-apps.json") -Encoding UTF8
Mark-Lap "3.scoop"
Write-Host "      Winget 条目: $($wingetApps.Count), Scoop 条目: $($scoopApps.Count)" -ForegroundColor Green

# 4. 采集桌面与开始菜单快捷方式 (.lnk 解析)
Write-Host "[4/9] 采集开始菜单与桌面快捷方式..." -ForegroundColor Yellow
$shortcutPaths = @(
    [Environment]::GetFolderPath('Desktop'),
    [Environment]::GetFolderPath('CommonDesktopDirectory'),
    (Join-Path ([Environment]::GetFolderPath('StartMenu')) "Programs"),
    (Join-Path ([Environment]::GetFolderPath('CommonStartMenu')) "Programs")
)

$wscriptShell = New-Object -ComObject WScript.Shell
$shortcuts = @()

foreach ($dir in $shortcutPaths) {
    if (Test-Path $dir) {
        Get-ChildItem -Path $dir -Filter "*.lnk" -Recurse -File | ForEach-Object {
            try {
                $lnk = $wscriptShell.CreateShortcut($_.FullName)
                $target = $lnk.TargetPath
                if ($target -and ($target -match "\.exe$")) {
                    $shortcuts += [PSCustomObject]@{
                        name        = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
                        target_path = $target
                        link_file   = $_.FullName
                        source      = "shortcut"
                        icon_file   = ""
                    }
                }
            } catch {}
        }
    }
}
$shortcuts = $shortcuts | Sort-Object name, target_path -Unique
$shortcuts | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "shortcuts.json") -Encoding UTF8
Write-Host "      共解析出 $($shortcuts.Count) 个有效应用程序快捷方式" -ForegroundColor Green
Mark-Lap "4.shortcuts"

# 5. 扫描便携 / 绿色软件目录
Write-Host "[5/9] 扫描便携与绿色软件目录..." -ForegroundColor Yellow
$defaultCandidateDirs = @(
    "D:\Portable", "D:\Tools", "D:\Software", "D:\Green", "D:\Apps",
    "E:\Portable", "E:\Tools", "E:\Software", "E:\Green", "E:\Apps",
    "C:\Portable", "C:\Tools", "C:\Green", "C:\Software",
    "$env:USERPROFILE\Tools", "$env:USERPROFILE\Portable"
)
$cleanCustom = @()
foreach ($d in $CustomPortableDirs) {
    if ($d) {
        $cleanCustom += ($d -split '[,;]+' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    }
}
$candidateDirs = ($defaultCandidateDirs + $cleanCustom) | Select-Object -Unique

$scannedPortable = @()
$foundDirs = @()

foreach ($dir in $candidateDirs) {
    if (Test-Path $dir) {
        $foundDirs += $dir
        # 扫描第一级和第二级目录中的可执行文件
        Get-ChildItem -Path $dir -Directory -Depth 1 | ForEach-Object {
            $folder = $_
            $exes = Get-ChildItem -Path $folder.FullName -Filter "*.exe" -File -Depth 1
            if ($exes.Count -gt 0) {
                # 寻找最可能的主程序 (同名 exe 或第一个 exe)
                $mainExe = $exes | Where-Object { $_.BaseName -eq $folder.Name } | Select-Object -First 1
                if (-not $mainExe) {
                    $mainExe = $exes | Where-Object { $_.Name -notmatch "unins|setup|update|crash|helper" } | Select-Object -First 1
                }
                if (-not $mainExe) { $mainExe = $exes[0] }

                $fileVersion = ""
                try {
                    $vi = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($mainExe.FullName)
                    if ($vi.ProductVersion) { $fileVersion = $vi.ProductVersion }
                    elseif ($vi.FileVersion) { $fileVersion = $vi.FileVersion }
                } catch {}

                $scannedPortable += [PSCustomObject]@{
                    name         = $folder.Name
                    folder_path  = $folder.FullName
                    main_exe     = $mainExe.FullName
                    version      = $fileVersion
                    source       = "portable_scan"
                    icon_file    = ""
                }
            }
        }
    }
}
$scannedPortable = $scannedPortable | Sort-Object folder_path -Unique
$scannedPortable | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "portable-apps.json") -Encoding UTF8
Write-Host "      发现便携目录: $($foundDirs -join ', ')" -ForegroundColor DarkGray
Mark-Lap "5.portable"
Write-Host "      扫描出便携软件: $($scannedPortable.Count) 个" -ForegroundColor Green

# 6. 抽取软件图标（只读 exe 资源；失败即跳过，绝不影响扫描；绝不读取任何敏感数据）
Write-Host "[6/9] 正在抽取软件图标..." -ForegroundColor Yellow
try {
    Add-Type -AssemblyName System.Drawing -ErrorAction Stop
    $iconDir = Join-Path $OutputDir "app-icons"
    New-Item -ItemType Directory -Force -Path $iconDir | Out-Null
    $iconCache = @{}

    function Get-AppIconFile([string]$rawPath) {
        if ([string]::IsNullOrWhiteSpace($rawPath)) { return "" }
        $p = $rawPath.Trim().Trim('"')
        # DisplayIcon 可能是 "C:\..\app.exe,0" 形式，剥掉索引
        if ($p -match '^(?<p>.+?\.exe)(,\s*-?\d+)?$') { $p = $matches['p'] }
        if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { return "" }
        $k = $p.ToLower()
        if ($iconCache.ContainsKey($k)) { return $iconCache[$k] }
        $file = ""
        try {
            $ico = [System.Drawing.Icon]::ExtractAssociatedIcon($p)
            if ($ico) {
                $bmp = $ico.ToBitmap()
                $sha = [System.Security.Cryptography.SHA1]::Create()
                $hash = [System.BitConverter]::ToString($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($k))).Replace('-', '')
                $file = $hash.Substring(0, 16) + ".png"
                $bmp.Save((Join-Path $iconDir $file), [System.Drawing.Imaging.ImageFormat]::Png)
                $bmp.Dispose()
                $ico.Dispose()
            }
        } catch { $file = "" }
        $iconCache[$k] = $file
        return $file
    }

    foreach ($a in $registryApps) { $a.icon_file = Get-AppIconFile $a.display_icon }
    foreach ($s in $shortcuts) { $s.icon_file = Get-AppIconFile $s.target_path }
    foreach ($p in $scannedPortable) { $p.icon_file = Get-AppIconFile $p.main_exe }

    # 回写带 icon_file 的证据 JSON
    $registryApps | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "registry-apps.json") -Encoding UTF8
    $shortcuts | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "shortcuts.json") -Encoding UTF8
    $scannedPortable | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "portable-apps.json") -Encoding UTF8

    $iconCount = @($iconCache.Values | Where-Object { $_ -ne "" }).Count
    Write-Host "      已抽取图标: $iconCount 个" -ForegroundColor Green
} catch {
    Write-Host "      图标抽取不可用，已跳过: $($_.Exception.Message)" -ForegroundColor DarkGray
}
Mark-Lap "6.icons"

# 6. 扫描 PATH 中的独立 CLI 工具
Write-Host "[7/9] 检查系统 PATH 环境变量中的 CLI 工具..." -ForegroundColor Yellow
$pathDirs = ($env:PATH -split ";") | Where-Object { $_.Trim() -and (Test-Path $_.Trim()) } | Select-Object -Unique
$cliTools = @()
$systemPathPrefixes = @("C:\Windows", "C:\Program Files\Common Files", "C:\Program Files (x86)\Common Files")

foreach ($p in $pathDirs) {
    $isSystem = $false
    foreach ($sys in $systemPathPrefixes) {
        if ($p.StartsWith($sys, [System.StringComparison]::OrdinalIgnoreCase)) {
            $isSystem = $true
            break
        }
    }
    if (-not $isSystem) {
        Get-ChildItem -Path $p -Filter "*.exe" -File | ForEach-Object {
            if ($_.Name -notmatch "unins|setup|update|helper") {
                $cliTools += [PSCustomObject]@{
                    name      = $_.BaseName
                    exe_path  = $_.FullName
                    file_size = $_.Length
                    source    = "path_env"
                }
            }
        }
    }
}
$cliTools = $cliTools | Sort-Object name, exe_path -Unique
$cliTools | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "cli-tools.json") -Encoding UTF8
Write-Host "      PATH 中独立 CLI 工具: $($cliTools.Count) 个" -ForegroundColor Green
Mark-Lap "6.cli-tools"

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host " [8/9] 采集开发环境清单 (Python / Rust / VS Code / Git / Node / Go / .NET)..." -ForegroundColor Yellow

# 开发环境清单：只跑白名单只读命令，绝不读取环境变量块、SSH key、凭据等敏感文件。
$devProviders = @()
$devEnvDir = Join-Path $OutputDir "dev-env"
if (-not (Test-Path $devEnvDir)) {
    New-Item -ItemType Directory -Path $devEnvDir -Force | Out-Null
} else {
    Get-ChildItem -Path $devEnvDir -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
}

function New-DevProvider([string]$Id, [string]$Label) {
    return [PSCustomObject]@{
        id               = $Id
        label            = $Label
        available        = $false
        summary          = ""
        items            = @()
        files            = @()
        restore_commands = @()
    }
}

# 写出 UTF-8 无 BOM 的清单文件，便于 pip / npm / cargo 直接读取
function Write-DevText([string]$Name, $Lines) {
    if ($null -eq $Lines) { $Lines = @() }
    $path = Join-Path $devEnvDir $Name
    $enc = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllLines($path, [string[]]$Lines, $enc)
}

# 脱敏：URL 内嵌凭据与 token / password / secret 值一律替换为 ***
function Protect-Secret([string]$Text) {
    if (-not $Text) { return $Text }
    $Text = [System.Text.RegularExpressions.Regex]::Replace($Text, '(?i)(://)[^/@\s:]+:[^/@\s]+@', '$1***:***@')
    $Text = [System.Text.RegularExpressions.Regex]::Replace($Text, '(?im)^(\s*[\w\.\-]*(?:token|password|passwd|secret|_auth|api[-_]?key)[\w\.\-]*\s*[=:]\s*).+$', '$1***')
    return $Text
}

# --- Python ---
$pv = New-DevProvider "python" "Python"
try {
    if (Get-Command python -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $files = @()
        $pyVer = ((& python --version 2>&1) | Out-String).Trim()
        if ($pyVer) { $items += [PSCustomObject]@{ name = "python"; version = $pyVer } }

        # pip 源 / 镜像：脱敏后另存一份，并摘出关键项展示
        # 先看是否真有 pip 配置（文件/环境变量）；没有就跳过，省下 `pip config list` 的 ~1s 启动。
        try {
            $pipCfgFiles = @($env:PIP_CONFIG_FILE,
                (Join-Path $env:APPDATA 'pip\pip.ini'),
                (Join-Path $env:PROGRAMDATA 'pip\pip.ini'),
                (Join-Path $env:USERPROFILE 'pip\pip.ini')) | Where-Object { $_ -and (Test-Path $_) }
            $pipEnvSet = @($env:PIP_INDEX_URL, $env:PIP_EXTRA_INDEX_URL, $env:PIP_TRUSTED_HOST) | Where-Object { $_ }
            if ($pipCfgFiles.Count -gt 0 -or $pipEnvSet.Count -gt 0) {
                $pipCfg = ((& python -m pip config list 2>$null) | Out-String).Trim()
                if ($pipCfg) {
                    $pipLines = @($pipCfg -split "`r?`n" | ForEach-Object { Protect-Secret $_.Trim() } | Where-Object { $_ })
                    Write-DevText "pip-config.txt" $pipLines
                    $files += [PSCustomObject]@{ name = "pip-config.txt"; count = $pipLines.Count }
                    foreach ($line in $pipLines) {
                        if ($line -match '^([\w\.\-]+)\s*=\s*(.+)$') {
                            $items += [PSCustomObject]@{ name = "pip $($matches[1])"; version = $matches[2] }
                        }
                    }
                }
            }
        } catch {}

        # 已安装包 -> requirements.txt，恢复命令直接用 -r
        # 走 importlib.metadata（约 0.6s），比 pip list（约 2.2s）快得多；失败再回退 pip list。
        $pkgs = @()
        try {
            $pyCode = @(
                'import json,importlib.metadata as m'
                'def _g(d):'
                '    try:'
                '        return d.metadata["Name"]'
                '    except Exception:'
                '        return None'
                'out=[{"name":n,"version":d.version} for d in m.distributions() for n in [_g(d)] if n]'
                'out.sort(key=lambda x:x["name"].lower())'
                'print(json.dumps(out))'
            ) -join "`n"
            $jsonText = ((& python -c $pyCode 2>$null) | Out-String).Trim().TrimStart([char]0xFEFF)
            if ($jsonText) {
                # PS 5.1 的 ConvertFrom-Json 把数组当单个对象输出，先赋值再 @() 展平
                $parsed = $jsonText | ConvertFrom-Json
                $pkgs = @($parsed)
            }
        } catch {}
        if ($pkgs.Count -eq 0) {
            try {
                $jsonText = ((& python -m pip list --format=json --disable-pip-version-check 2>$null) | Out-String).Trim().TrimStart([char]0xFEFF)
                if ($jsonText) {
                    $parsed = $jsonText | ConvertFrom-Json
                    $pkgs = @($parsed)
                }
            } catch {}
        }
        $reqLines = @()
        foreach ($pkg in $pkgs) {
            if ($pkg.name -and $pkg.version) {
                $items += [PSCustomObject]@{ name = $pkg.name; version = $pkg.version }
                $reqLines += "$($pkg.name)==$($pkg.version)"
            }
        }
        if ($reqLines.Count -gt 0) {
            Write-DevText "python-requirements.txt" $reqLines
            $restore += "pip install -r `"{{EVIDENCE}}/dev-env/python-requirements.txt`""
            $files += [PSCustomObject]@{ name = "python-requirements.txt"; count = $reqLines.Count }
        }

        # pipx
        if (Get-Command pipx -ErrorAction SilentlyContinue) {
            $pipxLines = @()
            foreach ($line in @(& pipx list --short 2>$null)) {
                if ($line -match '^(\S+)\s+(\S+)') {
                    $items += [PSCustomObject]@{ name = "pipx:$($matches[1])"; version = $matches[2] }
                    $pipxLines += "$($matches[1])==$($matches[2])"
                }
            }
            if ($pipxLines.Count -gt 0) {
                Write-DevText "python-pipx.txt" $pipxLines
                $restore += "Get-Content `"{{EVIDENCE}}/dev-env/python-pipx.txt`" | ForEach-Object { pipx install `$_ }"
                $files += [PSCustomObject]@{ name = "python-pipx.txt"; count = $pipxLines.Count }
            }
        }

        $pv.items = $items
        $pv.files = $files
        $pv.restore_commands = $restore
        $pv.summary = "$($reqLines.Count) 个 pip 包"
    }
} catch {}
$devProviders += $pv
Mark-Lap "7.python"

# --- Rust ---
$pv = New-DevProvider "rust" "Rust"
try {
    if (Get-Command rustup -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $files = @()

        # cargo 源 / 镜像配置：脱敏后另存一份
        try {
            $cargoCfgPath = Join-Path $env:USERPROFILE ".cargo\config.toml"
            if (-not (Test-Path $cargoCfgPath)) { $cargoCfgPath = Join-Path $env:USERPROFILE ".cargo\config" }
            if (Test-Path $cargoCfgPath) {
                $rawLines = @(Get-Content -Path $cargoCfgPath -Encoding UTF8 -ErrorAction SilentlyContinue)
                $sanitized = @($rawLines | ForEach-Object { Protect-Secret $_ })
                Write-DevText "cargo-config.toml" $sanitized
                $files += [PSCustomObject]@{ name = "cargo-config.toml"; count = $sanitized.Count }
                foreach ($line in $sanitized) {
                    if ($line -match '(?i)^\s*(registry|replace-with)\s*=') {
                        $items += [PSCustomObject]@{ name = "cargo.source"; version = $line.Trim() }
                    }
                }
            }
        } catch {}

        # toolchains
        $tc = @()
        foreach ($t in @(& rustup toolchain list 2>$null)) {
            $name = ($t -replace '\s*\(.*\)\s*$', '').Trim()
            if ($name) {
                $tc += $name
                $items += [PSCustomObject]@{ name = "toolchain"; version = $name }
            }
        }
        if ($tc.Count -gt 0) {
            Write-DevText "rust-toolchains.txt" $tc
            $restore += "Get-Content `"{{EVIDENCE}}/dev-env/rust-toolchains.txt`" | ForEach-Object { rustup toolchain install `$_ }"
            $files += [PSCustomObject]@{ name = "rust-toolchains.txt"; count = $tc.Count }
        }

        # components（过滤默认核心）
        $comp = @()
        foreach ($c in @(& rustup component list --installed 2>$null)) {
            $name = ($c -replace '\s*\(installed\)\s*$', '').Trim()
            if ($name -and $name -notmatch '^(rustc|cargo|rust-std|rust-docs|rust-mingw)-') {
                $comp += $name
                $items += [PSCustomObject]@{ name = "component"; version = $name }
            }
        }
        if ($comp.Count -gt 0) {
            Write-DevText "rust-components.txt" $comp
            $restore += "Get-Content `"{{EVIDENCE}}/dev-env/rust-components.txt`" | ForEach-Object { rustup component add `$_ }"
            $files += [PSCustomObject]@{ name = "rust-components.txt"; count = $comp.Count }
        }

        # cargo install 的 crate
        $crates = @()
        foreach ($line in @(& cargo install --list 2>$null)) {
            if ($line -match '^(\S+)\s+v(\S+):') {
                $crates += $matches[1]
                $items += [PSCustomObject]@{ name = "crate"; version = "$($matches[1]) v$($matches[2])" }
            }
        }
        if ($crates.Count -gt 0) {
            Write-DevText "rust-crates.txt" $crates
            $restore += "Get-Content `"{{EVIDENCE}}/dev-env/rust-crates.txt`" | ForEach-Object { cargo install `$_ }"
            $files += [PSCustomObject]@{ name = "rust-crates.txt"; count = $crates.Count }
        }

        $pv.items = $items
        $pv.files = $files
        $pv.restore_commands = $restore
        $pv.summary = "$($tc.Count) 个工具链"
    }
} catch {}
$devProviders += $pv
Mark-Lap "7.rust"

# --- VS Code 扩展 ---
$pv = New-DevProvider "vscode" "VS Code 扩展"
try {
    if (Get-Command code -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $files = @()
        $ids = @()
        foreach ($e in @(& code --list-extensions --show-versions 2>$null)) {
            $line = $e.Trim()
            if (-not $line) { continue }
            $id = $line
            $ver = ""
            if ($line.Contains('@')) {
                $parts = $line -split '@', 2
                $id = $parts[0]
                $ver = $parts[1]
            }
            $ids += $id
            $items += [PSCustomObject]@{ name = $id; version = $ver }
        }
        if ($ids.Count -gt 0) {
            Write-DevText "vscode-extensions.txt" $ids
            $restore += "Get-Content `"{{EVIDENCE}}/dev-env/vscode-extensions.txt`" | ForEach-Object { code --install-extension `$_ }"
            $files += [PSCustomObject]@{ name = "vscode-extensions.txt"; count = $ids.Count }
        }
        $pv.items = $items
        $pv.files = $files
        $pv.restore_commands = $restore
        $pv.summary = "$($ids.Count) 个扩展"
    }
} catch {}
$devProviders += $pv
Mark-Lap "7.vscode"

# --- Git 全局配置（白名单键 + 脱敏后的 .gitconfig）---
$pv = New-DevProvider "git" "Git 全局配置"
try {
    if (Get-Command git -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $files = @()
        $gitKeys = @("user.name", "user.email", "core.editor", "core.autocrlf", "init.defaultBranch", "pull.rebase", "core.pager", "merge.tool", "diff.tool")
        foreach ($k in $gitKeys) {
            $v = ((& git config --global --get $k 2>$null) | Out-String).Trim()
            if ($v) {
                $items += [PSCustomObject]@{ name = $k; version = $v }
                $restore += "git config --global $k `"$v`""
            }
        }
        foreach ($line in @(& git config --global --get-regexp '^alias\.' 2>$null)) {
            $line = $line.Trim()
            $idx = $line.IndexOf(' ')
            if ($idx -gt 0) {
                $key = $line.Substring(0, $idx)
                $val = $line.Substring($idx + 1).Trim()
                $items += [PSCustomObject]@{ name = $key; version = $val }
                $restore += "git config --global $key `"$val`""
            }
        }
        # 脱敏后另存一份 .gitconfig（token / insteadOf 凭据会被掩码）
        try {
            $gitCfgPath = Join-Path $env:USERPROFILE ".gitconfig"
            if (Test-Path $gitCfgPath) {
                $rawLines = @(Get-Content -Path $gitCfgPath -Encoding UTF8 -ErrorAction SilentlyContinue)
                $sanitized = @($rawLines | ForEach-Object { Protect-Secret $_ })
                Write-DevText "git-config.txt" $sanitized
                $files += [PSCustomObject]@{ name = "git-config.txt"; count = $sanitized.Count }
            }
        } catch {}
        $pv.items = $items
        $pv.files = $files
        $pv.restore_commands = $restore
        $pv.summary = "$($items.Count) 项配置"
    }
} catch {}
$devProviders += $pv
Mark-Lap "7.git"

# --- Node.js ---
$pv = New-DevProvider "node" "Node.js"
try {
    if (Get-Command npm -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $files = @()
        $globs = @()
        # 优先用 npm.cmd，避免 PowerShell 解析到 npm.ps1 包装器时丢失参数
        $npmCmd = Get-Command npm.cmd -ErrorAction SilentlyContinue
        if (-not $npmCmd) { $npmCmd = Get-Command npm -ErrorAction SilentlyContinue }
        $nodeVer = ((& node --version 2>&1) | Out-String).Trim()
        if ($nodeVer) { $items += [PSCustomObject]@{ name = "node"; version = $nodeVer } }
        # registry：优先环境变量 / .npmrc，避免再起一个 npm 进程（约省 0.5s）
        $registry = $env:npm_config_registry
        if (-not $registry) {
            foreach ($rc in @((Join-Path $env:USERPROFILE '.npmrc'), (Join-Path $env:APPDATA 'npm\etc\npmrc'))) {
                if (Test-Path $rc) {
                    $hit = Select-String -Path $rc -Pattern '^\s*registry\s*=\s*(.+?)\s*$' -ErrorAction SilentlyContinue | Select-Object -Last 1
                    if ($hit) { $registry = $hit.Matches[0].Groups[1].Value }
                }
            }
        }
        if (-not $registry) { $registry = 'https://registry.npmjs.org/' }
        $items += [PSCustomObject]@{ name = "registry"; version = $registry }
        # 全局包：直接读 `npm root -g` 下的 package.json（约 0.8s），
        # 比 `npm ls -g`（约 1.7s，需构建整棵树）快；失败再回退。
        $readGlobals = $false
        try {
            $globalRoot = ((& $npmCmd.Source root -g 2>$null) | Out-String).Trim()
            if ($globalRoot -and (Test-Path $globalRoot)) {
                $readGlobals = $true
                $pkgDirs = @()
                foreach ($d in @(Get-ChildItem -Path $globalRoot -Directory -ErrorAction SilentlyContinue)) {
                    if ($d.Name.StartsWith('.')) { continue }
                    if ($d.Name.StartsWith('@')) {
                        foreach ($sd in @(Get-ChildItem -Path $d.FullName -Directory -ErrorAction SilentlyContinue)) {
                            $pkgDirs += [PSCustomObject]@{ name = "$($d.Name)/$($sd.Name)"; file = (Join-Path $sd.FullName 'package.json') }
                        }
                    } else {
                        $pkgDirs += [PSCustomObject]@{ name = $d.Name; file = (Join-Path $d.FullName 'package.json') }
                    }
                }
                foreach ($p in $pkgDirs) {
                    if (-not (Test-Path $p.file)) { continue }
                    try {
                        $pj = Get-Content -Path $p.file -Raw -Encoding UTF8 | ConvertFrom-Json
                        $ver = $pj.version
                        if ($ver) {
                            $items += [PSCustomObject]@{ name = $p.name; version = $ver }
                            $globs += "$($p.name)@$ver"
                        }
                    } catch {}
                }
            }
        } catch {}
        if (-not $readGlobals) {
            try {
                $jsonText = ((& $npmCmd.Source ls -g --depth=0 --json 2>$null) | Out-String).Trim().TrimStart([char]0xFEFF)
                if ($jsonText) {
                    $obj = $jsonText | ConvertFrom-Json
                    if ($obj.dependencies) {
                        foreach ($prop in $obj.dependencies.PSObject.Properties) {
                            $ver = $prop.Value.version
                            if ($prop.Name -and $ver) {
                                $items += [PSCustomObject]@{ name = $prop.Name; version = $ver }
                                $globs += "$($prop.Name)@$ver"
                            }
                        }
                    }
                }
            } catch {}
        }
        if ($globs.Count -gt 0) {
            Write-DevText "node-globals.txt" $globs
            $restore += "Get-Content `"{{EVIDENCE}}/dev-env/node-globals.txt`" | ForEach-Object { npm.cmd install -g `$_ }"
            $files += [PSCustomObject]@{ name = "node-globals.txt"; count = $globs.Count }
        }
        $pv.items = $items
        $pv.files = $files
        $pv.restore_commands = $restore
        $pv.summary = "$($globs.Count) 个全局包"
    }
} catch {}
$devProviders += $pv
Mark-Lap "7.node"

# --- Go ---
$pv = New-DevProvider "go" "Go"
try {
    if (Get-Command go -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $files = @()
        $goVer = ((& go version 2>&1) | Out-String).Trim()
        if ($goVer) { $items += [PSCustomObject]@{ name = "version"; version = $goVer } }
        # 一次调用取 GOPATH + GOPROXY，省一次 go 进程
        $goEnvPairs = @(& go env GOPATH GOPROXY 2>$null)
        $gopath = if ($goEnvPairs.Count -ge 1) { "$($goEnvPairs[0])".Trim() } else { "" }
        if ($gopath) { $items += [PSCustomObject]@{ name = "GOPATH"; version = $gopath } }
        $goproxy = if ($goEnvPairs.Count -ge 2) { "$($goEnvPairs[1])".Trim() } else { "" }
        if ($goproxy) {
            $items += [PSCustomObject]@{ name = "GOPROXY"; version = $goproxy }
            $restore += "go env -w GOPROXY=$goproxy"
        }
        $pv.items = $items
        $pv.files = $files
        $pv.restore_commands = $restore
        $pv.summary = "$goVer"
    }
} catch {}
$devProviders += $pv
Mark-Lap "7.go"

# --- .NET ---
$pv = New-DevProvider "dotnet" ".NET"
try {
    if (Get-Command dotnet -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $files = @()
        $sdks = @(& dotnet --list-sdks 2>$null)
        foreach ($s in $sdks) {
            $line = $s.Trim()
            if ($line) { $items += [PSCustomObject]@{ name = "SDK"; version = $line } }
        }
        $tools = @()
        foreach ($t in @(& dotnet tool list -g 2>$null)) {
            $line = $t.Trim()
            if (-not $line) { continue }
            # 表头随系统语言 / 终端着色变化，只认「第二列是 x.y.z 版本号」的数据行
            if ($line -match '^(\S+)\s+(\d+\.\d+[^\s]*)\s*(.*)$') {
                $items += [PSCustomObject]@{ name = "tool"; version = "$($matches[1]) $($matches[2])" }
                $tools += $matches[1]
            }
        }
        if ($tools.Count -gt 0) {
            Write-DevText "dotnet-tools.txt" $tools
            $restore += "Get-Content `"{{EVIDENCE}}/dev-env/dotnet-tools.txt`" | ForEach-Object { dotnet tool install -g `$_ }"
            $files += [PSCustomObject]@{ name = "dotnet-tools.txt"; count = $tools.Count }
        }
        $pv.items = $items
        $pv.files = $files
        $pv.restore_commands = $restore
        $pv.summary = "$($sdks.Count) 个 SDK"
    }
} catch {}
$devProviders += $pv
Mark-Lap "7.dotnet"

[PSCustomObject]@{
    schema_version = 2
    machine_id     = $MachineId
    collected_at   = (Get-Date).ToString("yyyy-MM-ddTHH:mm:sszzz")
    files_dir      = "dev-env"
    providers      = $devProviders
} | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $OutputDir "dev-env.json") -Encoding UTF8
Write-Host "      开发环境 provider: $($devProviders.Count) 个" -ForegroundColor Green
Mark-Lap "7.devenv-json"

# 8. 采集浏览器扩展（只读元数据；绝不碰扩展存储 / cookie / 凭据）
Write-Host "[9/9] 采集浏览器扩展 (Chromium 系 / Firefox)..." -ForegroundColor Yellow

$browserEntries = @()

function Resolve-ChromiumName([string]$ExtDir, $Manifest) {
    $name = $Manifest.name
    if ($name -and $name -match '^__MSG_(.+)__$') {
        $key = $matches[1]
        $locales = @($Manifest.default_locale, 'en', 'en_US', 'zh_CN') | Where-Object { $_ } | Select-Object -Unique
        foreach ($loc in $locales) {
            $msgPath = Join-Path $ExtDir ("_locales\" + $loc + "\messages.json")
            if (Test-Path $msgPath) {
                try {
                    $msgs = Get-Content $msgPath -Raw -Encoding UTF8 | ConvertFrom-Json
                    $prop = $msgs.PSObject.Properties[$key]
                    if ($prop -and $prop.Value -and $prop.Value.message) { return $prop.Value.message }
                } catch {}
            }
        }
    }
    if ($name) { return $name }
    return ""
}

# Chromium 系候选根目录：存在才扫，不存在直接跳过
$chromiumCandidates = @(
    @{ id = 'edge';     label = 'Microsoft Edge'; root = (Join-Path $env:LOCALAPPDATA 'Microsoft\Edge\User Data') },
    @{ id = 'chrome';   label = 'Google Chrome';  root = (Join-Path $env:LOCALAPPDATA 'Google\Chrome\User Data') },
    @{ id = 'brave';    label = 'Brave';          root = (Join-Path $env:LOCALAPPDATA 'BraveSoftware\Brave-Browser\User Data') },
    @{ id = 'vivaldi';  label = 'Vivaldi';        root = (Join-Path $env:LOCALAPPDATA 'Vivaldi\User Data') },
    @{ id = 'chromium'; label = 'Chromium';       root = (Join-Path $env:LOCALAPPDATA 'Chromium\User Data') },
    @{ id = 'helium';   label = 'Helium';         root = (Join-Path $env:LOCALAPPDATA 'imput\Helium') },
    @{ id = 'helium';   label = 'Helium';         root = (Join-Path $env:LOCALAPPDATA 'imput\Helium-Agent-Profile') },
    @{ id = 'opera';    label = 'Opera';          root = (Join-Path $env:APPDATA 'Opera Software\Opera Stable') },
    @{ id = 'opera_gx'; label = 'Opera GX';       root = (Join-Path $env:APPDATA 'Opera Software\Opera GX Stable') }
)

foreach ($cand in $chromiumCandidates) {
    if (-not (Test-Path $cand.root)) { continue }
    $profiles = @()
    foreach ($profDir in @(Get-ChildItem -Path $cand.root -Directory -ErrorAction SilentlyContinue)) {
        $extRoot = Join-Path $profDir.FullName 'Extensions'
        if (-not (Test-Path $extRoot)) { continue }
        $exts = @()
        foreach ($idDir in @(Get-ChildItem -Path $extRoot -Directory -ErrorAction SilentlyContinue)) {
            if ($idDir.Name -notmatch '^[a-p]{32}$') { continue }
            foreach ($verDir in @(Get-ChildItem -Path $idDir.FullName -Directory -ErrorAction SilentlyContinue)) {
                $manifestPath = Join-Path $verDir.FullName 'manifest.json'
                if (-not (Test-Path $manifestPath)) { continue }
                try { $m = Get-Content $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { continue }
                $exts += [PSCustomObject]@{
                    id        = $idDir.Name
                    name      = Resolve-ChromiumName $verDir.FullName $m
                    version   = $m.version
                    enabled   = $null
                    type      = 'extension'
                    store_url = ''
                }
                break
            }
        }
        if ($exts.Count -gt 0) {
            $profiles += [PSCustomObject]@{ profile = $profDir.Name; extensions = $exts }
        }
    }
    if ($profiles.Count -gt 0) {
        $browserEntries += [PSCustomObject]@{ id = $cand.id; label = $cand.label; profiles = $profiles }
    }
}

# Firefox：读 extensions.json，只取 app-profile 项
$ffProfilesDir = Join-Path $env:APPDATA 'Mozilla\Firefox\Profiles'
if (Test-Path $ffProfilesDir) {
    $ffProfiles = @()
    foreach ($profDir in @(Get-ChildItem -Path $ffProfilesDir -Directory -ErrorAction SilentlyContinue)) {
        $extJson = Join-Path $profDir.FullName 'extensions.json'
        if (-not (Test-Path $extJson)) { continue }
        try { $data = Get-Content $extJson -Raw -Encoding UTF8 | ConvertFrom-Json } catch { continue }
        $exts = @()
        foreach ($a in @($data.addons)) {
            if ($a.location -ne 'app-profile') { continue }
            if ($a.type -ne 'extension') { continue }
            $nm = $a.name
            if (-not $nm -and $a.defaultLocale) { $nm = $a.defaultLocale.name }
            if (-not $nm) { $nm = $a.id }
            $exts += [PSCustomObject]@{
                id        = $a.id
                name      = $nm
                version   = $a.version
                enabled   = [bool]$a.active
                type      = $a.type
                store_url = $a.sourceURI
            }
        }
        if ($exts.Count -gt 0) {
            $ffProfiles += [PSCustomObject]@{ profile = $profDir.Name; extensions = $exts }
        }
    }
    if ($ffProfiles.Count -gt 0) {
        $browserEntries += [PSCustomObject]@{ id = 'firefox'; label = 'Mozilla Firefox'; profiles = $ffProfiles }
    }
}

[PSCustomObject]@{
    schema_version = 1
    machine_id     = $MachineId
    collected_at   = (Get-Date).ToString("yyyy-MM-ddTHH:mm:sszzz")
    browsers       = $browserEntries
} | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $OutputDir "browser-extensions.json") -Encoding UTF8
$totalExts = 0
foreach ($b in $browserEntries) { foreach ($pr in $b.profiles) { $totalExts += $pr.extensions.Count } }
Write-Host "      浏览器: $($browserEntries.Count) 个, 扩展: $totalExts 个" -ForegroundColor Green
Mark-Lap "8.browsers"

# 聚合耗时，写入 timings.json（便于分析；不进后台台账）
$timingTotal = [Math]::Round($script:TimingWatch.Elapsed.TotalSeconds, 3)
[PSCustomObject]@{
    schema_version = 1
    machine_id     = $MachineId
    collected_at   = (Get-Date).ToString("yyyy-MM-ddTHH:mm:sszzz")
    total_seconds  = $timingTotal
    laps           = $script:TimingLaps
} | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "timings.json") -Encoding UTF8
if ($Timing) { Write-Host ("      采集总耗时 {0:N2}s" -f $timingTotal) -ForegroundColor Cyan }

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host " 机器 [$MachineId] 采集完成！" -ForegroundColor Green
Write-Host " 所有证据已生成于: $OutputDir" -ForegroundColor White
Write-Host " 截图可直接放入: $screenshotDir" -ForegroundColor White
Write-Host "==================================================" -ForegroundColor Cyan
