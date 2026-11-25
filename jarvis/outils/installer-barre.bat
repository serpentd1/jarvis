@echo off
rem Installe la barre compilee (target-paquet) a la place de celle de %LOCALAPPDATA%\Programs\Jarvis,
rem en gardant l'ancienne sous JarvisBarre.exe.avant. A lancer avec explorer.exe (hors de l'appli Claude).
setlocal
set "NEUVE=%~dp0..\barre\target-paquet\release\JarvisBarre.exe"
set "DOSSIER=%LOCALAPPDATA%\Programs\Jarvis"
set "JOURNAL=%~dp0installer-barre.log"
echo %date% %time% : debut > "%JOURNAL%"
if not exist "%NEUVE%" (
  echo barre compilee introuvable >> "%JOURNAL%"
  exit /b 1
)
rem 1. la barre se ferme proprement (elle enregistre ses reglages)
curl -s -m 3 -X POST http://127.0.0.1:47802/quitter >nul 2>&1
timeout /t 2 /nobreak >nul
rem 2. sauvegarde de l'ancienne, puis copie (on force la fermeture si elle est encore la)
if not exist "%DOSSIER%\JarvisBarre.exe.avant" copy /y "%DOSSIER%\JarvisBarre.exe" "%DOSSIER%\JarvisBarre.exe.avant" >> "%JOURNAL%" 2>&1
set /a essai=0
:copie
copy /y "%NEUVE%" "%DOSSIER%\JarvisBarre.exe" >> "%JOURNAL%" 2>&1
if not errorlevel 1 goto lance
set /a essai+=1
if %essai% geq 5 (
  echo copie impossible >> "%JOURNAL%"
  goto lance
)
taskkill /im JarvisBarre.exe /f >> "%JOURNAL%" 2>&1
timeout /t 1 /nobreak >nul
goto copie
:lance
rem 3. on relance la barre (si la voix ne l'a pas deja fait)
timeout /t 1 /nobreak >nul
tasklist /fi "imagename eq JarvisBarre.exe" | find /i "JarvisBarre.exe" >nul || start "" /d "%DOSSIER%" "%DOSSIER%\JarvisBarre.exe"
echo %date% %time% : fin >> "%JOURNAL%"
endlocal
