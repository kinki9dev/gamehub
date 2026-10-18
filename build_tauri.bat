@echo off
chcp 65001 >nul
cd /d "%~dp0"
where cargo >nul 2>nul || (echo Rust ne najden. Ustanovite s rustup.rs & pause & exit /b 1)
where npm >nul 2>nul || (echo Node.js ne najden. Ustanovite s nodejs.org & pause & exit /b 1)
call npm install || (pause & exit /b 1)
call npx tauri build || (echo Oshibka sborki & pause & exit /b 1)
copy /y "src-tauri\target\release\gamehub.exe" "GameHub.exe" >nul
echo.
echo Gotovo: GameHub.exe lezhit ryadom s etim fajlom.
pause
