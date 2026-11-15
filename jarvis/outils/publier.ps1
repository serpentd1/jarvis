# Publie une nouvelle version sur GitHub : tous ceux qui ont installé Jarvis la reçoivent automatiquement
# (Jarvis vérifie au démarrage puis toutes les 6 h).
#   powershell -ExecutionPolicy Bypass -File jarvis\outils\publier.ps1 -Version 1.0.1 -Notes "Ce qui change"
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Notes = "Améliorations et corrections."
)
$jarvis = Split-Path $PSScriptRoot -Parent
$racine = Split-Path $jarvis -Parent
Set-Location $racine
function Echec($t) { Write-Host "ÉCHEC : $t" -ForegroundColor Red; exit 1 }

gh auth status *> $null
if ($LASTEXITCODE -ne 0) { Echec "connecte-toi d'abord à GitHub avec : gh auth login" }
$depot = gh repo view --json nameWithOwner -q .nameWithOwner 2>$null
if (-not $depot) { Echec "aucun dépôt GitHub relié à ce dossier" }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { Echec "la version doit ressembler à 1.0.1" }

Set-Content "$jarvis\VERSION" $Version -Encoding ascii
Set-Content "$jarvis\DEPOT" $depot -Encoding ascii
powershell -NoProfile -ExecutionPolicy Bypass -File "$jarvis\outils\construire.ps1"
if ($LASTEXITCODE -ne 0) { Echec "la construction de l'installeur a échoué" }

git add -A
git commit -m "Version $Version"
git push
gh release create "v$Version" "$jarvis\dist\Installer-Jarvis.exe" --title "Jarvis $Version" --notes $Notes
if ($LASTEXITCODE -ne 0) { Echec "la Release n'a pas été créée" }
Write-Host "`nVersion $Version publiée sur https://github.com/$depot/releases" -ForegroundColor Green
