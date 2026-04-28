@echo off
setlocal
if "%~1"=="" (
  echo Usage: register.bat USER PASSWORD SERVER_URL
  exit /b 1
)
if "%~2"=="" (
  echo Usage: register.bat USER PASSWORD SERVER_URL
  exit /b 1
)
if "%~3"=="" (
  echo Usage: register.bat USER PASSWORD SERVER_URL
  exit /b 1
)
%~dp0messanger.exe register --user "%~1" --password "%~2" --server "%~3"
