# Construit dist\Installer-Jarvis.exe : UN seul fichier à donner, pour UNE seule appli (une seule icône) :
# la voix de Jarvis + JarvisBarre.exe (l'île en haut de l'écran ET les widgets du bureau, ex-Givre) + extension Chrome.
# L'icône (jarvis.ico) se redessine avec : .venv\Scripts\python.exe outils\icone_jarvis.py
#   powershell -ExecutionPolicy Bypass -File outils\construire.ps1            (tout recompiler)
#   powershell -ExecutionPolicy Bypass -File outils\construire.ps1 -SansBarre (réutiliser la barre déjà compilée)
param([switch]$SansBarre)

$jarvis = Split-Path $PSScriptRoot -Parent
$racine = Split-Path $jarvis -Parent
$dist = Join-Path $jarvis "dist"
# Les exe publiés ne doivent pas contenir ton chemin (C:\Users\<toi>\…) : on le remplace par « ~ ».
$env:RUSTFLAGS = "--remap-path-prefix=$($env:USERPROFILE)=~"
# Connexion Google pour tout le monde (tes potes) : le client OAuth de ton appli Google (google.json) est intégré à la
# barre au moment de la compilation. Il n'est jamais dans le dépôt GitHub (google.json est exclu par .gitignore).
$googleJson = @("$env:APPDATA\Jarvis\barre\google.json", "$env:APPDATA\Givre\google.json") |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if ($googleJson) { $env:JARVIS_GOOGLE_CLIENT = Get-Content $googleJson -Raw }
function Etape($texte) { Write-Host "`n== $texte ==" -ForegroundColor Cyan }
function Verifier($quoi) { if ($LASTEXITCODE -ne 0) { Write-Host "ÉCHEC : $quoi" -ForegroundColor Red; exit 1 } }

# La barre (jarvis\barre) est compilée dans son propre dossier (target-paquet) : pas de conflit avec la barre
# qui tourne, et les options ci-dessus ne forcent pas à tout recompiler à chaque fois.
$barre = "$jarvis\barre\target-paquet\release\JarvisBarre.exe"
if (-not $SansBarre -or -not (Test-Path $barre)) {
    Etape "Compilation de la barre de Jarvis"
    if ($googleJson) { Write-Host "Connexion Google intégrée (depuis $googleJson)" }
    else { Write-Host "(pas de google.json : la connexion Google ne sera pas dans cette barre)" -ForegroundColor Yellow }
    cargo build --release --manifest-path "$jarvis\barre\Cargo.toml" --target-dir "$jarvis\barre\target-paquet"
    Verifier "la barre ne compile pas"
}
if (-not (Test-Path $barre)) { Write-Host "JarvisBarre.exe introuvable : compile la barre d'abord." -ForegroundColor Red; exit 1 }

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
Copy-Item "$jarvis\main.py", "$jarvis\requirements.txt", "$jarvis\LISEZMOI.md", "$jarvis\VERSION", $lanceur $charge
# Réglages NEUTRES (config.defaut.toml), jamais ton config.toml : il contient tes contacts, ta ville…
Copy-Item "$jarvis\config.defaut.toml" "$charge\config.toml"
if (Test-Path "$jarvis\DEPOT") { Copy-Item "$jarvis\DEPOT" $charge }
foreach ($dossier in "core", "modules") {
    New-Item -ItemType Directory "$charge\$dossier" | Out-Null
    Copy-Item "$jarvis\$dossier\*.py" "$charge\$dossier"
}
New-Item -ItemType Directory "$charge\outils" | Out-Null
Copy-Item "$jarvis\outils\telecharger_modeles.py" "$charge\outils"
Copy-Item "$jarvis\extension-chrome" "$charge\extension-chrome" -Recurse
Copy-Item $barre "$charge\JarvisBarre.exe"
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
