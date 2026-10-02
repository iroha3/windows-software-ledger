# scripts/collect.ps1
# 零依赖本地软件与环境线索采集脚本
param (
    [string]$MachineId = $env:COMPUTERNAME,
    [string[]]$CustomPortableDirs = @(),
    [string]$OutputDir = ""
)

$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

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
Write-Host "[1/7] 机器系统信息已记录" -ForegroundColor Green

# 2. 采集注册表已安装软件 (32位 + 64位 + 用户级)
Write-Host "[2/7] 正在读取 Windows 注册表已安装项..." -ForegroundColor Yellow
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
                source           = "registry"
            }
        }
    }
}
# 去除注册表内部完全同名同版本的重复
$registryApps = $registryApps | Sort-Object name, version -Unique
$registryApps | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "registry-apps.json") -Encoding UTF8
Write-Host "      共读取到 $($registryApps.Count) 个注册表安装条目" -ForegroundColor Green

# 3. 采集包管理器 (Winget / Scoop)
Write-Host "[3/7] 检查包管理器 (Winget / Scoop)..." -ForegroundColor Yellow
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
Write-Host "      Winget 条目: $($wingetApps.Count), Scoop 条目: $($scoopApps.Count)" -ForegroundColor Green

# 4. 采集桌面与开始菜单快捷方式 (.lnk 解析)
Write-Host "[4/7] 采集开始菜单与桌面快捷方式..." -ForegroundColor Yellow
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
                    }
                }
            } catch {}
        }
    }
}
$shortcuts = $shortcuts | Sort-Object name, target_path -Unique
$shortcuts | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "shortcuts.json") -Encoding UTF8
Write-Host "      共解析出 $($shortcuts.Count) 个有效应用程序快捷方式" -ForegroundColor Green

# 5. 扫描便携 / 绿色软件目录
Write-Host "[5/7] 扫描便携与绿色软件目录..." -ForegroundColor Yellow
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
                }
            }
        }
    }
}
$scannedPortable = $scannedPortable | Sort-Object folder_path -Unique
$scannedPortable | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDir "portable-apps.json") -Encoding UTF8
Write-Host "      发现便携目录: $($foundDirs -join ', ')" -ForegroundColor DarkGray
Write-Host "      扫描出便携软件: $($scannedPortable.Count) 个" -ForegroundColor Green

# 6. 扫描 PATH 中的独立 CLI 工具
Write-Host "[6/7] 检查系统 PATH 环境变量中的 CLI 工具..." -ForegroundColor Yellow
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

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host " [7/7] 采集开发环境清单 (Python / Rust / VS Code / Git / Node / Go / .NET)..." -ForegroundColor Yellow

# 开发环境清单：只跑白名单只读命令，绝不读取环境变量块、SSH key、凭据等敏感文件。
$devProviders = @()

function New-DevProvider([string]$Id, [string]$Label) {
    return [PSCustomObject]@{
        id               = $Id
        label            = $Label
        available        = $false
        summary          = ""
        items            = @()
        restore_commands = @()
    }
}

# --- Python ---
$pv = New-DevProvider "python" "Python"
try {
    if (Get-Command python -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $pyVer = ((& python --version 2>&1) | Out-String).Trim()
        if ($pyVer) { $items += [PSCustomObject]@{ name = "python"; version = $pyVer } }
        $pkgCount = 0
        try {
            $jsonText = ((& python -m pip list --format=json --disable-pip-version-check 2>$null) | Out-String).Trim()
            if ($jsonText) {
                $pkgs = $jsonText | ConvertFrom-Json
                foreach ($pkg in @($pkgs)) {
                    if ($pkg.name -and $pkg.version) {
                        $items += [PSCustomObject]@{ name = $pkg.name; version = $pkg.version }
                        $restore += "pip install `"$($pkg.name)==$($pkg.version)`""
                        $pkgCount++
                    }
                }
            }
        } catch {}
        if (Get-Command pipx -ErrorAction SilentlyContinue) {
            foreach ($line in @(& pipx list --short 2>$null)) {
                if ($line -match '^(\S+)\s+(\S+)') {
                    $items += [PSCustomObject]@{ name = "pipx:$($matches[1])"; version = $matches[2] }
                    $restore += "pipx install $($matches[1])==$($matches[2])"
                }
            }
        }
        $pv.items = $items
        $pv.restore_commands = $restore
        $pv.summary = "$pkgCount 个 pip 包"
    }
} catch {}
$devProviders += $pv

# --- Rust ---
$pv = New-DevProvider "rust" "Rust"
try {
    if (Get-Command rustup -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $toolchains = @(& rustup toolchain list 2>$null)
        foreach ($t in $toolchains) {
            $name = ($t -replace '\s*\(.*\)\s*$', '').Trim()
            if ($name) {
                $items += [PSCustomObject]@{ name = "toolchain"; version = $name }
                $restore += "rustup toolchain install $name"
            }
        }
        foreach ($c in @(& rustup component list --installed 2>$null)) {
            $name = ($c -replace '\s*\(installed\)\s*$', '').Trim()
            if ($name -and $name -notmatch '^(rustc|cargo|rust-std|rust-docs|rust-mingw)-') {
                $items += [PSCustomObject]@{ name = "component"; version = $name }
                $restore += "rustup component add $name"
            }
        }
        foreach ($line in @(& cargo install --list 2>$null)) {
            if ($line -match '^(\S+)\s+v(\S+):') {
                $items += [PSCustomObject]@{ name = "crate"; version = "$($matches[1]) v$($matches[2])" }
                $restore += "cargo install $($matches[1])"
            }
        }
        $pv.items = $items
        $pv.restore_commands = $restore
        $pv.summary = "$($toolchains.Count) 个工具链"
    }
} catch {}
$devProviders += $pv

# --- VS Code 扩展 ---
$pv = New-DevProvider "vscode" "VS Code 扩展"
try {
    if (Get-Command code -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $exts = @(& code --list-extensions --show-versions 2>$null)
        foreach ($e in $exts) {
            $line = $e.Trim()
            if (-not $line) { continue }
            $id = $line
            $ver = ""
            if ($line.Contains('@')) {
                $parts = $line -split '@', 2
                $id = $parts[0]
                $ver = $parts[1]
            }
            $items += [PSCustomObject]@{ name = $id; version = $ver }
            $restore += "code --install-extension $id"
        }
        $pv.items = $items
        $pv.restore_commands = $restore
        $pv.summary = "$($exts.Count) 个扩展"
    }
} catch {}
$devProviders += $pv

# --- Git 全局配置（白名单键，不碰 credential/url.insteadOf）---
$pv = New-DevProvider "git" "Git 全局配置"
try {
    if (Get-Command git -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
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
        $pv.items = $items
        $pv.restore_commands = $restore
        $pv.summary = "$($items.Count) 项配置"
    }
} catch {}
$devProviders += $pv

# --- Node.js ---
$pv = New-DevProvider "node" "Node.js"
try {
    if (Get-Command npm -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $pkgCount = 0
        # 优先用 npm.cmd，避免 PowerShell 解析到 npm.ps1 包装器时丢失参数
        $npmCmd = Get-Command npm.cmd -ErrorAction SilentlyContinue
        if (-not $npmCmd) { $npmCmd = Get-Command npm -ErrorAction SilentlyContinue }
        $nodeVer = ((& node --version 2>&1) | Out-String).Trim()
        if ($nodeVer) { $items += [PSCustomObject]@{ name = "node"; version = $nodeVer } }
        $registry = ((& $npmCmd.Source config get registry 2>$null) | Out-String).Trim()
        if ($registry) { $items += [PSCustomObject]@{ name = "registry"; version = $registry } }
        try {
            $jsonText = ((& $npmCmd.Source ls -g --depth=0 --json 2>$null) | Out-String).Trim().TrimStart([char]0xFEFF)
            if ($jsonText) {
                $obj = $jsonText | ConvertFrom-Json
                if ($obj.dependencies) {
                    foreach ($prop in $obj.dependencies.PSObject.Properties) {
                        $ver = $prop.Value.version
                        if ($prop.Name -and $ver) {
                            $items += [PSCustomObject]@{ name = $prop.Name; version = $ver }
                            $restore += "npm install -g $($prop.Name)@$ver"
                            $pkgCount++
                        }
                    }
                }
            }
        } catch {}
        $pv.items = $items
        $pv.restore_commands = $restore
        $pv.summary = "$pkgCount 个全局包"
    }
} catch {}
$devProviders += $pv

# --- Go ---
$pv = New-DevProvider "go" "Go"
try {
    if (Get-Command go -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $goVer = ((& go version 2>&1) | Out-String).Trim()
        if ($goVer) { $items += [PSCustomObject]@{ name = "version"; version = $goVer } }
        $gopath = ((& go env GOPATH 2>$null) | Out-String).Trim()
        if ($gopath) { $items += [PSCustomObject]@{ name = "GOPATH"; version = $gopath } }
        $goproxy = ((& go env GOPROXY 2>$null) | Out-String).Trim()
        if ($goproxy) {
            $items += [PSCustomObject]@{ name = "GOPROXY"; version = $goproxy }
            $restore += "go env -w GOPROXY=$goproxy"
        }
        $pv.items = $items
        $pv.restore_commands = $restore
        $pv.summary = "$goVer"
    }
} catch {}
$devProviders += $pv

# --- .NET ---
$pv = New-DevProvider "dotnet" ".NET"
try {
    if (Get-Command dotnet -ErrorAction SilentlyContinue) {
        $pv.available = $true
        $items = @()
        $restore = @()
        $sdks = @(& dotnet --list-sdks 2>$null)
        foreach ($s in $sdks) {
            $line = $s.Trim()
            if ($line) { $items += [PSCustomObject]@{ name = "SDK"; version = $line } }
        }
        foreach ($t in @(& dotnet tool list -g 2>$null)) {
            $line = $t.Trim()
            if (-not $line) { continue }
            # 表头随系统语言/终端着色变化，只认「第二列是 x.y.z 版本号」的数据行
            if ($line -match '^(\S+)\s+(\d+\.\d+[^\s]*)\s*(.*)$') {
                $items += [PSCustomObject]@{ name = "tool"; version = "$($matches[1]) $($matches[2])" }
                $restore += "dotnet tool install -g $($matches[1])"
            }
        }
        $pv.items = $items
        $pv.restore_commands = $restore
        $pv.summary = "$($sdks.Count) 个 SDK"
    }
} catch {}
$devProviders += $pv

[PSCustomObject]@{
    schema_version = 1
    machine_id     = $MachineId
    collected_at   = (Get-Date).ToString("yyyy-MM-ddTHH:mm:sszzz")
    providers      = $devProviders
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $OutputDir "dev-env.json") -Encoding UTF8
Write-Host "      开发环境 provider: $($devProviders.Count) 个" -ForegroundColor Green

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host " 机器 [$MachineId] 采集完成！" -ForegroundColor Green
Write-Host " 所有证据已生成于: $OutputDir" -ForegroundColor White
Write-Host " 截图可直接放入: $screenshotDir" -ForegroundColor White
Write-Host "==================================================" -ForegroundColor Cyan
