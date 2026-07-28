inherit("MrxSupport")
import("MrxSupportDesignatorSmoke")
import("MrxSupportDesignatorFlare")
import("MrxTransit")
import("MrxUtil")
import("MrxState")
import("WifPmcInterior")
import("MrxTutorialManager")
sDeliveryVehicle = "UH1 Transport (Transit)"
nAltitude = 250
local tVOOnTheWay = {
  "Ewan-None-Freeplay-Support-73",
  "Ewan-None-Freeplay-Support-38",
  "Ewan-None-Freeplay-Support-89"
}
local tVOGoHome = {
  "Ewan-None-Freeplay-Support-90",
  "Ewan-None-Freeplay-Support-99"
}
bTransitInterfaceActive = false

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetSmokeColor("blue")
  oDesignator:SetAATestLevel("none")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Copter")
  oNewSupport:SetModuleName("MrxSupportTransit")
  oNewSupport:SetPickupVehicle(self.sDeliveryVehicle)
  oNewSupport.bUnrestrictedByFuel = true
  return oNewSupport
end

function Commence(self, bFireImmediately)
  local uCharacter
  if "userdata" == type(self:GetOwner()) then
    uCharacter = Player.GetCharacter(self:GetOwner())
  end
  if Human.IsSwimming and uCharacter and Human.IsSwimming(uCharacter) then
    self:SetDesignator(MrxSupportDesignatorFlare:Create())
  end
  MrxSupport.Commence(self, bFireImmediately)
end

function DesignationCallback(self)
  if Net.IsClient() then
    return
  end
  if not MrxTransit.IsSystemInitialized() then
    MrxTransit.Reset()
  end
  if not MrxTransit.IsSystemEnabled() then
    return false
  end
  local uCharacter
  if "userdata" == type(self:GetOwner()) then
    uCharacter = Player.GetCharacter(self:GetOwner())
  end
  self.bWaterPickup = false
  if uCharacter and Human.IsSwimming(uCharacter) then
    self.bWaterPickup = true
  end
  local nDesX, nDesY, nDesZ = self.oDesignator:GetTarget()
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-150, MrxSupport.GetSpawnHeight(), 30, self.uOwner)
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
  MrxSupport.PlayRandomVOCue(tVOOnTheWay)
  Event.Create(Event.ObjectHibernation, {uHeli, "awake"}, _WaitCallback, {self, uHeli})
end

function _WaitCallback(self, uHeli)
  local nX, nY, nZ = self.oDesignator:GetTarget()
  local uDriver = Vehicle.GetDriver(uHeli)
  self.eAbort = MrxSupport.SetupDamageEvent(self, uHeli, false)
  if self.bWaterPickup then
    Ai.Goal({
      AIGuid = Vehicle.GetDriver(uHeli),
      Goal = "MoveTo",
      Location = {
        nX,
        nY,
        nZ
      },
      Priority = "hiPri",
      Force = true,
      Callback = _OpenTransitInterface,
      CallbackData = {
        self,
        uHeli,
        uDriver
      }
    })
  else
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
    self.EnterEvent = Event.CreatePersistent(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      uHeli,
      "a",
      "e"
    }, _OpenTransitInterface, {
      self,
      uHeli,
      uDriver
    })
    self.ExitEvent = Event.CreatePersistent(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      uHeli,
      "a",
      "x"
    }, _PlayerExited, {
      self,
      uHeli,
      uDriver
    })
  end
  self.DeathEvent = Event.Create(Event.ObjectDeath, {uHeli}, _HandleDeath, {self})
  self.HibernateEvent = Event.Create(Event.ObjectHibernation, {uHeli, "hibernated"}, _OnHibernate, {self, uHeli})
  self.PlayerLeftEvent = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerLeft",
    function(tData)
      return true
    end
  }, _PlayerLeftGame, {
    self,
    uHeli,
    uDriver
  })
end

function SetPickupVehicle(self, sVehicleTemplateName)
  if "table" ~= type(self) then
    return
  end
  if "string" ~= type(sVehicleTemplateName) then
    return
  end
  self.sDeliveryVehicle = sVehicleTemplateName
  self.uDeliveryVehicle = Pg.GetGuidByName(sVehicleTemplateName)
end

function _VehicleLanded(self, uHeli, uDriver, nState)
  if nState == 0 then
    MrxSupport.DenialMessage("abortnodrop")
    MrxSupport.GoHome(self, uHeli, uDriver)
    return
  end
  self.uIdleGoal = Ai.Goal({
    AIGuid = uDriver,
    Goal = "Idle",
    Priority = "hiPri",
    MaintainRotorSpeed = true
  })
  if self.TimeoutEvent == nil then
    self.TimeoutEvent = Event.Create(Event.TimerRelative, {45}, _RemoveTimeoutEvent, {
      self,
      uHeli,
      uDriver
    })
  end
  self.bVehicleLanded = true
end

function _RemoveTimeoutEvent(self, uHeli, uDriver)
  self.TimeoutEvent = nil
  MrxSupport.GoHome(self, uHeli, uDriver)
  self.bVehicleLanded = nil
  Event.Delete(self.EnterEvent)
  Net.SendCustomEvent("MrxSupportTransit", NETEVENT_CLEARMESSAGE, {}, true)
end

function _OnHibernate(self, uHeli)
  _Cleanup(self)
  Object.Remove(uHeli, true)
  if MrxSupportManager.GetRecruitTimes("Copter") ~= nil then
    MrxSupportManager.MakeRecruitAvailable("Copter")
  end
end

function _OpenTransitInterface(self, uHeli, uDriver, uPlayer)
  Event.Create(Event.TimerRelative, {0.5, true}, MrxVoSequence.Start, {
    MrxUtil.GetRandomTableElement({
      "Ewan.Transit.Ok1",
      "Ewan.Transit.Go3",
      "Ewan.Transit.Go2",
      "Ewan.Transit.Go1",
      "Ewan-In-Mission-Contract-Oil02-134"
    }),
    nil,
    MrxVoSequence.knPriorityFreeplay
  })
  if uPlayer == Player.GetSecondaryCharacter() then
    Net.SendCustomEvent("MrxSupportTransit", NETEVENT_SHOWMESSAGE, {})
  end
  if self.TimeoutEvent then
    Event.Delete(self.TimeoutEvent)
    self.TimeoutEvent = nil
  end
  if not self.bWaterPickup then
    for i, uPlayer in ipairs(Player.GetAllPlayers()) do
      local uVehicle = Player.GetControlledObject(uPlayer)
      if uVehicle ~= uHeli then
        return false
      end
    end
  end
  MrxTransit.OpenInterface(Player.GetLocalPlayer(), _TransitInterfaceCallback, {self, uHeli})
  bTransitInterfaceActive = true
  return true
end

function _PlayerExited(self, uHeli, uDriver, uPlayer)
  if uPlayer == Player.GetSecondaryCharacter() then
    Net.SendCustomEvent("MrxSupportTransit", NETEVENT_CLEARMESSAGE, {})
  end
  if not self.bWaterPickup then
    local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
    if oPda ~= nil and bTransitInterfaceActive == true then
      bTransitInterfaceActive = false
      oPda:Close()
    end
    for i, uPlayer in ipairs(Player.GetAllPlayers()) do
      local uVehicle = Player.GetControlledObject(uPlayer)
      if uVehicle == uHeli then
        return false
      end
    end
    if self.TimeoutEvent == nil and self.bVehicleLanded == true then
      self.TimeoutEvent = Event.Create(Event.TimerRelative, {10}, _RemoveTimeoutEvent, {
        self,
        uHeli,
        uDriver
      })
    end
  end
  return true
end

function _PlayerLeftGame(self, uHeli, uDriver)
  if not self.bWaterPickup then
    local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
    local bHeliHasNoPlayers = true
    local bAllPlayersInHeli = true
    for i, uPlayer in ipairs(Player.GetAllPlayers()) do
      local uVehicle = Player.GetControlledObject(uPlayer)
      if uVehicle == uHeli then
        bHeliHasNoPlayers = false
      else
        bAllPlayersInHeli = false
      end
    end
    if bAllPlayersInHeli == true and oPda ~= nil and bTransitInterfaceActive == false then
      MrxTransit.OpenInterface(Player.GetLocalPlayer(), _TransitInterfaceCallback, {self, uHeli})
      bTransitInterfaceActive = true
    end
    if bHeliHasNoPlayers == true and self.bVehicleLanded == true and self.TimeoutEvent == nil then
      self.TimeoutEvent = Event.Create(Event.TimerRelative, {10}, _RemoveTimeoutEvent, {
        self,
        uHeli,
        uDriver
      })
    end
  end
  return true
end

function _TransitInterfaceCallback(self, uHeli, nSelectedIndex, bSuccess)
  Net.SendCustomEvent("MrxSupportTransit", NETEVENT_CLEARMESSAGE, {})
  if bTransitInterfaceActive == false then
    return
  end
  bTransitInterfaceActive = false
  local uPoint = MrxTransit.GetTransitPoint(nSelectedIndex)
  if bSuccess then
    self:TransitToPoint(uHeli, uPoint)
  else
    AllPlayersExitVehicle(true)
  end
end

function TransitInterfaceCallbackBriefing(self, nSelectedIndex)
  if nSelectedIndex == 1 then
    return
  end
  bTransitInterfaceActive = true
  local uPlayer = Player.GetPrimaryCharacter()
  local oSupport = self:Create(uPlayer)
  local x, y, z = Object.GetPosition(uPlayer)
  local yaw = Object.GetYaw(uPlayer)
  local uHeli = Pg.Spawn(oSupport.uDeliveryVehicle, x, y + 20, z, yaw, false, true)
  Event.Create(Event.ObjectHibernation, {uHeli, "awake"}, _WaitCallbackBriefing, {
    self,
    oSupport,
    uHeli,
    uPlayer,
    nSelectedIndex
  })
end

function _WaitCallbackBriefing(self, oSupport, uHeli, uPlayer, nSelectedIndex)
  Vehicle.Enter(uHeli, uPlayer, "p", true, false)
  if WifPmcInterior.IsInside() and not WifPmcInterior.IsEntering() then
    WifPmcInterior.Exit(-1, false)
  end
  oSupport:_TransitInterfaceCallback(uHeli, nSelectedIndex, true)
end

function TransitToPoint(self, uHeli, uPoint)
  if uPoint and MrxUtil.GetDistanceBetween(uPoint, uHeli, false) > 15 then
    Debug.Printf("-----= Starting Transit with heli ", uHeli, " to Point ", uPoint)
    if self.bWaterPickup then
      Vehicle.Enter(uHeli, Player.GetPrimaryCharacter(), "p", true, false)
    end
    Net.SendCustomEvent("MrxSupportTransit", NETEVENT_ENTERVEHICLE, {uHeli}, true)
    Event.Post("transitStart", {uHeli})
    MrxTransit.StartTransit(_StartTransit, _FinishTransit, {
      self,
      uHeli,
      uPoint
    })
  end
end

function _StartTransit(self, uHeli, uPoint)
  local x, y, z = Object.GetPosition(uPoint)
  local yaw = Object.GetYaw(uPoint)
  local offset = 8
  Object.SetYaw(uHeli, yaw)
  Object.SetPosition(uHeli, x, y + offset, z)
  if self.HibernateEvent then
    Event.Delete(self.HibernateEvent)
  end
  Vehicle.SetCanPlayerUse(uHeli, "a", false)
  x, y, z = Object.GetPosition(uHeli)
  Debug.Printf("-----= Teleporting ", uHeli, " (", x, ", ", y, ", ", z, ")")
  MrxUtil.ClearVehiclesNearPoint(uPoint, uHeli)
end

function _FinishTransit(self, uHeli, uPoint)
  local x, y, z = Object.GetPosition(uPoint)
  local uDriver = Vehicle.GetDriver(uHeli)
  if self.eAbort then
    Event.Delete(self.eAbort)
    self.eAbort = nil
  end
  if self.uIdleGoal then
    Ai.RemoveGoal({
      AIGuid = uDriver,
      Handle = self.uIdleGoal
    })
    self.uIdleGoal = nil
  end
  local res = Ai.Goal({
    AIGuid = uDriver,
    Goal = "HeliLand",
    Location = {
      x,
      y,
      z
    },
    Priority = "hiPri",
    Force = true,
    Callback = _FinishTransit2,
    CallbackData = {self, uHeli}
  })
  Debug.Printf("-----= Landing ", uHeli, " res: ", res, " point: ", uPoint, " (", x, ", ", y, ", ", z, ")")
  x, y, z = Object.GetPosition(uHeli)
  Debug.Printf("-----= Heli at cur pos ", uHeli, " (", x, ", ", y, ", ", z, ")")
  _Cleanup(self)
end

function _FinishTransit2(self, uHeli)
  AllPlayersExitVehicle(true)
  Event.Post("transitEnd", {uHeli})
  Event.Create(Event.TimerRelative, {1.5}, MrxSupport.GoHome, {self, uHeli})
  self.HibernateEvent = Event.Create(Event.ObjectHibernation, {uHeli, "hibernated"}, _OnHibernate, {self, uHeli})
end

NETEVENT_ENTERVEHICLE = 0
NETEVENT_EXITVEHICLE = 1
NETEVENT_SHOWMESSAGE = 2
NETEVENT_CLEARMESSAGE = 3

function NetEventCallback(eventId, tArgs)
  if eventId == NETEVENT_ENTERVEHICLE then
    Vehicle.Enter(tArgs[1], Player.GetSecondaryCharacter(), "p", true, false)
    Player.SetCinematicMode(Player.GetSecondaryPlayer(), false)
    MrxState.SetQuickFade(false)
  elseif eventId == NETEVENT_EXITVEHICLE then
    AllPlayersExitVehicle()
  elseif eventId == NETEVENT_SHOWMESSAGE then
    MrxTutorialManager.ShowMessage("[Tutorial.ClientTransit]", true, "TransitTutorial")
  elseif eventId == NETEVENT_CLEARMESSAGE then
    MrxTutorialManager.HideMessage(true, "TransitTutorial")
  end
end

function AllPlayersExitVehicle(bSendEvent)
  if bSendEvent then
    Net.SendCustomEvent("MrxSupportTransit", NETEVENT_EXITVEHICLE, {}, true)
  end
  local uChar1 = Player.GetPrimaryCharacter()
  if uChar1 then
    local uVehicle1 = Player.GetControlledObject(Player.GetPrimaryPlayer())
    if uVehicle1 then
      Vehicle.Exit(uVehicle1, uChar1, false)
    end
  end
  local uChar2 = Player.GetSecondaryCharacter()
  if uChar2 then
    local uVehicle2 = Player.GetControlledObject(Player.GetSecondaryPlayer())
    if uVehicle2 then
      Vehicle.Exit(uVehicle2, uChar2, true)
    end
  end
  Net.SendCustomEvent("MrxSupportTransit", NETEVENT_CLEARMESSAGE, {}, true)
end

function _Cleanup(self)
  if self.EnterEvent then
    Event.Delete(self.EnterEvent)
    self.EnterEvent = nil
  end
  if self.ExitEvent then
    Event.Delete(self.ExitEvent)
    self.ExitEvent = nil
  end
  if self.PlayerLeftEvent then
    Event.Delete(self.PlayerLeftEvent)
    self.PlayerLeftEvent = nil
  end
  if self.TimeoutEvent then
    Event.Delete(self.TimeoutEvent)
    self.TimeoutEvent = nil
  end
  if self.DeathEvent then
    Event.Delete(self.DeathEvent)
    self.DeathEvent = nil
  end
  if self.eAbort then
    Event.Delete(self.eAbort)
    self.eAbort = nil
  end
  bTransitInterfaceActive = false
  Net.SendCustomEvent("MrxSupportTransit", NETEVENT_CLEARMESSAGE, {}, true)
end

function _HandleDeath(self)
  self.DeathEvent = nil
  _Cleanup(self)
end
