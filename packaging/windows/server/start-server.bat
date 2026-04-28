@echo off
setlocal
if not "%PORT%"=="" (
  echo Starting server on port %PORT%
) else (
  echo Starting server on default port 25655
)
%~dp0server.exe
