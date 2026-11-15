@echo off
rem Lance Givre en mode Dynamic Island (version de test).
rem Ferme d'abord un Givre deja ouvert, recompile si besoin, puis demarre l'ile.
cd /d "%~dp0"
echo Fermeture de Givre s'il est deja lance...
taskkill /IM givre.exe /F >nul 2>&1
echo Compilation (peut prendre 1 a 2 minutes la premiere fois)...
cargo build --release
if errorlevel 1 (
  echo La compilation a echoue.
  pause
  exit /b 1
)
start "" "%~dp0target\release\givre.exe" --ile
echo Dynamic Island lancee : passe la souris en haut au milieu de l'ecran.
echo Pour revenir aux widgets du bureau : clic droit sur l'ile ^> "Mode Dynamic Island".
timeout /t 4 >nul
