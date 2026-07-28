inherit("MrxSupport")
import("MrxSupportManager")
import("MrxSupportDesignatorSmoke")
import("MrxUtil")
import("MrxVoSequence")

function Create(self, uOwnerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oNewSupport.oTarget = self.oTarget
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  oNewSupport.sFinalDestination = self.sFinalDestination
  oNewSupport:SetRecruit("Copter")
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetAATestLevel("none")
  oDesignator:SetSmokeColor("blue")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uOwnerGuid)
  oNewSupport:SetModuleName("MrxSupportCopterDelivery")
  return oNewSupport
end

function DesignationCallback(self)
  if Net.IsClient() then
    return
  end
  local nDesX, nDesY, nDesZ = self.oDesignator:GetTarget()
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-150, MrxSupport.GetSpawnHeight(), -1, self.uOwner, 300 + math.randi(120))
  local uHeli = Pg.Spawn(self.uDeliveryVehicle, nTargetX, nTargetY, nTargetZ, 0, false, true)
  if not uHeli then
    return
  end
  local nHeliX, nHeliY, nHeliZ = Object.GetPosition(uHeli)
  local nTargetX, nTargetY, nTargetZ = self.oDesignator:GetTarget()
  local nOrientation = Math.GetXZHeading(nTargetX - nHeliX, nTargetY - nHeliY, nTargetZ - nHeliZ)
  Object.SetYaw(uHeli, nOrientation)
  MrxVoSequence.Start("Ewan-None-Freeplay-Support-10", nil, MrxVoSequence.knPriorityFreeplay)
  Event.Create(Event.ObjectHibernation, {uHeli, "awake"}, _HeliReady, {self, uHeli})
end

function _WaitCallback(self, uHeli)
end

function _HeliReady(self, uHeli)
  Object.AddToDisposer(uHeli, "Vehicle")
  Debug.Printf("C spawned and ready")
  local nX, nY, nZ = self.oDesignator:GetTarget()
  self.DamageEvent = MrxSupport.SetupDamageEvent(self, uHeli, false)
  self.LandGoal = Ai.Goal({
    AIGuid = Vehicle.GetDriver(uHeli),
    Goal = "HeliLand",
    Location = {
      nX,
      nY + 50,
      nZ
    },
    Priority = "hiPri",
    Force = true,
    Callback = _VehicleLanded,
    CallbackData = {self, uHeli}
  })
end

function SetFinalDestination(self, oFinalDestination)
  if "table" ~= type(self) then
    return
  end
  self.oFinalDestination = oFinalDestination
end

function _VehicleLanded(self, uHeli, uDriver, nState)
  Debug.Printf("<--> _VehicleLanded: " .. tostring(nState))
  Vehicle.SetCanPlayerUse(uHeli, "d", true)
  Event.Delete(self.DamageEvent)
  if nState == 0 then
    MrxSupport.DenialMessage("abortnodrop")
    Debug.Printf("MrxSupportCopterDelivery: Landing order failed!")
    GoHome(self, uHeli, uDriver)
    self:RefundCosts()
    return
  end
  if self.DamageEvent then
    Event.Delete(self.DamageEvent)
    self.DamageEvent = nil
  end
  Vehicle.Exit(uHeli, uDriver, false)
  Event.Create(Event.ObjectInSeat, {
    uDriver,
    uHeli,
    "D",
    "X"
  }, ExitedVehicle, {self, uDriver})
end

function ExitedVehicle(self, uDriver)
  local tVo = {
    "Ewan-In-Mission-Contract-Pmc03-128",
    "Ewan.Misc.DeliverHeli01",
    "Ewan.Misc.DeliverHeli02",
    "Ewan.Misc.DeliverHeli03"
  }
  local x, y, z = Object.GetPosition(uDriver)
  uGoal = Ai.Goal({
    AIGuid = uDriver,
    Goal = "MoveTo",
    Haste = 0.2,
    Location = {
      x - 10,
      y,
      z - 10
    },
    Priority = "hiPri",
    Callback = Object.FadeOut,
    CallbackData = {
      uDriver,
      1,
      true
    },
    Force = true
  })
  local sCue = MrxUtil.GetRandomTableElement(tVo)
  MrxVoSequence.Start(sCue, nil, MrxVoSequence.knPriorityFreeplay)
  self.Hibernation = Event.Create(Event.ObjectHibernation, {uDriver, "Hibernated"}, Object.Remove, {uDriver})
  Event.Create(Event.ObjectDelete, {uDriver}, MrxSupportManager.MakeRecruitAvailable, {"Copter"})
  self.Timer = Event.CreatePersistent(Event.TimerRelative, {3}, CheckEwan, {self, uDriver})
end

function CheckEwan(self, uDriver)
  if not Object.IsVisible(uDriver) then
    Object.Remove(uDriver)
    Event.Delete(self.Timer)
    Event.Delete(self.Hibernation)
  end
end
