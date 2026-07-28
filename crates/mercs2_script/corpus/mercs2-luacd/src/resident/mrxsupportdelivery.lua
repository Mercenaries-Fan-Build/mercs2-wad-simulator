inherit("MrxSupport")
import("MrxSupportManager")
import("MrxSupportDesignatorSmoke")
import("MrxGui")
import("MrxUtil")
sDeliveryVehicle = "UH1 Transport (PMC) (Driver)"
sCargoToDeliver = "box"
uCargoToDeliver = Pg.GetGuidByName(sCargoToDeliver)
nCargoDropHeight = 0.5
nAltitude = 250
local tVOOnTheWay = {
  "Ewan-None-Freeplay-Support-73",
  "Ewan-None-Freeplay-Support-10",
  "Ewan-None-Freeplay-Support-91",
  "Ewan-None-Freeplay-Support-90"
}

function Create(self, uOwnerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oNewSupport.bCareless = self.bCareless
  oNewSupport.oTarget = self.oTarget
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  oNewSupport.sFinalDestination = self.sFinalDestination
  oNewSupport.sCargoToDeliver = self.sCargoToDeliver
  oNewSupport.uCargoToDeliver = self.uCargoToDeliver
  oNewSupport.nCargoDropHeight = self.nCargoDropHeight
  oNewSupport.bNeedsConnection = false
  oNewSupport.oUpdateEvent = nil
  oDesignator = MrxSupportDesignatorSmoke:Create()
  oNewSupport:SetDesignator(oDesignator)
  oDesignator:SetSmokeColor("blue")
  oNewSupport:SetOwner(uOwnerGuid)
  oNewSupport:SetRecruit("Copter")
  oNewSupport:SetModuleName("MrxSupportDelivery")
  return oNewSupport
end

function DesignationCallback(self)
  _DesignatorCallback(self)
end

function SetCargoGuid(self, uCargoTemplate)
  self.uCargoToDeliver = uCargoTemplate
  self.bSetCargoGuidCalled = true
end

function SetCargo(self, sCargoTemplateName)
  self.sCargoToDeliver = sCargoTemplateName
  PickCargo(self, self.sCargoToDeliver)
end

function PickCargo(self, sCargoTemplateName)
  if not sCargoTemplateName then
    return
  end
  local sCargoToDeliver
  if type(sCargoTemplateName) == "table" then
    sCargoToDeliver = MrxUtil.GetRandomTableElement(sCargoTemplateName)
  else
    sCargoToDeliver = sCargoTemplateName
  end
  self.uCargoToDeliver = Pg.GetGuidByName(sCargoToDeliver)
end

function SetCareless(self, bCareless)
  if "table" ~= type(self) then
    return
  end
  Debug.Printf("Careless is set to " .. tostring(bCareless))
  self.bCareless = bCareless
end

function SetFinalDestination(self, oFinalDestination)
  if "table" ~= type(self) then
    return
  end
  self.oFinalDestination = oFinalDestination
end

function SetCargoDropHeight(self, sCargoTemplateName)
  if "table" ~= type(self) then
    return
  end
  if "number" ~= type(nCargoDropHeight) then
    return
  end
  self.nCargoDropHeight = nCargoDropHeight
end

_nHelicopterNumber = 0

function _DesignatorCallback(self)
  if Net.IsClient() then
    return
  end
  if not self.bSetCargoGuidCalled then
    PickCargo(self, self.sCargoToDeliver)
  end
  local nSpawnDistance = -Object.GetHibernationDistance(self.uCargoToDeliver) or -155
  nSpawnDistance = Math.max(nSpawnDistance + 5, -150)
  Debug.Printf("MrxSupportDelivery attempting to spawn: " .. tostring(self.uCargoToDeliver))
  local uCargo = Pg.SpawnFromCamera(self.uCargoToDeliver, nSpawnDistance, 1, true, Player.GetLocalPlayer(), false, true)
  if not uCargo then
    Debug.Printf("DELIVERY ERROR: No cargo spawned")
    return
  end
  local nDesX, nDesY, nDesZ = self.oDesignator:GetTarget()
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(nSpawnDistance, MrxSupport.GetSpawnHeight(), 30, self.uOwner, 300 + math.randi(120))
  if nDesY and nTargetY < nDesY + MrxSupport.GetSpawnHeight() then
    nTargetY = nDesY + MrxSupport.GetSpawnHeight()
  end
  local uHeli = Pg.Spawn(self.uDeliveryVehicle, nTargetX, nTargetY, nTargetZ, 0, false, true)
  self.uHeli = uHeli
  if not uHeli then
    Object.Remove(uCargo)
    Debug.Printf("DELIVERY ERROR: No copter spawned")
    return
  end
  local nHeliX, nHeliY, nHeliZ = Object.GetPosition(uHeli)
  local nTargetX, nTargetY, nTargetZ = self.oDesignator:GetTarget()
  local nOrientation = Math.GetXZHeading(nTargetX - nHeliX, nTargetY - nHeliY, nTargetZ - nHeliZ)
  Object.SetYaw(uHeli, nOrientation)
  Event.Create(Event.ObjectHibernation, {uHeli, "awake"}, _DeployWinch, {
    self,
    uHeli,
    uCargo
  })
end

function _DeployWinch(self, uHeli, uCargo)
  Debug.Printf("---------- Copter is awake")
  tVOOnTheWay = {
    PMC = {
      "Ewan-None-Freeplay-Support-10",
      "Ewan-None-Freeplay-Support-73",
      "Ewan-None-Freeplay-Support-89",
      "Ewan-None-Freeplay-Support-91",
      "Ewan-None-Freeplay-Support-99"
    },
    Allied = {
      "AlliedSoldier01.Support.Incoming02",
      "AlliedSoldier01.Support.Incoming03"
    },
    China = {
      "ChinaSoldier01.Support.Incoming01",
      "ChinaSoldier01.Support.Incoming02",
      "ChinaSoldier01.Support.Incoming03"
    },
    VZ = {
      "VZSoldier01.Support.Air01"
    },
    Guerilla = {
      "GurSoldier01.Support.Incoming01",
      "GurSoldier01.Support.Incoming02",
      "GurSoldier01.Support.Incoming03"
    },
    OC = {
      "OCSoldier01.Support.Incoming01",
      "OCSoldier01.Support.Incoming02",
      "OCSoldier01.Support.Incoming03"
    }
  }
  local sFaction = MrxUtil.GetFaction(uHeli)
  Debug.Printf("Playing VO Cue for " .. tostring(sFaction))
  if sFaction and tVOOnTheWay[sFaction] then
    MrxSupport.PlayRandomVOCue(tVOOnTheWay[sFaction])
  else
    Debug.Printf("No cue found for " .. tostring(tVOOnTheWay[sFaction]))
  end
  Object.SetWinchState(uHeli, "deployed")
  Event.Create(Event.TimerRelative, {0.1}, Event.Create, {
    Event.ObjectHibernation,
    {uCargo, "awake"},
    _WaitCallback,
    {
      self,
      uHeli,
      uCargo
    }
  })
end

function _WaitCallback(self, uHeli, uCargo)
  Debug.Printf("---------- Cargo is awake")
  local nX, nY, nZ = self.oDesignator:GetTarget()
  local pilot = Vehicle.GetDriver(uHeli)
  MrxSupport.SetupDamageEvent(self, uHeli, false)
  Object.SetYaw(uCargo, Object.GetYaw(uHeli))
  Object.AttachCargoToWinch(uCargo, uHeli)
  Object.AddToDisposer(uCargo, "vehicle")
  Ai.Deliver(Vehicle.GetDriver(uHeli), nX, nY, nZ, self.nCargoDropHeight, self.bCareless)
  Event.Create(Event.ObjectWinched, {
    uCargo,
    uHeli,
    "Detach"
  }, CargoDropped, {
    self,
    uHeli,
    uCargo
  })
  Object.AddLabel(uHeli, "Disposable")
end

function CargoDropped(self, uHeli, uCargo)
  MrxSupport.GoHome(self, uHeli)
  Object.AddToDisposer(uCargo, "vehicle")
end
