import("MrxGui")
import("MrxUtil_Shell")

function CreateGui(uPlayerGuid)
  ASSERT("userdata" == type(uPlayerGuid))
  if not _AllRequiredModulesLoaded() then
    table.insert(_tPendingList, uPlayerGuid)
    if not _bLoadingNow then
      MrxGui.LoadGuiFile("MrxGuiHudLayout2", HudLoaded, uPlayerGuid)
      MrxGui.LoadGuiFile("MrxGuiBinocularsLayout", ScopeLoaded, uPlayerGuid)
      MrxGui.LoadGuiFile("MrxGuiSatelliteLayout", SatelliteLoaded, uPlayerGuid)
      MrxGui.LoadGuiFile("MrxGuiPdaLayout", PdaLoaded, uPlayerGuid)
      _bLoadingNow = true
    end
    return
  end
  if not _tPlayerGuiList[uPlayerGuid] then
    local oHud, oScope, oSatellite, oPda
    if _bFirstGuiInQueue then
      oHud = {}
      oScope = {}
      oSatellite = {}
      oPda = {}
      oHud.AddedWidgetList = _oMasterHud.AddedWidgetList
      oScope.AddedWidgetList = _oMasterScope.AddedWidgetList
      oSatellite.AddedWidgetList = _oMasterSatellite.AddedWidgetList
      oPda.AddedWidgetList = _oMasterPda.AddedWidgetList
      _oMasterHud.AddedWidgetList = nil
      _oMasterScope.AddedWidgetList = nil
      _oMasterSatellite.AddedWidgetList = nil
      _oMasterPda.AddedWidgetList = nil
      _bFirstGuiInQueue = false
    else
      oHud = MrxGui.DuplicateLayout(_oMasterHud)
      oScope = MrxGui.DuplicateLayout(_oMasterScope)
      oSatellite = MrxGui.DuplicateLayout(_oMasterSatellite)
      oPda = MrxGui.DuplicateLayout(_oMasterPda)
    end
    MrxGui.AssignLayoutToPlayer(oHud, uPlayerGuid)
    MrxGui.AssignLayoutToPlayer(oScope, uPlayerGuid)
    MrxGui.AssignLayoutToPlayer(oSatellite, uPlayerGuid)
    MrxGui.AssignLayoutToPlayer(oPda, uPlayerGuid)
    MrxGui.PushAllTextToFront(oHud)
    local tGui = {}
    tGui.oHud = oHud
    tGui.oScope = oScope
    tGui.oSatellite = oSatellite
    tGui.oPda = oPda
    tGui.nHudState = 0
    _tPlayerGuiList[uPlayerGuid] = tGui
    _tHudStates[uPlayerGuid] = true
    if _tPendingHudWidgets[uPlayerGuid] then
      for oWidget, bAddChildren in pairs(_tPendingHudWidgets[uPlayerGuid]) do
        AddWidgetToHud(uPlayerGuid, oWidget, bAddChildren)
      end
      _tPendingHudWidgets[uPlayerGuid] = nil
    end
    if Player.GetLocalPlayer() == uPlayerGuid then
      _G.MessageBox = MrxGui.GetWidgetByName("MessageBox")
      _G.Minimap = MrxGui.GetWidgetByName("Minimap")
      _G.ObjectiveTray = MrxGui.GetWidgetByName("Objective Tray")
      _G.SubtitleBuffer = MrxGui.GetWidgetByName("Subtitle Buffer")
      _G.MapLabel = MrxGui.GetWidgetByName("Map Label")
    end
  end
  if Sys.NoHud() then
    ToggleHud(uPlayerGuid, false)
  end
  if MrxGui.IsE3HudModeActive() then
    local tNewEvent = {}
    tNewEvent.EventType = "E3HudMode"
    tNewEvent.bOn = MrxGui.IsE3HudModeActive()
    MrxGui.SendEvent(tNewEvent)
  end
  if _fLoadingDone then
    MrxUtil_Shell.CallWithOptionalArgs(_fLoadingDone, _tLoadingDoneData)
    _fLoadingDone = nil
    _tLoadingDoneData = nil
  end
end

function ToggleHud(uPlayerGuid, bEnable, sContext)
  if "userdata" ~= type(uPlayerGuid) then
    return
  end
  if _tPlayerGuiList[uPlayerGuid] and _tPlayerGuiList[uPlayerGuid].oHud then
    if bEnable then
      _tPlayerGuiList[uPlayerGuid].nHudState = _tPlayerGuiList[uPlayerGuid].nHudState - 1
    else
      _tPlayerGuiList[uPlayerGuid].nHudState = _tPlayerGuiList[uPlayerGuid].nHudState + 1
    end
    if bEnable then
      if _tPlayerGuiList[uPlayerGuid].nHudState <= 0 then
        _tPlayerGuiList[uPlayerGuid].nHudState = 0
        MrxGui.SetAllWidgetsSleep(_tPlayerGuiList[uPlayerGuid].oHud, false)
        _tHudStates[uPlayerGuid] = true
      end
    else
      MrxGui.SetAllWidgetsSleep(_tPlayerGuiList[uPlayerGuid].oHud, true)
      _tHudStates[uPlayerGuid] = false
      if "string" == type(sContext) then
        if "briefing" == sContext then
          _DetoggleWidgetRecursive("MessageBox", uPlayerGuid)
          _DetoggleWidgetRecursive("Subtitle Buffer", uPlayerGuid)
          _DetoggleWidget("Context Action Text", uPlayerGuid)
          _DetoggleWidget("Faction Display", uPlayerGuid)
          _DetoggleWidgetRecursive("Resource Counters", uPlayerGuid)
        elseif "hijack" == sContext then
          _DetoggleWidgetRecursive("MessageBox", uPlayerGuid)
          _DetoggleWidget("Action Hijack", uPlayerGuid)
          _DetoggleWidget("Subtitle Buffer", uPlayerGuid)
        elseif "satellite" == sContext then
          _DetoggleWidgetRecursive("MessageBox", uPlayerGuid)
          _DetoggleWidgetRecursive("Subtitle Buffer", uPlayerGuid)
          _DetoggleWidgetRecursive("tutorial", uPlayerGuid)
        elseif "scope" == sContext then
          _DetoggleWidgetRecursive("MessageBox", uPlayerGuid)
          _DetoggleWidgetRecursive("Subtitle Buffer", uPlayerGuid)
        end
      end
    end
  end
end

function _DetoggleWidget(sName, uOwner)
  local oWidget = MrxGui.GetWidgetByNameAndOwner(sName, uOwner)
  if oWidget then
    oWidget:SetSleeping(false)
  end
end

function _DetoggleWidgetRecursive(sName, uOwner)
  local oWidget = MrxGui.GetWidgetByNameAndOwner(sName, uOwner)
  if oWidget then
    _RecursiveWakeup(oWidget)
  end
end

function _RecursiveWakeup(oWidget)
  local tChildren = oWidget:GetChildren()
  oWidget:SetSleeping(false)
  for n, oChild in pairs(tChildren) do
    _RecursiveWakeup(oChild)
  end
end

function GetHudState(uPlayerGuid)
  return _tHudStates[uPlayerGuid]
end

function AddWidgetToHud(uPlayerGuid, oWidget, bIncludeChildren)
  if uPlayerGuid and _tPlayerGuiList[uPlayerGuid] and _tPlayerGuiList[uPlayerGuid].oHud then
    local oHud = _tPlayerGuiList[uPlayerGuid].oHud
    table.insert(oHud.AddedWidgetList, oWidget)
    if bIncludeChildren then
      local tChildren = oWidget:GetChildren()
      for n, oChild in ipairs(tChildren) do
        AddWidgetToHud(uPlayerGuid, oChild, true)
      end
    end
  elseif uPlayerGuid then
    if not _tPendingHudWidgets[uPlayerGuid] then
      _tPendingHudWidgets[uPlayerGuid] = {}
    end
    _tPendingHudWidgets[uPlayerGuid][oWidget] = bIncludeChildren or false
  end
end

function RemoveWidgetFromHud(uPlayerGuid, oWidget, bRemoveChildren)
  if uPlayerGuid and _tPlayerGuiList[uPlayerGuid] and _tPlayerGuiList[uPlayerGuid].oHud then
    local tList = _tPlayerGuiList[uPlayerGuid].oHud.AddedWidgetList
    local nPos
    local i = 1
    while tList[i] and not nPos do
      if tList[i] == oWidget then
        nPos = i
      end
      i = i + 1
    end
    if nPos then
      table.remove(tList, nPos)
    end
    if bRemoveChildren then
      local tChildren = oWidget:GetChildren()
      for n, oChild in ipairs(tChildren) do
        RemoveWidgetFromHud(uPlayerGuid, oChild, true)
      end
    end
  elseif _tPendingHudWidgets[uPlayerGuid] and _tPendingHudWidgets[uPlayerGuid][oWidget] then
    _tPendingHudWidgets[uPlayerGuid][oWidget] = nil
  end
end

function ToggleSatellite(uPlayerGuid, bEnable, sType, bSuppressMinigame)
  if bEnable then
    Player.SetPDAMapModeCallback(uPlayerGuid, false, ApplySatelliteUpdateEvent)
  else
    Player.SetPDAMapModeCallback(uPlayerGuid, false, DoNothing)
  end
  local tEvent = {
    EventType = "SatelliteStateChange",
    uPlayerGuid = uPlayerGuid,
    bActivate = bEnable,
    bAdvanced = "advanced" == sType,
    bMinigame = not bSuppressMinigame
  }
  MrxGui.SendEvent(tEvent)
  tEvent = {
    EventType = "SatelliteProgressUpdate",
    uPlayerGuid = uPlayerGuid,
    nX = nil,
    nY = nil,
    nZ = nil,
    nPercent = 0
  }
  MrxGui.SendEvent(tEvent)
end

function ApplySatelliteUpdateEvent(uPlayer, nX, nY, nZ, nPercent)
  local tEvent = {
    EventType = "SatelliteProgressUpdate",
    uPlayerGuid = uPlayer,
    nX = nX,
    nY = nY,
    nZ = nZ,
    nPercent = nPercent
  }
  MrxGui.SendEvent(tEvent)
end

function DoNothing()
end

function SetSatelliteSuccessCallback(uPlayer, fCallback, tData)
  local oWidget = MrxGui.GetWidgetByNameAndOwner("Satellite overlay", uPlayer)
  if oWidget then
    oWidget:SetMinigameCallback(fCallback, tData)
  else
    Debug.Printf("Failed to set callback, satellite designation will not work.")
  end
end

function SetSatelliteMinigameData(uPlayer, tData)
  local oWidget = MrxGui.GetWidgetByNameAndOwner("Satellite overlay", uPlayer)
  if oWidget and oWidget.SetMinigameSectors then
    oWidget:SetMinigameSectors(tData)
  end
end

function SetSatelliteCost(uPlayer, nCost)
  local oWidget = MrxGui.GetWidgetByNameAndOwner("Satellite overlay", uPlayer)
  if oWidget and oWidget.SetMinigameCost then
    oWidget:SetMinigameCost(nCost)
  end
end

function DeleteGui(uPlayerGuid)
  if "userdata" == type(uPlayerGuid) and _tPlayerGuiList[uPlayerGuid] then
    _RemoveAndDeleteWidgets(_tPlayerGuiList[uPlayerGuid].oHud)
    _RemoveAndDeleteWidgets(_tPlayerGuiList[uPlayerGuid].oScope)
    _RemoveAndDeleteWidgets(_tPlayerGuiList[uPlayerGuid].oSatellite)
    _RemoveAndDeleteWidgets(_tPlayerGuiList[uPlayerGuid].oPda)
    _tPlayerGuiList[uPlayerGuid] = nil
    _tHudStates[uPlayerGuid] = nil
    MrxGui.DeleteTransientWidgets(uPlayerGuid)
  end
end

function DeleteAllGuis()
  for uPlayerGuid in pairs(_tPlayerGuiList) do
    DeleteGui(uPlayerGuid)
  end
end

function SetLoadingCompleteCallback(fFunc, tData)
  local nNumGuis = 0
  for uPlayer, oGui in pairs(_tPlayerGuiList) do
    nNumGuis = nNumGuis + 1
  end
  if 0 < nNumGuis then
    MrxUtil_Shell.CallWithOptionalArgs(fFunc, tData)
  else
    _fLoadingDone = fFunc
    _tLoadingDoneData = tData
  end
end

function HudLoaded(oHudModule)
  _SetupMasterLayouts(oHudModule, nil, nil, nil)
end

function ScopeLoaded(oScopeModule)
  _SetupMasterLayouts(nil, nil, oScopeModule, nil)
end

function SatelliteLoaded(oSatelliteModule)
  _SetupMasterLayouts(nil, oSatelliteModule, nil, nil)
end

function PdaLoaded(oPdaModule)
  _SetupMasterLayouts(nil, nil, nil, oPdaModule)
end

function _SetupMasterLayouts(oHud, oSatellite, oScope, oPda)
  ASSERT(not _AllRequiredModulesLoaded())
  if oHud then
    ASSERT(not _oMasterHud)
    _oMasterHud = oHud
  end
  if oSatellite then
    ASSERT(not _oMasterSatellite)
    _oMasterSatellite = oSatellite
  end
  if oScope then
    ASSERT(not _oMasterScope)
    _oMasterScope = oScope
  end
  if oPda then
    ASSERT(not _oMasterPda)
    _oMasterPda = oPda
  end
  if _AllRequiredModulesLoaded() then
    for nIndex, uGuid in pairs(_tPendingList) do
      CreateGui(uGuid)
    end
    _tPendingList = {}
    _bLoadingNow = false
    _bFirstGuiInQueue = true
    MrxGui.UnloadGuiFile(_oMasterHud[1])
    MrxGui.UnloadGuiFile(_oMasterSatellite[1])
    MrxGui.UnloadGuiFile(_oMasterScope[1])
    MrxGui.UnloadGuiFile(_oMasterPda[1])
    _oMasterHud = false
    _oMasterSatellite = false
    _oMasterScope = false
    _oMasterPda = false
  end
end

function _AllRequiredModulesLoaded()
  return _oMasterHud and _oMasterSatellite and _oMasterScope and _oMasterPda
end

function _RemoveAndDeleteWidgets(tLayout)
  MrxGui.RemoveAllWidgetsInLayout(tLayout)
  for nIndex, oWidget in pairs(tLayout.AddedWidgetList) do
    oWidget:delete()
  end
end

_tPlayerGuiList = false
_oMasterHud = false
_oMasterSatellite = false
_oMasterScope = false
_oMasterPda = false
_tPendingList = false
_fLoadingDone = false
_tLoadingDoneData = false
_tHudStates = false
_tPendingHudWidgets = false
_bLoadingNow = false
_bFirstGuiInQueue = true

function Init()
  _tPlayerGuiList = {}
  _tPendingList = {}
  _tHudStates = {}
  _tPendingHudWidgets = {}
end
