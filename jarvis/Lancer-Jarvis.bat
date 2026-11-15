@echo off
chcp 65001 >nul
cd /d "%~dp0"
if not exist .venv\Scripts\python.exe (
  echo Jarvis n'est pas encore installe : double-clique d'abord sur Installer-Jarvis.bat
  pause
  exit /b 1
)
rem Ajoute --clavier pour parler avec la touche Entree, --texte pour ecrire, --test pour verifier.
.venv\Scripts\python.exe main.py %*
pause
