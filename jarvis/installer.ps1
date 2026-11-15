# Installation de Jarvis (version de test). Lancé par Installer-Jarvis.bat.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Etape($texte) { Write-Host "`n== $texte ==" -ForegroundColor Cyan }

Etape "Vérification des outils"
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Host "Il manque 'uv' (gestionnaire Python). Installe-le avec :  winget install astral-sh.uv" -ForegroundColor Yellow
    exit 1
}
if (-not (Get-Command ollama -ErrorAction SilentlyContinue)) {
    Write-Host "Il manque Ollama. Télécharge-le sur https://ollama.com/download puis relance." -ForegroundColor Yellow
    exit 1
}

Etape "Python 3.12 et bibliothèques (environ 1,5 Go)"
if (-not (Test-Path .venv\Scripts\python.exe)) { uv venv --python 3.12 .venv }
uv pip install --python .venv\Scripts\python.exe -r requirements.txt

Etape "Modèles de voix et d'écoute (environ 1,7 Go)"
.venv\Scripts\python.exe outils\telecharger_modeles.py

Etape "Modèle d'IA Ollama (environ 5 Go)"
$modele = .venv\Scripts\python.exe -c "from core import config; print(config.charger()['cerveau']['modele'])"
ollama pull $modele

Etape "Test complet"
.venv\Scripts\python.exe main.py --test
Write-Host "`nC'est prêt ! Lance Jarvis avec Lancer-Jarvis.bat" -ForegroundColor Green
