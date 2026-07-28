import("MrxCheatBootstrap")
import("MrxVoSequence")
_bMapBoundariesDrawn = false

function SetupBoundaryIntro()
  SetupBoundary("BoundaryIntro", false)
end

function SetupBoundary00()
  SetupBoundary("Boundary00", false)
end

function SetupBoundaryINTRO_OIL()
  SetupBoundary("INTRO_OIL", false)
end

function SetupBoundaryPOST_OIL()
  SetupBoundary("POST_OIL", false)
end

function SetupBoundaryPOST_EVA_PRE_PIR()
  if _sBoundaryName ~= "Boundary02" then
    SetupBoundary("POST_EVA_PRE_PIR", false)
  end
end

function SetupBoundaryPOST_EVA_POST_PIR()
  if _sBoundaryName ~= "Boundary02" then
    SetupBoundary("POST_EVA_POST_PIR", false)
  end
end

function SetupBoundaryPMCCON003()
  if _sBoundaryName ~= "Boundary02" then
    SetupBoundary("BoundaryPMCCON003", false)
  end
end

function SetupBoundary02()
  SetupBoundary("Boundary02", true)
end

function SetupBoundary(sBoundaryName, bShowMessage)
  if Net.IsClient() then
    return
  end
  Debug.Printf("@@@@@@@@@@ WifVzBoundary.SetupBoundary " .. sBoundaryName)
  RemoveWorldBoundary()
  _sBoundaryName = sBoundaryName
  _AddBoundaryToPlayers(true)
  _DrawWorldBoundaryOnMap(true)
  bShowMessage = bShowMessage and not MrxCheatBootstrap.IsSkipModeEnabled()
  if bShowMessage then
    Hud.MessageBox:AddMessage({
      sMessage = "[Fanfare.BoundaryExpanded]",
      bAllowsAppends = false,
      nDuration = 3,
      nFadeTime = 0.25
    })
  end
end

function BoundaryCallback(uPlayer, sType, sAction)
  if sType == "out" and sAction == "enter" and not bVoDelay then
    MrxVoSequence.Start({
      "Fiona-None-Freeplay-None-01"
    })
    bVoDelay = true
    Event.Create(Event.TimerRelative, {10}, function()
      bVoDelay = nil
    end, {})
  end
  if sType == "out" and sAction == "exit" then
    nVoIndex = 5 + math.randi(0, 3)
    if nVoIndex == 6 then
      nVoIndex = 6 + math.randi(1, 2)
    end
    MrxVoSequence.Start({
      "Fiona.fio_g0" .. nVoIndex
    })
    Event.Create(Event.TimerRelative, {10}, function()
      bVoDelay = nil
    end, {})
  end
  if sType == "out" then
    if sAction == "enter" then
      Sound.CueSound(0, "ui_static")
    else
      Sound.StopSound(0, "ui_static")
    end
  end
end

function RemoveWorldBoundary()
  if Net.IsClient() then
    return
  end
  Debug.Printf("@@@@@@@@@@ WifVzBoundary.RemoveWorldBoundary (_sBoundaryName=" .. tostring(_sBoundaryName) .. ")")
  if _sBoundaryName then
    _AddBoundaryToPlayers(false)
    _DrawWorldBoundaryOnMap(false)
    _sBoundaryName = nil
  end
end

function EnableExclusionBoundary(sBoundaryName, bEnable)
  if Net.IsClient() then
    return
  end
  _tExclusionBoundaries = _tExclusionBoundaries or {}
  if bEnable then
    _tExclusionBoundaries[sBoundaryName] = true
  else
    _tExclusionBoundaries[sBoundaryName] = false
  end
  local tPlayers = Player.GetAllPlayers()
  local uBoundary = Pg.GetGuidByName(sBoundaryName)
  for i, uPlayerGuid in ipairs(tPlayers) do
    if bEnable then
      Player.AddBoundary(uPlayerGuid, uBoundary)
    else
      Player.RemoveBoundary(uPlayerGuid, uBoundary)
    end
  end
end

function RemoveExclusionBoundaries()
  if Net.IsClient() then
    return
  end
  if not _tExclusionBoundaries then
    return
  end
  for sBoundaryName, bEnable in pairs(_tExclusionBoundaries) do
    EnableExclusionBoundary(sBoundaryName, false)
  end
  _tExclusionBoundaries = nil
end

function SetInteriorMode(bEnable)
  if Net.IsClient() then
    return
  end
  Debug.Printf("@@@@@@@@@@ WifVzBoundary.SetInteriorMode (_sBoundaryName=" .. tostring(_sBoundaryName) .. ")")
  _AddBoundaryToPlayers(not bEnable)
  if _tExclusionBoundaries then
    for sBoundaryName in pairs(_tExclusionBoundaries) do
      EnableExclusionBoundary(sBoundaryName, not bEnable)
    end
  end
  if bEnable then
    local tPlayers = Player.GetAllPlayers()
    for i, uPlayerGuid in ipairs(tPlayers) do
      Player.SetOutBoundary(uPlayerGuid, false)
    end
  end
end

function _AddBoundaryToPlayers(bEnable)
  local tPlayers = Player.GetAllPlayers()
  local uBoundary = Pg.GetGuidByName(_sBoundaryName)
  for i, uPlayerGuid in ipairs(tPlayers) do
    if bEnable then
      Player.AddBoundary(uPlayerGuid, uBoundary)
      Player.SetBoundaryCallback(uPlayerGuid, BoundaryCallback)
    else
      Player.RemoveAllBoundary(uPlayerGuid)
    end
  end
end

function _DrawWorldBoundaryOnMap(bEnable)
  if bEnable == _bMapBoundariesDrawn then
    return
  end
  if not _sBoundaryName then
    return
  end
  _DrawBoundaryOnMap(_sBoundaryName, bEnable, true)
  _bMapBoundariesDrawn = bEnable
end

function _DrawBoundaryOnMap(sBoundaryName, bEnable, bInvert)
  local uBoundary = Pg.GetGuidByName(sBoundaryName)
  if bEnable then
    Hud.Radar:AddLineRegion({
      uGuid = uBoundary,
      bInvert = bInvert,
      nRed = 0,
      nGreen = 0,
      nBlue = 0,
      nAlpha = 160
    })
    Pda.Map:AddLineRegion({
      uGuid = uBoundary,
      bInvert = bInvert,
      nRed = 0,
      nGreen = 0,
      nBlue = 0,
      nAlpha = 160
    })
  else
    Hud.Radar:RemoveLineRegion({uGuid = uBoundary})
    Pda.Map:RemoveLineRegion({uGuid = uBoundary})
  end
end

function DrawExclusionBoundaryOnMap(sBoundaryName, bEnable)
  _DrawBoundaryOnMap(sBoundaryName, bEnable, false)
end

function SaveSingleton()
  local tSaveData = {}
  if _sBoundaryName then
    tSaveData.sBoundaryName = _sBoundaryName
  end
  if _tExclusionBoundaries then
    tSaveData.tExclusionBoundaries = _tExclusionBoundaries
  end
  return tSaveData
end

function LoadSingleton(tSaveData, bAutoDeactivate)
  RemoveWorldBoundary()
  RemoveExclusionBoundaries()
  if not tSaveData then
    return
  end
  if not bAutoDeactivate then
    if tSaveData.sBoundaryName then
      SetupBoundary(tSaveData.sBoundaryName)
    end
    if tSaveData.tExclusionBoundaries then
      for sBoundaryName, bEnable in pairs(tSaveData.tExclusionBoundaries) do
        EnableExclusionBoundary(sBoundaryName, bEnable)
        DrawExclusionBoundaryOnMap(sBoundaryName, bEnable)
      end
    end
  else
    if tSaveData.sBoundaryName then
      _sBoundaryName = tSaveData.sBoundaryName
      _DrawWorldBoundaryOnMap(true)
    end
    if tSaveData.tExclusionBoundaries then
      _tExclusionBoundaries = tSaveData.tExclusionBoundaries
      for sBoundaryName, bEnable in pairs(tSaveData.tExclusionBoundaries) do
        DrawExclusionBoundaryOnMap(sBoundaryName, bEnable)
      end
    end
  end
end
