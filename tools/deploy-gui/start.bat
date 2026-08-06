@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0"

if not exist "config.json" (
  if exist "config.example.json" (
    echo config.json 이 없어 config.example.json 을 복사합니다.
    copy /Y "config.example.json" "config.json" >nul
    echo.
    echo  ※ 첫 실행입니다. notepad 로 config.json 의 IP·키·경로를 수정한 뒤
    echo    이 창을 닫고 start.bat 을 다시 실행하세요.
    echo.
    notepad "config.json"
    pause
    exit /b 1
  )
  echo config.json 과 config.example.json 이 없습니다.
  pause
  exit /b 1
)

where py >nul 2>&1
if %ERRORLEVEL%==0 (
  set "PY=py -3"
) else (
  where python >nul 2>&1
  if %ERRORLEVEL%==0 (
    set "PY=python"
  ) else (
    echo Python 을 찾을 수 없습니다. Python 3 를 PATH 에 추가하세요.
    pause
    exit /b 1
  )
)

echo 배포 GUI 시작 중... http://127.0.0.1:8765
echo 이 창을 닫으면 서버도 종료됩니다.
echo.

start "" cmd /c "timeout /t 1 /nobreak >nul & start http://127.0.0.1:8765"
%PY% server.py
if errorlevel 1 pause
endlocal
