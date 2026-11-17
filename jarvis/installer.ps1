# Installation de Jarvis (version de test). Lancé par Installer-Jarvis.bat.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Etape($texte) { Write-Host "`n== $texte ==" -ForegroundColor Cyan }

Etape "Vérification des outils"
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Host "Il manque 'uv' (gestionnaire Python). Installe-le avec :  winget install astral-sh.uv" -ForegroundColor Yellow
    exit 1
}

Etape "Python 3.12 et bibliothèques (environ 1,5 Go)"
if (-not (Test-Path .venv\Scripts\python.exe)) { uv venv --python 3.12 .venv }
uv pip uninstall --python .venv\Scripts\python.exe -y onnxruntime
uv pip install --python .venv\Scripts\python.exe -r requirements.txt
uv pip install --reinstall --python .venv\Scripts\python.exe onnxruntime-gpu==1.22.0

Etape "Modèles de voix et d'écoute (environ 1,7 Go)"
.venv\Scripts\python.exe outils\telecharger_modeles.py

Etape "Moteur d'IA : LM Studio (gratuit) et un petit modèle (environ 2,5 Go)"
$lms = Join-Path $env:USERPROFILE ".lmstudio\bin\lms.exe"
if (-not (Test-Path $lms)) {
    winget install --id ElementLabs.LMStudio -e --silent --accept-package-agreements --accept-source-agreements
    if ($LASTEXITCODE -ne 0) {
        Write-Host "L'installation automatique de LM Studio n'a pas marché. J'ouvre la page de téléchargement : installe LM Studio, puis ouvre-le une fois." -ForegroundColor Yellow
        explorer.exe "https://lmstudio.ai/download"
    }
}
if (Test-Path $lms) {
    & $lms get qwen3-4b --yes
} else {
    Write-Host "Ouvre LM Studio une fois, puis télécharge un modèle (par exemple Qwen3 4B)." -ForegroundColor Yellow
}
Write-Host "PC trop lent ou pas de place ? Tu peux aussi coller une clé gratuite (Google Gemini ou Groq) dans les réglages de Jarvis : clic droit sur la barre, Paramètres, IA." -ForegroundColor Yellow

Etape "Test complet"
.venv\Scripts\python.exe main.py --test
Write-Host "`nC'est prêt ! Lance Jarvis avec Lancer-Jarvis.bat" -ForegroundColor Green
