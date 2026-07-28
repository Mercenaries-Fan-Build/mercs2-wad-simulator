import("MrxGuiBase")
import("MrxPmc")
import("MrxGuiManager")
import("WifVzRegionNames")
import("MrxSupportData")
import("MrxGuiDialogBox")
import("MrxSound")
import("MrxPlayState")
import("WifMissionData")
import("MrxGuiHudFactionGauge")
import("MrxStatsManager")
import("MrxState")
import("MrxGui")
NETEVENT_SETSELECTEDMISSION = 0
NETEVENT_PDAOPEN = 1
NETEVENT_PDACLOSE = 2

function NetEventCallback(nEventType, tArgs)
  Debug.Printf("NetEventCallback!!!!!!!!!!!!!!!!!!!!")
  if nEventType == NETEVENT_SETSELECTEDMISSION then
    Debug.Printf("-- type == NETEVENT_SETSELECTEDMISSION")
    local oPda = MrxGuiBase.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
    if oPda then
      local sMission
      Debug.Printf("-- tArgs[1] = " .. tostring(tArgs[1]))
      if tArgs[1] then
        sMission = WifMissionData.GetMissionIdFromIndex(tArgs[1])
      end
      oPda:SetSelectedMission(sMission, true)
    else
      Debug.Printf("failed to set selected mission")
      Event.Create(Event.TimerRelative, {1}, NetEventCallback, {nEventType, tArgs})
    end
  elseif nEventType == NETEVENT_PDAOPEN then
    Event.Post("PDA Open", {
      uPlayer = Player.GetLocalPlayer()
    })
  elseif nEventType == NETEVENT_PDACLOSE then
    Event.Post("PDA Close", {})
  end
end

_knBlipLimit = 5000

function _PlayDelayedOpenSound(oPda)
  if oPda.CustomData.bActive then
    Sound.CueSound(0, "ui_PDA_Open_01_st")
  end
end

function _SetupDelayedOpenSound(fDelay, oPda)
  Event.Create(Event.TimerRelative, {fDelay, true}, _PlayDelayedOpenSound, {oPda})
end

function Open(oPda)
  if oPda.CustomData.bActive then
    return
  end
  if not oPda.CustomData.bHaveFlash then
    return
  end
  if Net.IsClient() then
    SetMissionChangeAllowed(oPda, false)
  end
  if MrxGuiDialogBox.oSystemDialogBoxFlash then
    return
  end
  if MrxState.IsLocked() then
    return
  end
  local oSupportMenu = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", oPda:GetOwner())
  if oSupportMenu and oSupportMenu.CustomData.bEnabled then
    oSupportMenu:Close()
  end
  if MrxGuiBase.GetCurrentControlHolder(oPda:GetOwner()) then
    return
  end
  if oPda.CustomData.nSuppressedCount > 0.5 then
    return
  end
  if oPda.CustomData.nCooldownFrames > 0 then
    return
  end
  oPda.CustomData.oMapFlash:Restart()
  oPda.CustomData.oMapFlash:Play()
  oPda.CustomData.bActive = true
  oPda.nAnalogInputHeld = 0
  MrxGuiBase.GetControlFocus(oPda, true)
  if oPda.CustomData.oMapFlash.BasicData.uId then
    _GuiInternal.RegisterForPdaUpdate(oPda.CustomData.oMapFlash.BasicData.uId, true)
  end
  _PopulateMapDisplay(oPda, nil, nil, _knBlipLimit, false)
  _PopulateSupportDisplay(oPda)
  MrxStatsManager.BuildStats(oPda)
  MrxStatsManager.PdaStatistics(oPda)
  _PopulateDatabaseDisplay(oPda)
  oPda:SetVisible(true)
  local tChildren = oPda:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:SetOwner(oPda:GetOwner())
    MrxGuiBase.AddWidgetWithChildren(oChild)
  end
  local bEnabled = MrxGuiManager.GetHudState(oPda:GetOwner())
  oPda.CustomData.bHudState = bEnabled
  if bEnabled then
    MrxGuiManager.ToggleHud(oPda:GetOwner(), false)
  end
  if not oPda.CustomData.nFakePlayerX then
    _GuiInternal.SetPlayerPDAWidget(oPda:GetOwner(), oPda.CustomData.oMapFlash.BasicData.uId)
  else
    _GuiInternal.SetPlayerPDAWidget(oPda:GetOwner(), 0)
    Sys.RequestGameState("PDA")
  end
  oPda:SetEventHandler("GuiUpdate", _HandlePDAUpdateEvent)
  _SetupDelayedOpenSound(0.5, oPda)
  MrxSound.EnterPDAState()
  if Sys.GetPlatform then
    local nPlatform = Sys.GetPlatform()
    if 1 == nPlatform then
      oPda.CustomData.oMapFlash:CallActionScriptCallback("setPlatform", {"PS3"})
    elseif 2 == nPlatform then
      oPda.CustomData.oMapFlash:CallActionScriptCallback("setPlatform", {"360"})
    end
  end
  Event.Post("PDA Open", {
    uPlayer = oPda:GetOwner()
  })
  if Net.IsClient() then
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_PDAOPEN, {})
  end
  Debug.Printf("OPEN!!!!!!!!!!!!!!!!!!!!!!!!")
  Debug.Printf("-- sSelectedMission = " .. tostring(oPda:GetSelectedMission()))
  Debug.Printf("-- bAllowTrackingChange = " .. tostring(oPda.CustomData.bAllowTrackingChange))
end

function Close(oPda)
  if oPda.CustomData.oMapFlash.BasicData.uId then
    _GuiInternal.RegisterForPdaUpdate(oPda.CustomData.oMapFlash.BasicData.uId, false)
  end
  oPda.CustomData.bActive = false
  MrxGuiBase.ReleaseControlFocus(oPda)
  if oPda.CustomData.bHaveFlash then
    oPda.CustomData.oMapFlash:CallActionScriptCallback("requestClose", {true})
  end
  oPda:SetVisible(false)
  local tChildren = oPda:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.RemoveWidgetWithChildren(oChild)
  end
  if _GuiInternal.SetPlayerPDAWidget then
    _GuiInternal.SetPlayerPDAWidget(oPda:GetOwner(), 0)
  end
  if oPda.CustomData.bHudState then
    MrxGuiManager.ToggleHud(oPda:GetOwner(), true)
  end
  oPda.CustomData.oSubtitle:ClearMessages()
  if oPda.CustomData.bMapMode then
    oPda.CustomData.nFramesWithoutInput = -1
    local oMap = oPda.CustomData.oMapFlash
    oMap:HandleLeftAnalogInput(0, 0)
    oMap:HandleRightAnalogInput(0, 0)
  end
  oPda.CustomData.nCooldownFrames = 20
  oPda:SetEventHandler("GuiUpdate", _PdaCooldown)
  if oPda.CustomData.oTransit then
    _RemoveTransitInterface(oPda)
  end
  Sound.CueSound(0, "ui_PDA_Close_01_st")
  MrxSound.ExitPDAState()
  Event.Post("PDA Close", {
    uPlayer = oPda:GetOwner()
  })
  if Net.IsClient() then
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_PDACLOSE, {})
  end
  local oMapFlash = oPda.CustomData.oMapFlash
  oMapFlash:SetSwfFile(nil)
  oPda.CustomData.bHaveFlash = false
  MrxGuiBase.AddWidget(oMapFlash)
  oMapFlash:SetSwfFile(oMapFlash.CustomData.sFile, _FinishPdaReload, {oPda})
  oPda.nAnalogInputHeld = 0
  MrxPmc.SetAllSupportViewed()
end

function _FinishPdaReload(oPda)
  _FinishLoad(oPda)
  MrxGuiBase.RemoveWidget(oPda.CustomData.oMapFlash)
end

function _PdaCooldown(oPda)
  oPda.CustomData.nCooldownFrames = oPda.CustomData.nCooldownFrames - 1
  if oPda.CustomData.nCooldownFrames <= 0 then
    oPda.CustomData.nCooldownFrames = 0
    oPda:SetEventHandler("GuiUpdate", nil)
  end
end

function SetSuppressed(oPda, bSuppress)
  if bSuppress then
    oPda.CustomData.nSuppressedCount = oPda.CustomData.nSuppressedCount + 1
  else
    oPda.CustomData.nSuppressedCount = oPda.CustomData.nSuppressedCount - 1
  end
  if oPda.CustomData.bActive and bSuppress then
    oPda:Close()
  end
end

function AddMapBlip(oPda, sName, nX, nY, sLabel, sDesc, uGuid, sTexture, sMission, nMeter, bSticky, bTodoList, sFaction, nSortOrder)
  local tNewBlipInfo = {
    sName = sName,
    nX = nX or 0,
    nY = nY or 0,
    sLabel = sLabel,
    sDesc = sDesc,
    uGuid = uGuid,
    sTexture = sTexture,
    sMission = sMission,
    nMeter = nMeter,
    bSticky = bSticky,
    bTodoList = bTodoList,
    sFaction = sFaction,
    nSortOrder = nSortOrder
  }
  oPda.CustomData.tMapBlips[sName] = tNewBlipInfo
end

function RemoveMapBlip(oPda, sName)
  oPda.CustomData.tMapBlips[sName] = nil
  if oPda.CustomData.bActive and oPda.CustomData.oMapFlash then
    _GuiInternal.RemovePdaBlip(oPda.CustomData.oMapFlash.BasicData.uId, sName)
  end
end

_nMissionCount = 1

function AddMapMission(oPda, sName, sLabel, sDesc, sFaction, sDefaultBlipTexture, sDefaultBlipLabel, bSuppress, bTrackable, nSortOrder)
  local tMissions = oPda.CustomData.tMissions
  if "string" ~= type(sName) then
    return false
  end
  local tData
  if tMissions[sName] then
    tData = tMissions[sName]
    tData.sLabel = sLabel or tData.sLabel
    tData.sDesc = sDesc or tData.sDesc
    tData.sFaction = sFaction or tData.sFaction
    tData.sDefaultBlipTexture = sDefaultBlipTexture or tData.sDefaultBlipTexture
    tData.sDefaultBlipLabel = sDefaultBlipLabel or tData.sDefaultBlipLabel
    tData.bSuppress = bSuppress or tData.bSuppress
    tData.nSortOrder = nSortOrder or tData.nSortOrder
    if bSuppress then
      bTrackable = false
    elseif nil == bTrackable then
      bTrackable = tData.bTrackable
    end
    tData.bTrackable = bTrackable or false
    if bSuppress then
      tData.sFaction = " "
    end
  else
    if not bSuppress and ("string" ~= type(sLabel) or "string" ~= type(sDesc) or "string" ~= type(sFaction)) then
      return false
    end
    if bSuppress then
      bTrackable = false
    elseif nil == bTrackable then
      bTrackable = true
    end
    local sId = tostring(_nMissionCount)
    _nMissionCount = _nMissionCount + 1
    tData = {
      sLabel = sLabel or " ",
      sDesc = sDesc or " ",
      sFaction = sFaction,
      sDefaultBlipTexture = sDefaultBlipTexture or "icon_yellow_mc",
      sDefaultBlipLabel = sDefaultBlipLabel or "DESIGNER ERROR",
      bSuppress = bSuppress or false,
      bTrackable = bTrackable or false,
      sId = sId,
      nSortOrder = nSortOrder
    }
    if bSuppress then
      tData.sFaction = " "
    end
    tMissions[sName] = tData
    oPda.CustomData.tMissionIds[sId] = sName
  end
  return true
end

function RemoveMapMission(oPda, sName)
  local tMissions = oPda.CustomData.tMissions
  if "string" ~= type(sName) then
    return
  end
  local tData = tMissions[sName]
  if tData then
    local sId = tData.sId
    oPda.CustomData.tMissionIds[sId] = nil
    tData = nil
  end
  tMissions[sName] = nil
  local tDeadBlips = {}
  for sBlipName, tBlip in pairs(oPda.CustomData.tMapBlips) do
    if sName == tBlip.sMission then
      table.insert(tDeadBlips, sBlipName)
    end
  end
  for nIndex, sBlipName in pairs(tDeadBlips) do
    oPda:RemoveMapBlip(sBlipName)
  end
  if oPda.CustomData.sSelectedMission == sName then
    SetSelectedMission(oPda, nil, true)
  end
end

function UpdateMapMission(oPda, sName, sLabel, sDesc, sFaction, sDefaultBlipTexture, sDefaultBlipLabel, bSuppress, nSortOrder)
  AddMapMission(oPda, sName, sLabel, sDesc, sFaction, sDefaultBlipTexture, sDefaultBlipLabel, bSuppress, bTrackable, nSortOrder)
end

function SetMissionTrackable(oPda, sName, bTrackable)
  if oPda.CustomData.tMissions[sName] then
    local tData = oPda.CustomData.tMissions[sName]
    if tData.bSuppress then
      bTrackable = false
    end
    tData.bTrackable = bTrackable or false
  end
end

function AddLineRegion(oPda, uRegionGuid, nRed, nGreen, nBlue, nAlpha, bInvert)
  if uRegionGuid then
    local tData = oPda.CustomData.tRegions[uRegionGuid] or {}
    local nR = nRed
    local nG = nGreen
    local nB = nBlue
    local nA = nAlpha
    nR = _Clamp(nR, 0, 255)
    nG = _Clamp(nG, 0, 255)
    nB = _Clamp(nB, 0, 255)
    nA = _Clamp(nA, 0, 255)
    nR = nR or 64
    nG = nG or 64
    nB = nB or 160
    nA = nA or 128
    nA = nA / 255 * 100
    tData.sColor = "0x" .. string.format("%02X", nR) .. string.format("%02X", nG) .. string.format("%02X", nB)
    tData.nAlpha = nA
    tData.bInvert = bInvert
    oPda.CustomData.tRegions[uRegionGuid] = tData
  end
end

function _Clamp(n, nMin, nMax)
  if not n or "number" ~= type(n) then
    return
  end
  if nMax < n then
    return nMax
  end
  if n < nMin then
    return nMin
  end
  return n
end

function RemoveLineRegion(oPda, uRegionGuid)
  if uRegionGuid then
    oPda.CustomData.tRegions[uRegionGuid] = nil
  end
end

function SetSelectedMission(oPda, sName, bForceOnClient)
  Debug.Printf("SETSELECTEDMISSION!!!!!!!!!!!!!!!!!!!!!!!!")
  if Net.IsClient() and bForceOnClient ~= true then
    return
  end
  if sName and oPda.CustomData.tMissions[sName] then
    oPda.CustomData.sSelectedMission = sName
  else
    oPda.CustomData.sSelectedMission = nil
  end
  Debug.Printf("-- sSelectedMission = " .. tostring(oPda.CustomData.sSelectedMission))
  if Net.IsServer() then
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_SETSELECTEDMISSION, {
      WifMissionData.GetMissionIndexFromId(oPda.CustomData.sSelectedMission)
    })
  end
end

function GetSelectedMission(oPda)
  return oPda.CustomData.sSelectedMission
end

function SetMissionTrackCallback(oPda, fCallback, tCallbackData)
  if "function" == type(fCallback) or nil == fCallback then
    oPda.CustomData.fMissionChangeCallback = fCallback
    oPda.CustomData.tMissionChangeData = tCallbackData
  end
end

function SetMissionChangeAllowed(oPda, bAllow)
  if Net.IsClient() then
    oPda.CustomData.bAllowTrackingChange = false
  else
    oPda.CustomData.bAllowTrackingChange = bAllow
  end
end

function SetFakePlayerLocation(oPda, nX, nY, nZ)
  if nil == nX then
    oPda.CustomData.nFakePlayerX = nil
    oPda.CustomData.nFakePlayerY = nil
    oPda.CustomData.nFakePlayerZ = nil
    return
  end
  if "number" == type(nX) and "number" == type(nY) and "number" == type(nZ) then
    oPda.CustomData.nFakePlayerX = nX
    oPda.CustomData.nFakePlayerY = nY
    oPda.CustomData.nFakePlayerZ = nZ
  end
end

function SetBeaconTutorialMode(oPda, bBeaconTutorialMode)
  oPda.CustomData.bBeaconTutorialMode = bBeaconTutorialMode
end

function _PopulateMapDisplay(oPda, oFlash, bCleanup, nBlipLimit, bHideTertiary)
  if nil == bCleanup then
    bCleanup = true
  end
  local nRotation = 0
  local uCamera = Player.GetCamera(oPda:GetOwner())
  if uCamera then
    nRotation = Camera.GetYaw(uCamera)
  end
  nBlipLimit = nBlipLimit or _knBlipLimit * 0.5
  if nBlipLimit < 0 then
    nBlipLimit = _knBlipLimit * 0.5
  end
  local nXOffset = 35
  local nZOffset = 40
  local nTrans = 1
  local tCurMission, sCurMissionDesc
  local sMissionDisplay = " "
  if oPda.CustomData.sSelectedMission then
    tCurMission = oPda.CustomData.tMissions[oPda.CustomData.sSelectedMission]
    if tCurMission and not tCurMission.bSuppress then
      sCurMissionDesc = tCurMission.sDesc
      sMissionDisplay = "[PDA.Map.CurrentMission]"
    end
  end
  local uChar = oPda:GetOwner()
  local uId = oPda.CustomData.oMapFlash.BasicData.uId
  uChar = Player.GetCharacter(uChar)
  local nX, nY, nZ
  if oPda.CustomData.nFakePlayerX then
    nX = oPda.CustomData.nFakePlayerX
    nY = oPda.CustomData.nFakePlayerY
    nZ = oPda.CustomData.nFakePlayerZ
  else
    nX, nY, nZ = Object.GetPosition(uChar)
  end
  oFlash = oFlash or oPda.CustomData.oMapFlash
  local tBlipTable = {}
  local tData
  local sName = string.format("[PDA.Map.Player:%d]", 1)
  tData = {
    "player1_mc",
    "player1_mc",
    nX + nXOffset,
    nZ + nZOffset,
    math.rad(nRotation),
    sName,
    sCurMissionDesc or sName,
    " ",
    " ",
    false,
    "  ",
    sMissionDisplay,
    true,
    false,
    false
  }
  table.insert(tBlipTable, tData)
  local tEmptyMission = {
    bNoMission = true,
    sId = "",
    sLabel = ""
  }
  local tCustomMission = {bNoMission = true, sId = "m"}
  local bSticky = false
  local bTrackable = false
  local sFactionName
  local tMissionsOrdered = {}
  for sName, tMission in pairs(oPda.CustomData.tMissions) do
    if not tMission.bSuppress then
      table.insert(tMissionsOrdered, tMission)
    end
  end
  table.sort(tMissionsOrdered, _MissionSortLessThan)
  for n, tMission in pairs(tMissionsOrdered) do
    sFactionName = _tFactionNameLookup[tMission.sFaction]
    tData = {
      tMission.sLabel,
      tMission.sDefaultBlipTexture,
      10000,
      10000,
      0,
      tMission.sLabel,
      tMission.sDesc,
      tMission.sFaction,
      sFactionName,
      true,
      tMission.sId,
      tMission.sLabel,
      false,
      false
    }
    if not bHideTertiary then
      table.insert(tBlipTable, tData)
    end
  end
  local bMissionBlip = false
  local nMaxLevel = 5
  local nMinLevel = 1
  local tLevels = {}
  for n = nMaxLevel, nMinLevel, -1 do
    tLevels[n] = {}
  end
  for sName, tBlip in pairs(oPda.CustomData.tMapBlips) do
    if tBlip.nSortOrder and tLevels[tBlip.nSortOrder] then
      table.insert(tLevels[tBlip.nSortOrder], tBlip)
    else
      table.insert(tLevels[nMaxLevel], tBlip)
    end
  end
  local tOrderedList = {}
  for n = nMaxLevel, nMinLevel, -1 do
    for nIndex, tBlip in pairs(tLevels[n]) do
      table.insert(tOrderedList, tBlip)
    end
  end
  local nBlipsAdded = 0
  local nIndex = 1
  if nBlipLimit < #tOrderedList then
    nIndex = #tOrderedList - (nBlipLimit + 1)
  end
  while tOrderedList[nIndex] do
    local tBlip = tOrderedList[nIndex]
    tEmptyMission.sFaction = nil
    sFactionName = nil
    if tBlip.uGuid then
      local nX, nY, nZ = Object.GetPosition(tBlip.uGuid)
      if nX and nZ then
        tBlip.nX = nX
        tBlip.nY = nZ
      end
    end
    local tMissionData
    bSticky = tBlip.bSticky or false
    bTrackable = false
    if tBlip.sMission then
      tMissionData = oPda.CustomData.tMissions[tBlip.sMission] or tEmptyMission
      if nil ~= tBlip.bSticky then
        bSticky = tBlip.bSticky
      elseif oPda.CustomData.sSelectedMission and oPda.CustomData.sSelectedMission == tBlip.sMission then
        bSticky = true
      end
      if oPda.CustomData.bAllowTrackingChange then
        bTrackable = tMissionData.bTrackable
      end
      if tMissionData.bSuppress then
        tMissionData = tEmptyMission
      end
    elseif tBlip.bTodoList then
      tMissionData = tCustomMission
      tCustomMission.sFaction = tBlip.sFaction
      tCustomMission.sId = tBlip.sName
      tCustomMission.sLabel = tBlip.sLabel
    else
      tMissionData = tEmptyMission
    end
    if tMissionData.sFaction then
      sFactionName = _tFactionNameLookup[tMissionData.sFaction]
    elseif tBlip.sFaction then
      sFactionName = _tFactionNameLookup[tBlip.sFaction]
    end
    bMissionBlip = false
    if not tMissionData.bNoMission and tMissionData.bTrackable then
      bMissionBlip = true
    end
    tData = {
      tBlip.sName,
      tBlip.sTexture or tMissionData.sDefaultBlipTexture or "icon_yellow_mc",
      tBlip.nX + nXOffset,
      tBlip.nY + nZOffset,
      0,
      tBlip.sLabel or tMissionData.sDefaultBlipLabel,
      tBlip.sDesc or tMissionData.sDesc,
      tMissionData.sFaction or tBlip.sFaction,
      sFactionName,
      bMissionBlip,
      tMissionData.sId,
      tMissionData.sLabel,
      bSticky,
      bTrackable,
      bSticky
    }
    local sTexture = tData[2]
    if bHideTertiary then
      if "icon_action_3_mc" ~= sTexture and "icon_outpost_3_mc" ~= sTexture and "icon_defend_3_mc" ~= sTexture and "icon_destroy_3_mc" ~= sTexture and "icon_verify_3_mc" ~= sTexture and "icon_deliverable_3_mc" ~= sTexture then
        table.insert(tBlipTable, tData)
      end
    else
      table.insert(tBlipTable, tData)
    end
    nIndex = nIndex + 1
  end
  tData = {
    "marker_mc",
    "marker_mc",
    oPda.CustomData.nMarkerX or nX,
    oPda.CustomData.nMarkerZ or nZ,
    0,
    "MARKER",
    "Destination Marker",
    "  ",
    "  ",
    false,
    "  ",
    "  ",
    true,
    false,
    false
  }
  table.insert(tBlipTable, tData)
  _GuiInternal.AddPdaMapBlips(oFlash.BasicData.uId, tBlipTable)
  AddPDATargetMarkers(oPda)
  if oPda.CustomData.nMarkerX and oPda.CustomData.nMarkerZ then
    oFlash:CallActionScriptCallback("SetMarker", {true})
  else
    oFlash:CallActionScriptCallback("SetMarker", {false})
  end
  UpdateAllPlayerMarkers(oPda)
  _DisplayRegions(oPda, nXOffset, nZOffset)
  local sSelectedMission
  if oPda.CustomData.sSelectedMission and not oPda.CustomData.bAllowTrackingChange then
    local tSelectedMissionData = oPda.CustomData.tMissions[oPda.CustomData.sSelectedMission]
    if tSelectedMissionData then
      sSelectedMission = tSelectedMissionData.sId
    end
  end
  oFlash:SetFlashEventHandler("beaconCheck", HandleBeaconCheck, {oFlash})
  if oPda.CustomData.bBeaconTutorialMode then
    oFlash:CallActionScriptCallback("beaconTutorial", {})
  end
  if bCleanup then
    oFlash:CallActionScriptCallback("LandingZone", {false})
    if sSelectedMission then
      oFlash:CallActionScriptCallback("activeContract", {sSelectedMission})
    end
    oFlash:CallActionScriptCallback("AddBlipFinish", {})
  end
end

function _MissionSortLessThan(tMission1, tMission2)
  if not tMission1.nSortOrder then
    return false
  elseif not tMission2.nSortOrder then
    return true
  end
  return tMission1.nSortOrder < tMission2.nSortOrder
end

function HandleBeaconCheck(oFlash, sArgs)
  local nX, nY
  local t = {}
  local nIndex = 1
  for nNumber in string.gmatch(sArgs, "-*%d+") do
    t[nIndex] = tonumber(nNumber)
    nIndex = nIndex + 1
  end
  if t[1] and t[2] then
    nX = t[1] * 2
    nY = t[2] * 2
  end
  if "number" == type(nX) and "number" == type(nY) and Player.IsPositionOutBoundary then
    local nXOffset = 35
    local nZOffset = 40
    if not Player.IsPositionOutBoundary(Player.GetLocalPlayer(), nX * 0.5 - nXOffset, 0, nY * 0.5 - nZOffset) then
      oFlash:CallActionScriptCallback("beaconCheckReturn", {true})
      return
    end
  end
  oFlash:CallActionScriptCallback("beaconCheckReturn", {false})
end

function _DisplayRegions(oPda, nXOffset, nYOffset)
  nXOffset = nXOffset or 0
  nYOffset = nYOffset or 0
  local oFlash = oPda.CustomData.oMapFlash
  local nId = 1
  for uRegionGuid, tData in pairs(oPda.CustomData.tRegions) do
    _DisplayRegion(oFlash, uRegionGuid, nId, tData.sColor, tData.nAlpha, nXOffset, nYOffset, tData.bInvert)
    nId = nId + 1
  end
end

function _DisplayRegion(oFlash, uRegion, nId, sColor, nAlpha, nXOffset, nYOffset, bInvert)
  local tX, tY = Pg.GetLineRegionPoints(uRegion, bInvert)
  local nY
  local bFirst = true
  for nIndex, nX in ipairs(tX) do
    nY = tY[nIndex]
    if bFirst then
      oFlash:CallActionScriptCallback("AddZone", {
        nId,
        true,
        false,
        nX + nXOffset,
        nY + nYOffset,
        sColor,
        nAlpha
      })
      bFirst = false
    else
      oFlash:CallActionScriptCallback("AddZone", {
        nId,
        false,
        false,
        nX + nXOffset,
        nY + nYOffset
      })
    end
  end
  oFlash:CallActionScriptCallback("AddZone", {
    nId,
    false,
    true
  })
end

function _HandleTrackEvent(oMapFlash, sId)
  local oPda = oMapFlash.oParentWidget
  local sMission = oPda.CustomData.tMissionIds[sId]
  oPda:SetSelectedMission(sMission)
  if oPda.CustomData.fMissionChangeCallback then
    local tData = {}
    if oPda.CustomData.tMissionChangeData then
      for nIndex, vData in ipairs(oPda.CustomData.tMissionChangeData) do
        tData[nIndex] = vData
      end
    end
    table.insert(tData, sMission)
    oPda.CustomData.fMissionChangeCallback(unpack(tData))
  end
end

function _HandleUntrackEvent(oMapFlash, sMission)
  local oPda = oMapFlash.oParentWidget
  oPda:SetSelectedMission(nil)
  if oPda.CustomData.fMissionChangeCallback then
    local tData = oPda.CustomData.tMissionChangeData or {}
    oPda.CustomData.fMissionChangeCallback(unpack(tData))
  end
end

function _HandleMissionCancel(oFlash, sUnused)
  local oCurrentMission = MrxPlayState.GetCurrentMission()
  if oCurrentMission then
    oCurrentMission:Cancel()
  end
end

function HandleMarkerUpdate(oPda, tEvent)
  oPda.CustomData.nMarkerX = tEvent.PosX
  oPda.CustomData.nMarkerZ = tEvent.PosZ
  Event.Post("GPS Beacon Set", {
    nX = tEvent.PosX,
    nY = tEvent.PosZ
  })
end

function HandleMarkerClear(oPda, tEvent)
  oPda.CustomData.nMarkerX = nil
  oPda.CustomData.nMarkerZ = nil
  Event.Post("GPS Beacon Cleared", {
    nX = tEvent.PosX,
    nY = tEvent.PosZ
  })
end

function OpenTransitInterface(oPda, tZones, fCallback, tCallbackData)
  if oPda.CustomData.bActive then
    return
  end
  oPda.CustomData.bActive = true
  local oTransit = oPda.CustomData.oTransit
  if not oTransit then
    oTransit = MrxGuiBase.FlashWidget:new()
    oTransit:SetAnchoring("center", "center")
    oTransit:SetFullscreen(true)
    oTransit:SetOwner(oPda:GetOwner())
    oPda.CustomData.oTransit = oTransit
    oPda:AddChild(oTransit)
    oTransit.oParentWidget = oPda
  end
  oTransit.CustomData.fCallback = fCallback
  oTransit.CustomData.tCallbackData = tCallbackData
  oTransit:SetSwfFile("landingzones", _FinishTransitInterfaceLoad, {
    oTransit,
    oPda,
    tZones
  })
  _SetupDelayedOpenSound(0.6, oPda)
  MrxSound.EnterPDAState()
  Event.Post("Transit Interface Open", {
    uPlayer = oPda:GetOwner()
  })
  Sys.RequestGameState("PDA")
end

function _FinishTransitInterfaceLoad(oTransit, oParentPda, tZones)
  MrxGuiBase.GetControlFocus(oParentPda, true)
  oTransit:Restart()
  oTransit:Play()
  _PopulateMapDisplay(oParentPda, oTransit, false, _knBlipLimit - #tZones, true)
  local nXOffset = 35
  local nZOffset = 40
  local oFlash = oTransit
  oFlash:CallActionScriptCallback("LandingZone", {1})
  oFlash:SetFlashEventHandler("LandingZone", _InvokeCallbackSuccess, {})
  oFlash:SetFlashEventHandler("closeMap", _HandleCloseEvent, {})
  local tZonesOrdered = {}
  for nId, tData in pairs(tZones) do
    tData.nId = nId
    table.insert(tZonesOrdered, tData)
  end
  table.sort(tZonesOrdered, _LandingZoneLessThan)
  for nId, tData in pairs(tZonesOrdered) do
    oFlash:CallActionScriptCallback("AddBlip", {
      tostring(tData.nId),
      "icon_lz_mc",
      tData.nX + nXOffset,
      tData.nY + nZOffset,
      0,
      tData.sName or "Needs localized name",
      "Landing Zone",
      " ",
      " ",
      false,
      " ",
      " "
    })
  end
  oParentPda:SetVisible(true)
  local tChildren = oParentPda:GetChildren()
  for nIndex, oChild in ipairs(tChildren) do
    if oChild ~= oParentPda.CustomData.oMapFlash then
      oChild:SetOwner(oParentPda:GetOwner())
      MrxGuiBase.AddWidgetWithChildren(oChild)
    else
      MrxGuiBase.RemoveWidgetWithChildren(oChild)
    end
  end
  local bEnabled = MrxGuiManager.GetHudState(oParentPda:GetOwner())
  oParentPda.CustomData.bHudState = bEnabled
  if bEnabled then
    MrxGuiManager.ToggleHud(oParentPda:GetOwner(), false)
  end
  oFlash:CallActionScriptCallback("AddBlipFinish", {})
end

function _LandingZoneLessThan(tData1, tData2)
  if not tData1.nSortOrder then
    return false
  elseif not tData2.nSortOrder then
    return true
  end
  return tData1.nSortOrder < tData2.nSortOrder
end

function _RemoveTransitInterface(oPda)
  local oTransit = oPda.CustomData.oTransit
  Sys.RequestGameState("ingame")
  if oTransit then
    oTransit:CallActionScriptCallback("requestClose", {true})
    oPda:RemoveChild(oTransit)
    oPda.CustomData.oTransit = nil
    Event.Create(Event.TimerRelative, {0.1, true}, _RemoveTransitInterfaceDelayed, {oTransit})
    if oTransit.CustomData.fCallback then
      _InvokeCallback(oTransit, "0", false)
    end
  end
end

function _RemoveTransitInterfaceDelayed(oTransit)
  oTransit:SetSwfFile(nil)
  oTransit:delete()
end

function _InvokeCallbackSuccess(oTransit, sNumber)
  Event.Post("Transit Interface Success", {
    uPlayer = oTransit:GetOwner()
  })
  _InvokeCallback(oTransit, sNumber, true)
end

function _InvokeCallback(oSelf, sNumber, bSuccess)
  local fCallback = oSelf.CustomData.fCallback
  local tData = oSelf.CustomData.tCallbackData
  oSelf.CustomData.fCallback = nil
  oSelf.CustomData.tCallbackData = nil
  if fCallback then
    tData = tData or {}
    table.insert(tData, 1, bSuccess)
    table.insert(tData, 1, tonumber(sNumber))
    fCallback(unpack(tData))
  end
end

nSupportId = 1
local _ksTypeAddFunc = {
  Airstrike = "AddSupportAirstrike",
  Civilian = "AddSupportCivilian",
  Light = "AddSupportLight",
  Heavy = "AddSupportHeavy",
  Heli = "AddSupportHelicopters",
  Boat = "AddSupportBoats",
  Supply = "AddSupportSupplies"
}

function AddSupport(oPda, tData, sKey)
  if oPda.CustomData.tSupport[tData.sName] then
    return UpdateSupport(oPda, tData)
  end
  local tSupportData = {}
  for key, value in pairs(tData) do
    tSupportData[key] = value
  end
  tSupportData.sId = "s" .. nSupportId
  tSupportData.sKey = sKey
  nSupportId = nSupportId + 1
  tSupportData.sAddFunc = _ksTypeAddFunc[tSupportData.sType]
  oPda.CustomData.tSupport[tData.sName] = tSupportData
  table.insert(oPda.CustomData.tSupportOrdered, tData.sName)
  oPda.CustomData.tSupportIdIndex[tSupportData.sId] = tData.sName
end

function RemoveSupport(oPda, sName)
  if oPda.CustomData.tSupport[sName] then
    local sId = oPda.CustomData.tSupport[sName].sId
    oPda.CustomData.tSupportIdIndex[sId] = nil
  end
  oPda.CustomData.tSupport[sName] = nil
  local nIndex = 1
  while oPda.CustomData.tSupportOrdered[nIndex] and oPda.CustomData.tSupportOrdered[nIndex] ~= sName do
    nIndex = nIndex + 1
  end
  oPda.CustomData.tSupportOrdered[nIndex] = nil
  for nSlot, sItem in pairs(oPda.CustomData.tEquippedSupport) do
    if sName == sItem then
      oPda.CustomData.tEquippedSupport[nSlot] = nil
      oPda.CustomData.tEquippedSupportIcons[nSlot] = nil
      local oSupportMenu = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", oPda:GetOwner())
      if oSupportMenu then
        oSupportMenu:RemoveItem(sName)
      end
    end
  end
end

function UpdateSupport(oPda, tData)
  local tSupportData = oPda.CustomData.tSupport[tData.sName]
  if not tSupportData then
    return false
  end
  for key, value in pairs(tData) do
    tSupportData[key] = value or tSupportData[key]
  end
  tSupportData.sAddFunc = _ksTypeAddFunc[tSupportData.sType]
  return true
end

function GetStockpile(oPda, sName)
  return MrxPmc.GetSupportQty(sName)
end

function _PopulateSupportDisplay(oPda)
  local oFlash = oPda.CustomData.oMapFlash
  local _tPrefix = {
    Airstrike = "[airstrike] ",
    Civilian = "[vehcivilian] ",
    Light = "[vehmlight] ",
    Heavy = "[vehmheavy] ",
    Heli = "[vehheli] ",
    Boat = "[vehboat] ",
    Supply = "[supply] "
  }
  oFlash:CallActionScriptCallback("AddStockpile", {
    MrxPmc.GetCashQty(),
    MrxPmc.GetFuelQty(),
    MrxPmc.GetFuelCapacity()
  })
  _UpdateSupportData(oPda)
  local tData, nQty, bEquippable, sDesignatorDisplay
  for nIndex, sName in pairs(oPda.CustomData.tSupportOrdered) do
    tData = oPda.CustomData.tSupport[sName]
    nQty = MrxPmc.GetSupportQty(tData.oSupport:GetSupportName())
    sDesignatorDisplay = " "
    if tData.oSupport:GetDesignator() then
      local sDesignator = tData.oSupport:GetDesignator():GetType()
      if "smoke" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Smoke]"
      elseif "satellite" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Satellite]"
      elseif "advanced satellite" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.AdvSatellite]"
      elseif "beacon" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Beacon]"
      elseif "laser" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Laser]"
      elseif "flare" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Flare]"
      end
    end
    bEquippable = MrxSupportData.IsSupportEquippable(tData.sKey)
    if tData.sAddFunc and nQty and 0 < nQty then
      local bNew = MrxPmc.IsSupportNew(tData.sKey)
      oFlash:CallActionScriptCallback(tData.sAddFunc, {
        tData.sId,
        (_tPrefix[tData.sType] or "") .. tData.sName,
        tData.sDescription,
        tData.sIcon,
        nQty,
        tData.nMaxStock,
        tData.nFuelCost,
        bNew,
        bEquippable,
        sDesignatorDisplay
      })
    end
  end
  for nSlot, sName in pairs(oPda.CustomData.tEquippedSupport) do
    local tData = oPda.CustomData.tSupport[sName]
    if tData and tData.sId then
      oFlash:CallActionScriptCallback("AddSupportEquipped", {
        nSlot,
        tData.sId,
        tData.sName,
        tData.sIcon
      })
    end
  end
  oFlash:SetFlashEventHandler("equipFailed", _ShowUnusableSupportMessage, {})
end

function _UpdateSupportData(oPda)
  for sName, tData in pairs(MrxSupportData.tSupportData) do
    local nQty = MrxPmc.GetSupportQty(sName)
    if nQty and not oPda:UpdateSupport(tData) then
      oPda:AddSupport(tData)
    end
  end
end

function _ShowUnusableSupportMessage(oFlash, sArg)
  local oPda = oFlash.oParentWidget
  local sMessage = "ERROR: No support denial condition specified."
  local sName = oPda.CustomData.tSupportIdIndex[sArg]
  local tData
  if sName then
    tData = oPda.CustomData.tSupport[sName]
  end
  if tData and tData.sKey then
    local bCanUse, sReason = MrxSupportData.IsSupportEquippable(tData.sKey)
    sMessage = sReason or sMessage
  end
  oFlash:CallActionScriptCallback("onlineMessage", {
    "[PDA.Support.EquipFail.Unavailable]",
    sMessage,
    0,
    "[Generic.Ok]",
    "[Generic.Ok]",
    nil
  })
end

function _HandleEquipEvent(oFlash, sData)
  local oPda = oFlash.oParentWidget
  local nSlot, sId = _ParseString(sData)
  local oSupportMenu = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", oPda:GetOwner())
  if oSupportMenu and "number" == type(nSlot) and "string" == type(sId) then
    local sName = oPda.CustomData.tSupportIdIndex[sId]
    local tData
    if sName then
      tData = oPda.CustomData.tSupport[sName]
    end
    if tData and oPda.CustomData.tEquippedSupport[nSlot] and tData.sName == oPda.CustomData.tEquippedSupport[nSlot] then
      return
    end
    if oPda.CustomData.tEquippedSupport[nSlot] then
      oSupportMenu:RemoveItem(oPda.CustomData.tEquippedSupport[nSlot])
    end
    oPda.CustomData.tEquippedSupport[nSlot] = nil
    oPda.CustomData.tEquippedSupportIcons[nSlot] = nil
    if sName and tData then
      local tDataCopy = {}
      for k, v in pairs(tData) do
        tDataCopy[k] = v
      end
      tDataCopy.bAnimate = true
      tDataCopy.bDontNetSync = true
      oPda.CustomData.tEquippedSupport[nSlot] = tData.sName
      oPda.CustomData.tEquippedSupportIcons[nSlot] = tData.sIcon
      oSupportMenu:AddItem(tDataCopy)
      oPda.CustomData.bOpenSupportMenuOnExit = true
    end
  end
end

function _ParseString(sData)
  local nSlot, sId
  for nNumber, sSeperator, sString in string.gmatch(sData, "(%d+)([, ]*)(%w+)") do
    nSlot = tonumber(nNumber)
    sId = sString
  end
  if "number" == type(nSlot) and "string" == type(sId) then
    return nSlot, sId
  end
  return nil
end

function _HandleUnequipEvent(oFlash, sUnused)
end

function _GetEquippedSupport(oPda, nSlot)
  if not nSlot then
    return nil
  end
  return oPda.CustomData.tEquippedSupport[nSlot], oPda.CustomData.tEquippedSupportIcons[nSlot]
end

function _SetEquippedSupport(oPda, sName, nSlot)
  if "number" ~= type(nSlot) then
    return
  end
  if sName and oPda:GetEquippedSupport(nSlot) == sName then
    return
  end
  local oSupportMenu = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", oPda:GetOwner())
  if oSupportMenu then
    if oPda.CustomData.tEquippedSupport[nSlot] then
      oSupportMenu:RemoveItem(oPda.CustomData.tEquippedSupport[nSlot])
    end
    oPda.CustomData.tEquippedSupport[nSlot] = nil
    oPda.CustomData.tEquippedSupportIcons[nSlot] = nil
    if sName then
      local tData = oPda.CustomData.tSupport[sName]
      if tData then
        local tDataCopy = {}
        for k, v in pairs(tData) do
          tDataCopy[k] = v
        end
        tDataCopy.bDontNetSync = true
        Debug.Printf("Adding item : " .. sName)
        oPda.CustomData.tEquippedSupport[nSlot] = tData.sName
        oPda.CustomData.tEquippedSupportIcons[nSlot] = tData.sIcon
        oPda.CustomData.bSupportNeedsEquipping = false
        oSupportMenu:AddItem(tDataCopy)
      end
    end
  end
end

function ReadEquippedSupport(oPda)
  local tEquipped = {
    oPda.CustomData.tEquippedSupport[1],
    oPda.CustomData.tEquippedSupport[2],
    oPda.CustomData.tEquippedSupport[3]
  }
  return tEquipped
end

function RestoreEquippedSupport(oPda, tSupportData)
  _EquipItemSilent(oPda, 1, tSupportData[1])
  _EquipItemSilent(oPda, 2, tSupportData[2])
  _EquipItemSilent(oPda, 3, tSupportData[3])
end

function _EquipItemSilent(oPda, nSlot, sName)
  if not sName then
    return
  end
  local oSupportMenu = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", oPda:GetOwner())
  if oSupportMenu and "number" == type(nSlot) and "string" == type(sName) then
    local tData = oPda.CustomData.tSupport[sName]
    if tData and oPda.CustomData.tEquippedSupport[nSlot] and tData.sName == oPda.CustomData.tEquippedSupport[nSlot] then
      return
    end
    if oPda.CustomData.tEquippedSupport[nSlot] then
      oSupportMenu:RemoveItem(oPda.CustomData.tEquippedSupport[nSlot])
    end
    oPda.CustomData.tEquippedSupport[nSlot] = nil
    oPda.CustomData.tEquippedSupportIcons[nSlot] = nil
    if sName and tData then
      local tDataCopy = {}
      for k, v in pairs(tData) do
        tDataCopy[k] = v
      end
      tDataCopy.bDontNetSync = true
      oPda.CustomData.tEquippedSupport[nSlot] = tData.sName
      oPda.CustomData.tEquippedSupportIcons[nSlot] = tData.sIcon
      oSupportMenu:AddItem(tDataCopy)
    end
  end
end

function SetFactionAttitude(oPda, sFaction, sIcon, nAttitude)
  if "string" == type(sFaction) and "string" == type(sIcon) and "number" == type(nAttitude) then
    if nAttitude < 0 then
      oPda.CustomData.tFactionAttitudes[sFaction] = nil
      return true
    end
    nAttitude = Math.max(Math.min(nAttitude, 100), 0)
    if not oPda.CustomData.tFactionAttitudes[sFaction] then
      oPda.CustomData.tFactionAttitudes[sFaction] = {sIcon, nAttitude}
    else
      oPda.CustomData.tFactionAttitudes[sFaction][1] = sIcon
      oPda.CustomData.tFactionAttitudes[sFaction][2] = nAttitude
    end
    return true
  end
  return false
end

nLogSize = 100

function AddLogEntry(oPda, sType, sName, sMessage, sColor)
  if "string" ~= type(sType) and "string" ~= type(sName) and "string" ~= type(sMessage) then
    return
  end
  if "dialog" ~= sType and "objective" ~= sType and "event" ~= sType then
    return
  end
  local tEntry = {
    sType = sType,
    sName = sName,
    sMessage = sMessage,
    sColor = sColor or "FFFFFF"
  }
  table.insert(oPda.CustomData.tLogEntries, 1, tEntry)
  while #oPda.CustomData.tLogEntries > nLogSize do
    table.remove(oPda.CustomData.tLogEntries)
  end
end

function AddDossierEntry(oPda, sTitle, sText, sIcon)
  if not sTitle then
    return
  end
  local tData
  if oPda.CustomData.tDataDossiersIndex[sTitle] then
    tData = oPda.CustomData.tDataDossiersIndex[sTitle]
  else
    tData = {}
    table.insert(oPda.CustomData.tDataDossiers, tData)
  end
  tData.sTitle = sTitle
  tData.sText = sText
  tData.sIcon = sIcon
  oPda.CustomData.tDataDossiersIndex[sTitle] = tData
end

function AddHelpEntry(oPda, sTitle, sText, sIcon)
  if not sTitle then
    return
  end
  local tData
  if oPda.CustomData.tDataHelpIndex[sTitle] then
    tData = oPda.CustomData.tDataHelpIndex[sTitle]
  else
    tData = {}
    table.insert(oPda.CustomData.tDataHelp, tData)
  end
  tData.sTitle = sTitle
  tData.sText = sText
  tData.sIcon = sIcon
  oPda.CustomData.tDataHelpIndex[sTitle] = tData
end

function AddStatisticCategory(oPda, sCategoryName, sIcon)
  local tData = {sCategoryName = sCategoryName, sIcon = sIcon}
  table.insert(oPda.CustomData.tStatCategories, tData)
end

function AddStatisticEntry(oPda, sCategoryName, sText, sData)
  if oPda.CustomData.tDataStats[sText] then
    _UpdateStatisticEntry(oPda, sText, sData)
    return
  end
  local tData = {
    sCategoryName = sCategoryName,
    sText = sText,
    sData = sData
  }
  table.insert(oPda.CustomData.tDataStatsOrdered, tData)
  oPda.CustomData.tDataStats[sText] = tData
end

function _UpdateStatisticEntry(oPda, sText, sData)
  if oPda.CustomData.tDataStats[sText] then
    oPda.CustomData.tDataStats[sText].sData = sData
  end
end

function _PopulateDatabaseDisplay(oPda)
  local oFlash = oPda.CustomData.oMapFlash
  oFlash:CallActionScriptCallback("checkOnline", {
    Net.IsConnectedToInternet()
  })
  oFlash:CallActionScriptCallback("multiplayerHost", {
    Net.IsServer()
  })
  oFlash:CallActionScriptCallback("multiplayerClient", {
    Net.IsClient()
  })
  if Sys.HaveActiveProfile then
    oFlash:CallActionScriptCallback("profileActive", {
      Sys.HaveActiveProfile()
    })
  end
  for sFactionName, tData in pairs(oPda.CustomData.tFactionAttitudes) do
    local nValue, sAttitudeName = MrxGuiHudFactionGauge.GetBarValueAndName(tData[2])
    oFlash:CallActionScriptCallback("AddFactionAttitude", {
      sFactionName,
      tData[1],
      sAttitudeName,
      nValue,
      false
    })
  end
  oFlash:CallActionScriptCallback("AddDatabaseItem", {
    2,
    0,
    "[PDA.Database.Log_All]",
    "Display all Log Events",
    "icon_categories_log",
    false
  })
  oFlash:CallActionScriptCallback("AddDatabaseItem", {
    2,
    0,
    "[PDA.Database.Log_Events]",
    "Filter Message Log by Events",
    "icon_categories_events",
    false
  })
  oFlash:CallActionScriptCallback("AddDatabaseItem", {
    2,
    0,
    "[PDA.Database.Log_Objectives]",
    "Filter Message Log by Objectives",
    "icon_categories_objectives",
    false
  })
  oFlash:CallActionScriptCallback("AddDatabaseItem", {
    2,
    0,
    "[PDA.Database.Log_Dialogue]",
    "Filter Message Log by Dialogue",
    "icon_categories_dialog",
    false
  })
  local n = #oPda.CustomData.tLogEntries
  while 0 < n do
    local tEntry = oPda.CustomData.tLogEntries[n]
    oFlash:CallActionScriptCallback("addMessageLog", {
      tEntry.sType,
      tEntry.sColor,
      tEntry.sName,
      tEntry.sMessage
    })
    n = n - 1
  end
  for n, tEntry in ipairs(oPda.CustomData.tDataDossiers) do
    oFlash:CallActionScriptCallback("AddDatabaseItem", {
      3,
      0,
      tEntry.sTitle,
      tEntry.sText,
      tEntry.sIcon,
      false
    })
  end
  local tCategoryNumbers = {}
  for n, tEntry in ipairs(oPda.CustomData.tStatCategories) do
    tCategoryNumbers[tEntry.sCategoryName] = n + 2
    oFlash:CallActionScriptCallback("AddDatabaseItem", {
      4,
      0,
      tEntry.sCategoryName,
      tEntry.sCategoryName,
      tEntry.sIcon,
      false
    })
  end
  local nCategory
  for n, tEntry in ipairs(oPda.CustomData.tDataStatsOrdered) do
    nCategory = tCategoryNumbers[tEntry.sCategoryName]
    oFlash:CallActionScriptCallback("addStats", {
      nCategory,
      tEntry.sText,
      tEntry.sData
    })
  end
end

function _Initialize(oPda)
  oPda.CustomData.bActive = true
  oPda.CustomData.oSubtitle = oPda:GetChildren()[1]
  oPda.CustomData.nCooldownFrames = 0
  local oBg = MrxGuiBase.ImageWidget:new()
  oBg:SetFullscreen(true)
  oBg:SetColor(0, 0, 0, 192)
  oBg:SetOwner(oPda:GetOwner())
  oPda:AddChild(oBg)
  oBg.oParentWidget = oPda
  local oMapFlash = MrxGuiBase.FlashWidget:new()
  oMapFlash:SetAnchoring("center", "center")
  oMapFlash:SetFullscreen(true)
  oMapFlash:SetOwner(oPda:GetOwner())
  oPda:AddChild(oMapFlash)
  oPda.CustomData.oMapFlash = oMapFlash
  oMapFlash.CustomData.sFile = "topbar"
  oMapFlash.oParentWidget = oPda
  oPda.Open = Open
  oPda.Close = Close
  oPda.SetSuppressed = SetSuppressed
  oPda.CustomData.nSuppressedCount = 0
  oPda:SetEventHandler("ControllerInput", _HandleInput)
  oPda.CustomData.tMapBlips = {}
  oPda.CustomData.tMissions = {}
  oPda.CustomData.tMissionIds = {}
  oPda.CustomData.tRegions = {}
  oPda.CustomData.bMapMode = false
  oPda.CustomData.bAllowTrackingChange = true
  oPda.CustomData.nFramesWithoutInput = -1
  oPda.CustomData.bHudState = true
  oPda.AddMapBlip = AddMapBlip
  oPda.RemoveMapBlip = RemoveMapBlip
  oPda.AddMapMission = AddMapMission
  oPda.RemoveMapMission = RemoveMapMission
  oPda.UpdateMapMission = UpdateMapMission
  oPda.SetMissionSticky = SetMissionSticky
  oPda.SetSelectedMission = SetSelectedMission
  oPda.GetSelectedMission = GetSelectedMission
  oPda.SetMarker = SetMarker
  oPda.AddLineRegion = AddLineRegion
  oPda.RemoveLineRegion = RemoveLineRegion
  oPda.SetMissionTrackable = SetMissionTrackable
  oPda.SetMissionTrackCallback = SetMissionTrackCallback
  oPda.SetMissionChangeAllowed = SetMissionChangeAllowed
  oPda.SetFakePlayerLocation = SetFakePlayerLocation
  oPda.SetBeaconTutorialMode = SetBeaconTutorialMode
  oPda.CustomData.tSupport = {}
  oPda.CustomData.tSupportOrdered = {}
  oPda.CustomData.tSupportIdIndex = {}
  oPda.CustomData.tEquippedSupport = {}
  oPda.CustomData.tEquippedSupportIcons = {}
  oPda.AddSupport = AddSupport
  oPda.RemoveSupport = RemoveSupport
  oPda.UpdateSupport = UpdateSupport
  oPda.GetStockpile = GetStockpile
  oPda.OpenTransitInterface = OpenTransitInterface
  oPda.GetEquippedSupport = _GetEquippedSupport
  oPda.SetEquippedSupport = _SetEquippedSupport
  oPda.ReadEquippedSupport = ReadEquippedSupport
  oPda.RestoreEquippedSupport = RestoreEquippedSupport
  for sName, tData in pairs(MrxSupportData.tSupportData) do
    oPda:AddSupport(tData, sName)
  end
  oPda.CustomData.tFactionAttitudes = {}
  oPda.CustomData.tLogEntries = {}
  oPda.CustomData.tDataDossiers = {}
  oPda.CustomData.tDataDossiersIndex = {}
  oPda.CustomData.tDataHelp = {}
  oPda.CustomData.tDataHelpIndex = {}
  oPda.CustomData.tStatCategories = {}
  oPda.CustomData.tDataStatsOrdered = {}
  oPda.CustomData.tDataStats = {}
  oPda.SetFactionAttitude = SetFactionAttitude
  oPda.AddLogEntry = AddLogEntry
  oPda.AddDossierEntry = AddDossierEntry
  oPda.AddHelpEntry = AddHelpEntry
  oPda.AddStatisticCategory = AddStatisticCategory
  oPda.AddStatisticEntry = AddStatisticEntry
  oPda.UpdateStatisticEntry = UpdateStatisticEntry
  oPda.nAnalogInputHeld = 0
  MrxGuiBase.AddWidget(oMapFlash)
  oMapFlash:SetSwfFile(oMapFlash.CustomData.sFile, _FinishLoadAndClose, {oPda})
  _evPlayerJoin = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, SendPlayerJoinEvents)
  Debug.Printf("created player join event " .. tostring(_evPlayerJoin))
  Pg.LoadAsset("pda_titles", "texture")
end

function SendPlayerJoinEvents()
  Debug.Printf("SendPlayerJoinEvents 1")
  if not Net.IsServer() then
    return
  end
  Debug.Printf("SendPlayerJoinEvents 2")
  local oPda = MrxGuiBase.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
  if oPda and oPda.CustomData then
    Debug.Printf("SendPlayerJoinEvents 3")
    Net.SendCustomEvent("MrxGuiPda", NETEVENT_SETSELECTEDMISSION, {
      WifMissionData.GetMissionIndexFromId(oPda.CustomData.sSelectedMission)
    }, true)
  end
  Debug.Printf("SendPlayerJoinEvents 4")
end

function _FinishLoad(oPda)
  oPda.CustomData.bHaveFlash = true
  oPda.CustomData.oMapFlash:Pause()
  oPda.CustomData.oMapFlash:SetFlashEventHandler("TrackBlip", _HandleTrackEvent, {})
  oPda.CustomData.oMapFlash:SetFlashEventHandler("UntrackBlip", _HandleUntrackEvent, {})
  oPda.CustomData.oMapFlash:SetFlashEventHandler("cancelContract", _HandleMissionCancel)
  oPda.CustomData.oMapFlash:SetFlashEventHandler("equip", _HandleEquipEvent, {})
  oPda.CustomData.oMapFlash:SetFlashEventHandler("unequip", _HandleUnequipEvent, {})
  oPda.CustomData.oMapFlash:SetFlashEventHandler("closePDA", _HandleCloseEvent, {})
  oPda.CustomData.oMapFlash:SetFlashEventHandler("currentPage", _HandlePageChangeEvent, {})
  oPda.CustomData.oMapFlash:SetFlashEventHandler("LTIupdateSupportQuickSlot", _LTIupdateSupportQuickSlot, {})
end

function _FinishLoadAndClose(oPda)
  _FinishLoad(oPda)
  oPda:Close()
end

function AddPDATargetMarkers(oPda)
  local nXOffset = 35
  local nZOffset = 40
  local oFlash = oPda.CustomData.oMapFlash
  local tBlipTable = {}
  local tData
  local tAllMarkerList = Player.GetAllTargetMarkerPos()
  for i, tMarkerInfo in ipairs(tAllMarkerList) do
    if tMarkerInfo[1] then
      local sTexture
      local sName = string.format("[PDA.Map.Player:%d]", i)
      if i == 1 then
        sTexture = "target1_mc"
      else
        sTexture = "target2_mc"
      end
      tData = {
        sName,
        sTexture,
        tMarkerInfo[2] + nXOffset,
        tMarkerInfo[3] + nZOffset,
        0,
        sName,
        sName,
        " ",
        " ",
        false,
        " ",
        " ",
        true,
        true
      }
      table.insert(tBlipTable, tData)
    end
  end
  _GuiInternal.AddPdaMapBlips(oFlash.BasicData.uId, tBlipTable)
end

function UpdatePDATargetMarkers(oPda)
  local nXOffset = 35
  local nZOffset = 40
  local oFlash = oPda.CustomData.oMapFlash
  local tAllMarkerList = Player.GetAllTargetMarkerPos()
  for i, tMarkerInfo in ipairs(tAllMarkerList) do
    local sName = string.format("[PDA.Map.Player:%d]", i)
    if tMarkerInfo[1] then
      local sTexture
      if i == 1 then
        sTexture = "target1_mc"
      else
        sTexture = "target2_mc"
      end
      _GuiInternal.UpdatePdaBlip(oFlash.BasicData.uId, {
        sName,
        sTexture,
        tMarkerInfo[2] + nXOffset,
        tMarkerInfo[3] + nZOffset,
        0,
        sName,
        sName,
        " ",
        " ",
        false,
        " ",
        " ",
        true,
        false,
        true
      })
    else
      _GuiInternal.RemovePdaBlip(oFlash.BasicData.uId, sName)
    end
  end
end

function UpdatePlayerMarkers(oPda, uPlayerGuid, i)
  local oFlash = oPda.CustomData.oMapFlash
  local nXOffset = 35
  local nZOffset = 40
  local sName
  if i == 1 then
    sName = "player1_mc"
  else
    sName = "player2_mc"
  end
  local nRotation = 0
  local uCamera = Player.GetCamera(uPlayerGuid)
  if uCamera then
    nRotation = Camera.GetYaw(uCamera)
  end
  local sLocalName = string.format("[PDA.Map.Player:%d]", i)
  local sCurMissionDesc = sName
  local sMissionDisplay = " "
  if oPda.CustomData.sSelectedMission then
    tCurMission = oPda.CustomData.tMissions[oPda.CustomData.sSelectedMission]
    if tCurMission and not tCurMission.bSuppress then
      sCurMissionDesc = tCurMission.sDesc
      sMissionDisplay = "[PDA.Map.CurrentMission]"
    end
  end
  local uChar = Player.GetCharacter(uPlayerGuid)
  local nX, nY, nZ
  if oPda.CustomData.nFakePlayerX then
    nX = oPda.CustomData.nFakePlayerX
    nY = oPda.CustomData.nFakePlayerY
    nZ = oPda.CustomData.nFakePlayerZ
  else
    nX, nY, nZ = Object.GetPosition(uChar)
  end
  _GuiInternal.UpdatePdaBlip(oFlash.BasicData.uId, {
    sName,
    sName,
    nX + nXOffset,
    nZ + nZOffset,
    -nRotation,
    sLocalName,
    sCurMissionDesc,
    " ",
    " ",
    false,
    " ",
    sMissionDisplay,
    true,
    false,
    false
  })
end

local nOtherPlayerIndex = 0

function UpdateAllPlayerMarkers(oPda)
  local oFlash = oPda.CustomData.oMapFlash
  local uPDAOwnerGuid = oPda:GetOwner()
  local tPlayers = Player.GetAllPlayers()
  for i, uPlayerGuid in ipairs(tPlayers) do
    if uPlayerGuid == uPDAOwnerGuid then
      UpdatePlayerMarkers(oPda, uPlayerGuid, i)
      break
    end
  end
  local nNewOtherPlayerIndex = 0
  for i, uPlayerGuid in ipairs(tPlayers) do
    if uPlayerGuid ~= uPDAOwnerGuid then
      UpdatePlayerMarkers(oPda, uPlayerGuid, i)
      nNewOtherPlayerIndex = i
    end
  end
  if nNewOtherPlayerIndex < nOtherPlayerIndex then
    local sName = "player" .. nOtherPlayerIndex .. "_mc"
    _GuiInternal.RemovePdaBlip(oFlash.BasicData.uId, sName)
  end
  nOtherPlayerIndex = nNewOtherPlayerIndex
end

local nPDAMarkerUpdateTime = 0

function _HandlePDAUpdateEvent(oPda, nDeltaTime)
  nPDAMarkerUpdateTime = nPDAMarkerUpdateTime + nDeltaTime
  if 1 < nPDAMarkerUpdateTime then
    nPDAMarkerUpdateTime = 0
    UpdatePDATargetMarkers(oPda)
    UpdateAllPlayerMarkers(oPda)
  end
  if 0 <= oPda.CustomData.nFramesWithoutInput and oPda.CustomData.bMapMode then
    oPda.CustomData.nFramesWithoutInput = oPda.CustomData.nFramesWithoutInput + 1
    if oPda.CustomData.nFramesWithoutInput > 2 then
      oPda.CustomData.nFramesWithoutInput = -1
      local oMap = oPda.CustomData.oMapFlash
      oMap:HandleLeftAnalogInput(0, 0)
      oMap:HandleRightAnalogInput(0, 0)
    end
  end
end

function _HandleToggleEvent(oPda, tUnused)
  if oPda.CustomData.bActive then
    oPda:Close()
  else
    local oFocus = MrxGuiBase.GetCurrentControlHolder(oPda:GetOwner())
    if not oFocus or "Support Menu" == oFocus:GetName() then
      oPda:Open()
    end
  end
end

function _HandleInput(oPda, tInput)
  local oMap = oPda.CustomData.oMapFlash
  if oPda.CustomData.oTransit then
    oMap = oPda.CustomData.oTransit
  end
  ASSERT(oMap)
  if bExitOnLeft and MrxGuiBase.Joystick.BUTTON_PAD1_L == tInput.ButtonPress then
    oPda:Close()
  end
  local bTesselationWasOn = 0 == oPda.nAnalogInputHeld
  oPda.nAnalogInputHeld = 0
  local nEpsilon = 1.0E-5
  if tInput.LeftAnalogX and nEpsilon < math.abs(tInput.LeftAnalogX) then
    oPda.nAnalogInputHeld = oPda.nAnalogInputHeld + 1
  end
  if tInput.LeftAnalogY and nEpsilon < math.abs(tInput.LeftAnalogY) then
    oPda.nAnalogInputHeld = oPda.nAnalogInputHeld + 1
  end
  if tInput.RightAnalogX and nEpsilon < math.abs(tInput.RightAnalogX) then
    oPda.nAnalogInputHeld = oPda.nAnalogInputHeld + 1
  end
  if tInput.RightAnalogY and nEpsilon < math.abs(tInput.RightAnalogY) then
    oPda.nAnalogInputHeld = oPda.nAnalogInputHeld + 1
  end
  local bTesselationIsOn = 0 == oPda.nAnalogInputHeld
  if bTesselationWasOn ~= bTesselationIsOn and oPda.CustomData.bMapMode then
    oMap:SetTesselationAllowed(bTesselationIsOn)
    if bTesselationIsOn then
      oMap:HandleLeftAnalogInput(0, 0)
    else
      oMap:CallActionScriptCallback("currentPOI", {" "})
    end
  end
  if 0 > oPda.nAnalogInputHeld then
    oPda.nAnalogInputHeld = 0
  end
  oMap.EventHandlers.ControllerInput(oMap, tInput)
  if oPda.CustomData.bMapMode or oPda.CustomData.oTransit then
    oPda.CustomData.nFramesWithoutInput = 0
    if tInput.LeftAnalogX or tInput.LeftAnalogY then
      oMap:HandleLeftAnalogInput(tInput.LeftAnalogX or 0, tInput.LeftAnalogY or 0)
    else
      oMap:HandleLeftAnalogInput(0, 0)
    end
    if tInput.RightAnalogX or tInput.RightAnalogY then
      oMap:HandleRightAnalogInput(tInput.RightAnalogX or 0, tInput.RightAnalogY or 0)
    else
      oMap:HandleRightAnalogInput(0, 0)
    end
  end
end

function IsAnalog(nValue)
  if not nValue then
    return false
  end
  if nValue >= MrxGuiBase.Joystick.BUTTON_L_STICK_L and nValue <= MrxGuiBase.Joystick.BUTTON_R_STICK_D then
    return true
  end
  return false
end

function _HandleCloseEvent(oMapFlash)
  local oPda = oMapFlash.oParentWidget
  oPda:Close()
end

function _HandlePageChangeEvent(oMapFlash, sArg)
  local oPda = oMapFlash.oParentWidget
  if "Map" == sArg then
    oPda.CustomData.bMapMode = true
  else
    if oPda.CustomData.bMapMode then
      oPda.CustomData.nFramesWithoutInput = -1
      local oMap = oPda.CustomData.oMapFlash
      oMap:HandleLeftAnalogInput(0, 0)
      oMap:HandleLeftAnalogInput(0, 0)
    end
    oPda.CustomData.bMapMode = false
  end
end

function _HandleMapLocationEvent(oMapFlash, sData)
  local t = {}
  local nIndex = 1
  for nNumber in string.gmatch(sData, "-*%d+") do
    t[nIndex] = tonumber(nNumber)
    nIndex = nIndex + 1
  end
  if t[1] and t[2] then
    local nX = t[1] * 2
    local nY = 0
    local nZ = t[2] * 2
    if Pg.IsPointInBoundary then
      for sGuidName, sName in pairs(WifVzRegionNames.tBoundaryList) do
        local uGuid = Pg.GetGuidByName(sGuidName)
        if uGuid and Pg.IsPointInBoundary(nX, nY, nZ, uGuid) then
          oMapFlash:CallActionScriptCallback("currentPOI", {sName})
          return
        end
      end
    end
  end
  oMapFlash:CallActionScriptCallback("currentPOI", {"Venezuela"})
end

_tFactionNameLookup = false

function Init()
  bExitOnLeft = true
  if Gui.IsPdaOnSelect and Gui.IsPdaOnSelect() then
    bExitOnLeft = false
  end
  _tFactionNameLookup = {
    AN = "[0x8c648d92]",
    PR = "[0x151ea816]",
    OC = "[0x0375c825]",
    GR = "[0xec76433f]",
    CH = "[0x0b54aa0b]",
    VZ = "[0xa7953946]",
    PMC = "[0xeb4191d9]"
  }
end

function _LTIupdateSupportQuickSlot(oFlash, sParm)
  LTILibName.LTIupdateSupportQuickSlot(sParm)
end

function EnableQuickSlot(sId)
  Debug.Printf("Lua Support Quick slot enabling :" .. sId)
  local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
  if not oPda then
    Debug.Printf("ERROR: No PDA found!")
    return
  end
  local oSupportMenu = MrxGuiBase.GetWidgetByNameAndOwner("Support Menu", oPda:GetOwner())
  if oSupportMenu then
    if oPda.CustomData.sCurrentlyEquippedSupport then
      oSupportMenu:RemoveItem(oPda.CustomData.sCurrentlyEquippedSupport)
    end
    oPda.CustomData.sCurrentlyEquippedSupport = nil
    if oPda.CustomData.tSupportIdIndex[sId] then
      local sName = oPda.CustomData.tSupportIdIndex[sId]
      local tData = oPda.CustomData.tSupport[sName]
      oPda.CustomData.sCurrentlyEquippedSupport = tData.sName
      oPda.CustomData.bSupportNeedsEquipping = true
      oSupportMenu:AddItem(tData)
    end
  end
  local sSupportName = oPda.CustomData.sCurrentlyEquippedSupport
  if sSupportName and oPda.CustomData.bSupportNeedsEquipping then
    local tSupportData = oPda.CustomData.tSupport[sSupportName]
    local oGunAmmoCounter = MrxGuiBase.GetWidgetByNameAndOwner("Current Gun", oPda:GetOwner())
    oGunAmmoCounter:SetSuppressAnimation(true)
    local oNewSupport = tSupportData.oSupport:Create(tSupportData.oSupport.uOwner)
    oNewSupport:SetSupportName(tSupportData.oSupport:GetSupportName())
    oNewSupport:SetFuelCost(tSupportData.oSupport:GetFuelCost())
    oNewSupport:SetCashCost(tSupportData.oSupport:GetCashCost())
    oNewSupport:Commence()
    Event.Create(Event.TimerRelative, {0.2}, oGunAmmoCounter.TriggerAnimation, {
      oGunAmmoCounter,
      tSupportData.sIcon
    })
    oPda.CustomData.bSupportNeedsEquipping = false
  end
end
