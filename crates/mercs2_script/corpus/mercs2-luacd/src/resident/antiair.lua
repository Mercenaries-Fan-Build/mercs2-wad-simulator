inherit("EnemyBlippable")
import("MrxSupport")
import("HomingMissile")
tColorAlly = false
tColorNeutral = false
tColorEmpty = false
tColorPmc = false
_tPrototype = {
  [1] = {
    sLevel = "basic",
    iArg = 1,
    tFlash = {
      255,
      255,
      255
    },
    sTexture = "radar_AA",
    nAARange = 100,
    nSize = 8,
    bSticky = true,
    nSortOrder = 2,
    tMarker = {
      sTexture = "HUD_anti-air",
      tFlash = {
        255,
        255,
        255
      },
      nVerticalOffset = 3.5,
      bJust2DCheck = true
    }
  },
  [2] = {
    sLevel = "medium",
    iArg = 2,
    tFlash = {
      255,
      255,
      255
    },
    sTexture = "radar_SAM",
    nAARange = 200,
    nSize = 8,
    bSticky = true,
    nSortOrder = 2,
    tMarker = {
      sTexture = "HUD_SAM",
      tFlash = {
        255,
        255,
        255
      },
      nVerticalOffset = 3.5,
      bJust2DCheck = true
    }
  },
  [3] = {
    sLevel = "advanced",
    iArg = 3,
    tFlash = {
      255,
      255,
      255
    },
    sTexture = "radar_AA",
    nAARange = 200,
    nSize = 8,
    bSticky = true,
    nSortOrder = 2,
    tMarker = {
      sTexture = "HUD_anti-air",
      tFlash = {
        255,
        255,
        255
      },
      nVerticalOffset = 3.5,
      bJust2DCheck = true
    }
  },
  [4] = {
    sLevel = "jammer",
    iArg = 4,
    tFlash = {
      255,
      255,
      255
    },
    sTexture = "radar_Jammer",
    nAARange = 200,
    nSize = 8,
    bSticky = true,
    nSortOrder = 2,
    tMarker = {
      sTexture = "HUD_jammer",
      tFlash = {
        255,
        255,
        255
      },
      nVerticalOffset = 3.5,
      bJust2DCheck = true
    }
  }
}
tEvent = tEvent or {}

function Init(param)
  local oModule = getfenv()
  for _, oProto in pairs(_tPrototype) do
    setmetatable(oProto, {__index = oModule})
  end
end

function OnActivate(uGuid, uRuntimeOwner, iArg)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Awake, {uGuid, iArg})
end

function Awake(uGuid, iArg)
  if not Object.IsAlive(uGuid) then
    return
  end
  tEvent[uGuid] = {}
  if Object.GetHibernationDistance(uGuid) <= _tPrototype[iArg].nAARange then
    ActivateAA(uGuid, iArg)
  else
    CreateNearnessEvent(uGuid, iArg)
  end
end

function OnDeactivate(uGuid)
  if tEvent[uGuid] and tEvent[uGuid].oClose then
    Event.Delete(tEvent[uGuid].oClose)
  end
  if tEvent[uGuid] and tEvent[uGuid].oFar then
    Event.Delete(tEvent[uGuid].oFar)
  end
  tEvent[uGuid] = nil
  EnemyBlippable.OnDeactivate(uGuid)
end

function CreateNearnessEvent(uGuid, iArg)
  Debug.Printf("CreateNearnessEvent " .. tostring(uGuid))
  if tEvent[uGuid].oClose then
    Debug.Printf("WARNING: Attempting to create event when one exists!")
    return
  end
  tEvent[uGuid].oClose = Event.Create(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uGuid,
    "<",
    _tPrototype[iArg].nAARange
  }, ActivateWithEvents, {uGuid, iArg})
end

function ActivateWithEvents(uGuid, iArg)
  Debug.Printf("ActivateWithEvents" .. tostring(uGuid))
  tEvent[uGuid].oClose = nil
  ActivateAA(uGuid, iArg)
  CreateDistanceEvent(uGuid, iArg)
end

function DeactivateWithEvents(uGuid, iArg)
  Debug.Printf("DeactivateWithEvents " .. tostring(uGuid))
  tEvent[uGuid].oFar = nil
  ClearBlipped(tEvent[uGuid].oInstance)
  CreateNearnessEvent(uGuid, iArg)
end

function ActivateAA(uGuid, iArg)
  Debug.Printf("AntiAir.ActivateAA " .. tostring(uGuid) .. " (" .. tostring(iArg) .. ")")
  if tEvent[uGuid] and tEvent[uGuid].oClose then
    Event.Delete(tEvent[uGuid].oClose)
    tEvent[uGuid].oClose = nil
  end
  local oPrototype = _tPrototype[iArg]
  tEvent[uGuid].oInstance = oPrototype:Create(uGuid)
end

function CreateDistanceEvent(uGuid, iArg)
  Debug.Printf("CreateDistanceEvent " .. tostring(uGuid))
  if tEvent[uGuid].oFar then
    Debug.Printf("WARNING: Attempting to create event when one exists!")
    return
  end
  Debug.Printf("##### Distance: " .. tostring(_tPrototype[iArg].nAARange))
  tEvent[uGuid].oFar = Event.Create(Event.ObjectProximity, {
    Player.GetAllCharacters(),
    uGuid,
    ">",
    _tPrototype[iArg].nAARange
  }, DeactivateWithEvents, {uGuid, iArg})
end

function SetBlipped(oSelf, bCalledByDriver)
  Debug.Printf("AntiAir.SetBlipped")
  EnemyBlippable.SetBlipped(oSelf)
  if oSelf.bHostile then
    MrxSupport.AddAntiAir(oSelf.uGuid, oSelf.sLevel)
  end
end

function ClearBlipped(oSelf, bCalledByDriver)
  _HomingLockClear(nil, {
    uOwnerGuid = oSelf.uOwnerGuid,
    uVehicleGuid = oSelf.uGuid,
    uPlayerGuid = oSelf.uPlayerGuid
  })
  if oSelf.bHostile then
    MrxSupport.RemoveAntiAir(oSelf.uGuid)
  end
  EnemyBlippable.ClearBlipped(oSelf)
end

_tLockOns = {}
_tLockOnState = {}
_tLockOnUpdates = {}
ksCueTargeted = "ui_hud_sam_targeted"
ksCueTargeting = "ui_hud_radar_targeting_alert"
ksCueAlert = "ui_hud_radar_targeting_new_alert"
knCueAlertCooldown = 1

function _CooldownComplete()
  _bAlertCoolingDown = false
end

function _SetSound(bPlay, sCue)
  if bPlay and sCue == ksCueAlert then
    if _bAlertCoolingDown then
      return
    end
    Event.Create(Event.TimerRelative, {knCueAlertCooldown}, _CooldownComplete)
    _bAlertCoolingDown = true
  end
end

function _UpdateHomingState(uPlayerGuid, bTargeted, sAction, bTransfer)
  if not _tLockOnState[uPlayerGuid] then
    _tLockOnState[uPlayerGuid] = {nTargeting = 0, nTargeted = 0}
  end
  local tState = _tLockOnState[uPlayerGuid]
  if bTargeted then
    if sAction == "add" then
      if tState.nTargeted == 0 then
        _SetSound(true, ksCueTargeted)
      elseif not bTransfer then
        _SetSound(true, ksCueAlert)
      end
      tState.nTargeted = tState.nTargeted + 1
    else
      tState.nTargeted = tState.nTargeted - 1
      if tState.nTargeted == 0 then
        _SetSound(false, ksCueTargeted)
      end
    end
  elseif sAction == "add" then
    if tState.nTargeting == 0 then
      _SetSound(true, ksCueTargeting)
    elseif not bTransfer then
      _SetSound(true, ksCueAlert)
    end
    tState.nTargeting = tState.nTargeting + 1
  else
    tState.nTargeting = tState.nTargeting - 1
    if tState.nTargeting == 0 then
      _SetSound(false, ksCueTargeting)
    end
  end
  if tState.nTargeting == 0 and tState.nTargeted == 0 then
    _tLockOnState[uPlayerGuid] = nil
  end
end

function _HomingLockStart(oWidget, tData)
  if _tLockOns[tData.uOwnerGuid] then
    return
  end
  _tLockOns[tData.uOwnerGuid] = {
    uLockTimer = Sys.RealTimeStamp()
  }
  tLockOn = _tLockOns[tData.uOwnerGuid]
  tLockOn.bTargeted = false
  tLockOn.bBlink = false
  tLockOn.tEvents = {}
  local tClearData = {
    uOwnerGuid = tData.uOwnerGuid,
    uVehicleGuid = tData.uVehicleGuid,
    uPlayerGuid = tData.uPlayerGuid
  }
  local uHomee
  if tData.uPlayerGuid then
    local uCharGuid = Player.GetCharacter(tData.uPlayerGuid)
    uHomee = Vehicle.GetFromRider(uCharGuid)
  end
  if uHomee then
    tLockOn.tEvents[1] = Event.Create(Event.ObjectDeath, {uHomee}, _HomingLockClear, {
      0,
      tClearData,
      1
    })
    tLockOn.tEvents[2] = Event.Create(Event.ObjectInSeat, {
      0,
      uHomee,
      "d",
      "x"
    }, _HomingLockClear, {
      0,
      tClearData,
      2
    })
  end
  local uHomer = tData.uVehicleGuid or tData.uOwnerGuid
  if uHomer then
    tLockOn.tEvents[3] = Event.Create(Event.ObjectDeath, {uHomer}, _HomingLockClear, {
      0,
      tClearData,
      3
    })
    tLockOn.tEvents[4] = Event.Create(Event.ObjectHibernation, {uHomer, "s"}, _HomingLockClear, {
      0,
      tClearData,
      4
    })
  end
  local oInstance = GetFromGuid(tData.uVehicleGuid)
  if oInstance and oInstance.bActive then
    oInstance.uOwnerGuid = tData.uOwnerGuid
    oInstance.uPlayerGuid = tData.uPlayerGuid
  end
  _UpdateHomingState(tData.uPlayerGuid, tLockOn.bTargeted, "add")
end

function _HomingLockUpdate(oWidget, tData)
  local tLockOn = _tLockOns[tData.uOwnerGuid]
  if not tLockOn then
    _HomingLockStart(oWidget, tData)
    return
  end
  local bLock = tData.nPercent >= 1
  if tLockOn.bTargeted ~= bLock then
    _UpdateHomingState(tData.uPlayerGuid, tLockOn.bTargeted, "remove")
    _UpdateHomingState(tData.uPlayerGuid, bLock, "add", true)
    tLockOn.bTargeted = bLock
  end
  local oInstance = GetFromGuid(tData.uVehicleGuid)
  if oInstance and oInstance.bActive then
    oInstance.uOwnerGuid = tData.uOwnerGuid
    oInstance.uPlayerGuid = tData.uPlayerGuid
    local nRate = bLock and 5 or 1 + 2 * tData.nPercent
    local nBlink = Sys.TimeStampGetElapsed(tLockOn.uLockTimer) * nRate
    local nLength = bLock and 0.1 or 0.2
    local bBlink = nBlink - math.floor(nBlink) < nLength * nRate
    if tLockOn.bBlink ~= bBlink then
      tLockOn.bBlink = bBlink
      oInstance:AddObjective(bBlink)
    end
  end
  local tClearData = {
    uOwnerGuid = tData.uOwnerGuid,
    uVehicleGuid = tData.uVehicleGuid,
    uPlayerGuid = tData.uPlayerGuid
  }
  Event.Delete(tLockOn.tEvents[5])
  tLockOn.tEvents[5] = Event.Create(Event.Timer, {1}, function(oWidget, tData, nEvent)
    Debug.Printf("AntiAir: HomingLock left hanging... tData=" .. tostring(tData) .. " nEvent=" .. tostring(nEvent))
    _HomingLockClear(oWidget, tData, nEvent)
  end, {
    0,
    tClearData,
    nil
  })
end

function _HomingLockClear(oWidget, tData, nEvent)
  local tLockOn = _tLockOns[tData.uOwnerGuid]
  if not tLockOn then
    return
  end
  for nIndex, uEvent in ipairs(tLockOn.tEvents) do
    if nIndex ~= nEvent then
      Event.Delete(uEvent)
    end
  end
  _UpdateHomingState(tData.uPlayerGuid, tLockOn.bTargeted, "remove", true)
  local oInstance = GetFromGuid(tData.uVehicleGuid)
  if oInstance and oInstance.bActive then
    oInstance.uOwnerGuid = tData.uOwnerGuid
    oInstance.uPlayerGuid = tData.uPlayerGuid
    tLockOn.bTargeted = false
    if tLockOn.bBlink then
      tLockOn.bBlink = false
      oInstance:AddObjective(false)
    end
  end
  _tLockOns[tData.uOwnerGuid] = nil
end

function _HomingLaunched(oWidget, tData)
  HomingMissile._HomingLaunched(oWidget, tData)
end
