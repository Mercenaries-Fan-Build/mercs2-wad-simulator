inherit("Blippable")
import("MrxGui")
import("MrxPlayState")
import("MrxPmc")
import("MrxSupportData")
import("MrxTutorialManager")
import("MrxUtil")
import("MrxVoSequence")
import("MrxMunitionsPickup")
_kDistance = 175
local tMunitions = {
  "artillery",
  "bombingrun",
  "bunkerbuster",
  "carpetbomb",
  "clusterbomb",
  "combatairpatrol",
  "cruisemissile",
  "daisycutter",
  "fuelairbomb",
  "harm",
  "laserguidedbomb",
  "moab",
  "rocketartillery",
  "smartbomb",
  "strategicmissile",
  "surgicalstrike",
  "tankbuster",
  {nFuel = 50},
  {nFuel = 500},
  {nFuel = 5000},
  {nCash = 100000}
}
_nTagged = 0

function OnActivate(uGuid, uRuntimeOwner, nStock)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Awake, {uGuid, nStock})
end

_tHideEvents = {}

function Awake(uGuid, nStock)
  if not Object.IsAlive(uGuid) then
    return
  end
  if not tMunitions[nStock] then
  end
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, nStock)
  oInstance.nStock = nStock
  if IsSupport(nStock) then
    oInstance.sTexture = "radar_Munition"
    oInstance.tColor = {
      51,
      102,
      51
    }
    oInstance.tFlash = {
      255,
      255,
      255
    }
    oInstance.nSize = 8
    oInstance.tMarker = {
      sTexture = "pickup_munitions",
      tColor = {
        153,
        255,
        153
      },
      nSize = 40,
      nNearDist = 5,
      nFarDist = 100
    }
  elseif IsFuel(nStock) then
    oInstance.sTexture = "radar_Oil"
    oInstance.tColor = {
      51,
      102,
      51
    }
    oInstance.tFlash = {
      255,
      255,
      255
    }
    oInstance.nSize = 8
    oInstance.tMarker = {
      sTexture = "pickup_fuel_2",
      tColor = {
        153,
        255,
        153
      },
      nSize = 40,
      nNearDist = 5,
      nFarDist = 100
    }
  elseif IsCash(nStock) then
    oInstance.sTexture = "radar_Money"
    oInstance.tColor = {
      51,
      102,
      51
    }
    oInstance.tFlash = {
      255,
      255,
      255
    }
    oInstance.nSize = 8
    oInstance.tMarker = {
      sTexture = "pickup_cash_2",
      tColor = {
        153,
        255,
        153
      },
      nSize = 40,
      nNearDist = 5,
      nFarDist = 100
    }
  end
  oInstance:AddContextAction()
  oInstance.NearnessEvent = Event.Create(Event.ObjectProximity, {
    uGuid,
    Player.GetLocalCharacter(),
    "<",
    _kDistance
  }, oInstance.Near, {oInstance})
end

function OnDeactivate(uGuid)
  local tThisInstance = tInstance[uGuid]
  if tThisInstance and MrxMunitionsPickup.bPickupInProgress and not tThisInstance.bPickedUp then
    Debug.Printf("Doing immediate pickup of hibernated munitions")
    PickupMunitions(uGuid)
  end
  Blippable.OnDeactivate(uGuid)
end

function HideTutorialMessage(uGuid)
  MrxTutorialManager.HideMessage(true)
  Event.Delete(_tHideEvents[uGuid])
  _tHideEvents[uGuid] = nil
end

_nBlippedVOCoolDownTime = 30
_bAllowBlippedVO = false
_eBlippedVOCoolDown = nil

function SetAllowBlippedVO(bAllow)
  _bAllowBlippedVO = bAllow
end

function PlayBlippedVO(nStock)
  local sCue
  if not _bAllowBlippedVO then
    if _eBlippedVOCoolDown == nil then
      _eBlippedVOCoolDown = Event.Create(Event.TimerRelative, {10}, SetAllowBlippedVO, {true})
    end
    return
  end
  if AreMunitionsTaggable() then
    if not _bPlayedCashHint2 and IsCash(nStock) then
      sCue = "Fiona.Misc.Cash01"
    elseif not _bPlayedFuelHint2 and IsFuel(nStock) then
      sCue = "Fiona.Misc.Fuel01"
    elseif not _bPlayedSupportHint2 and IsSupport(nStock) then
      sCue = "Fiona.Misc.Munition02"
    end
  elseif not _bPlayedCashHint1 and IsCash(nStock) then
    sCue = "Fiona.Misc.Cash02"
  elseif not _bPlayedFuelHint1 and IsFuel(nStock) then
    sCue = "Fiona.Misc.Fuel02"
  elseif not _bPlayedSupportHint1 and IsSupport(nStock) then
    sCue = "Fiona.Misc.Munition01"
  end
  if sCue and MrxVoSequence.Start(sCue, false, MrxVoSequence.knPriorityFreeplay, false) then
    SetAllowBlippedVO(false)
    _eBlippedVOCoolDown = Event.Create(Event.TimerRelative, {_nBlippedVOCoolDownTime}, SetAllowBlippedVO, {true})
    if AreMunitionsTaggable() then
      if IsCash(nStock) then
        _bPlayedCashHint2 = true
      elseif IsFuel(nStock) then
        _bPlayedFuelHint2 = true
      elseif IsSupport(nStock) then
        _bPlayedSupportHint2 = true
      end
    elseif IsCash(nStock) then
      _bPlayedCashHint1 = true
    elseif IsFuel(nStock) then
      _bPlayedFuelHint1 = true
    elseif IsSupport(nStock) then
      _bPlayedSupportHint1 = true
    end
  end
end

function Near(self)
  local nStock = self.nStock
  if MrxPlayState.IsFree() then
    PlayBlippedVO(nStock)
  end
  Event.Delete(self.NearnessEvent)
  self.NearnessEvent = nil
  self:SetBlipped()
  if Net.IsClient() then
    Net.SendCustomEvent("Munitions", NETEVENT_ISMUNITIONTAGGED, {
      self.uGuid
    })
  end
  self.FarnessEvent = Event.Create(Event.ObjectProximity, {
    self.uGuid,
    Player.GetLocalCharacter(),
    ">",
    _kDistance
  }, self.Far, {self})
end

function Far(self)
  Event.Delete(self.FarnessEvent)
  self.FarnessEvent = nil
  self:ClearBlipped()
  self.NearnessEvent = Event.Create(Event.ObjectProximity, {
    self.uGuid,
    Player.GetLocalCharacter(),
    "<",
    _kDistance
  }, self.Near, {self})
end

function AddContextAction(self)
  if Net.IsClient() then
    return
  end
  local uGuid = self.uGuid
  local nStock = self.nStock
  if Object.HasLabel(uGuid, "Vehicle") and table.getn(Vehicle.GetRiders(uGuid)) > 0 then
    self.VehicleExitEvent = Event.Create(Event.ObjectInSeat, {
      "Human",
      uGuid,
      "a",
      "x"
    }, function()
      OnActivate(uGuid, nil, nStock)
    end)
    return
  end
  local sMunName = tostring(Object.GetLocalizedName(uGuid))
  local sContextAction = "[ContextAction.TagMunition:" .. sMunName .. "]"
  if IsFuel(nStock) then
    sContextAction = "[ContextAction.TagFuel]"
  elseif IsCash(nStock) then
    sContextAction = "[ContextAction.TagCash]"
  end
  if not AreMunitionsTaggable() then
    sContextAction = "[neut]" .. sContextAction
  end
  Pg.AddContextAction(uGuid, sContextAction, -3, 0, 0, 0, 0)
  local fTagEvent = Event.Create
  fTagEvent = Event.CreatePersistent
  self.TagEvent = fTagEvent(Event.ContextAction, {
    Player.GetAnyCharacter(),
    uGuid
  }, self.Actioned, {self})
  if Object.HasLabel(uGuid, "Vehicle") then
    self.VehicleEnterEvent = Event.CreatePersistent(Event.ObjectInSeat, {
      "Human",
      uGuid,
      "a",
      "e"
    }, self.HumanControlled, {self})
  end
end

function CanActionTarget(uGuid, uHero, nStock)
  Debug.Printf("Calling CanActionTarget on " .. tostring(uGuid))
  if uHero ~= Player.GetLocalCharacter() then
    if Net.IsServer() then
      Debug.Printf("Querying client")
      Net.SendCustomEvent("Munitions", NETEVENT_CLIENTSTOCKPILEQUERY, {uGuid, nStock})
    end
    return false
  end
  local vStock = tMunitions[nStock]
  if not AreMunitionsTaggable() then
    if not _tHideEvents[uGuid] and MrxTutorialManager.ShowMessage("[support.munition.needpilot]", true) then
      _tHideEvents[uGuid] = Event.Create(Event.TimerRelative, {5}, HideTutorialMessage, {uGuid})
    end
    Debug.Printf("Munitions not taggable")
    return false
  end
  if IsFuel(nStock) and MrxPmc.GetFuelQty() >= MrxPmc.GetFuelCapacity() then
    if MrxTutorialManager.ShowMessage("[support.munition.fuelfull]", true) then
      _tHideEvents[uGuid] = Event.Create(Event.TimerRelative, {5}, HideTutorialMessage, {uGuid})
    end
    Debug.Printf("Fuel is maxed out")
    return false
  end
  if type(vStock) == "string" then
    local tSupport = MrxSupportData.tSupportData[vStock]
    local nQty = MrxPmc.GetSupportQty(vStock)
    if nQty and nQty >= tSupport.nMaxStock then
      if MrxTutorialManager.ShowMessage("[support.munition.full:" .. tSupport.sName .. "]", true) then
        _tHideEvents[uGuid] = Event.Create(Event.TimerRelative, {5}, HideTutorialMessage, {uGuid})
      end
      Debug.Printf("Support is maxed out")
      return false
    end
  end
  Debug.Printf("CanActionTarget returning true")
  return true
end

function Actioned(self, uHero)
  if self.bTagged then
    return
  end
  local uGuid = self.uGuid
  local nStock = self.nStock
  Debug.Printf("Attempted action on " .. tostring(uGuid))
  if CanActionTarget(uGuid, uHero, nStock) then
    ActionTarget(uGuid)
  end
end

function ActionTarget(uGuid)
  local self = tInstance[uGuid]
  Debug.Printf("Action on " .. tostring(uGuid) .. " success!")
  Pg.RemoveContextAction(uGuid)
  local oInstance = GetFromGuid(uGuid)
  if oInstance then
    oInstance:RemoveObjective()
    oInstance.tColor = {
      0,
      255,
      0
    }
    oInstance.tMarker.tColor = {
      0,
      255,
      0
    }
    oInstance:AddObjective(false)
  end
  Marker.Pulse(uGuid, 0, 255, 0)
  Net.SendCustomEvent("Munitions", NETEVENT_MARKERPULSE, {uGuid})
  self.bTagged = true
  _nTagged = _nTagged + 1
  local tCues = {
    "Fiona.Support.Munitions02",
    "Fiona.Support.Munitions03",
    {
      chris = "Chris-In-Mission-Contract-Oil02-55",
      mattias = "Mattias-In-Mission-Contract-Oil02-53",
      jennifer = "Jennifer-In-Mission-Contract-Oil02-54"
    }
  }
  local tSequence = {
    MrxUtil.GetRandomTableElement(tCues)
  }
  MrxVoSequence.Start(tSequence, nil, MrxVoSequence.knPriorityFreeplay)
  if _nTagged == 1 then
    MrxSupportData.AddFreebie("MunitionsPickup")
  end
end

function HumanControlled(self, uCharacter)
  local uGuid = self.uGuid
  local nStock = self.nStock
  self:Delete()
  self.VehicleExitEvent = Event.Create(Event.ObjectInSeat, {
    uCharacter,
    uGuid,
    "a",
    "x"
  }, function()
    OnActivate(uGuid, nil, nStock)
  end)
end

function Delete(self)
  Debug.Printf("DELETING MUNITIONS")
  local uGuid = self.uGuid
  Event.Post("UntagMunitions", {uGuid})
  if not Net.IsClient() then
    Pg.RemoveContextAction(uGuid)
  end
  self:ClearBlipped()
  if self.TagEvent then
    Event.Delete(self.TagEvent)
    self.TagEvent = nil
  end
  if self.VehicleEnterEvent then
    Event.Delete(self.VehicleEnterEvent)
    self.VehicleEnterEvent = nil
  end
  if self.VehicleExitEvent then
    Event.Delete(self.VehicleExitEvent)
    self.VehicleExitEvent = nil
  end
  if self.NearnessEvent then
    Event.Delete(self.NearnessEvent)
    self.NearnessEvent = nil
  end
  if self.FarnessEvent then
    Event.Delete(self.FarnessEvent)
    self.FarnessEvent = nil
  end
  if self.bTagged then
    self.bTagged = nil
    _nTagged = _nTagged - 1
  end
  self.bPickedUp = nil
  if self._uHideMessage then
    Event.Delete(self._uHideMessage)
    self._uHideMessage = nil
  end
  Debug.Printf("TAGGED: " .. tostring(_nTagged))
  if not Net.IsClient() and _nTagged < 1 then
    Debug.Printf("NO MORE MUNITIONS: REMOVING FREEBIE")
    MrxSupportData.RemoveFreebie("MunitionsPickup")
    Event.Post("NoMunitions", {})
  end
  Blippable.Delete(self)
end

function OnDeath(uGuid)
  Inheritable.OnDeath(uGuid)
  if Net.IsClient() then
    Pg.RemoveContextAction(uGuid)
  end
end

function IsCash(nStock)
  local tMunition = tMunitions[nStock]
  if tMunition and type(tMunition) == "table" and tMunition.nCash then
    return true
  end
  return false
end

function IsFuel(nStock)
  local tMunition = tMunitions[nStock]
  if tMunition and type(tMunition) == "table" and tMunition.nFuel then
    return true
  end
  return false
end

function IsSupport(nStock)
  local tMunition = tMunitions[nStock]
  if tMunition and type(tMunition) == "string" then
    return true
  end
  return false
end

_bMunitionsTaggable = true
_bPlayedCashHint1 = false
_bPlayedFuelHint1 = false
_bPlayedSupportHint1 = false
_bPlayedCashHint2 = false
_bPlayedFuelHint2 = false
_bPlayedSupportHint2 = false

function SetMunitionsTaggable(bTaggable)
  if type(bTaggable) == "boolean" then
    local bCurrentTaggable = _bMunitionsTaggable
    _bMunitionsTaggable = bTaggable
    if not Net.IsClient() and bCurrentTaggable ~= bTaggable then
      RefreshMunitions()
    end
    if Net.IsServer() then
      local iTaggable = 0
      if _bMunitionsTaggable then
        iTaggable = 1
      end
      Net.SendCustomEvent("Munitions", NETEVENT_SETTAGGABLE, {iTaggable})
    end
  end
end

function AreMunitionsTaggable()
  return _bMunitionsTaggable
end

function RefreshMunitions()
  for uGuid, tThisInstance in pairs(tInstance) do
    if Object.IsAlive(uGuid) and not tThisInstance.bTagged and not tThisInstance.bPickedUp then
      local sContextAction
      local nStock = tThisInstance.nStock
      if IsSupport(nStock) then
        sContextAction = "[ContextAction.TagMunition:" .. tostring(Object.GetLocalizedName(uGuid)) .. "]"
      elseif IsCash(nStock) then
        sContextAction = "[ContextAction.TagCash]"
      elseif IsFuel(nStock) then
        sContextAction = "[ContextAction.TagFuel]"
      end
      if sContextAction then
        if not AreMunitionsTaggable() then
          sContextAction = "[neut]" .. sContextAction
        end
        Pg.AddContextAction(uGuid, sContextAction, -3, 0, 0, 0, 0)
      end
    end
  end
end

function SaveSingleton()
  return {
    bMunitionsTaggable = AreMunitionsTaggable(),
    bPlayedCashHint1 = _bPlayedCashHint1,
    bPlayedFuelHint1 = _bPlayedFuelHint1,
    bPlayedSupportHint1 = _bPlayedSupportHint1,
    bPlayedCashHint2 = _bPlayedCashHint2,
    bPlayedFuelHint2 = _bPlayedFuelHint2,
    bPlayedSupportHint2 = _bPlayedSupportHint2,
    bPlayedFirstFuelPickupVO = _bPlayedFirstFuelPickupVO
  }
end

function LoadSingleton(tSaveData)
  if type(tSaveData) == "table" then
    SetMunitionsTaggable(tSaveData.bMunitionsTaggable)
    _bPlayedCashHint1 = tSaveData.bPlayedCashHint1
    _bPlayedFuelHint1 = tSaveData.bPlayedFuelHint1
    _bPlayedSupportHint1 = tSaveData.bPlayedSupportHint1
    _bPlayedCashHint2 = tSaveData.bPlayedCashHint2
    _bPlayedFuelHint2 = tSaveData.bPlayedFuelHint2
    _bPlayedSupportHint2 = tSaveData.bPlayedSupportHint2
    _bPlayedFirstFuelPickupVO = tSaveData.bPlayedFirstFuelPickupVO
  end
end

function PickupAllMunitions()
  for uGuid, tThisInstance in pairs(tInstance) do
    PickupMunitions(uGuid)
  end
end

function PickupMunitions(uGuid)
  local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
  if not oPda then
    Debug.Printf("ERROR: No PDA found!")
    return
  end
  tThisInstance = tInstance[uGuid]
  if not tThisInstance then
    Debug.Printf("Error: Attempting to pick up munitions that I have no record for")
    return
  end
  if tThisInstance.bTagged then
    local uGuid = tThisInstance.uGuid
    local nStock = tThisInstance.nStock
    local vStock = tMunitions[nStock]
    local bValidStock
    if IsSupport(nStock) then
      Event.Post("MunitionsPickup", {vStock, uGuid})
      MrxPmc.AddSupportQty(vStock, 1, true)
      oPda:UpdateSupport(vStock, nil, nil, MrxPmc.GetSupportQty(vStock))
      bValidStock = true
    elseif IsFuel(nStock) then
      if not _bPlayedFirstFuelPickupVO then
        MrxVoSequence.Start({
          "Fiona-In-Mission-Freeplay-None-25"
        }, nil, MrxVoSequence.knPriorityFreeplay, false)
        Debug.Printf("playing VO fuel pick up first time now")
        _bPlayedFirstFuelPickupVO = true
      end
      Event.Post("MunitionsPickup", {"Fuel", uGuid})
      MrxPmc.AddFuelQty(vStock.nFuel)
      bValidStock = true
    elseif IsCash(nStock) then
      Event.Post("MunitionsPickup", {"Cash", uGuid})
      MrxPmc.AddCashQty(vStock.nCash, nil, "[Generic.Pickups]")
      bValidStock = true
    end
    if bValidStock and Net.IsServer() then
      Net.SendCustomEvent("Munitions", NETEVENT_PICKUP, {nStock})
    end
    tThisInstance.bPickedUp = true
    OnDeactivate(uGuid)
    if not Object.IsWinched(uGuid) then
      Debug.Printf("Fading out munitions")
      if Object.IsAwake(uGuid) then
        Object.FadeOut(uGuid, 2, true)
      else
        Object.Remove(uGuid)
      end
    else
      Debug.Printf("NOT Fading out munitions--object is winched")
    end
  end
end

function GetTaggedMunition()
  for uGuid, tThisInstance in pairs(tInstance) do
    if tThisInstance.bTagged then
      return tThisInstance.uGuid
    end
  end
end

function IsMunitionTagged(uMunition)
  for uGuid, tThisInstance in pairs(tInstance) do
    if uGuid == uMunition then
      return tThisInstance.bTagged
    end
  end
end

function ClientTagAndBlip(uGuid)
  local oInstance = GetFromGuid(uGuid)
  if oInstance then
    oInstance:RemoveObjective()
    oInstance.tColor = {
      0,
      255,
      0
    }
    oInstance.tMarker.tColor = {
      0,
      255,
      0
    }
    oInstance:AddObjective(false)
  end
  Marker.Pulse(uGuid, 0, 255, 0)
end

function GetMunitionsCount()
  if _nTagged > 0 then
    return nTagged
  else
    return false, "nomunitions"
  end
end

NETEVENT_SETTAGGABLE = 0
NETEVENT_CLIENTSTOCKPILEQUERY = 1
NETEVENT_CLIENTSTOCKPILEACK = 2
NETEVENT_PICKUP = 3
NETEVENT_MARKERPULSE = 4
NETEVENT_ISMUNITIONTAGGED = 5

function NetEventCallback(nType, tArgs)
  if nType == NETEVENT_SETTAGGABLE then
    if tArgs[1] == 1 then
      SetMunitionsTaggable(true)
    else
      SetMunitionsTaggable(false)
    end
  elseif nType == NETEVENT_CLIENTSTOCKPILEQUERY then
    if Net.IsClient() then
      if CanActionTarget(tArgs[1], Player.GetLocalCharacter(), tArgs[2]) then
        Net.SendCustomEvent("Munitions", NETEVENT_CLIENTSTOCKPILEACK, {
          tArgs[1],
          1
        })
      else
        Net.SendCustomEvent("Munitions", NETEVENT_CLIENTSTOCKPILEACK, {
          tArgs[1],
          0
        })
      end
    end
  elseif nType == NETEVENT_CLIENTSTOCKPILEACK then
    if tArgs[2] == 1 then
      ActionTarget(tArgs[1])
    end
  elseif nType == NETEVENT_PICKUP then
    local nStock = tArgs[1]
    local vStock = tMunitions[nStock]
    if IsSupport(nStock) then
      MrxPmc.AddSupportQty(vStock, 1, true)
      local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
      if oPda then
        oPda:UpdateSupport(vStock, nil, nil, MrxPmc.GetSupportQty(vStock))
      end
    elseif IsFuel(nStock) then
      if not _bPlayedFirstFuelPickupVO then
        MrxVoSequence.Start({
          "Fiona-In-Mission-Freeplay-None-25"
        }, nil, MrxVoSequence.knPriorityFreeplay, false)
        Debug.Printf("playing VO fuel pick up first time now")
        _bPlayedFirstFuelPickupVO = true
      end
      MrxPmc.AddFuelQty(vStock.nFuel)
    elseif IsCash(nStock) then
      MrxPmc.AddCashQty(vStock.nCash, nil, "[Generic.Pickups]")
    end
  elseif nType == NETEVENT_MARKERPULSE then
    if Net.IsClient() then
      ClientTagAndBlip(tArgs[1])
    end
  elseif nType == NETEVENT_ISMUNITIONTAGGED and Net.IsServer() then
    local bIsTagged = IsMunitionTagged(tArgs[1])
    if bIsTagged then
      Net.SendCustomEvent("Munitions", NETEVENT_MARKERPULSE, {
        tArgs[1]
      })
    end
  end
end

function OnPlayerJoined()
  if Net.IsServer() then
    local iTaggable = 0
    if AreMunitionsTaggable() then
      iTaggable = 1
    end
    Net.SendCustomEvent("Munitions", NETEVENT_SETTAGGABLE, {iTaggable})
  end
end
