@echo off
if "%~1"=="" (
  %~dp0messanger.exe logout
) else (
  %~dp0messanger.exe logout --server "%~1"
)
