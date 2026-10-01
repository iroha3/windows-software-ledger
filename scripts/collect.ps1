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
Write-Host "[1/6] 机器系统信息已记录" -ForegroundColor Green

# 2. 采集注册表已安装软件 (32位 + 64位 + 用户级)
Write-Host "[2/6] 正在读取 Windows 注册表已安装项..." -ForegroundColor Yellow
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
Write-Host "[3/6] 检查包管理器 (Winget / Scoop)..." -ForegroundColor Yellow
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
Write-Host "[4/6] 采集开始菜单与桌面快捷方式..." -ForegroundColor Yellow
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
Write-Host "[5/6] 扫描便携与绿色软件目录..." -ForegroundColor Yellow
$candidateDirs = @(
    "D:\Portable", "D:\Tools", "D:\Software", "D:\Green", "D:\Apps",
    "E:\Portable", "E:\Tools", "E:\Software", "E:\Green", "E:\Apps",
    "C:\Portable", "C:\Tools", "C:\Green", "C:\Software",
    "$env:USERPROFILE\Tools", "$env:USERPROFILE\Portable"
) + $CustomPortableDirs

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
Write-Host "[6/6] 检查系统 PATH 环境变量中的 CLI 工具..." -ForegroundColor Yellow
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
Write-Host " 机器 [$MachineId] 采集完成！" -ForegroundColor Green
Write-Host " 所有证据已生成于: $OutputDir" -ForegroundColor White
Write-Host " 截图可直接放入: $screenshotDir" -ForegroundColor White
Write-Host "==================================================" -ForegroundColor Cyan
