# Installs the Thai fonts in this folder for the current user (no admin needed).
# Koharu lists installed system fonts in its font picker; restart Koharu afterwards.
$dest = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows\Fonts'
$reg = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts'
New-Item -ItemType Directory -Force $dest | Out-Null
foreach ($file in Get-ChildItem -Path $PSScriptRoot -Filter *.ttf) {
    $target = Join-Path $dest $file.Name
    Copy-Item $file.FullName $target -Force
    New-ItemProperty -Path $reg -Name "$($file.BaseName) (TrueType)" -Value $target -PropertyType String -Force | Out-Null
    "Installed $($file.Name)"
}
