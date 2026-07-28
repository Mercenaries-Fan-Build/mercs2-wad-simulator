import("MrxGui")
import("MrxSupportTransit")
import("MrxUnlockFanfare")
import("MrxFactionManager")
import("MrxUtil")
import("MrxSound")
import("MrxAchievements")
import("MrxState")
import("MrxStatsManager")
_nTransitFuelCost = 20

function OpenInterface(uPlayerGuid, fCallback, tCallbackData)
  if not _bInitialized or not _bEnabled then
    return false
  end
  local _tFactionOrder = {
    Pmc = 1,
    Oil = 2,
    Gur = 3,
    Pir = 4,
    All = 5,
    Chi = 6,
    Vza = 7
  }
  local tZones = {}
  local nCount = 0
  for nIndex, tData in pairs(_tLandingZones) do
    if tData.bEnabled and not tData.bSuppressed and not tData.bIsNuked then
      local nX, nY, nZ = Object.GetPosition(tData.uLocation1)
      Debug.Printf("Zone " .. tostring(nIndex) .. " name " .. tostring(tData.sName))
      local nSortOrder = 100
      nSortOrder = tData.sFactionAbbrev and _tFactionOrder[tData.sFactionAbbrev] or nSortOrder
      tZones[nIndex] = {
        sName = GetName(nIndex, true),
        nX = nX,
        nY = nZ,
        nSortOrder = nSortOrder
      }
      nCount = nCount + 1
    end
  end
  if 0 < nCount then
    _bInTransit = true
  end
  local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", uPlayerGuid)
  oPda:OpenTransitInterface(tZones, _InterfaceCallback, {
    fCallback,
    tCallbackData,
    oPda
  })
end

function _InterfaceCallback(sNumber, bSuccess, fCallback, tCallbackData, oPda)
  _bInTransit = nil
  oPda:Close()
  local nNumber = tonumber(sNumber)
  if "function" == type(fCallback) then
    if "table" ~= type(tCallbackData) then
      tCallbackData = {}
    end
    table.insert(tCallbackData, sNumber)
    table.insert(tCallbackData, bSuccess)
    fCallback(unpack(tCallbackData))
  end
end

function Transit(nLocation)
  if not _bInitialized or not _bEnabled then
    return false
  end
  local tData = _tLandingZones[nLocation]
  if not tData then
    return false
  end
  if not (not tData.bSuppressed and tData.bEnabled) or tData.bIsNuked then
    return false
  end
  if not tData.uLocation1 then
    return false
  end
  _bInTransit = true
  
  local function _TransitComplete()
    _bInTransit = nil
    MrxSound.EndTransit()
  end
  
  MrxUtil.TeleportHeroesToLocations({
    tData.uLocation1,
    tData.uLocation2
  }, _TransitComplete)
  MrxSound.BeginTransit()
  return true
end

function GetTransitPoint(nLocation)
  if not _bInitialized or not _bEnabled then
    return nil
  end
  local tData = _tLandingZones[nLocation]
  if not tData then
    return nil
  end
  return tData.uLocation1
end

function GetName(nId, bAppendIcon)
  local tLzData = _tLandingZones[nId]
  if tLzData then
    local sReturn = tLzData.sName
    if bAppendIcon then
      local sFactionAbbrev = tLzData.sFactionAbbrev
      if sFactionAbbrev then
        local sInlineIcon = MrxFactionManager.GetInlineIcon(sFactionAbbrev)
        if sInlineIcon then
          sReturn = sInlineIcon .. " " .. sReturn
        end
      end
    end
    return sReturn
  end
end

function GetUnlockedLocations()
  if not _bInitialized then
    return nil
  end
  local tReturn = {}
  for nIndex, tLzData in pairs(_tLandingZones) do
    if tLzData.bEnabled then
      tReturn[nIndex] = tLzData
    end
  end
  return tReturn
end

function GetUnlockableLocations()
  if not _tLandingZones then
    return
  end
  local tReturn = {}
  for nIndex, tLzData in pairs(_tLandingZones) do
    if not tLzData.bFake then
      tReturn[nIndex] = tLzData
    end
  end
  return tReturn
end

function GetNumValidLocations()
  if not _tLandingZones then
    return 0
  end
  local nReturn = 0
  for nIndex, tData in pairs(_tLandingZones) do
    if tData.bEnabled and not tData.bSuppressed and not tData.bIsNuked then
      nReturn = nReturn + 1
    end
  end
  return nReturn
end

function EnableFactionLocations(sFactionAbbrev, bAllow)
  if not _bInitialized then
    Reset()
  end
  local bSuppress = not bAllow
  for nIndex, tData in pairs(_tLandingZones) do
    if tData.sFactionAbbrev == sFactionAbbrev and tData.bSuppressed ~= bSuppress then
      tData.bSuppressed = bSuppress
    end
  end
end

function SetLocationEnabled(nLocation, sFactionAbbrev, bSuppressFanfare)
  if not _bInitialized then
    Reset()
  end
  if _tLandingZones and _tLandingZones[nLocation] then
    local tData = _tLandingZones[nLocation]
    if tData.bFake then
      return
    end
    if not tData.bEnabled and not bSuppressFanfare and not tData.bHasPlayedFanfare then
      tData.bHasPlayedFanfare = true
      MrxUnlockFanfare.AddUnlockedItem({
        sType = "landingzone",
        sFactionId = sFactionAbbrev,
        sName = Object.GetLocalizedName(tData.uLocation1)
      })
    end
    tData.sFactionAbbrev = sFactionAbbrev
    tData.bEnabled = true
    if not MrxFactionManager.TestAttitude(sFactionAbbrev, "Pmc", ">=", "Neutral") then
      tData.bSuppressed = true
    end
  end
  local tUnlocked = GetUnlockedLocations()
  local nUnlocked = _GetTableSizeSlow(tUnlocked)
  local tUnlockable = GetUnlockableLocations()
  local nUnlockable = _GetTableSizeSlow(tUnlockable)
  if nUnlocked >= nUnlockable then
    MrxAchievements.NetGrantAchievement("ACHIEVEMENT_BURN_THE_SKY", Player.GetPrimaryPlayer())
  end
end

function _GetTableSizeSlow(t)
  if "table" ~= type(t) then
    return 0
  end
  local n = 0
  for vIndex in pairs(t) do
    n = n + 1
  end
  return n
end

function SuppressLocation(nLocation, bSuppress)
  if not _bInitialized then
    Reset()
  end
  if _tLandingZones and _tLandingZones[nLocation] then
    local tData = _tLandingZones[nLocation]
    if tData.bSuppressed ~= bSuppress then
      tData.bSuppressed = bSuppress
    end
  end
end

function IsLocationEnabled(nLocation)
  return _tLandingZones and _tLandingZones[nLocation] and _tLandingZones[nLocation].bEnabled
end

function SetLocationIsNuked(nLocation, bIsNuked)
  if _bInitialized and _tLandingZones and _tLandingZones[nLocation] then
    local tData = _tLandingZones[nLocation]
    tData.bIsNuked = bIsNuked
    return true
  end
  return false
end

function IsSystemEnabled()
  return _bInitialized and _bEnabled
end

function SetSystemEnabled(bEnable, bAnimate, bHidden)
  Debug.Printf("SetSystemEnabled( " .. tostring(bEnable) .. ", " .. tostring(bAnimate) .. ", " .. tostring(bHidden))
  if bEnable == IsSystemEnabled() then
    return false
  end
  _bEnabled = bEnable
  if bHidden ~= true then
    if _bEnabled then
      local oTransit = MrxSupportTransit:Create(uPlayerGuid)
      oTransit:SetFuelCost(GetTransitFuelCost())
      Hud.SupportMenu:AddItem({
        vPlayer = nil,
        sName = "[support.transit.name]",
        sIcon = "HUD_ICON_support_helicopter",
        oSupport = oTransit,
        bAnimate = bAnimate
      })
    else
      Event.Delete(_evClientJoinedTransit)
      Hud.SupportMenu:RemoveItem({
        vPlayer = nil,
        sName = "[support.transit.name]"
      })
    end
  end
  if Net.IsServer() then
    local iEnable = 0
    local iAnimate = 0
    local iHidden = 0
    if bEnable then
      iEnable = 1
    end
    if bAnimate then
      iAnimate = 1
    end
    if bHidden then
      iHidden = 1
    end
    Debug.Printf("sending NETEVENT_CLIENTTRANSIT")
    Net.SendCustomEvent("MrxTransit", NETEVENT_CLIENTTRANSIT, {
      iEnable,
      iAnimate,
      iHidden
    })
    if _evClientJoinedTransit then
      Event.Delete(_evClientJoinedTransit)
    end
    _evClientJoinedTransit = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, Net.SendCustomEvent, {
      "MrxTransit",
      NETEVENT_CLIENTTRANSIT,
      {
        iEnable,
        iAnimate,
        iHidden
      },
      true
    })
  end
  return true
end

function IsSystemInitialized()
  return _bInitialized
end

_tLandingZones = false
_bInitialized = false
_bEnabled = false

function Reset()
  if not Pg.GetAllLandingZones then
    return
  end
  if _bInitialized then
    return
  end
  local tZones1 = Pg.GetAllLandingZones(1)
  local tZones2 = Pg.GetAllLandingZones(2)
  if not tZones1 or #tZones1 == 0 then
    return
  end
  _tLandingZones = {}
  for nIndex, uZoneGuid in pairs(tZones1) do
    local tZoneData = {
      uLocation1 = uZoneGuid,
      uLocation2 = tZones2[nIndex],
      sName = Object.GetLocalizedName(uZoneGuid),
      bEnabled = false
    }
    _tLandingZones[nIndex] = tZoneData
  end
  if _tLandingZones[6] then
    _tLandingZones[6].bFake = true
  end
  local tFactionAbbrevs = MrxFactionManager.GetFactionAbbrevs()
  for _, sFactionAbbrev in ipairs(tFactionAbbrevs) do
    Debug.Printf("Adding attitude events for " .. tostring(sFactionAbbrev))
    MrxFactionManager.CreatePersistentAttitudeChangeEvent({
      sFactionAbbrev,
      "Pmc",
      nil,
      nil
    }, function()
      local bEnable = MrxFactionManager.TestAttitude(sFactionAbbrev, "Pmc", ">=", "Neutral")
      EnableFactionLocations(sFactionAbbrev, bEnable)
    end)
  end
  _bInitialized = true
end

function SaveSingleton()
  if not _bInitialized then
    Reset()
  end
  local tSaveData = {}
  tSaveData.bEnabled = _bEnabled
  for nIndex, tData in pairs(_tLandingZones) do
    tSaveData[nIndex] = {}
    tSaveData[nIndex].sFactionAbbrev = tData.sFactionAbbrev
    tSaveData[nIndex].bHasPlayedFanfare = tData.bHasPlayedFanfare
    tSaveData[nIndex].bIsNuked = tData.bIsNuked
    tSaveData[nIndex].bEnabled = tData.bEnabled
  end
  return tSaveData
end

function LoadSingleton(tSaveData)
  if not _bInitialized then
    Reset()
  end
  if type(tSaveData) == "table" then
    SetSystemEnabled(tSaveData.bEnabled)
    for nIndex, tData in pairs(tSaveData) do
      if type(nIndex) == "number" then
        if type(tData) == "boolean" then
          _tLandingZones[nIndex].sFactionAbbrev = "Pmc"
          _tLandingZones[nIndex].bEnabled = tData
          Debug.Printf("Landing zone " .. tostring(nIndex) .. " using boolean (" .. tostring(tData) .. ")")
        else
          _tLandingZones[nIndex].sFactionAbbrev = tData.sFactionAbbrev
          _tLandingZones[nIndex].bHasPlayedFanfare = tData.bHasPlayedFanfare
          _tLandingZones[nIndex].bIsNuked = tData.bIsNuked
          _tLandingZones[nIndex].bEnabled = tData.bEnabled
          if tData.sFactionAbbrev then
            if not MrxFactionManager.TestAttitude(tData.sFactionAbbrev, "Pmc", ">=", "Neutral") then
              _tLandingZones[nIndex].bSuppressed = true
            end
            Debug.Printf("Landing zone " .. tostring(nIndex) .. " affiliated with " .. tostring(tData.sFactionAbbrev) .. " (" .. tostring(_tLandingZones[nIndex].bSuppressed) .. ")")
          end
        end
      end
    end
  end
end

function UnlockAllLandingZones()
  SetSystemEnabled(true, false)
  for nIndex, tData in pairs(_tLandingZones) do
    SetLocationEnabled(nIndex, "Pmc", true)
  end
end

function GetTransitFuelCost()
  return _nTransitFuelCost
end

function IsInTransit()
  return _bInTransit
end

function StartTransit(fEnterCallback, fExitCallback, tCallbackData)
  _bInTransit = true
  MrxStatsManager.IncreaseTransitCounter()
  MrxState.Enter(MrxState.STATE_WAITFORSTREAMING, fEnterCallback, tCallbackData, FinishTransit, {fExitCallback, tCallbackData})
  if Net.IsServer() then
    Net.SendCustomEvent("MrxTransit", NETEVENT_STARTTRANSIT, {})
  end
end

function FinishTransit(fCallback, tCallbackArgs)
  Event.Create(Event.TimerRelative, {0.75, true}, function()
    _bInTransit = nil
    MrxState.Exit(MrxState.STATE_WAITFORSTREAMING)
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
  end)
end

NETEVENT_CLIENTTRANSIT = 0
NETEVENT_STARTTRANSIT = 1

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_CLIENTTRANSIT then
    local bEnable = false
    local bAnimate = false
    local bHidden = false
    if tArgs[1] == 1 then
      bEnable = true
    end
    if tArgs[2] == 1 then
      bAnimate = true
    end
    if tArgs[3] == 1 then
      bHidden = true
    end
    Debug.Printf("received NETEVENT_CLIENTTRANSIT")
    SetSystemEnabled(bEnable, bAnimate, bHidden)
  elseif nEventType == NETEVENT_STARTTRANSIT then
    Debug.Printf("received NETEVENT_STARTTRANSIT")
    StartTransit(nil, nil, nil)
  end
end
