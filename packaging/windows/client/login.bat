@echo off
setlocal
if "%~1"=="" (
  echo Usage: login.bat USER PASSWORD SERVER_URL
  exit /b 1
)
if "%~2"=="" (
  echo Usage: login.bat USER PASSWORD SERVER_URL
  exit /b 1
)
if "%~3"=="" (
  echo Usage: login.bat USER PASSWORD SERVER_URL
  exit /b 1
)
%~dp0messanger.exe login --user "%~1" --password "%~2" --server "%~3"
