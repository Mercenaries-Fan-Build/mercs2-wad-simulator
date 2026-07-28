import("MrxState")
import("MrxPmc")
import("MrxSupportData")
import("WifPmcInterior")
import("MrxGuiGarage")
import("MrxUtil")
import("MrxVoSequence")
local _ksFionaCar = "Phoenix (Racing)"
local _ksFionaCarSpawn = "PMC Fiona Car Spawn"
local _ksPmcName = "_pmcoutpost_bld_hqgarage_livedin 0x000d3c78"
local _tRegions = {
  {
    "PMC Garage 1",
    "PMC Garage 2",
    "PMC Garage 3"
  },
  {
    "PMC Helipad"
  },
  {"PMC Dock"}
}
local _tDropOffs = {
  {
    "PMC Garage Dropoff",
    5,
    {
      "PMC Garage Exit 1",
      "PMC Garage Exit 2"
    }
  },
  {
    "PMC Helipad Dropoff",
    15,
    {
      "PMC Helipad Exit 1",
      "PMC Helipad Exit 2"
    }
  },
  {
    "PMC Dock Dropoff",
    10,
    {
      "PMC Dock Exit 1",
      "PMC Dock Exit 2"
    }
  }
}
local _tSpawnPoints = {
  {
    6,
    "PMC Garage Spawn Vehicle",
    {
      "PMC Garage Spawn Hero 1",
      "PMC Garage Spawn Hero 2"
    },
    "PMC Garage Spawn Eva"
  },
  {
    4,
    "PMC Helipad Spawn Vehicle",
    {
      "PMC Helipad Spawn Hero 1",
      "PMC Helipad Spawn Hero 2"
    },
    "PMC Helipad Spawn Eva"
  },
  {
    4,
    "PMC Dock Spawn Vehicle",
    {
      "PMC Dock Spawn Hero 1",
      "PMC Dock Spawn Hero 2"
    },
    "PMC Dock Spawn Eva"
  }
}
local _tGarageDoors = {
  {
    "PMC Garage Door 1",
    "PMC Garage Door Region 1"
  },
  {
    "PMC Garage Door 2",
    "PMC Garage Door Region 2"
  },
  {
    "PMC Garage Door 3",
    "PMC Garage Door Region 3"
  }
}
local _vCurrentRegion, _uCurrentRegion
local _bAdvanced = false
_tStorage = {
  {},
  {},
  {}
}
_tRegionEvents = {
  {},
  {},
  {}
}
_tGarageDoorEvents = {}
_tLastVehicles = {}
local _kvGarage = 1
local _knMaxVehicleSlots = 3
local _kvHelipad = 2
local _kvDock = 3

function Unlock()
  _bPmcAwake = true
  _OnAsleep()
end

function SetAdvancedMode()
  if _bAdvanced then
    return
  end
  Debug.Printf("Pmc Garage moving to advanced mode!")
  if _vCurrentRegion ~= nil then
    _OnRegionExit(_vCurrentRegion, _nCurrentSlot, nil, _uCurrentRegion)
  end
  _DeactivateGarageDoors()
  if _uDeath then
    Event.Delete(_uDeath)
  end
  _uDeath = nil
  for vId, tEvents in ipairs(_tRegionEvents) do
    for nSlot, uEvent in ipairs(tEvents) do
      if 1 < nSlot then
        Event.Delete(uEvent)
      end
    end
  end
  _tRegionEvents = {
    {},
    {},
    {}
  }
  _bAdvanced = true
  local tOldStorage = _tStorage
  _tStorage = {}
  for vId, tVehicles in ipairs(tOldStorage) do
    _tStorage[vId] = {}
    for nSlot, uVehicle in ipairs(tVehicles) do
      _AddVehicle(uVehicle, vId, -1)
      Object.RemoveLabel(uVehicle, "garage")
      Object.FadeOut(uVehicle, 0.2, true)
    end
  end
  _ActivateGarageDoors()
end

function ResetSingleton()
  if not _bAdvanced then
    for vId, tVehicles in ipairs(_tStorage) do
      for nSlot, uVehicle in ipairs(tVehicles) do
        Object.Remove(uVehicle)
      end
    end
  end
  _tStorage = {
    {},
    {},
    {}
  }
  _bAdvanced = false
  _OnAsleep()
end

function OpenVehicleInventory(fOnComplete, fOnCancel)
  if type(fOnComplete) ~= "function" then
    fOnComplete = false
  end
  if type(fOnCancel) ~= "function" then
    fOnCancel = false
  end
  if not _bAdvanced then
    if fOnCancel then
      fOnCancel()
    end
    return
  end
  local uCharacter = Player.GetLocalCharacter()
  local uPlayer = Player.GetLocalPlayer()
  MrxGuiGarage.Create(uPlayer)
  MrxGuiGarage.SetCallback(uPlayer, _CompleteCallback, {
    uCharacter,
    fOnComplete,
    fOnCancel
  })
  for vId, tVehicles in ipairs(_tStorage) do
    local sCategory
    if vId == _kvGarage then
      sCategory = "Civilian"
    elseif vId == _kvHelipad then
      sCategory = "Helicopters"
    elseif vId == _kvDock then
      sCategory = "Boats"
    end
    for uVehicle, nCount in pairs(tVehicles) do
      local sName = Object.GetLocalizedName(uVehicle)
      if sName then
        MrxGuiGarage.AddItem(uPlayer, Sys.GuidToString(uVehicle), sName, "needs description", sCategory, nCount, nCount, "what", false)
      end
    end
  end
  local nCount
  for sKey, tData in pairs(MrxSupportData.tSupportData) do
    nCount = MrxPmc.GetSupportQty(sKey)
    if (tData.sType == "Civilian" or tData.sType == "Heavy" or tData.sType == "Light" or tData.sType == "Heli" or tData.sType == "Boat") and nCount and 0 < nCount then
      local sId
      if tData.sType == "Heli" then
        sId = Sys.GuidToString(tData.oSupport.uDeliveryVehicle)
      else
        sId = Sys.GuidToString(tData.oSupport.uCargoToDeliver)
      end
      local sCategory = tData.sType
      if sCategory == "Heli" then
        sCategory = "Helicopters"
      elseif sCategory == "Boat" then
        sCategory = "Boats"
      end
      MrxGuiGarage.AddItem(uPlayer, sId, tData.sName, tData.sDescription, sCategory, nCount, tData.nMaxStock, tData.sIcon, false)
    end
  end
  MrxGuiGarage.Commence(uPlayer)
end

function _IsVehicleAvailable(uVehicle)
  local sName = _FindVehicleInSupport(uVehicle)
  if sName ~= nil then
    return MrxPmc.GetSupportQty(sName) > 0
  end
  return false
end

function _CompleteCallback(uCharacter, fOnComplete, fOnCancel, sVehicleGuid)
  if sVehicleGuid then
    if fOnComplete then
      fOnComplete()
    end
    local uVehicle = Sys.StringToGuid(sVehicleGuid)
    _SpawnVehicle(uCharacter, uVehicle)
  elseif fOnCancel then
    fOnCancel()
  end
end

function _SpawnVehicle(uCharacter, uParent)
  local bSuccess, vRegion = _RemoveVehicle(uParent)
  local uVehicle = MrxUtil.SpawnObject(uParent, _tSpawnPoints[vRegion][2])
  local tRiders = Vehicle.GetRiders(uVehicle)
  for i, uRider in ipairs(tRiders) do
    Object.Remove(uRider)
  end
  _ActivateEva(_tSpawnPoints[vRegion][4])
  MrxUtil.TeleportHeroesToLocations(_tSpawnPoints[vRegion][3])
  Event.Create(Event.TimerRelative, {0.03}, _SpawnVehicleCleanup, {vRegion, uVehicle})
end

function _SpawnVehicleCleanup(vRegion, uVehicle)
  local uDropOff = Pg.GetGuidByName(_tDropOffs[vRegion][1])
  local x, y, z = Object.GetPosition(uDropOff)
  local tObjects = Pg.GetObjectsInArea(x, y, z, _tDropOffs[vRegion][2], "Vehicle")
  for _, uObject in ipairs(tObjects) do
    if uObject == _tLastVehicles[vRegion] then
      _AddVehicle(_tLastVehicles[vRegion], vRegion, -1)
      Debug.Printf("Adding old vehicle back to storage - " .. tostring(_tLastVehicles[vRegion]))
    end
    if uObject ~= uVehicle then
      Debug.Printf(tostring(uObject) .. " is in the way, deleting")
      Object.Remove(uObject)
    end
  end
  _tLastVehicles[vRegion] = uVehicle
end

function _ActivateEva(sLocation)
  if _uEva then
    Debug.Printf("Eva activate, moving...")
    Object.SetTransformToObject(_uEva, sLocation)
    return
  end
  Debug.Printf("Activating Eva...")
  _uEva = MrxUtil.SpawnObject("Eva", sLocation)
  Object.SetInvincible(_uEva, true)
  Pg.AddContextAction(_uEva, "[ContextAction.Talk]", 2, 0, 0, 255, 2, false)
  _uEvaAction = Event.CreatePersistent(Event.ContextAction, {0, _uEva}, OpenVehicleInventory)
  Event.Create(Event.ObjectIsReady, {_uEva}, Event.Create, {
    Event.ObjectHibernation,
    {_uEva, "s"},
    _DeactivateEva
  })
end

function _DeactivateEva()
  Debug.Printf("Deactivating Eva...")
  if _uEvaAction then
    Event.Delete(_uEvaAction)
  end
  _uEvaAction = nil
  if _uEva then
    Pg.RemoveContextAction(_uEva)
    Object.FadeOut(_uEva, 0.3, true)
  end
  _uEva = nil
end

function _ActivateGarageDoors()
  for nIndex, tDoor in ipairs(_tGarageDoors) do
    if _bAdvanced and 1 < nIndex then
      return
    end
    local uRegion = Pg.GetGuidByName(tDoor[2])
    _tGarageDoorEvents[nIndex] = Event.Create(Event.Boundary, {
      Player.GetAnyCharacter(),
      uRegion,
      "Enter"
    }, _OnGarageDoorEnter, {nIndex})
  end
end

function _DeactivateGarageDoors()
  for i, uEvent in ipairs(_tGarageDoorEvents) do
    Event.Delete(uEvent)
  end
  _tGarageDoorEvents = {}
  for nIndex, tDoor in ipairs(_tGarageDoors) do
    local uDoor = Pg.GetGuidByName(tDoor[1])
    Object.CloseGate(uDoor)
  end
end

function _OnGarageDoorEnter(nIndex, uCharacter, uRegion)
  Debug.Printf("Opening Garage Door " .. nIndex)
  local uDoor = Pg.GetGuidByName(_tGarageDoors[nIndex][1])
  Object.OpenGate(uDoor)
  _tGarageDoorEvents[nIndex] = Event.Create(Event.Boundary, {
    Player.GetAnyCharacter(),
    uRegion,
    "Exit"
  }, _OnGarageDoorExit, {nIndex})
end

function _OnGarageDoorExit(nIndex, uCharacter, uRegion)
  Debug.Printf("Closing Garage Door " .. nIndex)
  local uDoor = Pg.GetGuidByName(_tGarageDoors[nIndex][1])
  Object.CloseGate(uDoor)
  _tGarageDoorEvents[nIndex] = Event.Create(Event.Boundary, {
    Player.GetAnyCharacter(),
    uRegion,
    "Enter"
  }, _OnGarageDoorEnter, {nIndex})
end

function _OnWakeUp()
  if _bPmcAwake then
    return
  end
  CheckFionaCar()
  _bPmcAwake = true
  local uPmc = Pg.GetGuidByName(_ksPmcName)
  Event.Create(Event.ObjectHibernation, {uPmc, "s"}, _OnAsleep)
end

function _OnAsleep()
  if not _bPmcAwake then
    return
  end
  _bPmcAwake = nil
  local uPmc = Pg.GetGuidByName(_ksPmcName)
  Event.Create(Event.ObjectHibernation, {uPmc, "a"}, _OnWakeUp)
end

function CheckFionaCar(bFromPmc)
  if bFromPmc then
    if _uFionaCarEvent then
      Event.Delete(_uFionaCarEvent)
      _uFionaCarEvent = nil
    end
    if _uFionaCarWinchEvent then
      Event.Delete(_uFionaCarWinchEvent)
      _uFionaCarWinchEvent = nil
    end
    if _uFionaCarEnterEvent then
      Event.Delete(_uFionaCarEnterEvent)
      _uFionaCarEnterEvent = nil
    end
    if _uFionaCar then
      Object.Remove(_uFionaCar)
    end
  else
    if _uFionaCar then
      return
    end
    if _bPMCAwake then
      return
    end
  end
  _uFionaCarEvent = nil
  _uFionaCarEnterEvent = nil
  if _uFionaCarDead then
    Object.Remove(_uFionaCarDead)
  end
  _uFionaCarDead = nil
  _uFionaCar = MrxUtil.SpawnObject(_ksFionaCar, _ksFionaCarSpawn)
  _uFionaCarEvent = Event.Create(Event.ObjectDeath, {_uFionaCar}, _OnFionaCarDeath)
  _uFionaCarEnterEvent = Event.Create(Event.ObjectInSeat, {
    "Hero",
    _uFionaCar,
    "a",
    "e"
  }, _OnFionaCarEnter)
  _uFionaCarWinchEvent = Event.Create(Event.ObjectWinched, {
    _uFionaCar,
    0,
    "a"
  }, _OnFionaCarEnter)
end

function _OnFionaCarEnter()
  if _uFionaCarWinchEvent then
    Event.Delete(_uFionaCarWinchEvent)
    _uFionaCarWinchEvent = nil
  end
  if _uFionaCarEnterEvent then
    Event.Delete(_uFionaCarEnterEvent)
    _uFionaCarEnterEvent = nil
  end
  local tVo = {
    "Fiona.Misc.CarTake01",
    "Fiona.Misc.CarTake02",
    "Fiona.Misc.CarTake03"
  }
  local sCue = MrxUtil.GetRandomTableElement(tVo)
  MrxVoSequence.Start({sCue}, nil, MrxVoSequence.knPriorityFreeplay)
end

function _OnFionaCarDeath()
  local tVo = {
    "Fiona.Misc.CarDestroy02",
    "Fiona.Misc.CarTake02"
  }
  local sCue = MrxUtil.GetRandomTableElement(tVo)
  MrxVoSequence.Start({
    sCue,
    0.2,
    {
      mattias = "Mattias.Reporting.Sorry.01",
      jennifer = "Jen.Reporting.Sorry.01",
      chris = "Chris.Reporting.Sorry.01"
    }
  }, nil, MrxVoSequence.knPriorityFreeplay)
  MrxPmc.AddCashQty(-10000, true, "[Garage.replacefionacar]")
  _uFionaCarEvent = Event.Create(Event.ObjectHibernation, {_uFionaCar, "s"}, CheckFionaCar)
  _uFionaCarDead = _uFionaCar
  _uFionaCar = nil
end

function _OnRegionEnter(vRegion, nSlot, uCharacter, uRegion)
  if _vCurrentRegion ~= nil then
    _OnRegionExit(_vCurrentRegion, _nCurrentSlot, uCharacter, _uCurrentRegion)
  end
  _tRegionEvents[vRegion][nSlot] = Event.Create(Event.Boundary, {
    uCharacter,
    uRegion,
    "Exit"
  }, _OnRegionExit, {vRegion, nSlot})
  _uVehicleExit = Event.CreatePersistent(Event.ObjectInSeat, {
    uCharacter,
    0,
    "d",
    "x"
  }, _OnVehicleExit, {vRegion, nSlot})
  if not _bAdvanced then
    _uVehicleEnter = Event.CreatePersistent(Event.ObjectInSeat, {
      uCharacter,
      0,
      "d",
      "e"
    }, _OnVehicleEnter, {vRegion, nSlot})
  end
  Debug.Printf("Entered region " .. tostring(vRegion) .. ", slot " .. tostring(nSlot))
  Hud.MessageBox:AddMessage({
    sMessage = "[Garage.RegionEnter]"
  })
  _vCurrentRegion = vRegion
  _uCurrentRegion = uRegion
  _nCurrentSlot = nSlot
end

function _OnRegionExit(vRegion, nSlot, uCharacter, uRegion)
  if _uVehicleExit then
    Event.Delete(_uVehicleExit)
  end
  _uVehicleExit = nil
  if _uVehicleEnter then
    Event.Delete(_uVehicleEnter)
  end
  _uVehicleEnter = nil
  if uRegion ~= nil and vRegion ~= nil then
    _tRegionEvents[vRegion][nSlot] = Event.Create(Event.Boundary, {
      Player.GetAnyCharacter(),
      uRegion,
      "Enter"
    }, _OnRegionEnter, {vRegion, nSlot})
  end
  Debug.Printf("Exited region " .. tostring(_vCurrentRegion) .. ", slot " .. tostring(_nCurrentSlot))
  _vCurrentRegion = nil
  _uCurrentRegion = nil
  _nCurrentSlot = nil
end

function _OnVehicleDeath(uVehicle)
  _RemoveVehicle(uVehicle)
end

function _OnVehicleEnter(vRegion, nSlot, uCharacter, uVehicle)
  _RemoveVehicle(uVehicle, vRegion, nSlot)
end

function _OnVehicleExit(vRegion, nSlot, uCharacter, uVehicle)
  if uVehicle == _uFionaCar then
    Hud.MessageBox:AddMessage({
      sMessage = "[Garage.StoreFionasCar]"
    })
    return
  end
  if not _bAdvanced then
    _AddVehicle(uVehicle, vRegion, nSlot)
    return
  end
  if not _AddVehicle(uVehicle, vRegion, -1) then
    Debug.Printf("Failed to add vehicle!")
    return
  end
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _CompleteOnVehicleEnter, {
    vRegion,
    nSlot,
    uCharacter,
    uVehicle
  })
end

function _CompleteOnVehicleEnter(vRegion, nSlot, uCharacter, uVehicle)
  _MoveCharacter(uCharacter, _tDropOffs[vRegion][3][1])
  local tRiders = Vehicle.GetRiders(uVehicle)
  for i, uRider in ipairs(tRiders) do
    if Object.IsPlayerControlled(uRider) then
      Vehicle.Exit(uVehicle, uRider, true)
      _MoveCharacter(uRider, _tDropOffs[vRegion][3][2])
      break
    end
  end
  Object.FadeOut(uVehicle, 0.2, true)
  Event.Create(Event.TimerRelative, {0.5}, _OnAddComplete, {
    uCharacter,
    Object.GetParent(uVehicle)
  })
end

function _MoveCharacter(uCharacter, sLocation)
  Object.SetTransformToObject(uCharacter, sLocation)
  local uPlayer = Object.IsPlayerControlled(uCharacter)
  local uCamera = Player.GetCamera(uPlayer)
  Camera.SetYaw(uCamera, 0)
end

function _OnAddComplete(uCharacter, uParent)
  MrxState.Exit(MrxState.STATE_WAITFORGAME)
  local sName = Object.GetLocalizedName(uParent)
  Hud.MessageBox:AddMessage({
    sMessage = string.format("[Garage.AddVehicle:%s]", sName)
  })
end

function _AddVehicle(uVehicle, vRegion, nSlot)
  Debug.Printf("Attemping to add " .. tostring(uVehicle) .. " to Storage")
  if vRegion == nil then
    Debug.Printf("Failed to add - Invalid region!")
    return false
  end
  local sType = Object.GetPhysicsType(uVehicle)
  if sType == nil then
    Debug.Printf("Failed to add - No vehicle type!")
    return false
  end
  local uParent = Object.GetParent(uVehicle)
  local tStorage
  if vRegion == _kvGarage then
    if sType ~= "car" and sType ~= "tank" then
      Debug.Printf("Failed to add - Not a car or tank")
      return false
    end
  elseif vRegion == _kvHelipad then
    if sType ~= "helicopter" then
      Debug.Printf("Failed to add - Not a helicopter")
      return false
    end
    if Vehicle.IsFlying(uVehicle) then
      Debug.Printf("Failed to add - Helicopter in flight")
      return false
    end
  elseif vRegion == _kvDock then
    if sType ~= "boat" then
      Debug.Printf("Failed to add - Not a boat")
      return false
    end
  else
    Debug.Printf("Failed to add - Invalid Region (" .. tostring(vRegion) .. ")")
    return false
  end
  if _bAdvanced then
    local bIsSupport = _AddVehicleToSupport(uVehicle)
    local nCount = _tStorage[vRegion][uParent]
    if bIsSupport then
      nCount = nil
    else
      if nCount == nil then
        nCount = 0
      end
      nCount = nCount + 1
    end
    _tStorage[vRegion][uParent] = nCount
  else
    if nSlot == nil then
      nSlot = 1
    end
    if _tStorage[vRegion][nSlot] ~= nil then
      _RemoveVehicle(_tStorage[vRegion][nSlot], vRegion)
    end
    _tStorage[vRegion][nSlot] = uVehicle
    Object.AddLabel(uVehicle, "garage")
  end
  Debug.Printf("Added " .. tostring(uVehicle) .. " to Storage")
  return true, vRegion
end

function _RemoveVehicle(uVehicle, vRegion)
  Debug.Printf("Attemping to remove " .. tostring(uVehicle) .. " from storage")
  if vRegion == nil then
    local sType = Object.GetPhysicsType(uVehicle)
    if sType == "car" or sType == "tank" then
      vRegion = _kvGarage
    elseif sType == "helicopter" then
      vRegion = _kvHelipad
    elseif sType == "boat" then
      vRegion = _kvDock
    else
      Debug.Printf("Failed to remove - invalid type")
      return false
    end
  end
  if _bAdvanced then
    local bIsSupport, nSupportQty = _RemoveVehicleFromSupport(uVehicle)
    local uParent
    if Object.IsTemplate(uVehicle) then
      uParent = uVehicle
    else
      uParent = Object.GetParent(uVehicle)
      if uParent == nil then
        return
      end
    end
    local nCount = _tStorage[vRegion][uParent]
    if bIsSupport then
      nCount = nil
    elseif nCount ~= nil then
      nCount = nCount - 1
      if nCount == 0 then
        nCount = nil
      end
    end
    _tStorage[vRegion][uParent] = nCount
  else
    for nSlot, uStoredVehicle in ipairs(_tStorage[vRegion]) do
      if uStoredVehicle == uVehicle then
        Object.RemoveLabel(uVehicle, "garage")
        _tStorage[vRegion][nSlot] = nil
        break
      end
    end
  end
  Debug.Printf("Removed " .. tostring(uVehicle) .. " from storage")
  return true, vRegion
end

function _FindVehicleInSupport(uVehicle)
  local uName = Object.GetLocalizedName(uVehicle, true)
  for sName, tData in pairs(MrxSupportData.tSupportData) do
    if String.GetHash(tData.sName, true) == uName then
      return sName
    end
  end
  Debug.Printf("Unable to find matching support for " .. Object.GetLocalizedName(uVehicle))
  return nil
end

function _AddVehicleToSupport(uVehicle)
  local sName = _FindVehicleInSupport(uVehicle)
  if sName ~= nil then
    MrxPmc.AddSupportQty(sName, 1)
    return true, MrxPmc.GetSupportQty(sName)
  end
  return false
end

function _RemoveVehicleFromSupport(uVehicle)
  local sName = _FindVehicleInSupport(uVehicle)
  if sName ~= nil then
    MrxPmc.AddSupportQty(sName, -1)
    return true, MrxPmc.GetSupportQty(sName)
  end
  return false
end

function SaveSingleton()
  local tVehicles, tHelicopters, tBoats, tStorage
  if _bAdvanced then
    tStorage = _tStorage
  else
    tStorage = {}
    for vId, tVehicles in ipairs(_tStorage) do
      tStorage[vId] = {}
      for nSlot, uVehicle in ipairs(tVehicles) do
        tStorage[vId][nSlot] = _GetVehicleData(uVehicle)
      end
    end
  end
  return {bAdvanced = _bAdvanced, tStorage = tStorage}
end

function _GetVehicleData(uVehicle)
  local uParent = Object.GetParent(uVehicle)
  local x, y, z = Object.GetPosition(uVehicle)
  local yaw = Object.GetYaw(uVehicle)
  return {
    uParent,
    x,
    y,
    z,
    yaw
  }
end

function LoadSingleton(tData)
  _bAdvanced = tData.bAdvanced
  if _bAdvanced then
    _tStorage = tData.tStorage
  else
    _tStorage = {}
    for vId, tVehicles in ipairs(tData.tStorage) do
      _tStorage[vId] = {}
      for nSlot, tVehicle in ipairs(tVehicles) do
        _tStorage[vId][nSlot] = _SpawnVehicleFromData(tVehicle)
      end
    end
  end
end

function _SpawnVehicleFromData(tVehicle)
  local uParent = tVehicle[1]
  local x, y, z = tVehicle[2], tVehicle[3], tVehicle[4]
  local yaw = tVehicle[5]
  return Pg.Spawn(uParent, x, y, z, yaw)
end
