#define MyAppName "RGX"
#define MyAppPublisher "Rech Group"
#define MyAppURL "https://rgx.rech-group.de/"

#ifndef MyAppVersion
  #define MyAppVersion GetEnv("RGX_VERSION")
#endif
#ifndef MyBinaryPath
  #define MyBinaryPath GetEnv("RGX_BINARY")
#endif

#if MyAppVersion == ""
  #error RGX_VERSION is required
#endif
#if MyBinaryPath == ""
  #error RGX_BINARY is required
#endif

[Setup]
AppId={{F1BE6B30-BD89-4580-B300-E2415F89CFFF}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL=https://github.com/0xRech/.rgx/issues
AppUpdatesURL=https://github.com/0xRech/.rgx/releases
DefaultDirName={localappdata}\Programs\RGX
DefaultGroupName=RGX
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#SourcePath}\dist
OutputBaseFilename=RGX-Setup-{#MyAppVersion}-windows-x86_64
SetupIconFile={#SourcePath}\rgx-file-icon.ico
UninstallDisplayIcon={app}\rgx-file-icon.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes
CloseApplications=yes
RestartApplications=no
UsePreviousAppDir=yes
LicenseFile={#SourcePath}\..\..\LICENSE

[Files]
Source: "{#MyBinaryPath}"; DestDir: "{app}"; DestName: "rgx.exe"; Flags: ignoreversion
Source: "{#SourcePath}\rgx-file-icon.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourcePath}\register-rgx.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourcePath}\rgx-shell.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourcePath}\rgx-update.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourcePath}\rgx-update.cmd"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Check for RGX updates"; Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\rgx-update.ps1"""; WorkingDir: "{app}"; IconFilename: "{app}\rgx-file-icon.ico"
Name: "{group}\RGX website"; Filename: "https://rgx.rech-group.de/"
Name: "{group}\Uninstall RGX"; Filename: "{uninstallexe}"

[Run]
Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\register-rgx.ps1"" -RgxExe ""{app}\rgx.exe"" -IconSource ""{app}\rgx-file-icon.ico"""; Flags: runhidden waituntilterminated

[UninstallRun]
Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\register-rgx.ps1"" -Unregister"; Flags: runhidden waituntilterminated skipifdoesntexist

[Code]
function NormalizePathEntry(Value: String): String;
var
  LastIndex: Integer;
begin
  Result := Lowercase(Trim(Value));
  StringChangeEx(Result, '/', '\', True);
  LastIndex := Length(Result);
  while (LastIndex > 3) and (Result[LastIndex] = '\') do
  begin
    Delete(Result, LastIndex, 1);
    LastIndex := Length(Result);
  end;
end;

function UserPathContains(PathToFind: String): Boolean;
var
  ExistingPath: String;
  Parts: TArrayOfString;
  I: Integer;
begin
  Result := False;
  if not RegQueryStringValue(HKCU, 'Environment', 'Path', ExistingPath) then
    Exit;

  Parts := StringSplit(ExistingPath, [';'], stAll);
  for I := 0 to GetArrayLength(Parts) - 1 do
  begin
    if NormalizePathEntry(Parts[I]) = NormalizePathEntry(PathToFind) then
    begin
      Result := True;
      Exit;
    end;
  end;
end;

procedure AddToUserPath(PathToAdd: String);
var
  ExistingPath: String;
  NewPath: String;
begin
  if UserPathContains(PathToAdd) then
    Exit;

  if not RegQueryStringValue(HKCU, 'Environment', 'Path', ExistingPath) then
    ExistingPath := '';

  if ExistingPath = '' then
    NewPath := PathToAdd
  else if ExistingPath[Length(ExistingPath)] = ';' then
    NewPath := ExistingPath + PathToAdd
  else
    NewPath := ExistingPath + ';' + PathToAdd;

  if not RegWriteExpandStringValue(HKCU, 'Environment', 'Path', NewPath) then
    RaiseException('RGX could not be added to the current user PATH.');
end;

procedure RemoveFromUserPath(PathToRemove: String);
var
  ExistingPath: String;
  Parts: TArrayOfString;
  I: Integer;
  NewPath: String;
begin
  if not RegQueryStringValue(HKCU, 'Environment', 'Path', ExistingPath) then
    Exit;

  Parts := StringSplit(ExistingPath, [';'], stAll);
  NewPath := '';
  for I := 0 to GetArrayLength(Parts) - 1 do
  begin
    if (Trim(Parts[I]) <> '') and
       (NormalizePathEntry(Parts[I]) <> NormalizePathEntry(PathToRemove)) then
    begin
      if NewPath <> '' then
        NewPath := NewPath + ';';
      NewPath := NewPath + Parts[I];
    end;
  end;

  if NewPath = '' then
    RegDeleteValue(HKCU, 'Environment', 'Path')
  else if not RegWriteExpandStringValue(HKCU, 'Environment', 'Path', NewPath) then
    RaiseException('RGX could not be removed from the current user PATH.');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    AddToUserPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveFromUserPath(ExpandConstant('{app}'));
end;
