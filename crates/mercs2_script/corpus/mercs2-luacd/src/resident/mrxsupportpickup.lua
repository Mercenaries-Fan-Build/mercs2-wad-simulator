inherit("MrxSupport")
import("MrxSupportManager")
import("MrxSupportDesignatorSmoke")
import("MrxUtil")
import("MrxTutorialManager")
import("MrxVoSequence")
import("MrxFactionManager")
sDeliveryVehicle = "UH1 Transport (PMC) (Driver)"
nAltitude = 250
tCues = {
  Allied = {
    Land = {
      "AlliedSoldier01.Extraction.Arrival01",
      "AlliedSoldier01.Extraction.Arrival02",
      "AlliedSoldier01.Extraction.Arrival03"
    },
    Inc = {
      "AlliedSoldier01.Extraction.Incoming01",
      "AlliedSoldier01.Extraction.Incoming02",
      "AlliedSoldier01.Extraction.Incoming03"
    },
    Leave = {
      "AlliedSoldier01.Extraction.TakeOff01",
      "AlliedSoldier01.Extraction.TakeOff02",
      "AlliedSoldier01.Extraction.TakeOff03"
    }
  },
  China = {
    Land = {
      "ChinaSoldier01.Extraction.Arrival01",
      "ChinaSoldier01.Extraction.Arrival02",
      "ChinaSoldier01.Extraction.Arrival03"
    },
    Inc = {
      "ChinaSoldier01.Extraction.Incoming01",
      "ChinaSoldier01.Extraction.Incoming02",
      "ChinaSoldier01.Extraction.Incoming03"
    },
    Leave = {
      "ChinaSoldier01.Extraction.TakeOff01",
      "ChinaSoldier01.Extraction.TakeOff02",
      "ChinaSoldier01.Extraction.TakeOff03"
    }
  },
  Guerilla = {
    Land = {
      "GurSoldier01.Extraction.Arrival01",
      "GurSoldier01.Extraction.Arrival02",
      "GurSoldier01.Extraction.Arrival03"
    },
    Inc = {
      "GurSoldier01.Extraction.Incoming01",
      "GurSoldier01.Extraction.Incoming02",
      "GurSoldier01.Extraction.Incoming03"
    },
    Leave = {
      "GurSoldier01.Extraction.TakeOff01",
      "GurSoldier01.Extraction.TakeOff02",
      "GurSoldier01.Extraction.TakeOff03"
    }
  },
  OC = {
    Land = {
      "OCSoldier01.Extraction.Arrival01",
      "OCSoldier01.Extraction.Arrival02",
      "OCSoldier01.Extraction.Arrival03"
    },
    Inc = {
      "OCSoldier01.Extraction.Incoming01",
      "OCSoldier01.Extraction.Incoming02",
      "OCSoldier01.Extraction.Incoming03"
    },
    Leave = {
      "OCSoldier01.Extraction.TakeOff01",
      "OCSoldier01.Extraction.TakeOff02",
      "OCSoldier01.Extraction.TakeOff03"
    }
  },
  Pirate = {
    Inc = {
      "Fiona.PirateCoverage.Extraction01"
    }
  },
  PMC = {
    Inc = {
      "Ewan-None-Freeplay-Support-28"
    }
  }
}

function Create(self, uOwnerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oNewSupport.oTarget = self.oTarget
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  oNewSupport.sFinalDestination = self.sFinalDestination
  oDesignator = MrxSupportDesignatorSmoke:Create()
  oNewSupport:SetDesignator(oDesignator)
  oDesignator:SetAATestLevel("none")
  oNewSupport:SetOwner(uOwnerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetModuleName("MrxSupportPickup")
  return oNewSupport
end

function DesignationCallback(self)
  if Net.IsClient() then
    return
  end
  local nDesX, nDesY, nDesZ = self.oDesignator:GetTarget()
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-150, MrxSupport.GetSpawnHeight(), -1, self.uOwner, 300 + math.randi(120))
  if nDesY and nTargetY < nDesY + MrxSupport.GetSpawnHeight() then
    nTargetY = nDesY + MrxSupport.GetSpawnHeight()
  end
  local uHeli = Pg.Spawn(self.uDeliveryVehicle, nTargetX, nTargetY, nTargetZ, 0, false, true)
  Object.AddLabel(uHeli, "ExtractionHelicopter")
  if not uHeli then
    return
  end
  if self.fHeliSpawnedCB then
    self.fHeliSpawnedCB(self, uHeli)
  end
  local nHeliX, nHeliY, nHeliZ = Object.GetPosition(uHeli)
  local nTargetX, nTargetY, nTargetZ = self.oDesignator:GetTarget()
  local nOrientation = Math.GetXZHeading(nTargetX - nHeliX, nTargetY - nHeliY, nTargetZ - nHeliZ)
  Object.SetYaw(uHeli, nOrientation)
  local sFaction = MrxUtil.GetFaction(uHeli)
  if sFaction and tCues[sFaction] and tCues[sFaction].Inc then
    local sCue = MrxUtil.GetRandomTableElement(tCues[sFaction].Inc)
    MrxVoSequence.Start(sCue, nil, MrxVoSequence.knPriorityFreeplay)
  end
  Event.Create(Event.ObjectHibernation, {uHeli, "awake"}, _WaitCallback, {self, uHeli})
end

function SetHeliDestroyedCB(self, fCallback, tArgs)
  self.fHeliDestroyedCB = fCallback
  self.tHeliDestroyedCBArgs = tArgs
end

function SetPilotKilledCB(self, fCallback, tArgs)
  self.fPilotKilledCB = fCallback
  self.tPilotKilledCBArgs = tArgs
end

function SetHeliLandedCB(self, fCallback, tArgs)
  self.fHeliLandedCB = fCallback
  self.tHeliLandedCBArgs = tArgs
end

function SetHeliSpawnedCB(self, fCallback, tArgs)
  self.fHeliSpawnedCB = fCallback
  self.fHeliSpawnedCBArgs = tArgs
end

function SetHeliDamagedCB(self, fCallback, tArgs)
  self.fHeliDamagedCB = fCallback
  self.fHeliDamagedCBArgs = tArgs
end

function _WaitCallback(self, uHeli)
  local nX, nY, nZ = self.oDesignator:GetTarget()
  self.DamageEvent = MrxSupport.SetupDamageEvent(self, uHeli, false)
  if self.fHeliDamagedCB then
    self.HeliDamagedCBEvent = Event.Create(Event.ObjectHealth, {
      uHeli,
      "<",
      Object.GetHealth(uHeli) - 25
    }, self.fHeliDamagedCB, {
      self.fHeliDamagedCBArgs
    })
  end
  if self.fHeliDestroyedCB then
    self.HeliDestroyedCBEvent = Event.Create(Event.ObjectDeath, {uHeli}, self.fHeliDestroyedCB, {
      self.tHeliDestroyedCBArgs
    })
  end
  local uPilot = Vehicle.GetDriver(uHeli)
  self.PilotKilledEvent = MrxSupport.SetupPilotKilledEvent(self, uHeli, false)
  if self.fPilotKilledCB then
    self.fPilotKilledCBEvent = Event.Create(Event.ObjectDeath, {uPilot}, function()
      Object.RemoveLabel(uGuid, "ExtractionHelicopter")
      self.fPilotKilledCB(self.tPilotKilledCBArgs)
    end, {})
  end
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(uHeli),
    Goal = "HeliLand",
    Location = {
      nX,
      nY,
      nZ
    },
    Priority = "hiPri",
    Force = true,
    Callback = _VehicleLanded,
    CallbackData = {self, uHeli}
  })
end

tFactionToId = {
  VZ = "Vza",
  Allied = "All",
  China = "Chi",
  Guerilla = "Gur",
  OC = "Oil",
  Pirate = "Pir",
  PMC = "Pmc",
  Civ = "Civ"
}

function SetPickupVehicle(self, sVehicleTemplateName)
  if "table" ~= type(self) then
    return
  end
  if "string" ~= type(sVehicleTemplateName) then
    return
  end
  self.sDeliveryVehicle = sVehicleTemplateName
  self.uDeliveryVehicle = Pg.GetGuidByName(sVehicleTemplateName)
  local sFaction = MrxUtil.GetFaction(self.uDeliveryVehicle)
  if sFaction then
    self:SetFaction(tFactionToId[sFaction])
  end
end

function SetFinalDestination(self, oFinalDestination)
  if "table" ~= type(self) then
    return
  end
  self.oFinalDestination = oFinalDestination
end

function _VehicleLanded(self, uHeli, uDriver, nState)
  if nState == 0 then
    MrxSupport.DenialMessage("abortnodrop")
    GoHome(self, uHeli, uDriver)
    return
  end
  Ai.Role({
    AIGuid = uDriver,
    Role = "Idle",
    Priority = "hiPri"
  })
  self.TimeOut = Event.Create(Event.TimerRelative, {30}, MrxSupport.GoHome, {self, uHeli})
  Event.Create(Event.ObjectInSeat, {
    "Prisoner",
    uHeli,
    "Any",
    "Enter"
  }, MrxSupport.GoHome, {
    self,
    uHeli,
    uDriver
  })
  Event.Create(Event.ObjectInSeat, {
    "Human",
    uHeli,
    "Driver",
    "x"
  }, DriverExited, {self, uHeli})
  local sFaction = MrxUtil.GetFaction(uHeli)
  if sFaction and tCues[sFaction] and tCues[sFaction].Land then
    local sCue = MrxUtil.GetRandomTableElement(tCues[sFaction].Land)
    MrxVoSequence.Start(sCue, nil, MrxVoSequence.knPriorityFreeplay)
  end
  if self.fHeliLandedCB then
    self.fHeliLandedCB(self.tHeliLandedCBArgs)
  end
end

function DriverExited(self, uGuid, uDriver)
  if uDriver and Object.IsAlive(uDriver) then
    Ai.Role({
      AIGuid = uDriver,
      Role = "Idle",
      Priority = "lowPri"
    })
  end
  if self.DamageEvent then
    Event.Delete(self.DamageEvent)
  end
  if self.PilotKilledEvent then
    Event.Delete(self.PilotKilledEvent)
  end
  if self.fPilotKilledCBEvent then
    Event.Delete(self.fPilotKilledCBEvent)
  end
  if self.HeliDamagedCBEvent then
    Event.Delete(self.HeliDamagedCBEvent)
  end
  if self.HeliDestroyedCBEvent then
    Event.Delete(self.HeliDestroyedCBEvent)
  end
  self:Abandon(uGuid)
  Object.RemoveLabel(uGuid, "ExtractionHelicopter")
end
