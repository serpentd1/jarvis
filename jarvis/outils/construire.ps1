# Construit dist\Installer-Jarvis.exe : UN seul fichier à donner (Jarvis + barre Givre + extension Chrome).
#   powershell -ExecutionPolicy Bypass -File outils\construire.ps1            (tout recompiler)
#   powershell -ExecutionPolicy Bypass -File outils\construire.ps1 -SansGivre (réutiliser givre.exe déjà compilé)
param([switch]$SansGivre)

$jarvis = Split-Path $PSScriptRoot -Parent
$racine = Split-Path $jarvis -Parent
$dist = Join-Path $jarvis "dist"
# Les exe publiés ne doivent pas contenir ton chemin (C:\Users\<toi>\…) : on le remplace par « ~ ».
$env:RUSTFLAGS = "--remap-path-prefix=$($env:USERPROFILE)=~"
function Etape($texte) { Write-Host "`n== $texte ==" -ForegroundColor Cyan }
function Verifier($quoi) { if ($LASTEXITCODE -ne 0) { Write-Host "ÉCHEC : $quoi" -ForegroundColor Red; exit 1 } }

if (-not $SansGivre) {
    Etape "Compilation de la barre Givre"
    cargo build --release --manifest-path "$racine\givre\Cargo.toml"
    Verifier "Givre ne compile pas"
}
$givre = "$racine\givre\target\release\givre.exe"
if (-not (Test-Path $givre)) { Write-Host "givre.exe introuvable : compile Givre d'abord." -ForegroundColor Red; exit 1 }

Etape "Compilation de Jarvis.exe"
cargo build --release --manifest-path "$jarvis\lanceur\Cargo.toml"
Verifier "Jarvis.exe ne compile pas"
$lanceur = "$jarvis\lanceur\target\release\Jarvis.exe"
try { Copy-Item $lanceur "$jarvis\Jarvis.exe" -Force -ErrorAction Stop }
catch { Write-Host "(jarvis\Jarvis.exe est en train de tourner : pas remplacé, l'installeur prend la version compilée)" -ForegroundColor Yellow }

Etape "Rassemblement des fichiers à installer"
$charge = Join-Path $dist "charge"
Remove-Item $charge -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $charge | Out-Null
Copy-Item "$jarvis\main.py", "$jarvis\config.toml", "$jarvis\requirements.txt", "$jarvis\LISEZMOI.md", "$jarvis\VERSION", $lanceur $charge
if (Test-Path "$jarvis\DEPOT") { Copy-Item "$jarvis\DEPOT" $charge }
foreach ($dossier in "core", "modules") {
    New-Item -ItemType Directory "$charge\$dossier" | Out-Null
    Copy-Item "$jarvis\$dossier\*.py" "$charge\$dossier"
}
New-Item -ItemType Directory "$charge\outils" | Out-Null
Copy-Item "$jarvis\outils\telecharger_modeles.py" "$charge\outils"
Copy-Item "$jarvis\extension-chrome" "$charge\extension-chrome" -Recurse
Copy-Item $givre "$charge\Givre.exe"
$zip = Join-Path $dist "charge.zip"
Remove-Item $zip -ErrorAction SilentlyContinue
Compress-Archive -Path "$charge\*" -DestinationPath $zip
Write-Host ("{0:N1} Mo à installer" -f ((Get-Item $zip).Length / 1MB))

Etape "Compilation de l'installeur"
$env:JARVIS_CHARGE = $zip
cargo build --release --manifest-path "$jarvis\installeur\Cargo.toml"
Verifier "l'installeur ne compile pas"
Copy-Item "$jarvis\installeur\target\release\Installer-Jarvis.exe" "$dist\Installer-Jarvis.exe" -Force
Write-Host "`nPrêt : $dist\Installer-Jarvis.exe ($("{0:N1}" -f ((Get-Item "$dist\Installer-Jarvis.exe").Length / 1MB)) Mo)" -ForegroundColor Green
