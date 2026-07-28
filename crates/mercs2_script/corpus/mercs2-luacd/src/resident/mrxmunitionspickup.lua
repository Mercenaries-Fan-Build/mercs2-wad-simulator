inherit("MrxSupport")
import("MrxSupportManager")
import("MrxSupportDesignatorSmoke")
import("Munitions")
import("MrxUtil")
import("MrxVoSequence")
sDeliveryVehicle = "UH1 Transport (PMC) (Driver)"

function Create(self, uOwnerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oNewSupport.oTarget = self.oTarget
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  oNewSupport.oUpdateEvent = nil
  oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetValidationFunction(nil)
  oDesignator:SetSmokeColor("green")
  oDesignator:SetAATestLevel("none")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uOwnerGuid)
  oNewSupport:SetModuleName("MrxMunitionsPickup")
  oNewSupport:SetRecruit("Copter")
  return oNewSupport
end

function DesignationCallback(self)
  _DesignatorCallback(self)
end

function SetDeliveryVehicle(self, sVehicleTemplateName)
  if "table" ~= type(self) then
    return
  end
  if "string" ~= type(sVehicleTemplateName) then
    return
  end
  self.sDeliveryVehicle = sVehicleTemplateName
  self.uDeliveryVehicle = Pg.GetGuidByName(sVehicleTemplateName)
end

function SetFinalDestination(self, oFinalDestination)
  if "table" ~= type(self) then
    return
  end
  self.oFinalDestination = oFinalDestination
end

function _DesignatorCallback(self)
  local nDesX, nDesY, nDesZ = self.oDesignator:GetTarget()
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-150, MrxSupport.GetSpawnHeight(), -1, self.uOwner)
  if nDesY and nTargetY < nDesY + MrxSupport.GetSpawnHeight() then
    nTargetY = nDesY + MrxSupport.GetSpawnHeight()
  end
  local uHeli = Pg.Spawn(self.uDeliveryVehicle, nTargetX, nTargetY, nTargetZ, 0, false, true)
  if not uHeli then
    return
  end
  local nHeliX, nHeliY, nHeliZ = Object.GetPosition(uHeli)
  local nTargetX, nTargetY, nTargetZ = self.oDesignator:GetTarget()
  local nOrientation = Math.GetXZHeading(nTargetX - nHeliX, nTargetY - nHeliY, nTargetZ - nHeliZ)
  Object.SetYaw(uHeli, nOrientation)
  Event.Create(Event.ObjectHibernation, {uHeli, "awake"}, _WaitCallback, {
    self,
    uHeli,
    nTargetX,
    nTargetY,
    nTargetZ
  })
end

function _WaitCallback(self, uHeli)
  bPickupInProgress = true
  local tVO = {
    "Ewan.Support.Munitions01",
    "Ewan.Support.Munitions02",
    "Ewan.Support.Munitions03",
    "Fiona.Support.Munitions01"
  }
  MrxVoSequence.Start(MrxUtil.GetRandomTableElement(tVO), nil, MrxVoSequence.knPriorityFreeplay)
  MrxSupport.SetupDamageEvent(self, uHeli, false)
  self.oNoMunitionsScriptEvent = Event.Create(Event.ScriptEvent, {
    "NoMunitions",
    function()
      return true
    end
  }, function()
    MrxSupport.Abort(self, uHeli, "NoMunitions")
    Debug.Printf("RECEIVED NOMUNITIONS EVENT")
  end)
  PickMunitionsTarget(self, uHeli)
end

function PickMunitionsTarget(self, uHeli)
  Object.DetachCargoFromWinch(uHeli)
  self.pu = Munitions.GetTaggedMunition()
  if not self.pu then
    Debug.Printf("Pickup Munition nil ")
    MrxSupport.Abort(self, uHeli, "NoMunitions")
    return
  end
  self.oMunitionsKilledEvent = Event.Create(Event.ObjectDeath, {
    self.pu
  }, PickMunitionsTarget, {self, uHeli})
  self.oMunitionsSleepEvent = Event.Create(Event.ObjectHibernation, {
    self.pu,
    "hibernated"
  }, PickMunitionsTarget, {self, uHeli})
  self.oUntagScriptEvent = Event.Create(Event.ScriptEvent, {
    "UntagMunitions",
    function(tData)
      return tData[1] == self.pu
    end
  }, function()
    PickMunitionsTarget(self, uHeli)
    Debug.Printf("CURRENT PICKUP TARGET UN-TAGGED")
  end)
  Debug.Printf("Pickup Munition " .. tostring(pu))
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(uHeli),
    Goal = "Pickup",
    Target = self.pu,
    Priority = "hiPri",
    Callback = Pickup,
    Force = true,
    CallbackData = {
      self,
      uHeli,
      Vehicle.GetDriver(uHeli),
      self.pu
    }
  })
  tImmediatePickupData = {
    self = self,
    uHeli = uHeli,
    AIGuid = Vehicle.GetDriver(uHeli),
    Target = self.pu
  }
end

bPickupInProgress = false
tImmediatePickupData = {}

function ImmediatePickup()
  if not bPickupInProgress or not Munitions.GetTaggedMunition() then
    return
  end
  Munitions.PickupAllMunitions()
  if tImmediatePickupData.self.oNoMunitionsScriptEvent then
    Debug.Printf("Cancelling event inside ImmediatePickup")
    Event.Delete(tImmediatePickupData.self.oNoMunitionsScriptEvent)
    tImmediatePickupData.self.oNoMunitionsScriptEvent = nil
  end
  if tImmediatePickupData.self.oMunitionsKilledEvent then
    Event.Delete(tImmediatePickupData.self.oMunitionsKilledEvent)
    tImmediatePickupData.self.oMunitionsKilledEvent = nil
  end
  if tImmediatePickupData.self.oUntagScriptEvent then
    Event.Delete(tImmediatePickupData.self.oUntagScriptEvent)
    tImmediatePickupData.self.oUntagScriptEvent = nil
  end
  if tImmediatePickupData.self.oMunitionsSleepEvent then
    Event.Delete(tImmediatePickupData.self.oMunitionsSleepEvent)
    tImmediatePickupData.self.oMunitionsSleepEvent = nil
  end
  Event.Create(Event.ObjectHibernation, {
    tImmediatePickupData.uHeli,
    "hibernated"
  }, Object.Remove, {
    tImmediatePickupData.uHeli
  })
  Event.Create(Event.ObjectDelete, {
    tImmediatePickupData.uHeli
  }, MrxSupportManager.MakeRecruitAvailable, {"Copter"})
  Object.FadeOut(tImmediatePickupData.uHeli, 0.1, true)
  if Object.IsWinched(tImmediatePickupData.Target) then
    Object.FadeOut(tImmediatePickupData.Target, 0.1, true)
  end
  tImmediatePickupData.self.bSupportComplete = true
  tImmediatePickupData = {}
  bPickupInProgress = false
end

function Pickup(self, uHeli, uDriver, pu, nState)
  Debug.Printf("PICKUP")
  if nState == 0 then
    Debug.Printf("Pickup command failed for some reason!")
    Object.DetachCargoFromWinch(uHeli)
    MrxSupport.GoHome(self, uHeli)
  end
  if self.oNoMunitionsScriptEvent then
    Debug.Printf("Cancelling event inside Pickup")
    Event.Delete(self.oNoMunitionsScriptEvent)
    self.oNoMunitionsScriptEvent = nil
  end
  if self.oUntagScriptEvent then
    Event.Delete(self.oUntagScriptEvent)
    self.oUntagScriptEvent = nil
  end
  if self.oMunitionsKilledEvent then
    Event.Delete(self.oMunitionsKilledEvent)
    self.oMunitionsKilledEvent = nil
  end
  if self.oMunitionsSleepEvent then
    Event.Delete(self.oMunitionsSleepEvent)
    self.oMunitionsSleepEvent = nil
  end
  local uFaction
  if MrxUtil.GetFaction(pu) then
    uFaction = Pg.GetGuidByName(MrxUtil.GetFaction(pu))
  end
  if uFaction then
    Debug.Printf("Posting Infraction (" .. tostring(MrxUtil.GetFaction(uFaction)) .. ")")
    Ai.AddInfraction(Player.GetPrimaryCharacter(), uFaction, 5)
    if Player.GetSecondaryCharacter() then
      Ai.AddInfraction(Player.GetSecondaryCharacter(), uFaction, 5)
    end
  else
    Debug.Printf("No faction found for munitions object")
  end
  Munitions.PickupAllMunitions()
  tImmediatePickupData = {}
  bPickupInProgress = false
  Debug.Printf("Pickup completed, going home")
  MrxSupport.GoHome(self, uHeli, pu)
end
