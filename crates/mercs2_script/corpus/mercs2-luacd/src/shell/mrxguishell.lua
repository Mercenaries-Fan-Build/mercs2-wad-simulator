import("MrxGui")
import("MrxGuiBase")
import("MrxGuiShellBootstrap")
import("MrxSound")
_nTimeToAttractMode = 60
_bWaitForDelay = 0

function _ClearPressSpaceDelay(data)
  Debug.Printf("Clearing wait for delay. Can now skip legal")
  _bWaitForDelay = 0
end

import("MrxMultiPageMenu")
_SkipButton = MrxGuiBase.Joystick.BUTTON_ALT1_2
tMissions = {
  "AllCon001",
  "AllCon002",
  "AllCon003",
  "AllCon008",
  "AllCon050",
  "AllCon052",
  "AllCon053",
  "AllJob002",
  "AllJob003",
  "AllJob010",
  "AllJob020",
  "ChiCon001",
  "ChiCon002",
  "ChiCon003",
  "ChiCon008",
  "ChiCon009",
  "ChiCon050",
  "ChiCon051",
  "ChiCon053",
  "ChiJob002",
  "ChiJob003",
  "ChiJob010",
  "ChiJob020",
  "GurCon001",
  "GurCon002",
  "GurCon003",
  "GurCon005",
  "GurCon050",
  "GurCon052",
  "GurCon053",
  "GurJob001",
  "GurJob002",
  "GurJob006",
  "GurJob012",
  "GurJob020",
  "JetCon001",
  "MecCon001",
  "OilCon001",
  "OilCon002",
  "OilCon003",
  "OilCon005",
  "OilCon020",
  "OilCon021",
  "OilCon050",
  "OilCon051",
  "OilCon052",
  "OilJob004",
  "OilJob008",
  "OilJob011",
  "OilJob012",
  "PirCon001",
  "PirCon002",
  "PirCon003",
  "PirCon004",
  "PirCon051",
  "PirCon052",
  "PirJob001",
  "PirJob012",
  "PirJob020",
  "PmcCon001",
  "PmcCon002",
  "PmcCon003",
  "PmcCon004",
  "PmcCon013",
  "PmcCon015",
  "PmcCon016",
  "PmcCon018",
  "PmcCon031",
  "PmcCon032",
  "PmcCon033",
  "PmcCon034",
  "VzaCon001"
}
_sMission = nil
Joystick = {
  BUTTON_PAD1_U = 1,
  BUTTON_PAD1_D = 2,
  BUTTON_PAD1_L = 3,
  BUTTON_PAD1_R = 4,
  BUTTON_PAD2_U = 5,
  BUTTON_PAD2_D = 6,
  BUTTON_PAD2_L = 7,
  BUTTON_PAD2_R = 8,
  BUTTON_L_STICK_L = 9,
  BUTTON_L_STICK_R = 10,
  BUTTON_L_STICK_U = 11,
  BUTTON_L_STICK_D = 12,
  BUTTON_R_STICK_L = 13,
  BUTTON_R_STICK_R = 14,
  BUTTON_R_STICK_U = 15,
  BUTTON_R_STICK_D = 16,
  BUTTON_ALT1_1 = 17,
  BUTTON_ALT1_2 = 18,
  BUTTON_ALT1_3 = 19,
  BUTTON_ALT2_1 = 20,
  BUTTON_ALT2_2 = 21,
  BUTTON_ALT2_3 = 22,
  BUTTON_SYS1 = 23,
  BUTTON_SYS2 = 24
}
tControlMap = {
  [Joystick.BUTTON_PAD1_U] = "[SHELL.Controls.Support_Menu]",
  [Joystick.BUTTON_PAD1_D] = "[SHELL.Controls.Support_Menu]",
  [Joystick.BUTTON_PAD1_L] = "[SHELL.Controls.Switch_Explosive]",
  [Joystick.BUTTON_PAD2_U] = "[SHELL.Controls.Action]",
  [Joystick.BUTTON_PAD2_D] = "[SHELL.Controls.Jump]",
  [Joystick.BUTTON_PAD2_L] = "[SHELL.Controls.Reload]",
  [Joystick.BUTTON_PAD2_R] = "[SHELL.Controls.Melee]",
  [Joystick.BUTTON_L_STICK_L] = "[SHELL.Controls.LeftRight]",
  [Joystick.BUTTON_L_STICK_U] = "[SHELL.Controls.UpDown]",
  [Joystick.BUTTON_R_STICK_L] = "[SHELL.Controls.TurnLR]",
  [Joystick.BUTTON_R_STICK_U] = "[SHELL.Controls.PitchUpDown]",
  [Joystick.BUTTON_ALT1_1] = "[SHELL.Controls.Crouch]",
  [Joystick.BUTTON_ALT1_2] = "[SHELL.Controls.Use_Explosive]",
  [Joystick.BUTTON_ALT1_3] = "[SHELL.Controls.Sprint]",
  [Joystick.BUTTON_ALT2_1] = "[SHELL.Controls.Fire_Primary]",
  [Joystick.BUTTON_ALT2_2] = "[SHELL.Controls.Switch_Primary]",
  [Joystick.BUTTON_ALT2_3] = "[SHELL.Controls.Zoom]",
  [Joystick.BUTTON_SYS1] = "[SHELL.Controls.Pause]",
  [Joystick.BUTTON_SYS2] = "[SHELL.Controls.PDA]"
}

function OpenSkipToMissionDialog(oShell)
  local oShellFlash = oShell.CustomData.oFlash
  local oMovie = oShell.CustomData.oMovie
  oShellFlash:SetVisible(false)
  oMovie:Pause()
  MrxMultiPageMenu.Reset()
  for n, sMission in pairs(tMissions) do
    MrxMultiPageMenu.AddOption(sMission, MissionSelected, {sMission, oShell}, false)
  end
  local sBriefingsOption = "Turn Briefings On"
  if Sys.GetINIBriefing() then
    sBriefingsOption = "Turn Briefings Off"
  end
  MrxMultiPageMenu.AddOption(sBriefingsOption, MissionSelected, {
    "EnableBriefings",
    oShell
  }, true, false)
  MrxMultiPageMenu.AddOption("Cancel", MissionSelected, {nil, oShell}, true, true)
  MrxMultiPageMenu.Display("Select starting mission:")
end

function MissionSelected(sMission, oShell)
  local oShellFlash = oShell.CustomData.oFlash
  local oMovie = oShell.CustomData.oMovie
  oShellFlash:SetVisible(true)
  oMovie:Play()
  if sMission == "EnableBriefings" then
    Sys.SetINIBriefing(not Sys.GetINIBriefing())
    OpenSkipToMissionDialog(oShell)
  elseif sMission then
    Sys.SetSkipMission(sMission)
    _sMission = sMission
  end
end

function OpenInstallDialog(oShell)
  local oShellFlash = oShell.CustomData.oFlash
  local oMovie = oShell.CustomData.oMovie
  oShellFlash:SetVisible(false)
  oMovie:Pause()
  MrxMultiPageMenu.Reset()
  MrxMultiPageMenu.AddOption("Install to HDD", InstallCallback, {1, oShell}, false)
  MrxMultiPageMenu.AddOption("Use Existing HDD Install", InstallCallback, {2, oShell}, false)
  MrxMultiPageMenu.AddOption("Use Source Media", InstallCallback, {3, oShell}, false)
  MrxMultiPageMenu.Display("HDD Installer options")
end

function InstallCallback(iOption, oShell)
  if iOption == 1 then
    Junk.InstallToHDD()
  elseif iOption == 2 then
    Junk.UseExistingInstall()
  end
  local oShellFlash = oShell.CustomData.oFlash
  local oMovie = oShell.CustomData.oMovie
  oShellFlash:SetVisible(true)
  oMovie:Play()
end

function GetShellGfxFilename()
  return "shell.gfx"
end

function ResetStartButtonState()
  local s = MrxGui.GetWidgetByName("Shell")
  if s then
    s.CustomData.bHitStart = nil
  end
end

function HandleResetStartButton(oShell)
  oShell.CustomData.bHitStart = nil
end

function HandleGameStateChangeEvent(oWidget, sStateName, sStateAction)
  if not sStateName or not sStateAction then
    return
  end
  if "Shell" ~= sStateName then
    return
  end
  if "Enter" == sStateAction then
    oWidget:Open()
  end
end

function ExitLoad()
  if MrxGui.IsE3HudModeActive() then
    Sys.RequestGameState("Pause")
    tEvent = {}
    tEvent.EventType = "ImposterShellEvent"
    tEvent.bOn = true
    MrxGui.SendEvent(tEvent)
    Event.Create(Event.GameStateChange, {"Pause", "Exit"}, function()
      Debug.Printf("BLACKSCREEN - FadeFromColor called from MrxGuiShell.ExitLoad GameStateChange callback")
      MrxGui.FadeFromColor(0)
    end)
  else
    Debug.Printf("BLACKSCREEN - FadeFromColor called from MrxGuiShell.ExitLoad")
    MrxGui.FadeFromColor(0)
  end
end

function HandleInitializationEvent(oWidget, tUnused)
  oWidget:SetFullscreen("pan and scan")
  local oMovie = MrxGuiBase.MovieWidget:new()
  oMovie:SetTransient(false)
  oMovie:SetOwner(oWidget:GetOwner())
  oMovie:SetAnchoring("right", "center")
  local nMovieWidth = 396.6667
  local nMovieX = 640 - nMovieWidth
  oMovie:SetLocation(nMovieX, 0, nMovieX + nMovieWidth, 480)
  oWidget:AddChild(oMovie)
  MrxGui.AddWidget(oMovie)
  oWidget.CustomData.oMovie = oMovie
  local oFlash = MrxGuiBase.FlashWidget:new()
  oFlash:SetTransient(false)
  if oFlash then
    oFlash:SetOwner(oWidget:GetOwner())
    oFlash:SetFullscreen(true)
    oFlash:SetAnchoring("center", "center")
    if not Sys.AutoLoad() then
      oFlash:SetSwfFile(GetShellGfxFilename(), CompleteFlashSetup, {oFlash})
      oFlash.CustomData.bHasFile = true
      oFlash:SetFlashEventHandler("LTIStartNewGame", _LTIStartNewGame, {})
      oFlash:SetFlashEventHandler("exitGame", _ExitGameFlashCallback, {})
      oFlash:SetFlashEventHandler("LTIFscommand", _LTIFscommand, {})
      oFlash:SetFlashEventHandler("LTIVideoSetGamma", _LTIVideoSetGamma, {})
      oFlash:SetFlashEventHandler("LTIVideoGetViewDistance", _LTIVideoGetViewDistance, {})
      oFlash:SetFlashEventHandler("LTIVideoSwitchOpt1", _LTIVideoSwitchOpt1, {})
      oFlash:SetFlashEventHandler("LTIInputGeneralOptions", _LTIInputGeneralOptions, {})
      oFlash:SetFlashEventHandler("LTIOverBoundResponse", _LTIOverBoundResponse, {})
      oFlash:SetFlashEventHandler("LTIInputKMChangeInput", _LTIInputKMChangeInput, {})
      oFlash:SetFlashEventHandler("LTIInputJoystickChangePrimary", _LTIInputJoystickChangePrimary, {})
      oFlash:SetFlashEventHandler("LTIInputJoystickChangeInput", _LTIInputJoystickChangeInput, {})
      oFlash:SetFlashEventHandler("LTIJoystickOverBoundResponse", _LTIJoystickOverBoundResponse, {})
      oFlash:SetFlashEventHandler("LTICamera", _LTICamera, {})
      oFlash:SetFlashEventHandler("LTIChoseOnline", _LTIChoseOnline, {})
      oFlash:SetFlashEventHandler("PauseItemChanged", _LTIPauseItemChanged, {})
      _bWaitForDelay = 0
      if LTILibName.FirstRun() == 1 then
        Debug.Printf("FirstRun - Wait for 4 seconds before can skip legal screen")
        _bWaitForDelay = 1
        Event.Create(Event.TimerRelative, {4.5, true}, _ClearPressSpaceDelay, 0)
      end
    end
    oFlash.CustomData.oParent = oWidget
    MrxGui.AddWidget(oFlash)
    oWidget:AddChild(oFlash)
    oWidget.CustomData.oFlash = oFlash
    oFlash.CustomData.oMovie = oMovie
  end
  oWidget:SetIgnoresPause(true)
  oWidget.Open = _OpenShell
  oWidget.Close = _CloseShell
  oWidget.tChildren = oWidget:GetChildren()
  oWidget.CustomData.nTimeToAttractMode = _nTimeToAttractMode
  oWidget.CustomData.bLoading = true
  oWidget.CustomData.tServerList = {}
  oWidget.CustomData.tServerOrderList = {}
  oWidget:SetEventHandler("OpenShell", _OpenShell)
  oWidget:SetEventHandler("CloseShell", _CloseShell)
  oWidget:SetEventHandler("SetAttractModeEnable", HandleAttractModeEnable)
  oWidget:SetEventHandler("ResetStartButton", HandleResetStartButton)
  oWidget:SetUseImmortalEvents(true)
end

function MakeFullscreen(oWidget)
  oWidget:SetFullscreen(true)
end

function HandleInput(oWidget, tEvent)
  local oFlash = oWidget.CustomData.oFlash
  if oFlash and _SkipButton == tEvent.ButtonPress and Sys.IsFinalConfig and not Sys.IsFinalConfig() then
    OpenSkipToMissionDialog(oWidget)
    return
  end
  if oFlash and oFlash.EventHandlers.ControllerInput then
    oFlash.EventHandlers.ControllerInput(oFlash, tEvent)
  end
  if _bWaitForDelay ~= 0 or MrxGuiBase.Joystick.BUTTON_PAD2_D ~= tEvent.ButtonPress or not oFlash.CustomData.bLoaded then
  elseif not oWidget.CustomData.bHitStart then
    oWidget.CustomData.bHitStart = true
    Gui.DoSigninCheck()
    if Junk.IsInstallable and Junk.IsInstallable() then
      OpenInstallDialog(oWidget)
    end
  end
  if 0 <= oWidget.CustomData.nTimeToAttractMode then
    oWidget.CustomData.nTimeToAttractMode = _nTimeToAttractMode
  end
end

function HandleUpdate(oWidget, nDeltaTime)
  if oWidget.CustomData.bLoading then
    local oFlash = oWidget.CustomData.oFlash
    if oFlash.CustomData.bLoaded then
      oWidget.CustomData.bLoading = false
      if Gui.OnShellLoaded then
        Gui.OnShellLoaded()
      end
    end
  end
  if oWidget.CustomData.nTimeToAttractMode < 0 then
    return
  end
  oWidget.CustomData.nTimeToAttractMode = oWidget.CustomData.nTimeToAttractMode - nDeltaTime
  if oWidget.CustomData.nTimeToAttractMode <= 0 then
    Sys.RequestGameState("attract")
    oWidget.CustomData.nTimeToAttractMode = _nTimeToAttractMode
    oWidget:Close(true)
  end
end

function HandleAttractModeEnable(oWidget, tEvent)
  if tEvent.bEnable then
    oWidget.CustomData.nTimeToAttractMode = _nTimeToAttractMode
  else
    oWidget.CustomData.nTimeToAttractMode = -1
  end
end

function HandleServerAdd(oWidget, tEvent)
  local tServerList = oWidget.CustomData.tServerList
  local tServerOrderList = oWidget.CustomData.tServerOrderList
  local nIdentifier
  if tServerList[tEvent.uKey] then
    nIdentifier = tServerList[tEvent.uKey].nIdentifier
    HandleServerRemove(oWidget, tEvent)
  end
  if not nIdentifier then
    if not oWidget.CustomData.nLastIdentifier then
      return
    end
    oWidget.CustomData.nLastIdentifier = oWidget.CustomData.nLastIdentifier + 1
    nIdentifier = oWidget.CustomData.nLastIdentifier
  end
  local tNewServer = {}
  tNewServer.uKey = tEvent.uKey
  tNewServer.sName = tEvent.sName
  tNewServer.nStatus = tEvent.nStatus
  tNewServer.sMap = tEvent.sMap
  tNewServer.sContract = tEvent.sContract
  tNewServer.bFriendlyFire = tEvent.bFriendlyFire
  tNewServer.nIdentifier = nIdentifier
  tServerList[tNewServer.uKey] = tNewServer
  local nIndex = 0
  while tServerOrderList[nIndex] do
    nIndex = nIndex + 1
  end
  tServerOrderList[nIndex] = tNewServer
  tNewServer.nFlashIndex = nIndex
  oWidget.CustomData.oFlash:CallActionScriptCallback("addServer", {
    tNewServer.nIdentifier,
    tNewServer.sName,
    tNewServer.nStatus,
    tNewServer.sMap,
    tNewServer.sContract,
    tNewServer.bFriendlyFire
  })
  oWidget.CustomData.nNumServers = oWidget.CustomData.nNumServers + 1
end

function HandleServerRemove(oWidget, tEvent)
  local tServerList = oWidget.CustomData.tServerList
  local tServerOrderList = oWidget.CustomData.tServerOrderList
  if tServerList[tEvent.uKey] then
    local nFlashIndex = tServerList[tEvent.uKey].nFlashIndex
    tServerList[tEvent.uKey] = nil
    tServerOrderList[nFlashIndex] = nil
    local n = nFlashIndex
    while n < oWidget.CustomData.nNumServers do
      tServerOrderList[n] = tServerOrderList[n + 1]
      n = n + 1
    end
    oWidget.CustomData.oFlash:CallActionScriptCallback("removeServer", {nFlashIndex})
    for nIndex, tData in pairs(tServerOrderList) do
      tData.nFlashIndex = nIndex
    end
    oWidget.CustomData.nNumServers = oWidget.CustomData.nNumServers - 1
  end
end

function HandleServerUpdate(oWidget, tEvent)
  local tServerList = oWidget.CustomData.tServerList
  local tServerOrderList = oWidget.CustomData.tServerOrderList
  if not tServerList[tEvent.uKey] then
    HandleServerAdd(oWidget, tEvent)
    return
  end
  local tNewServer = tServerList[tEvent.uKey]
  tNewServer.uKey = tEvent.uKey
  tNewServer.sName = tEvent.sName
  tNewServer.nStatus = tEvent.nStatus
  tNewServer.sMap = tEvent.sMap
  tNewServer.sContract = tEvent.sContract
  tNewServer.bFriendlyFire = tEvent.bFriendlyFire
  local nIndex = tNewServer.nFlashIndex
  tServerList[tNewServer.uKey] = tNewServer
  tServerOrderList[nIndex] = tNewServer
  local oFlash = oWidget.CustomData.oFlash
  local n = oWidget.CustomData.nNumServers - 1
  while nIndex <= n do
    oFlash:CallActionScriptCallback("removeServer", {n})
    n = n - 1
  end
  n = nIndex
  local tCurrentData
  while n < oWidget.CustomData.nNumServers do
    tCurrentData = tServerOrderList[n]
    oFlash:CallActionScriptCallback("addServer", {
      tCurrentData.nIdentifier,
      tCurrentData.sName,
      tCurrentData.nStatus,
      tCurrentData.sMap,
      tCurrentData.sContract,
      tCurrentData.bFriendlyFire
    })
    n = n + 1
  end
end

function _RepopulateServerList(oWidget)
  local tServerList = oWidget.CustomData.tServerList
  local tServerOrderList = oWidget.CustomData.tServerOrderList
  local oFlash = oWidget.CustomData.oFlash
  local n = oWidget.CustomData.nNumServers - 1
  while 0 <= n do
    oFlash:CallActionScriptCallback("removeServer", {n})
    n = n - 1
  end
  n = 0
  local tServer
  while n < oWidget.CustomData.nNumServers do
    tServer = tServerOrderList[n]
    oFlash:CallActionScriptCallback("addServer", {
      tServer.nIdentifier,
      tServer.sName,
      tServer.nStatus,
      tServer.sMap,
      tServer.sContract,
      tServer.bFriendlyFire
    })
    tServer.nFlashIndex = n
    n = n + 1
  end
end

function _OpenShell(oWidget)
  if oWidget.CustomData.bOpen then
    return
  end
  oWidget.CustomData.bOpen = true
  oWidget:SetVisible(true)
  Sys.SetNumberOfViewports(1)
  for nIndex, oChild in pairs(oWidget.tChildren) do
    oChild:SetEnabled(true)
  end
  MrxGuiBase.GetControlFocus(oWidget)
  local oFlash = oWidget.CustomData.oFlash
  if oFlash and not oFlash.CustomData.bHasFile then
    oFlash:SetSwfFile(GetShellGfxFilename(), CompleteFlashSetup, {oFlash})
    oFlash.CustomData.bHasFile = true
  elseif oFlash.CustomData.bHasFile and oFlash.CustomData.bLoaded then
    oFlash:SetVisible(true)
    oFlash:Play()
  end
  MrxSound.EnterShellState()
  oWidget.CustomData.nTimeToAttractMode = _nTimeToAttractMode
  oWidget.CustomData.bLoading = true
  oWidget:SetEventHandler("GuiUpdate", HandleUpdate)
end

function CompleteFlashSetup(oFlash)
  oFlash.CustomData.bLoaded = true
  oFlash:SetFlashEventHandler("newGame", _NewGameFlashCallback, {})
  oFlash:SetFlashEventHandler("Enter.Lobby", _EnterLobbyFlashCallback, {})
  oFlash:SetFlashEventHandler("Exit.Lobby", _ExitLobbyFlashCallback, {})
  oFlash:SetFlashEventHandler("joinGame", _JoinGameFlashCallback, {})
  oFlash:SetFlashEventHandler("onlineQuickmatch", _QuickmatchFlashCallback, {})
  oFlash:SetFlashEventHandler("onlineOptimatchSearch", _OptimatchFlashCallback, {})
  oFlash:SetFlashEventHandler("quickmatchJoinGame", _QuickmatchJoinGameFlashCallback, {})
  oFlash:SetFlashEventHandler("optimatchJoinGame", _OptimatchJoinGameFlashCallback, {})
  oFlash:SetFlashEventHandler("friendsLobbyEntered", _EnterFriendsLobbyFlashCallback, {})
  oFlash:SetFlashEventHandler("friendsLobbyExited", _ExitFriendsLobbyFlashCallback, {})
  oFlash:SetFlashEventHandler("joinFriendsGame", _JoinFriendsGameFlashCallback, {})
  oFlash:SetFlashEventHandler("startMovie", _HandleMovieStartFlashCommand, {})
  oFlash:SetFlashEventHandler("stopMovie", _HandleMovieStopFlashCommand, {})
  if Sys.GetVersion then
    local sCode, sData = Sys.GetVersion()
    if sCode and sData then
      sCode = tostring(sCode)
      sData = tostring(sData)
    end
    if sCode and sData then
      sCode = string.gsub(sCode, "%d", "%0 ", 5)
      sData = string.gsub(sData, "%d", "%0 ", 5)
    end
    if sCode and sData then
      oFlash:CallActionScriptCallback("buildNumberCode", {sCode})
      oFlash:CallActionScriptCallback("buildNumberData", {sData})
    else
      oFlash:CallActionScriptCallback("buildNumberCode", {
        "0 0 0 0 0 0"
      })
      oFlash:CallActionScriptCallback("buildNumberData", {
        "0 0 0 0 0 0"
      })
    end
  end
  if Sys.GetShellCode then
    local sShellCode = Sys.GetShellCode()
    if sShellCode then
      sShellCode = string.gsub(sShellCode, "%w", "%0 ", 5)
      oFlash:CallActionScriptCallback("buildNumberJenNumber", {sShellCode})
    else
      oFlash:CallActionScriptCallback("buildNumberJenNumber", {
        "3 9 2 1 4 0"
      })
    end
  end
end

function _CloseShell(oWidget, bRetainShellSwf)
  if oWidget.CustomData.bOpen ~= true then
    return
  end
  oWidget:SetVisible(false)
  oWidget.CustomData.bOpen = false
  for nIndex, oChild in pairs(oWidget.tChildren) do
    oChild:SetEnabled(false)
  end
  if "boolean" ~= type(bRetainShellSwf) then
    bRetainShellSwf = false
  end
  MrxGuiBase.ReleaseControlFocus(oWidget)
  Debug.Printf("BLACKSCREEN - FadeToColor called from MrxGuiShell._CloseShell")
  MrxGui.FadeToColor(0)
  Event.Create(Event.GameStateChange, {"Loading", "Exit"}, ExitLoad)
  oWidget.CustomData.oMovie:Stop()
  local oFlash = oWidget.CustomData.oFlash
  if oFlash and oFlash.CustomData.bHasFile then
    if bRetainShellSwf then
      oFlash:Pause()
      oFlash:SetVisible(false)
    else
      oFlash:SetSwfFile(nil)
      oFlash.CustomData.bHasFile = false
      oFlash.CustomData.bLoaded = false
    end
  end
  MrxSound.ExitShellState()
  oWidget:SetEventHandler("GuiUpdate", nil)
end

function _NewGame(fCallback, tData)
  MrxGuiShellBootstrap.nPlayersSelected = 1
  local sLevelName = Sys.GetLevelName()
  local sMasterScript = Sys.GetMasterScriptName()
  ASSERT("string" == type(sLevelName))
  if Net.IsMatchmakingInternet() and Net.IsPlatformConnected() then
    Net.StartServer(Net.GetHostName(), Sys.GetLevelName(), Sys.GetMasterScriptName())
  else
    Sys.StartSingleplayer(sLevelName, sMasterScript)
  end
  MrxGuiShellBootstrap.bNeedsReloading = true
end

function _NewGameFlashCallback(oFlash, sCharacter)
  if _sMission then
    Sys.SetSkipMission(_sMission)
  end
  if "chris" == sCharacter then
    MrxGuiShellBootstrap.SetSelectedCharacter("Chris")
  elseif "jennifer" == sCharacter then
    MrxGuiShellBootstrap.SetSelectedCharacter("Jen")
  else
    MrxGuiShellBootstrap.SetSelectedCharacter("mattias")
  end
  _NewGame()
end

function _EnterLobbyFlashCallback(oFlash, sUnused)
  oFlash.CustomData.oParent.CustomData.nNumServers = 0
  oFlash.CustomData.oParent.CustomData.nLastIdentifier = 0
  Net.EnterLobby()
  Debug.Printf("Entered lan lobby")
  if not Net.IsMatchmakingInternet() then
    Event.Create(Event.TimerRelative, {0.2}, _RepopulateServerList, {
      oFlash.CustomData.oParent
    })
  end
end

function _ExitLobbyFlashCallback(oFlash, sUnused)
  Sys.RequestGameState("Shell")
  Debug.Printf("Exited lan lobby")
  local oShell = oFlash.CustomData.oParent
  oShell.CustomData.tServerList = {}
  oShell.CustomData.tServerOrderList = {}
  oShell.CustomData.nNumServers = 0
  oShell.CustomData.nLastIdentifier = 0
  if not Net.IsMatchmakingInternet() and not Net.IsOnlineConnected() then
    Net.Stop()
  end
end

function _EnterFriendsLobbyFlashCallback(oFlash, sUnused)
  oFlash.CustomData.oParent.CustomData.nNumServers = 0
  oFlash.CustomData.oParent.CustomData.nLastIdentifier = 0
  Net.EnterFriendsLobby()
  Debug.Printf("Entered friends lobby")
  if not Net.IsMatchmakingInternet() then
    Event.Create(Event.TimerRelative, {0.2}, _RepopulateServerList, {
      oFlash.CustomData.oParent
    })
  end
end

function _ExitFriendsLobbyFlashCallback(oFlash, sUnused)
  Debug.Printf("Exited friends lobby")
  local oShell = oFlash.CustomData.oParent
  oShell.CustomData.tServerList = {}
  oShell.CustomData.tServerOrderList = {}
  oShell.CustomData.nNumServers = 0
  oShell.CustomData.nLastIdentifier = 0
  Net.ExitFriendsLobby()
  if not Net.IsMatchmakingInternet() and not Net.IsOnlineConnected() then
    Net.Stop()
  end
end

function _JoinGameFlashCallback(oFlash, sNumber)
  local oShell = oFlash.CustomData.oParent
  local tServerOrderList = oShell.CustomData.tServerOrderList
  Debug.Printf("Received joinGame event, index number: " .. sNumber)
  local nIdentifier = tonumber(sNumber)
  if not nIdentifier then
    return
  end
  local tData
  for nIndex, tTestData in pairs(tServerOrderList) do
    if nIdentifier == tTestData.nIdentifier then
      tData = tTestData
    end
  end
  if tData then
    Debug.Printf("joinGame info: " .. tData.sName .. tData.sIPAddress)
  end
  if tData then
    local bSuccess = Net.ConnectToServer(tData.sName, tData.sIPAddress)
    if bSuccess then
      Event.Create(Event.TimerRelative, {0.01}, oShell.Close, {oShell})
    end
  end
end

function _JoinFriendsGameFlashCallback(oFlash, sName)
  local oShell = oFlash.CustomData.oParent
  local tServerOrderList = oShell.CustomData.tServerOrderList
  Debug.Printf("Received joinFriendsGame event, name: " .. sName)
  local bSuccess = Net.ConnectToServer(sName, "")
end

function _QuickmatchFlashCallback()
  Debug.Printf("Received quickmatch event, ")
end

function _OptimatchFlashCallback()
  Debug.Printf("Received optimatch event, ")
end

function _QuickmatchJoinGameFlashCallback(oFlash, sServerName)
  if sServerName and sServerName ~= "undefined" and sServerName ~= "" then
    Debug.Printf("Received quickmatch join game event: server_name:" .. sServerName)
    local oShell = oFlash.CustomData.oParent
    local bSuccess = Net.ConnectToServer(sServerName, "online")
    if bSuccess then
      Event.Create(Event.TimerRelative, {0.01}, oShell.Close, {oShell})
    end
  else
    Debug.Printf("Not a valid quickmatch join game event: server_name:" .. sServerName)
  end
end

function _OptimatchJoinGameFlashCallback(oFlash, sServerName)
  if sServerName and sServerName ~= "undefined" and sServerName ~= "" then
    Debug.Printf("Received optimatch join game event: server_name:" .. sServerName)
    local oShell = oFlash.CustomData.oParent
    local bSuccess = Net.ConnectToServer(sServerName, "online")
    if bSuccess then
      Event.Create(Event.TimerRelative, {0.01}, oShell.Close, {oShell})
    end
  else
    Debug.Printf("Not a valid optimatch join game event: server_name:" .. sServerName)
  end
end

function _HandleMovieStartFlashCommand(oFlash, sMovieName)
  oFlash.CustomData.oMovie:SetMovie(sMovieName)
  oFlash.CustomData.oMovie:Play(true)
end

function _HandleMovieStopFlashCommand(oFlash, sMovieName)
  oFlash.CustomData.oMovie:Stop()
end

function _LTIStartNewGame(oFlash, sUnused)
  if "chris" == sCharacter then
    MrxGuiShellBootstrap.SetSelectedCharacter("Chris")
  elseif "jennifer" == sCharacter then
    MrxGuiShellBootstrap.SetSelectedCharacter("Jen")
  else
    MrxGuiShellBootstrap.SetSelectedCharacter("mattias")
  end
  _NewGame()
  oFlash.CustomData.oParent:Close()
end

function _ExitGameFlashCallback(oFlash, sUnused)
  Sys.RequestGameState("Exiting")
  MrxGui.FadeToColor(0)
  oFlash.CustomData.oParent:Close()
end

function _LTIFscommand(oFlash, sFuncName)
  if sFuncName == "LTIVideoEnter" then
    LTILibName.LTIVideoEnter()
  elseif sFuncName == "LTIVideoSwitchMode" then
    LTILibName.LTIVideoSwitchMode()
  elseif sFuncName == "LTIVideoNextRes" then
    LTILibName.LTIVideoNextRes()
  elseif sFuncName == "LTIVideoPrevRes" then
    LTILibName.LTIVideoPrevRes()
  elseif sFuncName == "LTIVideoNextRefresh" then
    LTILibName.LTIVideoNextRefresh()
  elseif sFuncName == "LTIVideoPrevRefresh" then
    LTILibName.LTIVideoPrevRefresh()
  elseif sFuncName == "LTIVideoApplyChanges" then
    LTILibName.LTIVideoApplyChanges()
  elseif sFuncName == "LTIVideoDefault" then
    LTILibName.LTIVideoDefault()
  elseif sFuncName == "LTIVideoCancel" then
    LTILibName.LTIVideoCancel()
  elseif sFuncName == "LTIVideoAdvanceEnter" then
    LTILibName.LTIVideoAdvanceEnter()
  elseif sFuncName == "LTIVideoAdvanceDefault" then
    LTILibName.LTIVideoAdvanceDefault()
  elseif sFuncName == "LTIInputGeneralEnter" then
    LTILibName.LTIInputGeneralEnter()
  elseif sFuncName == "LTIInputKMEnter" then
    LTILibName.LTIInputKMEnter()
  elseif sFuncName == "LTIInputKMApplyChanges" then
    LTILibName.LTIInputKMApplyChanges()
  elseif sFuncName == "LTIInputKMDefault" then
    LTILibName.LTIInputKMDefault()
  elseif sFuncName == "LTIInputKMCancelInput" then
    LTILibName.LTIInputKMCancelInput()
  elseif sFuncName == "LTIInputKMExit" then
    LTILibName.LTIInputKMExit()
  elseif sFuncName == "LTIInputJoystickEnter" then
    LTILibName.LTIInputJoystickEnter()
  elseif sFuncName == "LTIInputJoystickApplyChanges" then
    LTILibName.LTIInputJoystickApplyChanges()
  elseif sFuncName == "LTIInputJoystickDefault" then
    LTILibName.LTIInputJoystickDefault()
  elseif sFuncName == "LTIInputJoystickCancel" then
    LTILibName.LTIInputJoystickCancel()
  elseif sFuncName == "LTIInputJoystickExit" then
    LTILibName.LTIInputJoystickExit()
  elseif sFuncName == "LTIInputJoystickReEnter" then
    LTILibName.LTIInputJoystickReEnter()
  elseif sFuncName == "LTIStartKeyboardInput" then
    LTILibName.LTIProfileEnter()
  elseif sFuncName == "LTIEndKeyboardInput" then
    LTILibName.LTIProfileExit()
  elseif sFuncName == "LTIenterControlDisplay" then
    for nControlIndex, sControlLabel in pairs(tControlMap) do
      oFlash:CallActionScriptCallback("controllerDisplay", {nControlIndex, sControlLabel})
    end
  elseif sFuncName == "LTIGetStartButton" then
    LTILibName.LTIGetStartButton()
  elseif sFuncName == "LTIGetDateFormat" then
    LTILibName.LTIGetDateFormat()
  end
end

function _LTIEnter(oFlash, iNumber)
  if iNumber == "1" then
    LTILibName.LTIVideoEnter()
  elseif iNumber == "2" then
    LTILibName.LTIVideoAdvanceEnter()
  elseif iNumber == "3" then
    LTILibName.LTIInputGeneralEnter()
  elseif iNumber == "4" then
    LTILibName.LTIInputKMEnter()
  elseif iNumber == "5" then
    LTILibName.LTIInputJoystickEnter()
  elseif iNumber == "6" then
    LTILibName.LTIProfileEnter()
  end
end

function _LTIVideo(oFlash, iNumber)
  if iNumber == "2" then
    LTILibName.LTIVideoSwitchMode()
  elseif iNumber == "3" then
    LTILibName.LTIVideoNextRes()
  elseif iNumber == "4" then
    LTILibName.LTIVideoPrevRes()
  elseif iNumber == "5" then
    LTILibName.LTIVideoNextRefresh()
  elseif iNumber == "6" then
    LTILibName.LTIVideoPrevRefresh()
  elseif iNumber == "7" then
    LTILibName.LTIVideoApplyChanges()
  elseif iNumber == "8" then
    LTILibName.LTIVideoCancel()
  end
end

function _LTIVideoSetGamma(oFlash, fNumber)
  LTILibName.LTIVideoSetGamma(fNumber)
end

function _LTIVideoGetViewDistance(oFlash, iNumber)
  LTILibName.LTIVideoGetViewDistance(iNumber)
end

function _LTIVideoSwitchOpt1(oFlash, iNumber)
  LTILibName.LTIVideoSwitchOpt1(iNumber)
end

function _LTIVideoAdvanceDefault(oFlash, sUnused)
  LTILibName.LTIVideoAdvanceDefault()
end

function _LTIInputGeneralOptions(oFlash, sString)
  LTILibName.LTIInputGeneralOptions(sString)
end

function _LTIInputKM(oFlash, iNumber)
  if iNumber == "1" then
    LTILibName.LTIInputKMApplyChanges()
  elseif iNumber == "2" then
    LTILibName.LTIInputKMDefault()
  elseif iNumber == "3" then
    LTILibName.LTIInputKMCancelInput()
  elseif iNumber == "4" then
    LTILibName.LTIInputKMExit()
  end
end

function _LTIInputKMChangeInput(oFlash, iNumber)
  LTILibName.LTIInputKMChangeInput(iNumber)
end

function _LTIOverBoundResponse(oFlash, iNumber)
  LTILibName.LTIOverBoundResponse(iNumber)
end

function _LTIInputJoystick(oFlash, iNumber)
  if iNumber == "1" then
    LTILibName.LTIInputJoystickApplyChanges()
  elseif iNumber == "2" then
    LTILibName.LTIInputJoystickDefault()
  elseif iNumber == "3" then
    LTILibName.LTIInputJoystickCancel()
  elseif iNumber == "4" then
    LTILibName.LTIInputJoystickExit()
  elseif iNumber == "5" then
    LTILibName.LTIInputJoystickReEnter()
  end
end

function _LTIInputJoystickChangePrimary(oFlash, iNumber)
  LTILibName.LTIInputJoystickChangePrimary(iNumber)
end

function _LTIInputJoystickChangeInput(oFlash, iNumber)
  LTILibName.LTIInputJoystickChangeInput(iNumber)
end

function _LTIJoystickOverBoundResponse(oFlash, iNumber)
  LTILibName.LTIJoystickOverBoundResponse(iNumber)
end

function _LTIProfileExit(oFlash, sUnused)
  LTILibName.LTIProfileExit()
end

function _LTICamera(oFlash, iNumber)
  LTILibName.LTICamera(iNumber)
end

function _LTIChoseOnline(oFlash, iNumber)
  LTILibName.LTIChoseOnline(iNumber)
end

function _LTIPauseItemChanged(oFlash, iNumber)
  LTILibName.LTIPauseItemChanged(iNumber)
end
